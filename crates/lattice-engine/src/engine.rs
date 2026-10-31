//! The sequencer and mixer, driven entirely from the audio callback.
//!
//! Signal path: each track's instrument, then its effects, then its mixer strip (level, pan,
//! mute and solo) into the master bus, with a post-fader send into the send bus. The send bus
//! runs its own effects into the master bus, which runs its effects, then the click is added.
//!
//! The UI thread talks to the engine through a [`Handle`]: commands go over a lock-free queue,
//! and playback position comes back through atomics in [`Status`]. Nothing on the audio side
//! blocks or allocates. Replaced patterns and removed tracks are sent back to the UI thread to be
//! freed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use lattice_core::{Grid, MAX_STEPS, Mix, Step, Track, db_to_gain, realise_into};
pub use lattice_core::{MAX_EFFECTS, MAX_TRACKS};
use rtrb::{Consumer, Producer, PushError, RingBuffer};

use crate::click::Click;
use crate::clock::Clock;
use crate::denormal;
use crate::param::Smoothed;
use crate::processor::{Event, EventKind, Processor};

const BEATS_PER_BAR: u64 = 4;
/// Largest block rendered in one go. Longer host buffers are split.
const MAX_BLOCK: usize = 256;
/// Events per track per block; more are dropped rather than allocating.
const MAX_EVENTS: usize = 128;
/// Notes a track can hold at once before the oldest is cut.
const MAX_HELD: usize = 16;
const COMMAND_QUEUE: usize = 256;

const PREVIEW_SECONDS: f32 = 0.3;
const CLICK_PITCH: u8 = 84;
const CLICK_ACCENT_PITCH: u8 = 96;

/// Messages from the UI thread to the audio thread.
#[derive(Debug)]
pub enum Command {
    Play,
    Stop,
    SetBpm(f64),
    SetClick(bool),
    /// Replace a track's pattern; heard from the current step on. The old pattern is sent back
    /// to be freed off the audio thread. Tracks longer than `MAX_STEPS` are ignored.
    SetTrack {
        index: usize,
        track: Box<Track>,
    },
    /// Insert a track and its instrument at `index` (or at the end, if past it), moving later
    /// tracks along. Joins in step with the others. Build the player off the audio thread with
    /// [`TrackPlayer::new`]; it's sent straight back if there are already `MAX_TRACKS` tracks.
    InsertTrack {
        index: usize,
        player: Box<TrackPlayer>,
    },
    /// Swap the instrument on the track at `index`. The old one goes back to the UI thread to be
    /// freed, as does this one if there's no such track. Build it off the audio thread.
    SetInstrument {
        index: usize,
        instrument: Box<dyn Processor>,
    },
    /// Remove the track at `index`, moving later tracks back. It goes back to the UI thread,
    /// instrument and all, to be freed.
    RemoveTrack(usize),
    /// Move a track to another index, keeping its instrument and any notes still sounding.
    MoveTrack {
        from: usize,
        to: usize,
    },
    /// Audition a note on a track's instrument.
    Preview {
        track: usize,
        pitch: u8,
        velocity: u8,
    },
    /// Insert an effect into a chain at `index` (or at the end, if past it). Build it off the
    /// audio thread with [`EffectSlot::new`]; it's sent straight back if the chain is full or
    /// doesn't exist.
    InsertEffect {
        chain: Chain,
        index: usize,
        effect: Box<EffectSlot>,
    },
    /// Remove an effect from a chain. It goes back to the UI thread to be freed.
    RemoveEffect {
        chain: Chain,
        index: usize,
    },
    /// Set an instrument's or effect's parameter, in its own units (see
    /// [`Processor::params`]).
    SetParam {
        target: Target,
        id: u32,
        value: f32,
    },
    /// Set the send or master bus's level, in dB. Ignored for a track's chain.
    SetBusLevel {
        bus: Chain,
        level: f32,
    },
}

/// A chain of effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chain {
    /// After a track's instrument, before its mixer strip.
    Track(usize),
    /// The send bus, fed by every track's send.
    Send,
    /// The master bus, after everything is mixed.
    Master,
}

/// Something with parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Instrument(usize),
    Effect { chain: Chain, slot: usize },
}

/// An effect in a chain, with room for the events it receives.
pub struct EffectSlot {
    processor: Box<dyn Processor>,
    events: Vec<Event>,
}

impl EffectSlot {
    /// Allocates, so call it off the audio thread, then send it with
    /// [`Command::InsertEffect`].
    pub fn new(processor: Box<dyn Processor>) -> Box<Self> {
        Box::new(Self {
            processor,
            events: Vec::with_capacity(MAX_EVENTS),
        })
    }
}

impl std::fmt::Debug for EffectSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectSlot").finish_non_exhaustive()
    }
}

/// Room for `MAX_EFFECTS`, boxed for the same reason tracks are.
#[allow(clippy::vec_box)]
type Effects = Vec<Box<EffectSlot>>;

fn effects() -> Effects {
    Vec::with_capacity(MAX_EFFECTS)
}

/// Runs a block through each effect in turn.
fn run_chain(effects: &mut [Box<EffectSlot>], left: &mut [f32], right: &mut [f32]) {
    for slot in effects {
        sort_by_offset(&mut slot.events);
        slot.processor.process(&slot.events, left, right);
        slot.events.clear();
    }
}

/// A track's mixer strip: its level, pan and send, gliding to each new setting.
#[derive(Debug, Clone, Copy)]
struct Strip {
    /// Linear, and 0 when muted or another track is soloed.
    gain: Smoothed,
    pan: Smoothed,
    send: Smoothed,
}

impl Strip {
    fn new(sample_rate: f32) -> Self {
        let smoothed = |value| Smoothed::new(value, sample_rate);
        Self {
            gain: smoothed(0.0),
            pan: smoothed(0.0),
            send: smoothed(0.0),
        }
    }

    fn targets(mix: &Mix, any_solo: bool) -> [f32; 3] {
        let audible = !mix.mute && (mix.solo || !any_solo);
        let gain = if audible { db_to_gain(mix.level) } else { 0.0 };
        [gain, mix.pan, db_to_gain(mix.send)]
    }

    fn set(&mut self, mix: &Mix, any_solo: bool) {
        let [gain, pan, send] = Self::targets(mix, any_solo);
        self.gain.set(gain);
        self.pan.set(pan);
        self.send.set(send);
    }

    /// Starts at the settings rather than gliding to them, for a track that isn't sounding yet.
    fn reset(&mut self, mix: &Mix, any_solo: bool) {
        let [gain, pan, send] = Self::targets(mix, any_solo);
        self.gain.reset(gain);
        self.pan.reset(pan);
        self.send.reset(send);
    }

    /// Mixes a track's block into the master and send buses, returning its peak level.
    fn apply(&mut self, track: [&[f32]; 2], master: [&mut [f32]; 2], send: [&mut [f32]; 2]) -> f32 {
        let [tl, tr] = track;
        let [ml, mr] = master;
        let [sl, sr] = send;
        let settled = self.gain.is_settled() && self.pan.is_settled() && self.send.is_settled();
        // Constant gains for the whole block unless something is gliding: pan needs a cosine.
        let mut gains = self.gains();
        let mut peak = 0.0f32;
        for i in 0..tl.len() {
            if !settled {
                gains = self.gains();
            }
            let [gl, gr, send] = gains;
            let (l, r) = (tl[i] * gl, tr[i] * gr);
            ml[i] += l;
            mr[i] += r;
            sl[i] += l * send;
            sr[i] += r * send;
            peak = peak.max(l.abs()).max(r.abs());
        }
        peak
    }

    /// The next left and right gains and send level. Balance-style pan: the centre is unity,
    /// and panning turns the other side down.
    #[inline]
    fn gains(&mut self) -> [f32; 3] {
        let gain = self.gain.tick();
        let pan = self.pan.tick();
        let send = self.send.tick();
        // Clamped: in f32, a quarter turn's cosine is a hair below 0.
        let away = (pan.abs() * std::f32::consts::FRAC_PI_2).cos().max(0.0);
        let (l, r) = if pan > 0.0 { (away, 1.0) } else { (1.0, away) };
        [gain * l, gain * r, send]
    }
}

/// Playback state the audio thread publishes for the UI.
pub struct Status {
    playing: AtomicBool,
    /// Per track index: `pass << 32 | step`, or `u64::MAX` when stopped or not yet played. One
    /// atomic, so the step and pass are always read together.
    positions: [AtomicU64; MAX_TRACKS],
    /// Per track index: the loudest sample since the UI last looked, as `f32` bits. Levels are
    /// never negative, so comparing the bits compares the levels.
    peaks: [AtomicU32; MAX_TRACKS],
    /// The same for the send and master buses, after their levels.
    bus_peaks: [AtomicU32; 2],
}

impl Status {
    fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            positions: std::array::from_fn(|_| AtomicU64::new(u64::MAX)),
            peaks: std::array::from_fn(|_| AtomicU32::new(0)),
            bus_peaks: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    /// The step `track` is playing, and which pass through the track it's on.
    pub fn position(&self, track: usize) -> Option<(usize, u32)> {
        let packed = self.positions.get(track)?.load(Ordering::Relaxed);
        (packed != u64::MAX).then_some(((packed & 0xffff_ffff) as usize, (packed >> 32) as u32))
    }

    /// The loudest sample `track` has played since the last call, after its mixer strip.
    pub fn take_peak(&self, track: usize) -> f32 {
        self.peaks
            .get(track)
            .map_or(0.0, |p| f32::from_bits(p.swap(0, Ordering::Relaxed)))
    }

    /// The loudest sample the send or master bus has played since the last call.
    pub fn take_bus_peak(&self, bus: Chain) -> f32 {
        let peak = match bus {
            Chain::Send => &self.bus_peaks[0],
            Chain::Master => &self.bus_peaks[1],
            Chain::Track(track) => return self.take_peak(track),
        };
        f32::from_bits(peak.swap(0, Ordering::Relaxed))
    }

    fn add_peak(&self, track: usize, peak: f32) {
        self.peaks[track].fetch_max(peak.to_bits(), Ordering::Relaxed);
    }

    fn set_position(&self, track: usize, packed: u64) {
        self.positions[track].store(packed, Ordering::Relaxed);
    }

    fn set_stopped(&self) {
        self.playing.store(false, Ordering::Relaxed);
        for p in &self.positions {
            p.store(u64::MAX, Ordering::Relaxed);
        }
    }
}

/// What the audio thread has finished with, sent back to be freed on the UI thread. Only ever
/// dropped, never read.
#[allow(dead_code)]
enum Garbage {
    Track(Box<Track>),
    Player(Box<TrackPlayer>),
    Effect(Box<EffectSlot>),
    Instrument(Box<dyn Processor>),
}

/// The UI thread's end of the engine.
pub struct Handle {
    commands: Producer<Command>,
    garbage: Consumer<Garbage>,
    status: Arc<Status>,
}

impl Handle {
    /// Queues a command for the audio thread. Returns false if the queue is full.
    pub fn send(&mut self, command: Command) -> bool {
        self.commands.push(command).is_ok()
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Frees patterns and tracks the audio thread has finished with. Call regularly.
    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
    }
}

#[derive(Debug, Clone, Copy)]
struct Held {
    pitch: u8,
    steps_left: u16,
}

/// A track as the engine plays it: the pattern, its instrument and the buffers to play it with.
pub struct TrackPlayer {
    track: Track,
    instrument: Box<dyn Processor>,
    effects: Effects,
    strip: Strip,
    /// The track as realised for the current pass.
    steps: Vec<Step>,
    playhead: usize,
    pass: u32,
    events: Vec<Event>,
    held: Vec<Held>,
    /// A previewed note and the samples until it's released.
    preview: Option<(u8, usize)>,
    left: Vec<f32>,
    right: Vec<f32>,
    /// What was last published to [`Status`], so it can be published again if the track moves.
    position: u64,
}

impl TrackPlayer {
    /// Allocates, so call it off the audio thread, then send the player with
    /// [`Command::InsertTrack`].
    pub fn new(track: Track, instrument: Box<dyn Processor>, sample_rate: f32) -> Box<Self> {
        Box::new(Self {
            track,
            instrument,
            effects: effects(),
            strip: Strip::new(sample_rate),
            steps: Vec::with_capacity(MAX_STEPS),
            playhead: 0,
            pass: 0,
            events: Vec::with_capacity(MAX_EVENTS),
            held: Vec::with_capacity(MAX_HELD),
            preview: None,
            left: vec![0.0; MAX_BLOCK],
            right: vec![0.0; MAX_BLOCK],
            position: u64::MAX,
        })
    }
}

impl std::fmt::Debug for TrackPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackPlayer")
            .field("track", &self.track.name)
            .finish_non_exhaustive()
    }
}

pub struct Engine {
    clock: Clock,
    playing: bool,
    steps_per_beat: u64,
    step_index: u64,
    /// Boxed, so players arrive and leave without being moved out of their boxes, which would
    /// free the boxes here. Room for `MAX_TRACKS`.
    #[allow(clippy::vec_box)]
    tracks: Vec<Box<TrackPlayer>>,
    send_effects: Effects,
    master_effects: Effects,
    send_left: Vec<f32>,
    send_right: Vec<f32>,
    /// Bus levels, linear.
    send_gain: Smoothed,
    master_gain: Smoothed,
    click: Click,
    click_events: Vec<Event>,
    click_left: Vec<f32>,
    click_right: Vec<f32>,
    click_enabled: bool,
    preview_samples: usize,
    commands: Consumer<Command>,
    garbage: Producer<Garbage>,
    status: Arc<Status>,
}

impl Engine {
    /// `instruments[i]` plays `grid.tracks[i]`. Starts stopped. Allocates, so build it off the
    /// audio thread, then move the engine into the audio callback and keep the handle.
    pub fn new(
        sample_rate: f32,
        bpm: f64,
        grid: &Grid,
        instruments: Vec<Box<dyn Processor>>,
    ) -> (Self, Handle) {
        assert_eq!(
            grid.tracks.len(),
            instruments.len(),
            "one instrument per track"
        );
        assert!(grid.tracks.len() <= MAX_TRACKS, "too many tracks");
        assert!(grid.tracks.iter().all(|t| t.steps.len() <= MAX_STEPS));

        // Room for every track up front, so inserting one never reallocates.
        let mut tracks = Vec::with_capacity(MAX_TRACKS);
        tracks.extend(
            grid.tracks
                .iter()
                .zip(instruments)
                .map(|(track, instrument)| {
                    TrackPlayer::new(track.clone(), instrument, sample_rate)
                }),
        );
        let any_solo = grid.tracks.iter().any(|t| t.mix.solo);
        for t in &mut tracks {
            t.strip.reset(&t.track.mix, any_solo);
        }
        let (command_tx, command_rx) = RingBuffer::new(COMMAND_QUEUE);
        let (garbage_tx, garbage_rx) = RingBuffer::new(COMMAND_QUEUE);
        let status = Arc::new(Status::new());

        let engine = Self {
            clock: Clock::new(f64::from(sample_rate), bpm, grid.steps_per_beat),
            playing: false,
            steps_per_beat: u64::from(grid.steps_per_beat.max(1)),
            step_index: 0,
            tracks,
            send_effects: effects(),
            master_effects: effects(),
            send_left: vec![0.0; MAX_BLOCK],
            send_right: vec![0.0; MAX_BLOCK],
            send_gain: Smoothed::new(1.0, sample_rate),
            master_gain: Smoothed::new(1.0, sample_rate),
            click: Click::new(sample_rate),
            click_events: Vec::with_capacity(MAX_EVENTS),
            click_left: vec![0.0; MAX_BLOCK],
            click_right: vec![0.0; MAX_BLOCK],
            click_enabled: true,
            preview_samples: (PREVIEW_SECONDS * sample_rate) as usize,
            commands: command_rx,
            garbage: garbage_tx,
            status: Arc::clone(&status),
        };
        let handle = Handle {
            commands: command_tx,
            garbage: garbage_rx,
            status,
        };
        (engine, handle)
    }

    /// Starts playback from the top.
    pub fn play(&mut self) {
        self.stop();
        self.playing = true;
        self.status.playing.store(true, Ordering::Relaxed);
        self.clock.restart();
    }

    /// Stops playback, releasing every sequenced note.
    pub fn stop(&mut self) {
        for t in &mut self.tracks {
            for h in t.held.drain(..) {
                push(&mut t.events, note_off(0, h.pitch));
            }
            t.playhead = 0;
            t.pass = 0;
            t.position = u64::MAX;
        }
        self.step_index = 0;
        self.playing = false;
        self.status.set_stopped();
    }

    /// Renders the next stretch of stereo audio. Real-time safe: no allocation, no locks.
    ///
    /// Steps become events stamped with their exact sample offset, so timing is sample-accurate
    /// whatever the buffer size.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        denormal::flush_to_zero();
        self.run_commands();
        for (l, r) in left.chunks_mut(MAX_BLOCK).zip(right.chunks_mut(MAX_BLOCK)) {
            self.process_block(l, r);
        }
    }

    fn run_commands(&mut self) {
        while let Ok(command) = self.commands.pop() {
            match command {
                Command::Play => self.play(),
                Command::Stop => self.stop(),
                Command::SetBpm(bpm) => self.clock.set_bpm(bpm),
                Command::SetClick(on) => self.click_enabled = on,
                Command::SetTrack { index, mut track } => {
                    if let Some(t) = self.tracks.get_mut(index)
                        && track.steps.len() <= MAX_STEPS
                    {
                        std::mem::swap(&mut t.track, &mut track);
                        if t.playhead >= t.track.steps.len() {
                            t.playhead = 0;
                        }
                        // Re-realise so the edit is heard on this pass, not the next.
                        realise_into(&t.track, t.pass, &mut t.steps);
                    }
                    self.discard(Garbage::Track(track));
                }
                Command::InsertTrack { index, player } => self.insert_track(index, player),
                Command::RemoveTrack(index) => {
                    if index < self.tracks.len() {
                        let player = self.tracks.remove(index);
                        self.discard(Garbage::Player(player));
                        self.publish_positions(index);
                    }
                }
                Command::MoveTrack { from, to } => {
                    if from < self.tracks.len() && to < self.tracks.len() {
                        // Within capacity, so neither reallocates.
                        let player = self.tracks.remove(from);
                        self.tracks.insert(to, player);
                        self.publish_positions(from.min(to));
                    }
                }
                Command::Preview {
                    track,
                    pitch,
                    velocity,
                } => {
                    if let Some(t) = self.tracks.get_mut(track) {
                        if let Some((old, _)) = t.preview.take() {
                            push(&mut t.events, note_off(0, old));
                        }
                        push(&mut t.events, note_on(0, pitch, velocity));
                        t.preview = Some((pitch, self.preview_samples));
                    }
                }
                Command::InsertEffect {
                    chain,
                    index,
                    effect,
                } => match self.chain_mut(chain) {
                    Some(effects) if effects.len() < MAX_EFFECTS => {
                        // Within capacity, so it never reallocates.
                        effects.insert(index.min(effects.len()), effect);
                    }
                    _ => self.discard(Garbage::Effect(effect)),
                },
                Command::RemoveEffect { chain, index } => {
                    if let Some(effects) = self.chain_mut(chain)
                        && index < effects.len()
                    {
                        let effect = effects.remove(index);
                        self.discard(Garbage::Effect(effect));
                    }
                }
                Command::SetInstrument { index, instrument } => {
                    let old = match self.tracks.get_mut(index) {
                        Some(t) => {
                            // Notes held on the old instrument end with it.
                            t.held.clear();
                            t.preview = None;
                            std::mem::replace(&mut t.instrument, instrument)
                        }
                        None => instrument,
                    };
                    self.discard(Garbage::Instrument(old));
                }
                Command::SetBusLevel { bus, level } => match bus {
                    Chain::Send => self.send_gain.set(db_to_gain(level)),
                    Chain::Master => self.master_gain.set(db_to_gain(level)),
                    Chain::Track(_) => {}
                },
                Command::SetParam { target, id, value } => {
                    let events = match target {
                        Target::Instrument(track) => {
                            self.tracks.get_mut(track).map(|t| &mut t.events)
                        }
                        Target::Effect { chain, slot } => self
                            .chain_mut(chain)
                            .and_then(|effects| effects.get_mut(slot))
                            .map(|slot| &mut slot.events),
                    };
                    if let Some(events) = events {
                        push(
                            events,
                            Event {
                                offset: 0,
                                kind: EventKind::Param { id, value },
                            },
                        );
                    }
                }
            }
        }
    }

    fn chain_mut(&mut self, chain: Chain) -> Option<&mut Effects> {
        match chain {
            Chain::Track(index) => self.tracks.get_mut(index).map(|t| &mut t.effects),
            Chain::Send => Some(&mut self.send_effects),
            Chain::Master => Some(&mut self.master_effects),
        }
    }

    fn insert_track(&mut self, index: usize, mut player: Box<TrackPlayer>) {
        if self.tracks.len() == MAX_TRACKS || player.track.steps.len() > MAX_STEPS {
            self.discard(Garbage::Player(player));
            return;
        }
        let any_solo = self.tracks.iter().any(|t| t.track.mix.solo);
        player.strip.reset(&player.track.mix, any_solo);
        // Join in step, as though the track had been playing from the start.
        let len = player.track.steps.len() as u64;
        if self.playing && len > 0 {
            player.playhead = (self.step_index % len) as usize;
            player.pass = (self.step_index / len) as u32;
        }
        realise_into(&player.track, player.pass, &mut player.steps);
        let index = index.min(self.tracks.len());
        self.tracks.insert(index, player);
        self.publish_positions(index);
    }

    /// Publishes every position from track `from` on, after tracks have moved between indices.
    fn publish_positions(&self, from: usize) {
        for index in from..MAX_TRACKS {
            let packed = self.tracks.get(index).map_or(u64::MAX, |t| t.position);
            self.status.set_position(index, packed);
        }
    }

    /// Sends something back to the UI thread to be freed.
    fn discard(&mut self, garbage: Garbage) {
        if let Err(PushError::Full(garbage)) = self.garbage.push(garbage) {
            // Never free on the audio thread; leaking is the lesser evil.
            std::mem::forget(garbage);
        }
    }

    fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len();

        // Sequence: turn every step starting in this block into events.
        if self.playing {
            let mut pos = 0;
            loop {
                let until = self.clock.frames_until_step() as usize;
                if until == 0 {
                    self.clock.take_step();
                    self.sequence_step(pos as u32);
                    continue;
                }
                let frames = until.min(len - pos);
                self.clock.skip(frames as u64);
                pos += frames;
                if pos == len {
                    break;
                }
            }
        }

        for t in &mut self.tracks {
            if let Some((pitch, remaining)) = t.preview {
                if remaining < len {
                    push(&mut t.events, note_off(remaining as u32, pitch));
                    t.preview = None;
                } else {
                    t.preview = Some((pitch, remaining - len));
                }
            }
        }

        // Render and mix.
        left.fill(0.0);
        right.fill(0.0);
        let (sl, sr) = (&mut self.send_left[..len], &mut self.send_right[..len]);
        sl.fill(0.0);
        sr.fill(0.0);
        let any_solo = self.tracks.iter().any(|t| t.track.mix.solo);
        for (index, t) in self.tracks.iter_mut().enumerate() {
            sort_by_offset(&mut t.events);
            let (tl, tr) = (&mut t.left[..len], &mut t.right[..len]);
            tl.fill(0.0);
            tr.fill(0.0);
            t.instrument.process(&t.events, tl, tr);
            t.events.clear();
            run_chain(&mut t.effects, tl, tr);
            t.strip.set(&t.track.mix, any_solo);
            let peak = t.strip.apply([tl, tr], [left, right], [sl, sr]);
            self.status.add_peak(index, peak);
        }
        run_chain(&mut self.send_effects, sl, sr);
        let peak = apply_gain(&mut self.send_gain, sl, sr);
        self.status.bus_peaks[0].fetch_max(peak.to_bits(), Ordering::Relaxed);
        mix_into(left, sl);
        mix_into(right, sr);
        run_chain(&mut self.master_effects, left, right);
        let peak = apply_gain(&mut self.master_gain, left, right);
        self.status.bus_peaks[1].fetch_max(peak.to_bits(), Ordering::Relaxed);
        let (cl, cr) = (&mut self.click_left[..len], &mut self.click_right[..len]);
        self.click.process(&self.click_events, cl, cr);
        self.click_events.clear();
        if self.click_enabled {
            mix_into(left, cl);
            mix_into(right, cr);
        }
        for sample in left.iter_mut().chain(right.iter_mut()) {
            *sample = sample.tanh();
        }
    }

    fn sequence_step(&mut self, offset: u32) {
        if self.step_index.is_multiple_of(self.steps_per_beat) {
            let beat = self.step_index / self.steps_per_beat;
            let pitch = if beat.is_multiple_of(BEATS_PER_BAR) {
                CLICK_ACCENT_PITCH
            } else {
                CLICK_PITCH
            };
            push(&mut self.click_events, note_on(offset, pitch, 100));
        }
        self.step_index += 1;

        for (index, t) in self.tracks.iter_mut().enumerate() {
            // Note-offs first, so a voice can be reused by this step's note.
            t.held.retain_mut(|h| {
                h.steps_left -= 1;
                if h.steps_left == 0 {
                    push(&mut t.events, note_off(offset, h.pitch));
                }
                h.steps_left > 0
            });
            if t.track.steps.is_empty() {
                continue;
            }

            if t.playhead == 0 {
                realise_into(&t.track, t.pass, &mut t.steps);
            }
            t.position = u64::from(t.pass) << 32 | t.playhead as u64;
            self.status.set_position(index, t.position);
            if let Some(note) = t.steps[t.playhead].note {
                // Retriggering a held pitch, or out of room: end the old note first.
                let existing = t.held.iter().position(|h| h.pitch == note.pitch);
                let full = (t.held.len() == MAX_HELD).then_some(0);
                if let Some(i) = existing.or(full) {
                    push(&mut t.events, note_off(offset, t.held[i].pitch));
                    t.held.remove(i);
                }
                push(&mut t.events, note_on(offset, note.pitch, note.velocity));
                t.held.push(Held {
                    pitch: note.pitch,
                    steps_left: note.length.max(1),
                });
            }

            t.playhead += 1;
            if t.playhead >= t.steps.len() {
                t.playhead = 0;
                t.pass += 1;
            }
        }
    }
}

fn note_on(offset: u32, pitch: u8, velocity: u8) -> Event {
    Event {
        offset,
        kind: EventKind::NoteOn { pitch, velocity },
    }
}

fn note_off(offset: u32, pitch: u8) -> Event {
    Event {
        offset,
        kind: EventKind::NoteOff { pitch },
    }
}

/// Pushes without ever growing the buffer.
fn push(events: &mut Vec<Event>, event: Event) {
    if events.len() < events.capacity() {
        events.push(event);
    }
}

/// Stable insertion sort: keeps note-offs ahead of note-ons at the same offset, and unlike
/// `sort_by_key` it never allocates.
fn sort_by_offset(events: &mut [Event]) {
    for i in 1..events.len() {
        let mut j = i;
        while j > 0 && events[j - 1].offset > events[j].offset {
            events.swap(j - 1, j);
            j -= 1;
        }
    }
}

/// Scales a stereo block by a gliding gain, returning its peak level afterwards.
fn apply_gain(gain: &mut Smoothed, left: &mut [f32], right: &mut [f32]) -> f32 {
    let mut peak = 0.0f32;
    for (l, r) in left.iter_mut().zip(right.iter_mut()) {
        let g = gain.tick();
        *l *= g;
        *r *= g;
        peak = peak.max(l.abs()).max(r.abs());
    }
    peak
}

fn mix_into(out: &mut [f32], src: &[f32]) {
    for (o, s) in out.iter_mut().zip(src) {
        *o += s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::{AdsrParams, Wave};
    use crate::synth::{Patch, Synth};
    use lattice_core::{Condition, Rule, Transform};

    fn click_only(sample_rate: f32, bpm: f64) -> Engine {
        let grid = Grid {
            name: "empty".into(),
            steps_per_beat: 4,
            tracks: vec![],
        };
        let (mut engine, _) = Engine::new(sample_rate, bpm, &grid, vec![]);
        engine.play();
        engine
    }

    fn lead() -> Track {
        Track {
            name: "lead".into(),
            steps: [60, 0, 63, 67, 0, 70, 72]
                .map(|p| {
                    if p == 0 {
                        Step::default()
                    } else {
                        Step::note(p)
                    }
                })
                .to_vec(),
            mix: Default::default(),
            instrument: Default::default(),
            effects: vec![],
            rules: vec![Rule {
                transform: Transform::Drift(1),
                when: Condition::Always,
            }],
        }
    }

    fn with_synth(sample_rate: f32) -> (Engine, Handle) {
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![lead()],
        };
        let patch = Patch {
            wave: Wave::Square,
            amp: AdsrParams {
                attack: 0.003,
                decay: 0.1,
                sustain: 0.3,
                release: 0.05,
            },
            cutoff: 500.0,
            env_amount: 4_000.0,
            resonance: 0.4,
            gain: 0.3,
        };
        let synth: Box<dyn Processor> = Box::new(Synth::new(patch, sample_rate));
        let (mut engine, handle) = Engine::new(sample_rate, 133.0, &grid, vec![synth]);
        engine.play();
        (engine, handle)
    }

    /// Samples where sound starts: non-silent after silence.
    fn onsets(audio: &[f32]) -> Vec<usize> {
        (0..audio.len())
            .filter(|&i| audio[i] != 0.0 && (i == 0 || audio[i - 1] == 0.0))
            .collect()
    }

    fn render(engine: &mut Engine, block: usize, total: usize) -> Vec<f32> {
        let mut left = vec![0.0; total];
        let mut right = vec![0.0; total];
        for (l, r) in left.chunks_mut(block).zip(right.chunks_mut(block)) {
            engine.process(l, r);
        }
        assert_eq!(left, right);
        left
    }

    #[test]
    fn clicks_land_on_exact_samples() {
        // 137 bpm at 44.1 kHz: 19,313.868... samples per beat.
        let audio = render(&mut click_only(44_100.0, 137.0), 441, 100_000);
        let spb = 44_100.0 * 60.0 / 137.0;
        let expected: Vec<usize> = (0..6).map(|k| (k as f64 * spb).ceil() as usize).collect();
        assert_eq!(onsets(&audio), expected);
    }

    #[test]
    fn output_does_not_depend_on_block_size() {
        let reference = render(&mut with_synth(48_000.0).0, 1, 100_000);
        assert!(reference.iter().any(|x| x.abs() > 0.05));
        for block in [7, 64, 333, 4096] {
            assert_eq!(
                render(&mut with_synth(48_000.0).0, block, 100_000),
                reference,
                "block {block}"
            );
        }
    }

    #[test]
    fn status_reports_position_and_stop_silences() {
        let (mut engine, mut handle) = with_synth(48_000.0);
        // 133 bpm, 16ths: about 5,413 samples per step. Ten steps in, one full pass done.
        render(&mut engine, 256, 5_413 * 9 + 100);
        assert!(handle.status().playing());
        assert_eq!(handle.status().position(0), Some((2, 1)));

        handle.send(Command::Stop);
        render(&mut engine, 256, 256);
        assert!(!handle.status().playing());
        assert_eq!(handle.status().position(0), None);
        let tail = render(&mut engine, 256, 48_000);
        assert!(tail[24_000..].iter().all(|&x| x == 0.0));
    }

    #[test]
    fn set_track_swaps_the_pattern_and_returns_the_old_one() {
        let (mut engine, mut handle) = with_synth(48_000.0);
        engine.click_enabled = false;
        let mut silent = lead();
        silent.steps = vec![Step::default(); 4];
        handle.send(Command::SetTrack {
            index: 0,
            track: Box::new(silent),
        });
        let audio = render(&mut engine, 256, 48_000);
        assert!(audio.iter().all(|&x| x == 0.0));
        assert!(handle.garbage.pop().is_ok(), "old track sent back");
    }

    fn silent() -> Box<dyn Processor> {
        struct Silent;
        impl Processor for Silent {
            fn process(&mut self, _: &[Event], left: &mut [f32], right: &mut [f32]) {
                left.fill(0.0);
                right.fill(0.0);
            }
        }
        Box::new(Silent)
    }

    #[test]
    fn tracks_are_inserted_and_removed_while_playing() {
        let (mut engine, mut handle) = with_synth(48_000.0);
        // Ten steps in, as above: the seven-step lead is on step 2 of pass 1.
        render(&mut engine, 256, 5_413 * 9 + 100);
        let mut four = lead();
        four.steps.truncate(4);
        handle.send(Command::InsertTrack {
            index: 0,
            player: TrackPlayer::new(four, silent(), 48_000.0),
        });
        render(&mut engine, 1, 1);
        assert_eq!(engine.tracks.capacity(), MAX_TRACKS);
        assert_eq!(engine.tracks[0].track.steps.len(), 4);
        // Not played yet, but already in step: ten steps in, it starts on step 2 of pass 2.
        assert_eq!(handle.status().position(0), None);
        assert_eq!(
            handle.status().position(1),
            Some((2, 1)),
            "the lead's position moved up with it"
        );
        assert_eq!((engine.tracks[0].playhead, engine.tracks[0].pass), (2, 2));
        render(&mut engine, 256, 5_413);
        assert_eq!(handle.status().position(0), Some((2, 2)));
        assert_eq!(handle.status().position(1), Some((3, 1)));

        handle.send(Command::MoveTrack { from: 0, to: 1 });
        render(&mut engine, 1, 1);
        assert_eq!(engine.tracks[1].track.steps.len(), 4);
        assert_eq!(handle.status().position(0), Some((3, 1)));
        assert_eq!(handle.status().position(1), Some((2, 2)));

        handle.send(Command::RemoveTrack(0));
        render(&mut engine, 1, 1);
        assert_eq!(engine.tracks.len(), 1);
        assert_eq!(handle.status().position(0), Some((2, 2)));
        assert_eq!(handle.status().position(1), None, "nothing left at index 1");
        assert!(
            matches!(handle.garbage.pop(), Ok(Garbage::Player(_))),
            "removed track sent back"
        );
        // Removing past the end does nothing.
        handle.send(Command::RemoveTrack(5));
        render(&mut engine, 1, 1);
        assert_eq!(engine.tracks.len(), 1);
    }

    #[test]
    fn inserting_past_the_limit_sends_the_track_back() {
        let (mut engine, mut handle) = with_synth(48_000.0);
        for _ in 0..MAX_TRACKS {
            handle.send(Command::InsertTrack {
                index: usize::MAX,
                player: TrackPlayer::new(lead(), silent(), 48_000.0),
            });
        }
        render(&mut engine, 256, 256);
        assert_eq!(engine.tracks.len(), MAX_TRACKS);
        assert!(matches!(handle.garbage.pop(), Ok(Garbage::Player(_))));
        assert!(handle.garbage.pop().is_err());
    }

    #[test]
    fn preview_plays_while_stopped() {
        let (mut engine, mut handle) = with_synth(48_000.0);
        engine.stop();
        handle.send(Command::Preview {
            track: 0,
            pitch: 64,
            velocity: 100,
        });
        let audio = render(&mut engine, 256, 48_000);
        assert!(audio[..4_800].iter().any(|x| x.abs() > 0.05));
        assert!(audio[40_000..].iter().all(|&x| x == 0.0));
    }

    /// A steady 0.5 on both channels, for checking the mixer.
    fn dc() -> Box<dyn Processor> {
        struct Dc;
        impl Processor for Dc {
            fn process(&mut self, _: &[Event], left: &mut [f32], right: &mut [f32]) {
                left.fill(0.5);
                right.fill(0.5);
            }
        }
        Box::new(Dc)
    }

    /// Tracks of steady 0.5, stopped and without the click, so only the mixer shapes the sound.
    fn mixer(mixes: &[Mix]) -> (Engine, Handle) {
        let tracks: Vec<Track> = mixes.iter().map(|&mix| Track { mix, ..lead() }).collect();
        let grid = Grid {
            name: "mix".into(),
            steps_per_beat: 4,
            tracks,
        };
        let instruments = mixes.iter().map(|_| dc()).collect();
        let (mut engine, handle) = Engine::new(1_000.0, 120.0, &grid, instruments);
        engine.click_enabled = false;
        (engine, handle)
    }

    fn render_stereo(engine: &mut Engine, total: usize) -> (Vec<f32>, Vec<f32>) {
        let mut left = vec![0.0; total];
        let mut right = vec![0.0; total];
        engine.process(&mut left, &mut right);
        (left, right)
    }

    fn set_mix(handle: &mut Handle, index: usize, mix: Mix) {
        handle.send(Command::SetTrack {
            index,
            track: Box::new(Track { mix, ..lead() }),
        });
    }

    #[test]
    fn level_and_pan_shape_each_side() {
        let half_left = Mix {
            level: -6.0,
            pan: -0.5,
            ..Mix::default()
        };
        let (mut engine, _) = mixer(&[half_left]);
        let (left, right) = render_stereo(&mut engine, 10);
        let gain = db_to_gain(-6.0);
        // Balance law: the near side is untouched, the far side follows a quarter cosine.
        assert!((left[0] - (0.5 * gain).tanh()).abs() < 1e-6);
        let far = 0.5 * gain * std::f32::consts::FRAC_PI_4.cos();
        assert!((right[0] - far.tanh()).abs() < 1e-6);
    }

    #[test]
    fn mute_glides_out_without_a_click_and_reports_peaks() {
        let (mut engine, mut handle) = mixer(&[Mix::default()]);
        render_stereo(&mut engine, 10);
        assert_eq!(handle.status().take_peak(0), 0.5);
        assert_eq!(handle.status().take_peak(0), 0.0, "taking resets it");

        set_mix(
            &mut handle,
            0,
            Mix {
                mute: true,
                ..Mix::default()
            },
        );
        // 1 kHz: a 20-sample glide.
        let (left, _) = render_stereo(&mut engine, 30);
        assert!(left.windows(2).all(|w| w[1] <= w[0] && w[0] - w[1] < 0.03));
        assert!(left[0] > 0.4);
        assert_eq!(left[19..], [0.0; 11]);
    }

    #[test]
    fn solo_silences_every_other_track() {
        let soloed = Mix {
            solo: true,
            pan: -1.0,
            ..Mix::default()
        };
        let right_only = Mix {
            pan: 1.0,
            ..Mix::default()
        };
        let (mut engine, mut handle) = mixer(&[soloed, right_only]);
        let (left, right) = render_stereo(&mut engine, 10);
        assert!(left[0] > 0.4);
        assert_eq!(right[0], 0.0, "starts silent rather than gliding out");

        set_mix(&mut handle, 0, Mix::default());
        let (_, right) = render_stereo(&mut engine, 30);
        assert!(right[29] > 0.4, "unsoloed: the other track comes back");
    }

    #[test]
    fn sends_reach_the_send_bus_effects() {
        let sending = Mix {
            send: 0.0,
            ..Mix::default()
        };
        let (mut engine, mut handle) = mixer(&[sending]);
        // A track that sends, through a send bus that just doubles: 0.5 dry plus 1.0 sent.
        struct Double;
        impl Processor for Double {
            fn process(&mut self, _: &[Event], left: &mut [f32], right: &mut [f32]) {
                left.iter_mut()
                    .chain(right.iter_mut())
                    .for_each(|x| *x *= 2.0);
            }
        }
        handle.send(Command::InsertEffect {
            chain: Chain::Send,
            index: 0,
            effect: EffectSlot::new(Box::new(Double)),
        });
        let (left, _) = render_stereo(&mut engine, 10);
        assert!((left[0] - 1.5f32.tanh()).abs() < 1e-6);

        handle.send(Command::RemoveEffect {
            chain: Chain::Send,
            index: 0,
        });
        let (left, _) = render_stereo(&mut engine, 10);
        assert!((left[0] - 1.0f32.tanh()).abs() < 1e-6);
        assert!(matches!(handle.garbage.pop(), Ok(Garbage::Effect(_))));
    }

    #[test]
    fn instruments_are_swapped_and_the_old_one_sent_back() {
        let (mut engine, mut handle) = mixer(&[Mix::default()]);
        let (left, _) = render_stereo(&mut engine, 10);
        assert!(left[9] > 0.4);
        for index in [0, 5] {
            handle.send(Command::SetInstrument {
                index,
                instrument: silent(),
            });
        }
        let (left, _) = render_stereo(&mut engine, 200);
        assert!(left[199].abs() < 1e-3, "the new instrument plays");
        for _ in 0..2 {
            assert!(matches!(handle.garbage.pop(), Ok(Garbage::Instrument(_))));
        }
    }

    #[test]
    fn params_reach_effects_and_full_chains_send_effects_back() {
        let (mut engine, mut handle) = mixer(&[Mix::default()]);
        for _ in 0..=MAX_EFFECTS {
            handle.send(Command::InsertEffect {
                chain: Chain::Master,
                index: 0,
                effect: EffectSlot::new(Box::new(crate::Delay::new(1_000.0))),
            });
        }
        handle.send(Command::InsertEffect {
            chain: Chain::Track(3),
            index: 0,
            effect: EffectSlot::new(silent()),
        });
        render_stereo(&mut engine, 1);
        assert_eq!(engine.master_effects.len(), MAX_EFFECTS);
        assert!(matches!(handle.garbage.pop(), Ok(Garbage::Effect(_))));
        assert!(
            matches!(handle.garbage.pop(), Ok(Garbage::Effect(_))),
            "no track 3"
        );

        let target = Target::Effect {
            chain: Chain::Master,
            slot: 2,
        };
        handle.send(Command::SetParam {
            target,
            id: crate::delay::MIX,
            value: 0.5,
        });
        engine.run_commands();
        assert_eq!(
            engine.master_effects[2].events,
            [Event {
                offset: 0,
                kind: EventKind::Param { id: 3, value: 0.5 }
            }]
        );
        render_stereo(&mut engine, 1);
        assert!(engine.master_effects[2].events.is_empty(), "delivered");
    }

    #[test]
    fn bus_levels_scale_the_buses_and_report_peaks() {
        let sending = Mix {
            send: 0.0,
            ..Mix::default()
        };
        let (mut engine, mut handle) = mixer(&[sending]);
        handle.send(Command::SetBusLevel {
            bus: Chain::Send,
            level: lattice_core::MIN_DB,
        });
        handle.send(Command::SetBusLevel {
            bus: Chain::Master,
            level: -6.0,
        });
        render_stereo(&mut engine, 30);
        // Past the glide: start the meters afresh.
        handle.status().take_bus_peak(Chain::Send);
        handle.status().take_bus_peak(Chain::Master);
        let (left, _) = render_stereo(&mut engine, 10);
        // The send is silenced, leaving the track's 0.5, then the master halves it.
        let master = 0.5 * db_to_gain(-6.0);
        assert!((left[0] - master.tanh()).abs() < 1e-6);
        assert_eq!(handle.status().take_bus_peak(Chain::Send), 0.0);
        assert!((handle.status().take_bus_peak(Chain::Master) - master).abs() < 1e-6);
    }

    #[test]
    fn sort_is_stable() {
        let mut events = [note_on(5, 60, 1), note_off(5, 60), note_off(2, 61)];
        sort_by_offset(&mut events);
        assert_eq!(
            events,
            [note_off(2, 61), note_on(5, 60, 1), note_off(5, 60)]
        );
    }
}
