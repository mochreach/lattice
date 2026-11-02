//! The sequencer and mixer, driven entirely from the audio callback.
//!
//! Signal path: each track's instrument, then its effects, then its mixer strip (level, pan,
//! mute and solo) into its group's bus or the master bus, with a post-fader send into the send
//! bus. Each group bus runs its effects and level into the master, as does the send bus; the
//! master runs its effects, then the click is added.
//!
//! The UI thread talks to the engine through a [`Handle`]: commands go over a lock-free queue,
//! and playback position comes back through atomics in [`Status`]. Nothing on the audio side
//! blocks or allocates. Replaced patterns and removed tracks are sent back to the UI thread to be
//! freed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use lattice_core::{
    Change, Conductor, Gesture, Grid, Lane, Lock, Locks, MACRO, MAX_LOCKS, MAX_PARAMS, MAX_STEPS,
    Mix, ParamInfo, SongGesture, Step, TICKS, Track, db_to_gain, realise_into,
};
pub use lattice_core::{MAX_EFFECTS, MAX_GROUPS, MAX_TRACKS};
use rtrb::{Consumer, Producer, PushError, RingBuffer};

use crate::click::Click;
use crate::clock::Clock;
use crate::denormal;
use crate::looper::{Looper, Take};
use crate::param::Smoothed;
use crate::processor::{Event, EventKind, Processor, frames};
use crate::routing::Routing;
use crate::sampler::Sample;
use lattice_core::macros;

const BEATS_PER_BAR: u64 = 4;
/// Largest block rendered in one go. Longer host buffers are split.
pub(crate) const MAX_BLOCK: usize = 256;
/// Events per track per block; more are dropped rather than allocating.
const MAX_EVENTS: usize = 128;
/// Notes a track can hold at once before the oldest is cut.
const MAX_HELD: usize = 16;
/// Events a track's gestures can have waiting for later in its step; more are dropped.
const MAX_DUE: usize = 64;
/// An arpeggio's notes in each step.
const ARPEGGIO: u64 = 6;
const COMMAND_QUEUE: usize = 256;
/// Most input waiting to be read, in frames, before the oldest is dropped: it builds up if the
/// input's clock runs faster than the output's, and adds to the latency.
const MAX_BACKLOG: usize = 8192;

const PREVIEW_SECONDS: f32 = 0.3;
/// The longest a held preview lasts, in case its release never comes.
const HELD_PREVIEW_SECONDS: f32 = 15.0;
/// Previews a track can sound at once before the oldest is cut.
const MAX_PREVIEWS: usize = 8;
const CLICK_PITCH: u8 = 84;
const CLICK_ACCENT_PITCH: u8 = 96;

/// Messages from the UI thread to the audio thread.
#[derive(Debug)]
pub enum Command {
    /// Starts from the top, after this many bars of clicks (heard even with the click off).
    Play {
        count_in: u32,
    },
    Stop,
    SetBpm(f64),
    SetClick(bool),
    /// Replace a track's pattern; heard from the current step on. The old pattern is sent back
    /// to be freed off the audio thread. Tracks longer than `MAX_STEPS` are ignored, and so is a
    /// track sent before a pattern change the sender hadn't seen yet: `swaps` is
    /// [`Status::swaps`] as the sender last saw it.
    SetTrack {
        index: usize,
        track: Box<Track>,
        swaps: u64,
    },
    /// How many steps the playing pattern lasts before the next starts; 0 for for ever.
    SetLength(u64),
    /// The tracks the song mutes, a bit each: `now`, and `next`, from when the playing pattern
    /// ends.
    SetMutes {
        now: u32,
        next: u32,
    },
    /// What happens when the playing pattern ends. Kept while stopped, for the next play.
    Cue(Cue),
    /// Replace the playing pattern's conductor; heard from the current step on. The old one is
    /// sent back to be freed. Ignored if it's longer than `MAX_STEPS`, or sent before a pattern
    /// change the sender hadn't seen, as [`Command::SetTrack`] is.
    SetConductor {
        conductor: Box<Conductor>,
        swaps: u64,
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
    /// Audition a note on a track's instrument: briefly, or `held` until [`Command::Release`]
    /// (or a long while, if that never comes). A track can audition several notes at once.
    Preview {
        track: usize,
        pitch: u8,
        velocity: u8,
        held: bool,
    },
    /// Audition a note held and repeated: on every step that's a multiple of `every`, in time
    /// with the song (or, stopped, with the tempo), until [`Command::Release`]. `now` plays it
    /// at once as well.
    Repeat {
        track: usize,
        pitch: u8,
        velocity: u8,
        every: u32,
        now: bool,
    },
    /// The player's muted or soloed the track at this index themselves: their mute and solo
    /// take over from the conductor's again.
    Unconduct(usize),
    /// End a previewed or repeated note.
    Release {
        track: usize,
        pitch: u8,
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
    /// Route a chain's effects through these lanes and sends, or none for one after another.
    /// Build it off the audio thread with [`Routing::new`]; the old one goes back to be freed.
    SetRouting {
        chain: Chain,
        routing: Option<Box<Routing>>,
    },
    /// Set an instrument's or effect's parameter, in its own units (see
    /// [`Processor::params`]).
    SetParam {
        target: Target,
        id: u32,
        value: f32,
    },
    /// Lock a track's setting now, as though a step had: it holds until the track's next note
    /// or lock. Ignored when stopped. For hearing a lock as it's recorded.
    Lock {
        track: usize,
        lock: Lock,
    },
    /// Turn a track's macro, from 0 to 1. The settings it turns follow.
    SetMacro {
        track: usize,
        index: usize,
        value: f32,
    },
    /// Set a bus's level, in dB. Ignored for a track's chain.
    SetBusLevel {
        bus: Chain,
        level: f32,
    },
    /// Route the track at `index` into a group's bus (below [`MAX_GROUPS`]), or straight to the
    /// master. The route moves with the track.
    SetGroup {
        index: usize,
        group: Option<usize>,
    },
    /// Read the audio input from here: interleaved stereo frames at the engine's rate. The old
    /// input goes back to be freed.
    SetInput(Consumer<f32>),
    /// Record a pass of the input into a take: see [`crate::looper`]. Two can wait at once;
    /// more are sent straight back, unfinished, through [`Handle::recorded`], as all takes are.
    Record(Box<Take>),
    /// Stop recording once the pass recording ends, sending back the takes waiting to start.
    StopRecording,
    /// Frames to add to the latency the host measures, or take off it.
    SetLatencyOffset(i64),
    /// Play this sound file on the instrument at `index`, if it plays one, without building it
    /// again: what's playing carries on, from the new file.
    SetSample {
        index: usize,
        sample: Option<Arc<Sample>>,
    },
}

/// A chain of effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chain {
    /// After a track's instrument, before its mixer strip.
    Track(usize),
    /// A group's bus, fed by its tracks, below [`MAX_GROUPS`].
    Group(usize),
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
    /// Each parameter's setting, to go back to when a step's lock ends.
    base: [f32; MAX_PARAMS],
}

impl EffectSlot {
    /// Allocates, so call it off the audio thread, then send it with
    /// [`Command::InsertEffect`].
    pub fn new(processor: Box<dyn Processor>) -> Box<Self> {
        Box::new(Self {
            base: defaults(processor.params()),
            processor,
            events: Vec::with_capacity(MAX_EVENTS),
        })
    }

    /// Like [`EffectSlot::new`], for a processor built with these settings rather than its
    /// defaults.
    pub fn with_values(processor: Box<dyn Processor>, values: &[f32]) -> Box<Self> {
        let mut slot = Self::new(processor);
        for (base, &value) in slot.base.iter_mut().zip(values) {
            *base = value;
        }
        slot
    }
}

/// Each parameter's default, in a fixed array.
fn defaults(params: &[ParamInfo]) -> [f32; MAX_PARAMS] {
    let mut values = [0.0; MAX_PARAMS];
    for (value, param) in values.iter_mut().zip(params) {
        *value = param.default;
    }
    values
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

/// Runs a block through the effects: each in turn, or through the lanes and sends of their
/// routing.
fn run_chain(
    effects: &mut [Box<EffectSlot>],
    routing: &mut Option<Box<Routing>>,
    left: &mut [f32],
    right: &mut [f32],
) {
    let run = |slot: &mut EffectSlot, left: &mut [f32], right: &mut [f32]| {
        sort_by_offset(&mut slot.events);
        slot.processor.process(&slot.events, left, right);
        slot.events.clear();
    };
    match routing {
        None => effects.iter_mut().for_each(|slot| run(slot, left, right)),
        Some(routing) => {
            routing.run(left, right, |i, l, r| {
                if let Some(slot) = effects.get_mut(i) {
                    run(slot, l, r);
                }
            });
            // Any left over while the routing catches up with the effects.
            effects.iter_mut().for_each(|slot| slot.events.clear());
        }
    }
}

/// A group's bus: its tracks mix into it, then it runs its effects and level into the master.
struct GroupBus {
    effects: Effects,
    routing: Option<Box<Routing>>,
    /// Linear.
    gain: Smoothed,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl GroupBus {
    fn new(sample_rate: f32) -> Self {
        Self {
            effects: effects(),
            routing: None,
            gain: Smoothed::new(1.0, sample_rate),
            left: vec![0.0; MAX_BLOCK],
            right: vec![0.0; MAX_BLOCK],
        }
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
        let frames = (tl.iter().zip(tr))
            .zip(ml.iter_mut().zip(mr.iter_mut()))
            .zip(sl.iter_mut().zip(sr.iter_mut()));
        for (((&tl, &tr), (ml, mr)), (sl, sr)) in frames {
            if !settled {
                gains = self.gains();
            }
            let [gl, gr, send] = gains;
            let (l, r) = (tl * gl, tr * gr);
            *ml += l;
            *mr += r;
            *sl += l * send;
            *sr += r * send;
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

/// What happens when the playing pattern ends. Whichever it is, every track then starts again
/// from its first step and first pass.
#[derive(Debug, Default)]
pub enum Cue {
    /// It plays again.
    #[default]
    Loop,
    /// Another pattern takes over.
    Pattern(Box<Section>),
    /// Playback stops.
    Stop,
}

/// A pattern to play: a lane for each track, its conductor, and how many steps it lasts.
#[derive(Debug)]
pub struct Section {
    pub lanes: Vec<Lane>,
    pub conductor: Conductor,
    pub steps: u64,
    /// Whether it takes over at the next bar, rather than when the playing pattern ends.
    pub at_bar: bool,
}

/// Playback state the audio thread publishes for the UI.
pub struct Status {
    playing: AtomicBool,
    /// Steps of the count-in still to come, counting the one sounding; 0 once the song starts.
    count_in: AtomicU32,
    /// Per track index: `pass << 32 | step`, or `u64::MAX` when stopped or not yet played. One
    /// atomic, so the step and pass are always read together.
    positions: [AtomicU64; MAX_TRACKS],
    /// Per track index: the loudest sample since the UI last looked, as `f32` bits. Levels are
    /// never negative, so comparing the bits compares the levels.
    peaks: [AtomicU32; MAX_TRACKS],
    /// The same for the send and master buses, after their levels.
    bus_peaks: [AtomicU32; 2],
    group_peaks: [AtomicU32; MAX_GROUPS],
    /// Per track index: the last note it started, packed by [`pack_note`], 0 for none yet.
    notes: [AtomicU32; MAX_TRACKS],
    /// Patterns started since play began, after the first: looped or changed.
    boundaries: AtomicU64,
    /// How many of those changed pattern.
    swaps: AtomicU64,
    /// The step playing, counted from the top of the pattern.
    pattern_step: AtomicU64,
    /// Per track index: the locks in force, each packed by [`pack_lock`], 0 for none.
    locks: [[AtomicU64; MAX_LOCKS]; MAX_TRACKS],
    /// Whether a take is recording the input.
    recording: AtomicBool,
    /// Things leaked on the audio thread because the garbage queue was full.
    leaked: AtomicU64,
    /// The tempo playing, as `f64` bits: the song's, or the conductor's.
    tempo: AtomicU64,
    /// The conductor's step playing, or `u64::MAX` when there's none.
    conductor_step: AtomicU64,
}

/// What the conductor did that the UI thread should know of, as it does it itself when the user
/// asks: see [`Handle::conducted`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conducted {
    /// A `P` gesture: cue this pattern, from 0, to take over at the next bar. The engine can't,
    /// as it doesn't hold the patterns.
    Pattern(usize),
    /// An `M` gesture muted the track at this index, or unmuted it, until the song stops.
    Mute { track: usize, muted: bool },
    /// An `S` gesture soloed it, or unsoloed it, until the song stops.
    Solo { track: usize, soloed: bool },
}

/// A lock in one atomic: a present bit, the effect (255 for the instrument), the setting and the
/// value's bits.
fn pack_lock(lock: &Lock) -> u64 {
    1 << 48
        | u64::from(lock.fx.unwrap_or(u8::MAX)) << 40
        | u64::from(lock.param) << 32
        | u64::from(lock.value.to_bits())
}

/// A note in one atomic: a present bit, how many notes the track's started, the velocity and the
/// pitch.
fn pack_note(count: u32, pitch: u8, velocity: u8) -> u32 {
    1 << 31 | (count & 0x7fff) << 16 | u32::from(velocity) << 8 | u32::from(pitch)
}

fn unpack_lock(packed: u64) -> Option<Lock> {
    (packed >> 48 == 1).then(|| Lock {
        fx: Some((packed >> 40) as u8).filter(|&fx| fx != u8::MAX),
        param: (packed >> 32) as u8,
        value: f32::from_bits(packed as u32),
        ..Default::default()
    })
}

impl Status {
    /// How many things the audio thread has leaked rather than free.
    pub fn leaked(&self) -> u64 {
        self.leaked.load(Ordering::Relaxed)
    }

    fn new() -> Self {
        Self {
            boundaries: AtomicU64::new(0),
            swaps: AtomicU64::new(0),
            pattern_step: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            count_in: AtomicU32::new(0),
            positions: std::array::from_fn(|_| AtomicU64::new(u64::MAX)),
            peaks: std::array::from_fn(|_| AtomicU32::new(0)),
            bus_peaks: std::array::from_fn(|_| AtomicU32::new(0)),
            group_peaks: std::array::from_fn(|_| AtomicU32::new(0)),
            notes: std::array::from_fn(|_| AtomicU32::new(0)),
            locks: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU64::new(0))),
            recording: AtomicBool::new(false),
            leaked: AtomicU64::new(0),
            tempo: AtomicU64::new(0),
            conductor_step: AtomicU64::new(u64::MAX),
        }
    }

    /// The tempo playing, in BPM: the song's, or the one its conductor set.
    pub fn tempo(&self) -> f64 {
        f64::from_bits(self.tempo.load(Ordering::Relaxed))
    }

    /// The conductor's step playing, if it has steps and the song's playing.
    pub fn conductor_step(&self) -> Option<usize> {
        match self.conductor_step.load(Ordering::Relaxed) {
            u64::MAX => None,
            step => Some(step as usize),
        }
    }

    /// Whether a take is recording the input now, rather than waiting for its pass to start.
    pub fn recording(&self) -> bool {
        self.recording.load(Ordering::Relaxed)
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    /// Steps of the count-in still to come, counting the one sounding: 0 when it's over, or
    /// there wasn't one.
    pub fn count_in(&self) -> u32 {
        self.count_in.load(Ordering::Relaxed)
    }

    /// How many times a pattern has ended and another (or the same again) started, since play
    /// began; 0 when stopped.
    pub fn boundaries(&self) -> u64 {
        self.boundaries.load(Ordering::Relaxed)
    }

    /// How many of [`Status::boundaries`] changed to a cued pattern.
    pub fn swaps(&self) -> u64 {
        self.swaps.load(Ordering::Relaxed)
    }

    /// The step playing, counted from the top of the pattern (not wrapping round with a track,
    /// as [`Status::position`] does); 0 when stopped.
    pub fn pattern_step(&self) -> u64 {
        self.pattern_step.load(Ordering::Relaxed)
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

    /// The loudest sample a bus has played since the last call.
    pub fn take_bus_peak(&self, bus: Chain) -> f32 {
        let peak = match bus {
            Chain::Send => &self.bus_peaks[0],
            Chain::Master => &self.bus_peaks[1],
            Chain::Group(g) => match self.group_peaks.get(g) {
                Some(peak) => peak,
                None => return 0.0,
            },
            Chain::Track(track) => return self.take_peak(track),
        };
        f32::from_bits(peak.swap(0, Ordering::Relaxed))
    }

    /// The locks `track` is playing with: none when it's stopped, or between locked steps.
    pub fn locks(&self, track: usize) -> Locks {
        let mut locks = Locks::default();
        for packed in self.locks.get(track).into_iter().flatten() {
            if let Some(lock) = unpack_lock(packed.load(Ordering::Relaxed)) {
                locks.set(lock);
            }
        }
        locks
    }

    fn set_locks(&self, track: usize, locks: &Locks) {
        let mut packed = locks.iter().map(pack_lock);
        for slot in self.locks.get(track).into_iter().flatten() {
            slot.store(packed.next().unwrap_or(0), Ordering::Relaxed);
        }
    }

    /// The last note `track` started: its pitch, its velocity, and how many it's started, which
    /// changes with each new one.
    pub fn last_note(&self, track: usize) -> Option<(u8, u8, u16)> {
        let packed = self.notes.get(track)?.load(Ordering::Relaxed);
        (packed >> 31 == 1).then_some((
            packed as u8,
            (packed >> 8) as u8,
            ((packed >> 16) & 0x7fff) as u16,
        ))
    }

    fn started_note(&self, track: usize, pitch: u8, velocity: u8) {
        let Some(slot) = self.notes.get(track) else {
            return;
        };
        let count = (slot.load(Ordering::Relaxed) >> 16) + 1;
        slot.store(pack_note(count, pitch, velocity), Ordering::Relaxed);
    }

    fn add_peak(&self, track: usize, peak: f32) {
        if let Some(slot) = self.peaks.get(track) {
            slot.fetch_max(peak.to_bits(), Ordering::Relaxed);
        }
    }

    fn set_position(&self, track: usize, packed: u64) {
        if let Some(slot) = self.positions.get(track) {
            slot.store(packed, Ordering::Relaxed);
        }
    }

    fn set_stopped(&self) {
        self.playing.store(false, Ordering::Relaxed);
        self.count_in.store(0, Ordering::Relaxed);
        self.boundaries.store(0, Ordering::Relaxed);
        self.swaps.store(0, Ordering::Relaxed);
        self.pattern_step.store(0, Ordering::Relaxed);
        self.conductor_step.store(u64::MAX, Ordering::Relaxed);
        for p in &self.positions {
            p.store(u64::MAX, Ordering::Relaxed);
        }
        for slot in self.locks.iter().flatten() {
            slot.store(0, Ordering::Relaxed);
        }
        self.recording.store(false, Ordering::Relaxed);
    }
}

/// What the audio thread has finished with, sent back to be freed on the UI thread. Only ever
/// dropped, never read.
#[allow(dead_code)]
enum Garbage {
    Track(Box<Track>),
    Player(Box<TrackPlayer>),
    Effect(Box<EffectSlot>),
    Routing(Box<Routing>),
    Instrument(Box<dyn Processor>),
    Section(Box<Section>),
    Conductor(Box<Conductor>),
    Take(Box<Take>),
    Input(Consumer<f32>),
    Sample(Option<Arc<Sample>>),
}

/// The UI thread's end of the engine.
pub struct Handle {
    commands: Producer<Command>,
    garbage: Consumer<Garbage>,
    takes: Consumer<Box<Take>>,
    conducted: Consumer<Conducted>,
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

    /// A take the looper's finished with, recorded or not: see [`Take::complete`].
    pub fn recorded(&mut self) -> Option<Box<Take>> {
        self.takes.pop().ok()
    }

    /// Frees patterns and tracks the audio thread has finished with. Call regularly.
    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
    }

    /// The next thing the conductor did, if it's done anything since this was last called.
    pub fn conducted(&mut self) -> Option<Conducted> {
        self.conducted.pop().ok()
    }
}

#[derive(Debug, Clone, Copy)]
struct Held {
    pitch: u8,
    velocity: u8,
    steps_left: u16,
    /// An arpeggio it goes round each step: its root, and the two steps above it.
    arpeggio: Option<(u8, u8, u8)>,
}

/// Something a track's gestures play later in a step, at a frame counted from the engine's
/// start.
#[derive(Debug, Clone, Copy)]
struct Due {
    at: u64,
    what: Later,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Later {
    /// A delayed note starts, if it's still held.
    Start { pitch: u8, velocity: u8 },
    /// The held note `from` plays again as `to`: a retrigger, or an arpeggio's next note.
    Change { from: u8, to: u8, velocity: u8 },
    /// Every held note ends.
    Cut,
    /// The track's next step, played faster than the song's.
    Step,
}

/// A track as the engine plays it: the pattern, its instrument and the buffers to play it with.
pub struct TrackPlayer {
    track: Track,
    instrument: Box<dyn Processor>,
    effects: Effects,
    routing: Option<Box<Routing>>,
    strip: Strip,
    /// The group bus it mixes into, if any.
    group: Option<usize>,
    /// The track as realised for the current pass.
    steps: Vec<Step>,
    playhead: usize,
    pass: u32,
    events: Vec<Event>,
    held: Vec<Held>,
    /// Previewed notes. Room for `MAX_PREVIEWS`.
    previews: Vec<Preview>,
    left: Vec<f32>,
    right: Vec<f32>,
    /// What was last published to [`Status`], so it can be published again if the track moves.
    position: u64,
    /// The instrument's settings, to go back to when a step's lock ends.
    base: [f32; MAX_PARAMS],
    /// The locks in force: each setting's latest, until its next.
    locked: Locks,
    /// Settings sliding to a later step's lock, one per setting.
    ramps: [Option<Ramp>; MAX_LOCKS],
    /// The conductor's mute and solo (`M2`, `S-2`) over the track's own, while the song plays.
    conducted_mute: Option<bool>,
    conducted_solo: Option<bool>,
    /// The pass `steps` was realised for, if it's been realised since playing began.
    realised: Option<u32>,
    /// What the step's gestures play later in it. Room for `MAX_DUE`.
    due: Vec<Due>,
    /// Where the playhead's gestures have it going: backwards (`B`), so many times as fast
    /// (`*2`) or as slow (`/2`), how many more times it plays this step (`W2`), and how many of
    /// the song's steps it sits out before its next, at a slow speed.
    backwards: bool,
    faster: u8,
    slower: u8,
    waits: Option<u8>,
    rest: u8,
    /// The frame the song's step playing ends on, which a fast track's steps fit within.
    step_ends: u64,
}

/// A setting sliding from one value to the next, along its curve, for a slide lock.
#[derive(Debug, Clone, Copy)]
struct Ramp {
    /// The setting, and the value it's sliding to.
    to: Lock,
    /// Where it started, as a share of the setting's range.
    from: f32,
    /// How far along it is, and how far it goes, in frames.
    done: f64,
    length: f64,
}

impl TrackPlayer {
    /// Allocates, so call it off the audio thread, then send the player with
    /// [`Command::InsertTrack`].
    pub fn new(track: Track, instrument: Box<dyn Processor>, sample_rate: f32) -> Box<Self> {
        let mut base = defaults(instrument.params());
        for (base, &value) in base.iter_mut().zip(&track.instrument.values) {
            *base = value;
        }
        Box::new(Self {
            base,
            locked: Locks::default(),
            ramps: [None; MAX_LOCKS],
            conducted_mute: None,
            conducted_solo: None,
            realised: None,
            due: Vec::with_capacity(MAX_DUE),
            backwards: false,
            faster: 1,
            slower: 1,
            waits: None,
            rest: 0,
            step_ends: 0,
            track,
            instrument,
            effects: effects(),
            routing: None,
            strip: Strip::new(sample_rate),
            group: None,
            steps: Vec::with_capacity(MAX_STEPS),
            playhead: 0,
            pass: 0,
            events: Vec::with_capacity(MAX_EVENTS),
            held: Vec::with_capacity(MAX_HELD),
            previews: Vec::with_capacity(MAX_PREVIEWS),
            left: vec![0.0; MAX_BLOCK],
            right: vec![0.0; MAX_BLOCK],
            position: u64::MAX,
        })
    }
}

impl TrackPlayer {
    /// Back to the top of its steps, going forwards at the song's speed, with nothing due.
    fn rewind(&mut self) {
        self.playhead = 0;
        self.pass = 0;
        self.realised = None;
        self.due.clear();
        self.backwards = false;
        self.faster = 1;
        self.slower = 1;
        self.waits = None;
        self.rest = 0;
    }

    /// Takes the earliest event due before frame `until`, if there is one.
    fn take_due(&mut self, until: u64) -> Option<Due> {
        let (i, _) = (self.due.iter().enumerate())
            .filter(|(_, d)| d.at < until)
            .min_by_key(|(_, d)| d.at)?;
        // In range, as it was just found.
        Some(self.due.remove(i))
    }

    /// Plays a note event that's come due, `offset` frames into the block.
    fn play_later(&mut self, what: Later, offset: u32) {
        match what {
            Later::Start { pitch, velocity } => {
                if self.held.iter().any(|h| h.pitch == pitch) {
                    push(&mut self.events, note_on(offset, pitch, velocity));
                }
            }
            Later::Change { from, to, velocity } => {
                if let Some(h) = self.held.iter_mut().find(|h| h.pitch == from) {
                    push(&mut self.events, note_off(offset, from));
                    push(&mut self.events, note_on(offset, to, velocity));
                    h.pitch = to;
                }
            }
            Later::Cut => {
                for h in self.held.drain(..) {
                    push(&mut self.events, note_off(offset, h.pitch));
                }
            }
            Later::Step => {}
        }
    }

    /// How the track sits in the mix: its own, with the conductor's mute and solo over it.
    fn mix(&self) -> Mix {
        let mut mix = self.track.mix;
        mix.mute = self.conducted_mute.unwrap_or(mix.mute);
        mix.solo = self.conducted_solo.unwrap_or(mix.solo);
        mix
    }

    /// Plays a step's locks at `offset`: each sets its setting, or slides it, from here until
    /// the setting's next lock, or puts it back. The other settings hold.
    fn lock(&mut self, step: Locks, offset: u32) {
        let mut next = self.locked;
        for lock in step.iter() {
            self.ramps.iter_mut().for_each(|r| {
                if r.is_some_and(|r| r.to.same_param(lock)) {
                    *r = None;
                }
            });
            match lock.change {
                Change::Reset => next.retain(|l| !l.same_param(lock)),
                // A track has at most as many columns as a step has room, so there's room.
                Change::Set | Change::Slide => {
                    next.set(Lock {
                        change: Change::Set,
                        ..*lock
                    });
                }
            }
        }
        self.apply_locks(next, offset);
    }

    /// Ends the locks in force that `next` doesn't set again, then sets `next`'s, at `offset`.
    fn apply_locks(&mut self, next: Locks, offset: u32) {
        let ending = self.locked;
        self.locked = next;
        let settings = |l: &&Lock| !l.is_macro();
        for lock in ending.iter().filter(settings) {
            if next.find(lock).is_none() {
                self.settle(lock.fx, lock.param, offset);
            }
        }
        for lock in next.iter().filter(settings) {
            if ending.find(lock).is_none_or(|l| l.value != lock.value) {
                self.param_event(lock.fx, lock.param, lock.value, offset);
            }
        }
        if !macro_locks(&ending).eq(macro_locks(&next)) {
            self.settle_macros(offset);
        }
    }

    /// Ends every lock and slide: the settings go back to their own.
    fn unlock(&mut self) {
        self.ramps = [None; MAX_LOCKS];
        self.apply_locks(Locks::default(), 0);
    }

    /// After step `at`'s locks have played, starts a slide for each of its settings whose next
    /// lock, `steps` on and coming round the track if need be, slides, arriving on that step.
    fn start_ramps(&mut self, at: usize, step: Locks, frames_per_step: f64) {
        let len = self.steps.len();
        for lock in step.iter() {
            let ahead = (1..=len).find_map(|d| {
                let next = self.steps.get((at + d) % len)?.locks;
                next.find(lock).map(|next| (d, *next))
            });
            let Some((steps, to)) = ahead.filter(|(_, to)| to.change == Change::Slide) else {
                continue;
            };
            let (Some(info), Some(now)) = (
                self.info(lock.fx, lock.param),
                self.settled(lock.fx, lock.param),
            ) else {
                continue;
            };
            let ramp = Ramp {
                to,
                from: info.fraction(now),
                done: 0.0,
                length: steps as f64 * frames_per_step,
            };
            if let Some(slot) = self.ramps.iter_mut().find(|r| r.is_none()) {
                *slot = Some(ramp);
            }
        }
    }

    /// Moves each slide on by a block of `len` frames, sending its setting where it's got to.
    fn slide(&mut self, len: usize) {
        for i in 0..self.ramps.len() {
            let Some(Some(mut ramp)) = self.ramps.get(i).copied() else {
                continue;
            };
            let Some(info) = self.info(ramp.to.fx, ramp.to.param) else {
                continue;
            };
            ramp.done += len as f64;
            let along = (ramp.done / ramp.length).min(1.0) as f32;
            let to = info.fraction(ramp.to.value);
            let value = info.at(ramp.from + (to - ramp.from) * along);
            let mut next = self.locked;
            next.set(Lock {
                value,
                change: Change::Set,
                ..ramp.to
            });
            self.locked = next;
            if ramp.to.is_macro() {
                self.settle_macros(0);
            } else {
                self.param_event(ramp.to.fx, ramp.to.param, value, 0);
            }
            if let Some(slot) = self.ramps.get_mut(i) {
                *slot = (along < 1.0).then_some(ramp);
            }
        }
    }

    /// What's known of a setting of the instrument, an effect, or a macro.
    fn info(&self, fx: Option<u8>, id: u8) -> Option<&ParamInfo> {
        match fx {
            Some(MACRO) => Some(&macros::MACRO_INFO),
            None => self.instrument.params().get(usize::from(id)),
            Some(fx) => (self.effects.get(usize::from(fx)))
                .and_then(|slot| slot.processor.params().get(usize::from(id))),
        }
    }

    /// Ends the effects' locks, leaving the instrument's and the macros': before the chain
    /// changes, so the settings go back to the effects they were on.
    fn unlock_effects(&mut self) {
        let mut next = self.locked;
        next.retain(|l| l.fx.is_none() || l.is_macro());
        self.ramps.iter_mut().for_each(|r| {
            if r.is_some_and(|r| r.to.fx.is_some() && !r.to.is_macro()) {
                *r = None;
            }
        });
        self.apply_locks(next, 0);
    }

    /// Macro `index`'s value: a step's lock's, or its own.
    fn macro_value(&self, index: usize) -> f32 {
        (self.locked.iter())
            .find(|l| l.is_macro() && usize::from(l.param) == index)
            .map_or_else(
                || self.track.macros.get(index).map_or(0.0, |m| m.value),
                |l| l.value,
            )
    }

    /// What a setting is now: a step's lock's value, or its own moved by the macros.
    fn settled(&self, fx: Option<u8>, id: u8) -> Option<f32> {
        if let Some(lock) =
            (self.locked.iter()).find(|l| !l.is_macro() && l.fx == fx && l.param == id)
        {
            return Some(lock.value);
        }
        let id = usize::from(id);
        let (base, info) = match fx {
            None => (*self.base.get(id)?, self.instrument.params().get(id)),
            Some(fx) => {
                let slot = self.effects.get(usize::from(fx))?;
                (*slot.base.get(id)?, slot.processor.params().get(id))
            }
        };
        let offset = macros::offset(&self.track.macros, |m| self.macro_value(m), fx, id as u8);
        Some(info.map_or(base, |info| macros::moved(info, base, offset)))
    }

    /// Sends a setting what it is now.
    fn settle(&mut self, fx: Option<u8>, id: u8, offset: u32) {
        if let Some(value) = self.settled(fx, id) {
            self.param_event(fx, id, value, offset);
        }
    }

    /// Sends every setting the macros turn what it is now.
    fn settle_macros(&mut self, offset: u32) {
        // By index, as settling needs the player mutably; each target is copied out first.
        let target = |p: &Self, m: usize, t: usize| {
            let targets = &p.track.macros.get(m)?.targets;
            targets.get(t).copied()
        };
        for m in 0..self.track.macros.len() {
            let mut t = 0;
            while let Some(target) = target(self, m, t) {
                self.settle(target.fx, target.param, offset);
                t += 1;
            }
        }
    }

    fn param_event(&mut self, fx: Option<u8>, id: u8, value: f32, offset: u32) {
        let event = Event {
            offset,
            kind: EventKind::Param {
                id: u32::from(id),
                value,
            },
        };
        match fx {
            None => push(&mut self.events, event),
            Some(fx) => {
                if let Some(slot) = self.effects.get_mut(usize::from(fx)) {
                    push(&mut slot.events, event);
                }
            }
        }
    }

    /// Whether a step's lock holds this setting, so the mixer's change waits until it ends.
    fn is_locked(&self, fx: Option<usize>, id: u32) -> bool {
        (self.locked.iter())
            .any(|l| !l.is_macro() && l.fx.map(usize::from) == fx && u32::from(l.param) == id)
    }
}

/// The locks of macros among `locks`.
fn macro_locks(locks: &Locks) -> impl Iterator<Item = Lock> + '_ {
    locks.iter().filter(|l| l.is_macro()).copied()
}

impl std::fmt::Debug for TrackPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackPlayer")
            .field("track", &self.track.name)
            .finish_non_exhaustive()
    }
}

/// Sends a take back to the UI thread, or if there's no room, to be freed.
fn send_take(takes: &mut Producer<Box<Take>>, garbage: &mut Producer<Garbage>, take: Box<Take>) {
    if let Err(PushError::Full(take)) = takes.push(take)
        && let Err(PushError::Full(take)) = garbage.push(Garbage::Take(take))
    {
        std::mem::forget(take);
    }
}

/// A previewed note on a track.
#[derive(Debug, Clone, Copy)]
struct Preview {
    pitch: u8,
    velocity: u8,
    /// Samples until it's released.
    remaining: usize,
    /// Played again on every step that's a multiple of this; 0 for never.
    every: u32,
}

pub struct Engine {
    clock: Clock,
    playing: bool,
    steps_per_beat: u64,
    step_index: u64,
    /// Count-in steps still to play before step 0.
    count_in: u64,
    /// How long the playing pattern lasts, in steps (0 for for ever), how far into it playback
    /// is, and what happens when it ends.
    pattern_len: u64,
    pattern_step: u64,
    /// Steps since the pattern began, through its loops: the conductor goes round its own
    /// length, which can be longer than the pattern's.
    conductor_at: u64,
    cue: Cue,
    /// The tracks the song mutes now, and once the playing pattern ends, a bit each.
    mutes: u32,
    next_mutes: u32,
    /// Pattern changes since play began, as [`Status::swaps`].
    swaps: u64,
    /// Boxed, so players arrive and leave without being moved out of their boxes, which would
    /// free the boxes here. Room for `MAX_TRACKS`.
    #[allow(clippy::vec_box)]
    tracks: Vec<Box<TrackPlayer>>,
    send_effects: Effects,
    master_effects: Effects,
    send_routing: Option<Box<Routing>>,
    master_routing: Option<Box<Routing>>,
    groups: [GroupBus; MAX_GROUPS],
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
    held_preview_samples: usize,
    commands: Consumer<Command>,
    garbage: Producer<Garbage>,
    status: Arc<Status>,
    /// The audio input, if there is one, and the block of it being played along with.
    input: Option<Consumer<f32>>,
    input_left: Vec<f32>,
    input_right: Vec<f32>,
    /// Frames from a step being played to the input played along with it reaching the engine:
    /// as the host measures it, waiting to be read, and the user's adjustment.
    latency: u64,
    backlog: u64,
    latency_offset: i64,
    /// Frames rendered since the engine started.
    frames: u64,
    looper: Looper,
    takes: Producer<Box<Take>>,
    /// The playing pattern's conductor, and what it's told the UI thread.
    conductor: Conductor,
    conducted: Producer<Conducted>,
    /// The song's tempo, which `T--` goes back to, and the one playing.
    song_bpm: f64,
    bpm: f64,
    /// Whether the tempo playing is the conductor's, rather than the song's.
    conducted_tempo: bool,
    /// Whether an `X` has stopped the song at the end of the step playing.
    stopping: bool,
}

impl Engine {
    /// `instruments[i]` plays `grid.tracks[i]`. Starts stopped. Allocates, so build it off the
    /// audio thread, then move the engine into the audio callback and keep the handle.
    ///
    /// The song keeps to the engine's limits, but rather than panic if it didn't: it plays as
    /// many tracks as there are instruments, up to [`MAX_TRACKS`], and only the first
    /// [`MAX_STEPS`] steps of a longer track, as more wouldn't fit the room kept for them.
    pub fn new(
        sample_rate: f32,
        bpm: f64,
        grid: &Grid,
        instruments: Vec<Box<dyn Processor>>,
    ) -> (Self, Handle) {
        // Room for every track up front, so inserting one never reallocates.
        let mut tracks = Vec::with_capacity(MAX_TRACKS);
        tracks.extend(grid.tracks.iter().zip(instruments).take(MAX_TRACKS).map(
            |(track, instrument)| {
                let mut track = track.clone();
                track.steps.truncate(MAX_STEPS);
                TrackPlayer::new(track, instrument, sample_rate)
            },
        ));
        let any_solo = tracks.iter().any(|t| t.track.mix.solo);
        for t in &mut tracks {
            t.strip.reset(&t.track.mix, any_solo);
        }
        let (command_tx, command_rx) = RingBuffer::new(COMMAND_QUEUE);
        let (garbage_tx, garbage_rx) = RingBuffer::new(COMMAND_QUEUE);
        let (takes_tx, takes_rx) = RingBuffer::new(COMMAND_QUEUE);
        let (conducted_tx, conducted_rx) = RingBuffer::new(COMMAND_QUEUE);
        let status = Arc::new(Status::new());
        status.tempo.store(bpm.to_bits(), Ordering::Relaxed);

        let engine = Self {
            clock: Clock::new(f64::from(sample_rate), bpm, grid.steps_per_beat),
            playing: false,
            steps_per_beat: u64::from(grid.steps_per_beat.max(1)),
            step_index: 0,
            count_in: 0,
            pattern_len: 0,
            mutes: 0,
            next_mutes: 0,
            pattern_step: 0,
            conductor_at: 0,
            cue: Cue::Loop,
            swaps: 0,
            tracks,
            send_effects: effects(),
            master_effects: effects(),
            send_routing: None,
            master_routing: None,
            groups: std::array::from_fn(|_| GroupBus::new(sample_rate)),
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
            held_preview_samples: (HELD_PREVIEW_SECONDS * sample_rate) as usize,
            commands: command_rx,
            garbage: garbage_tx,
            status: Arc::clone(&status),
            input: None,
            input_left: vec![0.0; MAX_BLOCK],
            input_right: vec![0.0; MAX_BLOCK],
            latency: 0,
            backlog: 0,
            latency_offset: 0,
            frames: 0,
            looper: Looper::default(),
            takes: takes_tx,
            conductor: Conductor::default(),
            conducted: conducted_tx,
            song_bpm: bpm,
            bpm,
            conducted_tempo: false,
            stopping: false,
        };
        let handle = Handle {
            commands: command_tx,
            garbage: garbage_rx,
            takes: takes_rx,
            conducted: conducted_rx,
            status,
        };
        (engine, handle)
    }

    /// Starts playback from the top.
    pub fn play(&mut self) {
        self.play_after(0);
    }

    /// Starts playback from the top, after `bars` bars of clicks.
    pub fn play_after(&mut self, bars: u32) {
        self.stop();
        self.playing = true;
        self.count_in = u64::from(bars) * BEATS_PER_BAR * self.steps_per_beat;
        self.status.playing.store(true, Ordering::Relaxed);
        self.clock.restart();
    }

    /// The latency the host measures, in frames: from the engine rendering a frame to its being
    /// heard, plus from the input hearing a frame to its reaching [`Command::SetInput`]'s queue.
    /// Set it before each [`Engine::process`].
    pub fn set_latency(&mut self, frames: u64) {
        self.latency = frames;
    }

    /// Stops playback, releasing every sequenced note.
    pub fn stop(&mut self) {
        let (takes, garbage) = (&mut self.takes, &mut self.garbage);
        self.looper.stop(true, |t| send_take(takes, garbage, t));
        for t in &mut self.tracks {
            for h in t.held.drain(..) {
                push(&mut t.events, note_off(0, h.pitch));
            }
            t.unlock();
            t.rewind();
            t.position = u64::MAX;
            // The conductor's mutes and solos last while the song plays.
            t.conducted_mute = None;
            t.conducted_solo = None;
        }
        self.step_index = 0;
        self.count_in = 0;
        self.pattern_step = 0;
        self.conductor_at = 0;
        self.swaps = 0;
        self.playing = false;
        self.stopping = false;
        // The conductor's tempo lasts while the song plays.
        self.conducted_tempo = false;
        self.set_tempo(self.song_bpm, false);
        self.status.set_stopped();
    }

    /// Plays at `bpm`: from the step just taken if `now`, as a step's tempo is, or else from the
    /// next.
    fn set_tempo(&mut self, bpm: f64, now: bool) {
        if bpm != self.bpm {
            match now {
                true => self.clock.retime(bpm),
                false => self.clock.set_bpm(bpm),
            }
        }
        self.bpm = bpm;
        self.status.tempo.store(bpm.to_bits(), Ordering::Relaxed);
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
                Command::Play { count_in } => self.play_after(count_in),
                Command::Stop => self.stop(),
                Command::SetBpm(bpm) => {
                    // Under a tempo from the conductor, the song's waits for `T--`.
                    self.song_bpm = bpm;
                    if !self.conducted_tempo {
                        self.set_tempo(bpm, false);
                    }
                }
                Command::SetClick(on) => self.click_enabled = on,
                Command::SetTrack {
                    index,
                    mut track,
                    swaps,
                } => {
                    if let Some(t) = self.tracks.get_mut(index)
                        && track.steps.len() <= MAX_STEPS
                        && swaps == self.swaps
                    {
                        std::mem::swap(&mut t.track, &mut track);
                        if t.playhead >= t.track.steps.len() {
                            t.playhead = 0;
                        }
                        // Settings the macros turned before, and turn now, follow them.
                        for target in track.macros.iter().flat_map(|m| &m.targets) {
                            t.settle(target.fx, target.param, 0);
                        }
                        t.settle_macros(0);
                        // Re-realise so the edit is heard on this pass, not the next.
                        realise_into(&t.track, t.pass, &mut t.steps);
                        t.realised = Some(t.pass);
                    }
                    self.discard(Garbage::Track(track));
                }
                Command::SetConductor {
                    mut conductor,
                    swaps,
                } => {
                    if conductor.steps.len() <= MAX_STEPS && swaps == self.swaps {
                        std::mem::swap(&mut self.conductor, &mut conductor);
                    }
                    self.discard(Garbage::Conductor(conductor));
                }
                Command::SetLength(steps) => self.pattern_len = steps,
                Command::SetMutes { now, next } => (self.mutes, self.next_mutes) = (now, next),
                Command::Cue(cue) => {
                    if let Cue::Pattern(old) = std::mem::replace(&mut self.cue, cue) {
                        self.discard(Garbage::Section(old));
                    }
                }
                Command::InsertTrack { index, player } => self.insert_track(index, player),
                Command::RemoveTrack(index) => {
                    if index < self.tracks.len() {
                        let player = self.tracks.remove(index);
                        self.discard(Garbage::Player(player));
                        self.publish_positions(index);
                        self.retarget_takes(|t| match t.cmp(&index) {
                            std::cmp::Ordering::Less => Some(t),
                            std::cmp::Ordering::Equal => None,
                            std::cmp::Ordering::Greater => Some(t - 1),
                        });
                    }
                }
                Command::MoveTrack { from, to } => {
                    if from < self.tracks.len() && to < self.tracks.len() {
                        // Within capacity, so neither reallocates.
                        let player = self.tracks.remove(from);
                        self.tracks.insert(to, player);
                        self.publish_positions(from.min(to));
                        self.retarget_takes(|t| {
                            Some(if t == from {
                                to
                            } else if from < t && t <= to {
                                t - 1
                            } else if to <= t && t < from {
                                t + 1
                            } else {
                                t
                            })
                        });
                    }
                }
                Command::Preview {
                    track,
                    pitch,
                    velocity,
                    held,
                } => {
                    let remaining = if held {
                        self.held_preview_samples
                    } else {
                        self.preview_samples
                    };
                    let preview = Preview {
                        pitch,
                        velocity,
                        remaining,
                        every: 0,
                    };
                    self.preview(track, preview, true);
                }
                Command::Repeat {
                    track,
                    pitch,
                    velocity,
                    every,
                    now,
                } => {
                    let preview = Preview {
                        pitch,
                        velocity,
                        remaining: self.held_preview_samples,
                        every: every.max(1),
                    };
                    self.preview(track, preview, now);
                }
                Command::Unconduct(track) => {
                    if let Some(t) = self.tracks.get_mut(track) {
                        t.conducted_mute = None;
                        t.conducted_solo = None;
                    }
                }
                Command::Release { track, pitch } => {
                    if let Some(t) = self.tracks.get_mut(track)
                        && let Some(i) = t.previews.iter().position(|p| p.pitch == pitch)
                    {
                        push(&mut t.events, note_off(0, pitch));
                        t.previews.remove(i);
                    }
                }
                Command::InsertEffect {
                    chain,
                    index,
                    effect,
                } => match self.unlock_effects(chain).chain_mut(chain) {
                    Some(effects) if effects.len() < MAX_EFFECTS => {
                        // Within capacity, so it never reallocates.
                        effects.insert(index.min(effects.len()), effect);
                        self.settle_macros(chain);
                    }
                    _ => self.discard(Garbage::Effect(effect)),
                },
                Command::SetRouting { chain, mut routing } => {
                    let Some(slot) = self.routing_mut(chain) else {
                        if let Some(routing) = routing {
                            self.discard(Garbage::Routing(routing));
                        }
                        continue;
                    };
                    if let (Some(new), Some(old)) = (&mut routing, &*slot) {
                        new.take_over(old);
                    }
                    if let Some(old) = std::mem::replace(slot, routing) {
                        self.discard(Garbage::Routing(old));
                    }
                }
                Command::RemoveEffect { chain, index } => {
                    if let Some(effects) = self.unlock_effects(chain).chain_mut(chain)
                        && index < effects.len()
                    {
                        let effect = effects.remove(index);
                        self.discard(Garbage::Effect(effect));
                        self.settle_macros(chain);
                    }
                }
                Command::SetInstrument { index, instrument } => {
                    let old = match self.tracks.get_mut(index) {
                        Some(t) => {
                            // Notes held on the old instrument end with it.
                            t.held.clear();
                            t.previews.clear();
                            // The new one starts at its own settings, which follow.
                            t.base = defaults(instrument.params());
                            t.locked.retain(|l| l.fx.is_some());
                            let old = std::mem::replace(&mut t.instrument, instrument);
                            t.settle_macros(0);
                            old
                        }
                        None => instrument,
                    };
                    self.discard(Garbage::Instrument(old));
                }
                Command::SetBusLevel { bus, level } => match bus {
                    Chain::Send => self.send_gain.set(db_to_gain(level)),
                    Chain::Master => self.master_gain.set(db_to_gain(level)),
                    Chain::Group(g) => {
                        if let Some(group) = self.groups.get_mut(g) {
                            group.gain.set(db_to_gain(level));
                        }
                    }
                    Chain::Track(_) => {}
                },
                Command::SetGroup { index, group } => {
                    if let Some(t) = self.tracks.get_mut(index) {
                        t.group = group.filter(|&g| g < MAX_GROUPS);
                    }
                }
                Command::SetParam { target, id, value } => self.set_param(target, id, value),
                Command::SetMacro {
                    track,
                    index,
                    value,
                } => {
                    if let Some(t) = self.tracks.get_mut(track)
                        && let Some(m) = t.track.macros.get_mut(index)
                    {
                        m.value = value;
                        t.settle_macros(0);
                    }
                }
                Command::Lock { track, lock } => {
                    if self.playing
                        && let Some(t) = self.tracks.get_mut(track)
                    {
                        let mut step = Locks::default();
                        if step.set(lock) {
                            t.lock(step, 0);
                        }
                    }
                }
                Command::SetInput(input) => {
                    if let Some(old) = self.input.replace(input) {
                        self.discard(Garbage::Input(old));
                    }
                }
                Command::Record(take) => {
                    if let Err(take) = self.looper.arm(take) {
                        send_take(&mut self.takes, &mut self.garbage, take);
                    }
                }
                Command::StopRecording => {
                    let (takes, garbage) = (&mut self.takes, &mut self.garbage);
                    self.looper.stop(false, |t| send_take(takes, garbage, t));
                }
                Command::SetLatencyOffset(frames) => self.latency_offset = frames,
                Command::SetSample { index, mut sample } => {
                    if let Some(t) = self.tracks.get_mut(index) {
                        t.instrument.swap_sample(&mut sample);
                    }
                    // The old one, or the new if nothing took it.
                    self.discard(Garbage::Sample(sample));
                }
            }
        }
    }

    /// Sets a parameter from the mixer. While a step's lock holds it, only the setting to go
    /// back to changes.
    fn set_param(&mut self, target: Target, id: u32, value: f32) {
        let (track, fx) = match target {
            Target::Instrument(track) => (Some(track), None),
            Target::Effect {
                chain: Chain::Track(track),
                slot,
            } => (Some(track), Some(slot)),
            Target::Effect { .. } => (None, None),
        };
        let locked = track
            .and_then(|t| self.tracks.get(t))
            .is_some_and(|t| t.is_locked(fx, id));
        let (events, base) = match target {
            Target::Instrument(track) => match self.tracks.get_mut(track) {
                Some(t) => (&mut t.events, &mut t.base),
                None => return,
            },
            Target::Effect { chain, slot } => {
                match self.chain_mut(chain).and_then(|e| e.get_mut(slot)) {
                    Some(slot) => (&mut slot.events, &mut slot.base),
                    None => return,
                }
            }
        };
        if let Some(base) = base.get_mut(id as usize) {
            *base = value;
        }
        if locked {
            return;
        }
        if track.is_none() {
            push(
                events,
                Event {
                    offset: 0,
                    kind: EventKind::Param { id, value },
                },
            );
        } else if let Some(t) = track.and_then(|t| self.tracks.get_mut(t)) {
            // On a track, the macros move it from there.
            t.settle(fx.map(|f| f as u8), id as u8, 0);
        }
    }

    /// Settles the settings a track's macros turn, after its chain changes.
    fn settle_macros(&mut self, chain: Chain) {
        if let Chain::Track(track) = chain
            && let Some(t) = self.tracks.get_mut(track)
        {
            t.settle_macros(0);
        }
    }

    /// Ends the effect locks on a track's chain before it changes. For chaining.
    fn unlock_effects(&mut self, chain: Chain) -> &mut Self {
        if let Chain::Track(track) = chain
            && let Some(t) = self.tracks.get_mut(track)
        {
            t.unlock_effects();
        }
        self
    }

    fn routing_mut(&mut self, chain: Chain) -> Option<&mut Option<Box<Routing>>> {
        match chain {
            Chain::Track(index) => self.tracks.get_mut(index).map(|t| &mut t.routing),
            Chain::Group(g) => self.groups.get_mut(g).map(|group| &mut group.routing),
            Chain::Send => Some(&mut self.send_routing),
            Chain::Master => Some(&mut self.master_routing),
        }
    }

    fn chain_mut(&mut self, chain: Chain) -> Option<&mut Effects> {
        match chain {
            Chain::Track(index) => self.tracks.get_mut(index).map(|t| &mut t.effects),
            Chain::Group(g) => self.groups.get_mut(g).map(|group| &mut group.effects),
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
        player.realised = self.playing.then_some(player.pass);
        player.settle_macros(0);
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

    /// Starts a preview on a track, sounding it `now` or leaving it to its repeats.
    fn preview(&mut self, track: usize, preview: Preview, now: bool) {
        let Some(t) = self.tracks.get_mut(track) else {
            return;
        };
        // Retriggering a pitch, or out of room: end the old one first.
        let same = t.previews.iter().position(|p| p.pitch == preview.pitch);
        let full = (t.previews.len() == MAX_PREVIEWS).then_some(0);
        if let Some(i) = same.or(full)
            && let Some(old) = t.previews.get(i)
        {
            push(&mut t.events, note_off(0, old.pitch));
            t.previews.remove(i);
        }
        t.previews.push(preview);
        if now {
            push(&mut t.events, note_on(0, preview.pitch, preview.velocity));
            self.status
                .started_note(track, preview.pitch, preview.velocity);
            self.choke_others(track, 0, &[]);
        }
    }

    /// Plays again the repeated previews due on step `step`.
    fn repeat_previews(&mut self, step: u64, offset: u32) {
        for index in 0..self.tracks.len() {
            let Some(t) = self.tracks.get_mut(index) else {
                break;
            };
            let mut hit = false;
            for p in &t.previews {
                if p.every > 0 && step.is_multiple_of(u64::from(p.every)) {
                    push(&mut t.events, note_off(offset, p.pitch));
                    push(&mut t.events, note_on(offset, p.pitch, p.velocity));
                    hit = true;
                }
            }
            if hit {
                self.choke_others(index, offset, &[]);
            }
        }
    }

    fn retarget_takes(&mut self, to: impl Fn(usize) -> Option<usize>) {
        let (takes, garbage) = (&mut self.takes, &mut self.garbage);
        self.looper.retarget(to, |t| send_take(takes, garbage, t));
    }

    /// Frames from a step being heard to what's played along with it being read.
    fn latency(&self) -> u64 {
        (self.latency as i64 + self.backlog as i64 + self.latency_offset).max(0) as u64
    }

    /// Reads a block of the input, or silence where there's none yet.
    fn read_input(&mut self, len: usize) {
        let (left, right) = (
            frames(&mut self.input_left, len),
            frames(&mut self.input_right, len),
        );
        left.fill(0.0);
        right.fill(0.0);
        let Some(input) = &mut self.input else {
            return;
        };
        let mut waiting = input.slots() / 2;
        let excess = waiting.saturating_sub(MAX_BACKLOG + len);
        if excess > 0
            && let Ok(chunk) = input.read_chunk(excess * 2)
        {
            chunk.commit_all();
            waiting -= excess;
        }
        self.backlog = waiting as u64;
        let frames = waiting.min(len);
        if let Ok(chunk) = input.read_chunk(frames * 2) {
            let mut samples = chunk.into_iter();
            for (l, r) in left.iter_mut().zip(right.iter_mut()).take(frames) {
                *l = samples.next().unwrap_or(0.0);
                *r = samples.next().unwrap_or(0.0);
            }
        }
    }

    /// Sends something back to the UI thread to be freed.
    fn discard(&mut self, garbage: Garbage) {
        if let Err(PushError::Full(garbage)) = self.garbage.push(garbage) {
            // Never free on the audio thread; leaking is the lesser evil.
            std::mem::forget(garbage);
            self.status.leaked.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len();
        self.read_input(len);

        // Slides move on a block at a time, from where the last block left them.
        if self.playing {
            for t in &mut self.tracks {
                t.slide(len);
            }
        }

        // Sequence: turn every step starting in this block into events. The clock runs while
        // stopped too, so repeated previews keep time.
        let mut pos = 0;
        loop {
            let until = self.clock.frames_until_step() as usize;
            if until == 0 {
                // What the last step's gestures left due before this one comes first.
                if self.playing {
                    self.play_due(self.frames + pos as u64);
                }
                let step = self.clock.step();
                self.clock.take_step();
                if self.playing {
                    self.sequence_step(pos as u32);
                }
                self.repeat_previews(step, pos as u32);
                continue;
            }
            let frames = until.min(len - pos);
            self.clock.skip(frames as u64);
            pos += frames;
            if pos == len {
                break;
            }
        }
        if self.playing {
            self.play_due(self.frames + len as u64);
        }

        for t in &mut self.tracks {
            let events = &mut t.events;
            t.previews.retain_mut(|p| {
                if p.remaining < len {
                    push(events, note_off(p.remaining as u32, p.pitch));
                    false
                } else {
                    p.remaining -= len;
                    true
                }
            });
        }

        // Render and mix.
        left.fill(0.0);
        right.fill(0.0);
        let (sl, sr) = (
            frames(&mut self.send_left, len),
            frames(&mut self.send_right, len),
        );
        sl.fill(0.0);
        sr.fill(0.0);
        // Which group buses have tracks in them this block.
        let mut fed = [false; MAX_GROUPS];
        for t in &self.tracks {
            if let Some(fed) = t.group.and_then(|g| fed.get_mut(g)) {
                *fed = true;
            }
        }
        for (group, _) in self.groups.iter_mut().zip(fed).filter(|(_, fed)| *fed) {
            frames(&mut group.left, len).fill(0.0);
            frames(&mut group.right, len).fill(0.0);
        }
        let any_solo = self.tracks.iter().any(|t| t.mix().solo);
        for (index, t) in self.tracks.iter_mut().enumerate() {
            let mix = t.mix();
            sort_by_offset(&mut t.events);
            let (tl, tr) = (frames(&mut t.left, len), frames(&mut t.right, len));
            tl.fill(0.0);
            tr.fill(0.0);
            t.instrument.process(&t.events, tl, tr);
            t.events.clear();
            run_chain(&mut t.effects, &mut t.routing, tl, tr);
            t.strip.set(&mix, any_solo);
            // Groups are checked to be in range as they're set, so a track goes to the master
            // only when it's in none.
            let out = match t.group.and_then(|g| self.groups.get_mut(g)) {
                Some(group) => [frames(&mut group.left, len), frames(&mut group.right, len)],
                None => [&mut *left, &mut *right],
            };
            let peak = t.strip.apply([tl, tr], out, [sl, sr]);
            self.status.add_peak(index, peak);
            self.status.set_locks(index, &t.locked);
        }
        let buses = self
            .groups
            .iter_mut()
            .zip(fed)
            .zip(&self.status.group_peaks);
        for ((group, fed), peaks) in buses {
            // A bus with no tracks still runs its effects, so their tails ring out.
            if !fed && group.effects.is_empty() && group.routing.is_none() {
                continue;
            }
            let (gl, gr) = (frames(&mut group.left, len), frames(&mut group.right, len));
            if !fed {
                gl.fill(0.0);
                gr.fill(0.0);
            }
            run_chain(&mut group.effects, &mut group.routing, gl, gr);
            let peak = apply_gain(&mut group.gain, gl, gr);
            peaks.fetch_max(peak.to_bits(), Ordering::Relaxed);
            mix_into(left, gl);
            mix_into(right, gr);
        }
        run_chain(&mut self.send_effects, &mut self.send_routing, sl, sr);
        let peak = apply_gain(&mut self.send_gain, sl, sr);
        self.status.bus_peaks[0].fetch_max(peak.to_bits(), Ordering::Relaxed);
        mix_into(left, sl);
        mix_into(right, sr);
        run_chain(
            &mut self.master_effects,
            &mut self.master_routing,
            left,
            right,
        );
        let peak = apply_gain(&mut self.master_gain, left, right);
        self.status.bus_peaks[1].fetch_max(peak.to_bits(), Ordering::Relaxed);
        let (cl, cr) = (
            frames(&mut self.click_left, len),
            frames(&mut self.click_right, len),
        );
        self.click.process(&self.click_events, cl, cr);
        self.click_events.clear();
        mix_into(left, cl);
        mix_into(right, cr);
        for sample in left.iter_mut().chain(right.iter_mut()) {
            *sample = sample.tanh();
        }

        let (takes, garbage) = (&mut self.takes, &mut self.garbage);
        let (il, ir) = (
            frames(&mut self.input_left, len),
            frames(&mut self.input_right, len),
        );
        self.looper
            .write(self.frames, il, ir, |t| send_take(takes, garbage, t));
        self.frames += len as u64;
        let recording = self.looper.recording(self.frames);
        self.status.recording.store(recording, Ordering::Relaxed);
    }

    fn sequence_step(&mut self, offset: u32) {
        // Counting in: only clicks, accenting each bar's first beat.
        if self.count_in > 0 {
            self.status
                .count_in
                .store(self.count_in as u32, Ordering::Relaxed);
            if self.count_in.is_multiple_of(self.steps_per_beat) {
                let bar = self
                    .count_in
                    .is_multiple_of(BEATS_PER_BAR * self.steps_per_beat);
                let pitch = if bar { CLICK_ACCENT_PITCH } else { CLICK_PITCH };
                push(&mut self.click_events, note_on(offset, pitch, 100));
            }
            self.count_in -= 1;
            return;
        }
        self.status.count_in.store(0, Ordering::Relaxed);
        // An `X` on the step before: the song ends as it does.
        if self.stopping {
            self.stop();
            return;
        }
        let bar = BEATS_PER_BAR * self.steps_per_beat;
        let at_bar = matches!(&self.cue, Cue::Pattern(s) if s.at_bar)
            && self.pattern_step > 0
            && self.pattern_step.is_multiple_of(bar);
        if at_bar || self.pattern_len > 0 && self.pattern_step >= self.pattern_len {
            self.next_pattern();
            if !self.playing {
                return;
            }
        }
        self.status
            .pattern_step
            .store(self.pattern_step, Ordering::Relaxed);
        // Before the tracks, so a tempo the step sets times its own locks' slides.
        self.conduct(self.conductor_at);
        self.pattern_step += 1;
        self.conductor_at += 1;
        if self.click_enabled && self.step_index.is_multiple_of(self.steps_per_beat) {
            let beat = self.step_index / self.steps_per_beat;
            let pitch = if beat.is_multiple_of(BEATS_PER_BAR) {
                CLICK_ACCENT_PITCH
            } else {
                CLICK_PITCH
            };
            push(&mut self.click_events, note_on(offset, pitch, 100));
        }
        self.step_index += 1;

        // Which tracks hit on this step, for choking.
        let mut hits = [false; MAX_TRACKS];
        let ends = self.frames + u64::from(offset) + self.clock.samples_per_step().round() as u64;
        for index in 0..self.tracks.len() {
            let Some(t) = self.tracks.get_mut(index) else {
                continue;
            };
            t.step_ends = ends;
            // A slow track sits out the song's steps between its own.
            if t.rest > 0 {
                t.rest -= 1;
                continue;
            }
            if self.play_track_step(index, offset)
                && let Some(hit) = hits.get_mut(index)
            {
                *hit = true;
            }
        }
        for (index, &hit) in hits.iter().enumerate() {
            if hit {
                self.choke_others(index, offset, &hits);
            }
        }
    }

    /// Plays what's due on every track before frame `until`, in time order: the notes its
    /// gestures play later in a step, and a fast track's steps between the song's.
    fn play_due(&mut self, until: u64) {
        for index in 0..self.tracks.len() {
            while let Some(due) = self.tracks.get_mut(index).and_then(|t| t.take_due(until)) {
                // Due before `until`, so within the block.
                let offset = u32::try_from(due.at.saturating_sub(self.frames)).unwrap_or(0);
                match due.what {
                    Later::Step => {
                        if self.play_track_step(index, offset) {
                            self.choke_others(index, offset, &[]);
                        }
                    }
                    what => {
                        if let Some(t) = self.tracks.get_mut(index) {
                            t.play_later(what, offset);
                        }
                    }
                }
            }
        }
    }

    /// Plays track `index`'s next step, `offset` frames into the block, and moves its playhead
    /// on, as its gestures say. Whether it played a note.
    fn play_track_step(&mut self, index: usize, offset: u32) -> bool {
        // When this step's played, and how long until what's played along with it reaches the
        // engine.
        let (at, latency) = (self.frames + u64::from(offset), self.latency());
        let muted = self.mutes >> index & 1 == 1;
        let song_step = self.clock.samples_per_step();
        let Some(t) = self.tracks.get_mut(index) else {
            return false;
        };
        if t.track.steps.is_empty() {
            // Note-offs still come, so nothing's left sounding.
            t.held.retain_mut(|h| {
                h.steps_left -= 1;
                if h.steps_left == 0 {
                    push(&mut t.events, note_off(offset, h.pitch));
                }
                h.steps_left > 0
            });
            return false;
        }
        if t.realised != Some(t.pass) {
            realise_into(&t.track, t.pass, &mut t.steps);
            t.realised = Some(t.pass);
            self.looper.pass_starts(index, at, latency);
        }
        t.position = u64::from(t.pass) << 32 | t.playhead as u64;
        self.status.set_position(index, t.position);
        // The playhead is kept within the steps, being reset whenever they change.
        let Some(&realised) = t.steps.get(t.playhead) else {
            t.playhead = 0;
            return false;
        };
        // The playhead's gestures, once as it arrives rather than each time a wait plays the
        // step again: a speed sets this step's own length.
        if t.waits.is_none() {
            for gesture in realised.gestures() {
                match gesture {
                    Gesture::Faster(n) => (t.faster, t.slower) = (n.max(1), 1),
                    Gesture::Slower(n) => (t.faster, t.slower) = (1, n.max(1)),
                    Gesture::Backwards => t.backwards = !t.backwards,
                    _ => {}
                }
            }
        }
        let slower = t.slower;
        let length = song_step * f64::from(slower) / f64::from(t.faster);
        let ends = at + length.round() as u64;
        let ticks = |n: u8| at + (length * f64::from(n) / f64::from(TICKS)).round() as u64;

        // Note-offs first, so a voice can be reused by this step's note; a note held on goes
        // on round its arpeggio.
        let (held, events, due) = (&mut t.held, &mut t.events, &mut t.due);
        held.retain_mut(|h| {
            h.steps_left -= 1;
            if h.steps_left == 0 {
                push(events, note_off(offset, h.pitch));
            } else if let Some(arpeggio) = h.arpeggio {
                arpeggiate(due, h.pitch, arpeggio, 0, (at, ends), h.velocity);
            }
            h.steps_left > 0
        });

        // Muted in this pattern or by the song, its notes go by unplayed, but its playhead
        // still goes where its gestures send it.
        let step = match t.track.pattern_mute || muted {
            true => Step::default(),
            false => realised,
        };
        if !step.locks.is_empty() {
            // Before the note, so it plays with the step's settings.
            t.lock(step.locks, offset);
            t.start_ramps(t.playhead, step.locks, length);
        }
        let delay = (step.gestures()).find_map(|g| match g {
            Gesture::Delay(n) => Some(n),
            _ => None,
        });
        let starts = delay.map_or(at, ticks);
        let mut hit = false;
        if let Some(mut note) = step.note {
            if step.gestures().any(|g| g == Gesture::Accent) {
                note.velocity = 127;
            }
            // Retriggering a held pitch, or out of room: end the old note first.
            let existing = t.held.iter().position(|h| h.pitch == note.pitch);
            let full = (t.held.len() == MAX_HELD).then_some(0);
            if let Some(i) = existing.or(full)
                && let Some(old) = t.held.get(i)
            {
                push(&mut t.events, note_off(offset, old.pitch));
                t.held.remove(i);
            }
            match delay {
                Some(_) => queue(
                    &mut t.due,
                    starts,
                    Later::Start {
                        pitch: note.pitch,
                        velocity: note.velocity,
                    },
                ),
                None => push(&mut t.events, note_on(offset, note.pitch, note.velocity)),
            }
            self.status.started_note(index, note.pitch, note.velocity);
            hit = true;
            t.held.push(Held {
                pitch: note.pitch,
                velocity: note.velocity,
                steps_left: note.length.max(1),
                arpeggio: None,
            });
        }
        // The note gestures act on the step's note, or else the latest still sounding, from
        // the step's start; on the step's own note, from its first hit.
        let first = u64::from(hit);
        let (held, due) = (&mut t.held, &mut t.due);
        if let Some(h) = held.last_mut() {
            for gesture in step.gestures() {
                match gesture {
                    Gesture::Retrigger(n) | Gesture::Fade(n) => {
                        let n = u64::from(n);
                        for k in first..n {
                            let velocity = match gesture {
                                Gesture::Fade(_) => (u64::from(h.velocity) * (n - k) / n).max(1),
                                _ => u64::from(h.velocity),
                            } as u8;
                            let at = starts + (ends.saturating_sub(starts)) * k / n;
                            let (from, to) = (h.pitch, h.pitch);
                            queue(due, at, Later::Change { from, to, velocity });
                        }
                    }
                    Gesture::Arpeggio(a, b) => {
                        let arpeggio = (h.pitch, a, b);
                        h.arpeggio = Some(arpeggio);
                        arpeggiate(due, h.pitch, arpeggio, first, (starts, ends), h.velocity);
                    }
                    Gesture::Cut(n) => queue(due, ticks(n), Later::Cut),
                    _ => {}
                }
            }
        }

        // On to the next step: none yet if it's waiting, the one it jumps to, or the one
        // before or after.
        let wait = realised.gestures().find_map(|g| match g {
            Gesture::Wait(n) => Some(n),
            _ => None,
        });
        t.waits = match (wait, t.waits) {
            (Some(n), None) => Some(n),
            (Some(_), Some(left)) if left > 1 => Some(left - 1),
            _ => None,
        };
        if t.waits.is_none() {
            let len = t.steps.len();
            let jump = realised.gestures().find_map(|g| match g {
                Gesture::Jump(to) => Some(usize::from(to) % len),
                Gesture::Wander => Some(wander(t.pass, t.playhead, index, len)),
                _ => None,
            });
            let (next, top) = match jump {
                Some(to) => (to, to == 0),
                None if t.backwards => match t.playhead.checked_sub(1) {
                    Some(before) => (before, false),
                    None => (len - 1, true),
                },
                None if t.playhead + 1 >= len => (0, true),
                None => (t.playhead + 1, false),
            };
            t.playhead = next;
            // Round to the top of its loop: a new pass, at the song's speed.
            if top {
                t.pass += 1;
                (t.faster, t.slower) = (1, 1);
            }
        }
        // Its next step, once this one's over: within the song's step if it's fast, or so many
        // of the song's steps on if it's slow.
        if ends + 1 < t.step_ends {
            queue(&mut t.due, ends, Later::Step);
        } else {
            t.rest = slower - 1;
        }
        hit
    }

    /// Plays the conductor's step for `pattern_step` steps into the pattern, its loops
    /// included: its gestures, and a step's share of any tempo slide on its way.
    fn conduct(&mut self, pattern_step: u64) {
        let len = self.conductor.steps.len() as u64;
        let Some(at) = pattern_step.checked_rem(len).map(|at| at as usize) else {
            self.status
                .conductor_step
                .store(u64::MAX, Ordering::Relaxed);
            return;
        };
        self.status
            .conductor_step
            .store(at as u64, Ordering::Relaxed);
        let Some(&gestures) = self.conductor.steps.get(at) else {
            return;
        };
        let mut tempo = None;
        for gesture in gestures.into_iter().flatten() {
            match gesture {
                SongGesture::Tempo(bpm) | SongGesture::Slide(bpm) => {
                    tempo = Some(Some(f64::from(bpm)))
                }
                SongGesture::SongTempo => tempo = Some(None),
                SongGesture::Stop => self.stopping = true,
                SongGesture::Pattern(p) => {
                    let pattern = usize::from(p).saturating_sub(1);
                    // Full, the UI thread's fallen behind; it'll miss this cue.
                    let _ = self.conducted.push(Conducted::Pattern(pattern));
                }
                SongGesture::Mute { track, on } => {
                    let track = usize::from(track).saturating_sub(1);
                    if let Some(player) = self.tracks.get_mut(track) {
                        player.conducted_mute = Some(on);
                        let _ = self.conducted.push(Conducted::Mute { track, muted: on });
                    }
                }
                SongGesture::Solo { track, on } => {
                    let track = usize::from(track).saturating_sub(1);
                    if let Some(player) = self.tracks.get_mut(track) {
                        player.conducted_solo = Some(on);
                        let _ = self.conducted.push(Conducted::Solo { track, soloed: on });
                    }
                }
            }
        }
        // With no tempo of its own, the step might be on the way to a slide's: from the last
        // tempo there, a step's share of the way at a time, arriving on the slide's step.
        let tempo = tempo.or_else(|| {
            let (ahead, target) =
                (self.conductor.steps.iter().skip(at + 1).enumerate()).find_map(|(k, step)| {
                    let gesture = step.iter().flatten().rev().find(|g| g.is_tempo())?;
                    Some((k + 1, *gesture))
                })?;
            let SongGesture::Slide(target) = target else {
                return None;
            };
            let share = 1.0 / (ahead as f64 + 1.0);
            Some(Some(self.bpm + (f64::from(target) - self.bpm) * share))
        });
        match tempo {
            Some(Some(bpm)) => {
                self.conducted_tempo = true;
                self.set_tempo(bpm, true);
            }
            Some(None) => {
                self.conducted_tempo = false;
                self.set_tempo(self.song_bpm, true);
            }
            None => {}
        }
    }

    /// The playing pattern has ended: carry out the cue, and start every track from the top.
    fn next_pattern(&mut self) {
        self.mutes = self.next_mutes;
        match std::mem::take(&mut self.cue) {
            Cue::Stop => {
                self.stop();
                return;
            }
            Cue::Loop => {
                // Each track carries on round its own loop, so tracks of different lengths keep
                // drifting against each other.
                self.pattern_step = 0;
                self.status.boundaries.fetch_add(1, Ordering::Relaxed);
                return;
            }
            Cue::Pattern(mut section) => {
                let fits = section.lanes.len() == self.tracks.len()
                    && section
                        .lanes
                        .iter()
                        .all(|l| (1..=MAX_STEPS).contains(&l.steps.len()));
                if fits {
                    for (t, lane) in self.tracks.iter_mut().zip(&mut section.lanes) {
                        lane.swap(&mut t.track);
                    }
                    if section.conductor.steps.len() <= MAX_STEPS {
                        std::mem::swap(&mut self.conductor, &mut section.conductor);
                    }
                    self.pattern_len = section.steps;
                    self.swaps += 1;
                    self.status.swaps.store(self.swaps, Ordering::Relaxed);
                }
                // Holding the old lanes now, to be freed off the audio thread.
                self.discard(Garbage::Section(section));
            }
        }
        for t in &mut self.tracks {
            t.unlock();
            t.rewind();
        }
        self.pattern_step = 0;
        self.conductor_at = 0;
        // Bars count from the top of each pattern, for the click's accent.
        self.step_index = 0;
        self.status.boundaries.fetch_add(1, Ordering::Relaxed);
    }

    /// Chokes every other track in `index`'s group with its choke number, except those that hit
    /// at the same time (`hits`), which both play.
    fn choke_others(&mut self, index: usize, offset: u32, hits: &[bool]) {
        let Some(t) = self.tracks.get(index) else {
            return;
        };
        let (Some(group), choke) = (t.group, t.track.choke) else {
            return;
        };
        if choke == 0 {
            return;
        }
        for (j, other) in self.tracks.iter_mut().enumerate() {
            let same = other.group == Some(group) && other.track.choke == choke;
            if same && j != index && !hits.get(j).copied().unwrap_or(false) {
                push(
                    &mut other.events,
                    Event {
                        offset,
                        kind: EventKind::Choke,
                    },
                );
            }
        }
    }
}

/// Puts an event in a track's queue for later in the step, unless it's full.
fn queue(due: &mut Vec<Due>, at: u64, what: Later) {
    if due.len() < due.capacity() {
        due.push(Due { at, what });
    }
}

/// Queues an arpeggio's notes from the `first`th of its step (each a sixth of the way from
/// `start` to `end`), starting from the held note `from`: its root, then the steps above, in turn.
fn arpeggiate(
    due: &mut Vec<Due>,
    from: u8,
    (root, a, b): (u8, u8, u8),
    first: u64,
    (start, end): (u64, u64),
    velocity: u8,
) {
    let mut from = from;
    for k in first..ARPEGGIO {
        let up = match k % 3 {
            0 => 0,
            1 => a,
            _ => b,
        };
        let to = root.saturating_add(up).min(127);
        let at = start + (end.saturating_sub(start)) * k / ARPEGGIO;
        queue(due, at, Later::Change { from, to, velocity });
        from = to;
    }
}

/// The step a `J?` jumps to, out of `len`: the same for the same pass, step and track, so the
/// song plays the same each time.
fn wander(pass: u32, step: usize, track: usize, len: usize) -> usize {
    // SplitMix64's finaliser, to scatter neighbouring passes and steps.
    let mut x = u64::from(pass) << 32 ^ (step as u64) << 8 ^ track as u64;
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x % len.max(1) as u64) as usize
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
        let Some(sorted) = events.get_mut(..=i) else {
            break;
        };
        let Some((last, before)) = sorted.split_last() else {
            continue;
        };
        // After any at the same offset, so it stays behind them.
        let at = before.partition_point(|e| e.offset <= last.offset);
        if let Some(moving) = sorted.get_mut(at..) {
            moving.rotate_right(1);
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
            groups: vec![],
        };
        let (mut engine, _) = Engine::new(sample_rate, bpm, &grid, vec![]);
        engine.play();
        engine
    }

    fn lead() -> Track {
        Track {
            face: None,
            pattern_mute: false,
            columns: vec![],
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
            effects: Default::default(),
            choke: 0,
            macros: vec![],
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
            groups: vec![],
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
    fn patterns_change_on_the_boundary_as_cued() {
        let (mut engine, mut handle) = with_synth(48_000.0);
        let step = 48_000.0 * 60.0 / (133.0 * 4.0);
        let lane = Lane {
            mute: false,
            steps: vec![Step::note(50); 3],
            rules: vec![],
        };
        handle.send(Command::SetLength(4));
        handle.send(Command::Cue(Cue::Pattern(Box::new(Section {
            conductor: Default::default(),
            lanes: vec![lane],
            steps: 6,
            at_bar: false,
        }))));
        render(&mut engine, 256, (step * 3.5) as usize);
        assert_eq!(handle.status().position(0), Some((3, 0)));
        assert_eq!(handle.status().boundaries(), 0);
        // Into the fifth step: the new pattern, from its top.
        render(&mut engine, 256, step as usize);
        assert_eq!(handle.status().position(0), Some((0, 0)));
        assert_eq!(
            (handle.status().boundaries(), handle.status().swaps()),
            (1, 1)
        );
        assert_eq!(engine.tracks[0].track.steps.len(), 3);
        assert!(engine.tracks[0].track.rules.is_empty());

        // A track sent before the change was seen is ignored; after, it's heard.
        let stale = |swaps| Command::SetTrack {
            index: 0,
            track: Box::new(lead()),
            swaps,
        };
        handle.send(stale(0));
        render(&mut engine, 256, 64);
        assert_eq!(engine.tracks[0].track.steps.len(), 3);
        handle.send(stale(1));
        render(&mut engine, 256, 64);
        assert_eq!(engine.tracks[0].track.steps.len(), 7);

        // Six steps on, it stops, as cued.
        handle.send(Command::Cue(Cue::Stop));
        render(&mut engine, 256, (step * 5.0) as usize);
        assert!(handle.status().playing());
        render(&mut engine, 256, step as usize);
        assert!(!handle.status().playing());
        assert_eq!(handle.status().boundaries(), 0);
    }

    #[test]
    fn a_track_muted_in_a_pattern_plays_nothing_there() {
        let grid = Grid {
            name: "mutes".into(),
            steps_per_beat: 4,
            tracks: vec![Track {
                steps: vec![Step::note(60); 4],
                rules: vec![],
                pattern_mute: true,
                ..lead()
            }],
            groups: vec![],
        };
        let (mut engine, mut handle) = Engine::new(1_000.0, 120.0, &grid, vec![Box::new(Ticks)]);
        engine.click_enabled = false;
        engine.play();
        // 125 samples a step: four steps of a muted pattern, then one where it plays.
        handle.send(Command::SetLength(4));
        handle.send(Command::Cue(Cue::Pattern(Box::new(Section {
            conductor: Default::default(),
            lanes: vec![Lane {
                steps: vec![Step::note(60); 2],
                ..Lane::default()
            }],
            steps: 4,
            at_bar: false,
        }))));
        let (left, _) = render_stereo(&mut engine, 750);
        assert_eq!(onsets(&left), [500, 625]);
        assert!(!engine.tracks[0].track.pattern_mute);
    }

    #[test]
    fn a_track_the_song_mutes_plays_nothing_until_it_unmutes() {
        let grid = Grid {
            name: "mutes".into(),
            steps_per_beat: 4,
            tracks: vec![Track {
                steps: vec![Step::note(60); 2],
                rules: vec![],
                ..lead()
            }],
            groups: vec![],
        };
        let (mut engine, mut handle) = Engine::new(1_000.0, 120.0, &grid, vec![Box::new(Ticks)]);
        engine.click_enabled = false;
        // Muted for the first time round four steps, and playing as it loops.
        handle.send(Command::SetLength(4));
        handle.send(Command::SetMutes { now: 1, next: 0 });
        handle.send(Command::Cue(Cue::Loop));
        engine.play();
        let (left, _) = render_stereo(&mut engine, 750);
        assert_eq!(onsets(&left), [500, 625]);
    }

    #[test]
    fn a_looping_pattern_leaves_its_tracks_running() {
        let (mut engine, mut handle) = with_synth(48_000.0);
        let step = 48_000.0 * 60.0 / (133.0 * 4.0);
        assert_eq!(engine.tracks[0].track.steps.len(), 7);
        handle.send(Command::SetLength(4));
        render(&mut engine, 256, (step * 5.5) as usize);
        // Round the pattern once, but the seven-step track's still on its first pass.
        assert_eq!(handle.status().boundaries(), 1);
        assert_eq!(handle.status().pattern_step(), 1);
        assert_eq!(handle.status().position(0), Some((5, 0)));
    }

    #[test]
    fn a_pattern_picked_by_hand_takes_over_at_the_next_bar() {
        let (mut engine, mut handle) = with_synth(48_000.0);
        let step = 48_000.0 * 60.0 / (133.0 * 4.0);
        handle.send(Command::SetLength(64));
        handle.send(Command::Cue(Cue::Pattern(Box::new(Section {
            conductor: Default::default(),
            lanes: vec![Lane {
                mute: false,
                steps: vec![Step::note(50); 3],
                rules: vec![],
            }],
            steps: 64,
            at_bar: true,
        }))));
        render(&mut engine, 256, (step * 15.5) as usize);
        assert_eq!(handle.status().swaps(), 0);
        render(&mut engine, 256, step as usize);
        assert_eq!(handle.status().swaps(), 1);
        assert_eq!(handle.status().position(0), Some((0, 0)));
    }

    #[test]
    fn clicks_land_on_exact_samples() {
        // 137 bpm at 44.1 kHz: 19,313.868... samples per beat.
        let audio = render(&mut click_only(44_100.0, 137.0), 441, 100_000);
        let spb = 44_100.0 * 60.0 / 137.0;
        let expected: Vec<usize> = (0..6).map(|k| (k as f64 * spb).ceil() as usize).collect();
        assert_eq!(onsets(&audio), expected);
    }

    fn conductor(steps: &[(usize, &str)], len: usize) -> Conductor {
        let mut conductor = Conductor::new(len);
        for &(at, text) in steps {
            let step = &mut conductor.steps[at];
            let free = step.iter_mut().find(|g| g.is_none()).unwrap();
            *free = Some(text.parse().unwrap());
        }
        conductor
    }

    #[test]
    fn the_conductors_tempo_times_its_own_step_until_the_next() {
        // 120 BPM at 48 kHz: 6,000 samples a step, so a beat's 24,000; at 60 BPM, twice that.
        let mut engine = click_only(48_000.0, 120.0);
        engine.conductor = conductor(&[(0, "T60"), (4, "T--")], 8);
        let audio = render(&mut engine, 256, 100_000);
        // Step 4 goes back to 120, then the lane comes round to `T60` again on step 8.
        assert_eq!(onsets(&audio), [0, 48_000, 72_000]);
        assert_eq!(engine.status.tempo(), 60.0);
        assert_eq!(engine.status.conductor_step(), Some(2));
        engine.stop();
        assert_eq!(
            engine.status.tempo(),
            120.0,
            "stopping goes back to the song's"
        );
    }

    #[test]
    fn a_tempo_slide_arrives_on_its_step() {
        let mut engine = click_only(48_000.0, 120.0);
        engine.conductor = conductor(&[(0, "T60"), (4, "~120")], 16);
        let audio = render(&mut engine, 256, 50_000);
        // Steps of 60, 75, 90 and 105 BPM: 12,000, 9,600, 8,000 and 6,857.1 samples.
        assert_eq!(onsets(&audio), [0, 36_458]);
        assert_eq!(engine.status.tempo(), 120.0);
        assert!(engine.conducted_tempo);
    }

    #[test]
    fn a_conductor_longer_than_its_pattern_goes_on_round_the_loops() {
        let mut engine = click_only(48_000.0, 120.0);
        engine.pattern_len = 4;
        engine.conductor = conductor(&[(4, "T60")], 8);
        // Four steps at 120, then the pattern loops and the conductor goes on to its step 4.
        let audio = render(&mut engine, 256, 80_000);
        assert_eq!(onsets(&audio), [0, 24_000, 72_000]);
        // Step 8 is round again to the conductor's first.
        assert_eq!(engine.status.conductor_step(), Some(0));
    }

    #[test]
    fn the_conductor_stops_mutes_solos_and_cues() {
        let (mut engine, mut handle) = with_synth(48_000.0);
        engine.conductor = conductor(&[(1, "M1"), (1, "P3"), (2, "S1"), (3, "M-1"), (3, "X")], 8);
        let step = (48_000.0 * 60.0 / (133.0 * 4.0)) as usize;
        render(&mut engine, 256, step * 3 + 10);
        let player = &engine.tracks[0];
        assert!(!player.mix().mute && player.mix().solo);
        assert!(!player.track.mix.solo, "the track's own is as it was");
        assert_eq!(
            [(); 5].map(|()| handle.conducted()),
            [
                Some(Conducted::Mute {
                    track: 0,
                    muted: true
                }),
                Some(Conducted::Pattern(2)),
                Some(Conducted::Solo {
                    track: 0,
                    soloed: true
                }),
                Some(Conducted::Mute {
                    track: 0,
                    muted: false
                }),
                None
            ]
        );
        // `X` on step 3: it plays, then the song stops as step 4 would start.
        assert!(handle.status().playing());
        render(&mut engine, 256, step);
        assert!(!handle.status().playing());
        assert!(
            !engine.tracks[0].mix().solo,
            "they last while the song plays"
        );
    }

    #[test]
    fn a_count_in_clicks_a_bar_even_with_the_click_off_then_plays() {
        let (mut engine, handle) = with_synth(48_000.0);
        engine.click_enabled = false;
        engine.play_after(1);
        // 133 bpm, 16ths: about 5,413 samples per step, so a bar is 16 of them.
        let step = 48_000.0 * 60.0 / (133.0 * 4.0);
        let start = render(&mut engine, 256, 100);
        assert_eq!(handle.status().count_in(), 16);
        assert_eq!(handle.status().position(0), None);
        let rest = render(&mut engine, 256, (step * 15.5) as usize - 100);
        let audio = [start, rest].concat();
        let expected: Vec<usize> = (0..4)
            .map(|beat| (beat as f64 * step * 4.0).ceil() as usize)
            .collect();
        assert_eq!(onsets(&audio), expected);
        assert_eq!(handle.status().count_in(), 1);
        // Then the song, from its first step.
        render(&mut engine, 256, step as usize);
        assert_eq!(handle.status().count_in(), 0);
        assert_eq!(handle.status().position(0), Some((0, 0)));
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
            swaps: 0,
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

    type Notes = std::sync::Arc<std::sync::Mutex<Vec<(u64, bool, u8, u8)>>>;

    /// An instrument that writes down each note on and off: its frame, whether it's on, its
    /// pitch and velocity.
    fn logger() -> (Box<dyn Processor>, Notes) {
        struct Log(u64, Notes);
        impl Processor for Log {
            fn process(&mut self, events: &[Event], left: &mut [f32], _: &mut [f32]) {
                let mut log = self.1.lock().unwrap();
                for e in events {
                    let at = self.0 + u64::from(e.offset);
                    match e.kind {
                        EventKind::NoteOn { pitch, velocity } => {
                            log.push((at, true, pitch, velocity))
                        }
                        EventKind::NoteOff { pitch } => log.push((at, false, pitch, 0)),
                        _ => {}
                    }
                }
                self.0 += left.len() as u64;
            }
        }
        let notes = Notes::default();
        (Box::new(Log(0, notes.clone())), notes)
    }

    /// A track of `pitches` with these gestures, playing at 120 BPM and 48 kHz: 6,000 frames a
    /// step, and 250 a 24th.
    fn gesturing(pitches: &[u8], gestures: &[(usize, &str)]) -> (Engine, Notes) {
        let mut steps: Vec<Step> = pitches.iter().map(|&p| Step::note(p)).collect();
        for &(at, gesture) in gestures {
            let fields = &mut steps[at].gestures;
            let free = fields.iter_mut().find(|g| g.is_none()).unwrap();
            *free = Some(gesture.parse().unwrap());
        }
        let grid = Grid {
            name: "gestures".into(),
            steps_per_beat: 4,
            tracks: vec![Track {
                steps,
                ..Track::default()
            }],
            groups: vec![],
        };
        let (instrument, notes) = logger();
        let (mut engine, _) = Engine::new(48_000.0, 120.0, &grid, vec![instrument]);
        engine.play();
        (engine, notes)
    }

    #[test]
    fn note_gestures_play_within_their_step() {
        let gestures = [
            (0, "R3"),
            (1, "D8"),
            (2, "X6"),
            (3, "A37"),
            (4, "!"),
            (4, "r2"),
        ];
        let (mut engine, notes) = gesturing(&[60, 62, 64, 65, 67], &gestures);
        render(&mut engine, 256, 30_000);
        let on = |at, pitch, velocity| (at, true, pitch, velocity);
        let off = |at, pitch| (at, false, pitch, 0);
        assert_eq!(
            *notes.lock().unwrap(),
            [
                // Three hits in the step.
                on(0, 60, 100),
                off(2_000, 60),
                on(2_000, 60, 100),
                off(4_000, 60),
                on(4_000, 60, 100),
                // 8/24 late.
                off(6_000, 60),
                on(8_000, 62, 100),
                // Cut 6/24 in.
                off(12_000, 62),
                on(12_000, 64, 100),
                off(13_500, 64),
                // The note, +3 and +7, a sixth of a step each.
                on(18_000, 65, 100),
                off(19_000, 65),
                on(19_000, 68, 100),
                off(20_000, 68),
                on(20_000, 72, 100),
                off(21_000, 72),
                on(21_000, 65, 100),
                off(22_000, 65),
                on(22_000, 68, 100),
                off(23_000, 68),
                on(23_000, 72, 100),
                // Accented, then a second hit at half that.
                off(24_000, 72),
                on(24_000, 67, 127),
                off(27_000, 67),
                on(27_000, 67, 63),
            ]
        );
    }

    #[test]
    fn a_gesture_on_a_step_without_a_note_acts_on_the_one_sounding() {
        let (mut engine, notes) = gesturing(&[60], &[]);
        let track = &mut engine.tracks[0].track;
        track.steps[0].note.as_mut().unwrap().length = 2;
        track.steps.push(Step::default());
        track.steps[1].gestures[0] = Some("X12".parse().unwrap());
        render(&mut engine, 256, 11_000);
        // Held on into step 1, and cut halfway through it.
        assert_eq!(
            *notes.lock().unwrap(),
            [(0, true, 60, 100), (9_000, false, 60, 0)]
        );
    }

    /// The pitches played, in order, on a track whose step `i` plays pitch `i`.
    fn played(notes: &Notes) -> Vec<(u64, u8)> {
        (notes.lock().unwrap().iter())
            .filter(|n| n.1)
            .map(|n| (n.0, n.2))
            .collect()
    }

    #[test]
    fn playhead_gestures_jump_wait_and_turn_round() {
        let pitches: Vec<u8> = (0..8).collect();
        let gestures = [(2, "J5"), (6, "W1"), (7, "B")];
        let (mut engine, notes) = gesturing(&pitches, &gestures);
        render(&mut engine, 256, 6_000 * 14);
        let order: Vec<u8> = played(&notes).into_iter().map(|(_, p)| p).collect();
        // On to 5 after 2; 6 twice; backwards from 7, waiting on 6 again; then round 5 to 2.
        assert_eq!(order, [0, 1, 2, 5, 6, 6, 7, 6, 6, 5, 4, 3, 2, 5]);
    }

    #[test]
    fn speed_gestures_last_until_the_next_or_the_top_of_the_loop() {
        let (mut engine, notes) = gesturing(&[0, 1, 2, 3], &[(2, "*2")]);
        render(&mut engine, 100, 30_000);
        let fast = [
            (0, 0),
            (6_000, 1),
            (12_000, 2),
            (15_000, 3),
            (18_000, 0),
            (24_000, 1),
        ];
        assert_eq!(played(&notes), fast);

        let (mut engine, notes) = gesturing(&[0, 1, 2, 3], &[(1, "/2")]);
        render(&mut engine, 100, 45_000);
        let slow = [(0, 0), (6_000, 1), (18_000, 2), (30_000, 3), (42_000, 0)];
        assert_eq!(played(&notes), slow);
        // Half speed holds notes twice as long.
        assert!(notes.lock().unwrap().contains(&(30_000, false, 2, 0)));
    }

    #[test]
    fn a_random_jump_is_the_same_each_time_the_song_plays() {
        let pitches: Vec<u8> = (0..16).collect();
        let order = || {
            let (mut engine, notes) = gesturing(&pitches, &[(3, "J?")]);
            render(&mut engine, 256, 6_000 * 40);
            played(&notes)
                .into_iter()
                .map(|(_, p)| p)
                .collect::<Vec<_>>()
        };
        let first = order();
        assert_eq!(first, order());
        assert_eq!(first[..4], [0, 1, 2, 3]);
        assert!(
            first.iter().filter(|&&p| p == 3).count() > 2,
            "it comes round"
        );
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
            held: false,
        });
        let audio = render(&mut engine, 256, 48_000);
        assert!(audio[..4_800].iter().any(|x| x.abs() > 0.05));
        assert!(audio[40_000..].iter().all(|&x| x == 0.0));
    }

    #[test]
    fn a_held_preview_sounds_until_released_alongside_others() {
        let (mut engine, mut handle) = with_synth(48_000.0);
        engine.stop();
        for pitch in [60, 64] {
            handle.send(Command::Preview {
                track: 0,
                pitch,
                velocity: 100,
                held: true,
            });
        }
        // Held well past a brief preview's 0.3 s, both notes still sound.
        render(&mut engine, 256, 48_000);
        assert_eq!(engine.tracks[0].previews.len(), 2);
        assert!(
            render(&mut engine, 256, 4_800)
                .iter()
                .any(|x| x.abs() > 0.05)
        );
        for pitch in [60, 64] {
            handle.send(Command::Release { track: 0, pitch });
        }
        let audio = render(&mut engine, 256, 48_000);
        assert!(engine.tracks[0].previews.is_empty());
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
            groups: vec![],
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
            swaps: 0,
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
    fn grouped_tracks_mix_through_their_group_bus() {
        let (mut engine, mut handle) = mixer(&[Mix::default(), Mix::default()]);
        for index in 0..2 {
            handle.send(Command::SetGroup {
                index,
                group: Some(3),
            });
        }
        handle.send(Command::SetBusLevel {
            bus: Chain::Group(3),
            level: -6.0,
        });
        // The route moves with its track.
        handle.send(Command::MoveTrack { from: 0, to: 1 });
        handle.send(Command::SetGroup {
            index: 1,
            group: None,
        });
        render_stereo(&mut engine, 30);
        handle.status().take_bus_peak(Chain::Group(3));
        let (left, _) = render_stereo(&mut engine, 10);
        // One track straight through, the other at the group's -6 dB.
        let group = 0.5 * db_to_gain(-6.0);
        assert!((left[0] - (0.5 + group).tanh()).abs() < 1e-6);
        assert!((handle.status().take_bus_peak(Chain::Group(3)) - group).abs() < 1e-6);
        assert_eq!(handle.status().take_bus_peak(Chain::Group(0)), 0.0);
        assert_eq!(handle.status().take_bus_peak(Chain::Group(99)), 0.0);
    }

    /// Marks each note-on with a sample of 1.
    struct Ticks;

    impl Processor for Ticks {
        fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
            for e in events {
                if let EventKind::NoteOn { .. } = e.kind {
                    left[e.offset as usize] = 1.0;
                    right[e.offset as usize] = 1.0;
                }
            }
        }
    }

    #[test]
    fn a_repeated_preview_hits_on_the_step_grid_until_released() {
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![lead()],
            groups: vec![],
        };
        let (mut engine, mut handle) = Engine::new(48_000.0, 120.0, &grid, vec![Box::new(Ticks)]);
        // 120 bpm, 16ths: 6,000 samples a step. Stopped, the clock still runs.
        render(&mut engine, 256, 1_000);
        handle.send(Command::Repeat {
            track: 0,
            pitch: 60,
            velocity: 100,
            every: 2,
            now: true,
        });
        // At once, then on steps 2 and 4, at samples 12,000 and 24,000.
        let audio = render(&mut engine, 256, 25_000);
        assert_eq!(onsets(&audio), [0, 11_000, 23_000]);
        handle.send(Command::Release {
            track: 0,
            pitch: 60,
        });
        assert!(render(&mut engine, 256, 30_000).iter().all(|&x| x == 0.0));

        // Playing, the grid is the song's: from its first step.
        handle.send(Command::Play { count_in: 0 });
        render(&mut engine, 256, 3_000);
        handle.send(Command::Repeat {
            track: 0,
            pitch: 99,
            velocity: 100,
            every: 4,
            now: false,
        });
        let audio = render(&mut engine, 256, 25_000);
        assert!(onsets(&audio).contains(&(24_000 - 3_000)));
    }

    /// Counts the chokes it's sent.
    struct Chokes(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl Processor for Chokes {
        fn process(&mut self, events: &[Event], _: &mut [f32], _: &mut [f32]) {
            let chokes = events.iter().filter(|e| e.kind == EventKind::Choke);
            (self.0).fetch_add(chokes.count(), std::sync::atomic::Ordering::Relaxed);
        }
    }

    #[test]
    fn hits_choke_the_tracks_in_their_group_with_their_choke_number() {
        // Hits on steps 0, 2, 0, 0 and 2: the third shares the first's step, the fourth has
        // another choke number and the fifth no group.
        let hits = [0, 2, 0, 0, 2];
        let chokes = [1, 1, 1, 2, 1];
        let tracks = hits
            .iter()
            .zip(chokes)
            .map(|(&hit, choke)| {
                let mut steps = vec![Step::default(); 4];
                steps[hit] = Step::note(60);
                Track {
                    steps,
                    choke,
                    rules: vec![],
                    ..lead()
                }
            })
            .collect();
        let grid = Grid {
            name: "choke".into(),
            steps_per_beat: 4,
            tracks,
            groups: vec![],
        };
        let counts: Vec<std::sync::Arc<std::sync::atomic::AtomicUsize>> =
            hits.iter().map(|_| Default::default()).collect();
        let instruments = counts
            .iter()
            .map(|count| Box::new(Chokes(count.clone())) as Box<dyn Processor>)
            .collect();
        let (mut engine, mut handle) = Engine::new(1_000.0, 120.0, &grid, instruments);
        engine.click_enabled = false;
        for index in 0..4 {
            handle.send(Command::SetGroup {
                index,
                group: Some(0),
            });
        }
        engine.play();
        // 125 samples a step: steps 0 to 2.
        render_stereo(&mut engine, 300);
        let counts: Vec<_> = counts
            .iter()
            .map(|c| c.load(std::sync::atomic::Ordering::Relaxed))
            .collect();
        // Step 0 chokes track 1 twice, and step 2 chokes tracks 0 and 2; tracks 0 and 2 don't
        // choke each other.
        assert_eq!(counts, [1, 2, 1, 0, 0]);
    }

    /// Notes down each setting it's sent, with when, and the time of each note.
    #[derive(Default)]
    struct Settings {
        at: usize,
        log: std::sync::Arc<std::sync::Mutex<Vec<(usize, String)>>>,
    }

    impl Processor for Settings {
        fn process(&mut self, events: &[Event], left: &mut [f32], _: &mut [f32]) {
            let mut log = self.log.lock().unwrap();
            for e in events {
                let at = self.at + e.offset as usize;
                match e.kind {
                    EventKind::Param { id, value } => log.push((at, format!("{id}={value}"))),
                    EventKind::NoteOn { .. } => log.push((at, "note".into())),
                    _ => {}
                }
            }
            self.at += left.len();
        }

        fn params(&self) -> &[ParamInfo] {
            &SETTINGS
        }
    }

    /// Four settings, each 0 to 8.
    const SETTINGS: [ParamInfo; 4] = [setting(0), setting(1), setting(2), setting(3)];

    const fn setting(id: u32) -> ParamInfo {
        ParamInfo {
            id,
            name: "setting",
            min: 0.0,
            max: 8.0,
            default: 0.0,
            unit: "",
            curve: lattice_core::Curve::Linear,
        }
    }

    #[test]
    fn macros_move_settings_from_where_they_are() {
        let mut locked = Step::note(60);
        locked.locks.set(Lock::of_macro(0, 0.25));
        let mut track = Track {
            steps: vec![locked, Step::default(), Step::note(62), Step::default()],
            rules: vec![],
            ..lead()
        };
        track.instrument.values = vec![0.0; 4];
        track.macros.push(lattice_core::Macro {
            name: "m".into(),
            value: 0.0,
            targets: vec![lattice_core::MacroTarget {
                fx: None,
                param: 1,
                depth: 0.5,
            }],
        });
        let grid = Grid {
            name: "macros".into(),
            steps_per_beat: 4,
            tracks: vec![track],
            groups: vec![],
        };
        let settings = Settings::default();
        let log = settings.log.clone();
        let (mut engine, mut handle) = Engine::new(1_000.0, 120.0, &grid, vec![Box::new(settings)]);
        engine.click_enabled = false;
        // Turned all the way, it moves the setting half its range.
        handle.send(Command::SetMacro {
            track: 0,
            index: 0,
            value: 1.0,
        });
        render_stereo(&mut engine, 10);
        // Turned by hand, the setting moves, and the macro moves it from there.
        handle.send(Command::SetParam {
            target: Target::Instrument(0),
            id: 1,
            value: 2.0,
        });
        render_stereo(&mut engine, 10);
        // A step locks the macro at a quarter, and it holds, through the next note.
        handle.send(Command::Play { count_in: 0 });
        render_stereo(&mut engine, 400);
        let log = log.lock().unwrap();
        let log: Vec<&str> = (log.iter())
            .map(|(_, what)| what.as_str())
            .filter(|w| w.starts_with("1="))
            .collect();
        assert_eq!(log, ["1=4", "1=6", "1=3"]);
    }

    #[test]
    fn a_lock_sent_while_playing_holds_until_stopped() {
        let mut track = Track {
            steps: vec![
                Step::note(60),
                Step::default(),
                Step::default(),
                Step::note(62),
            ],
            rules: vec![],
            ..lead()
        };
        track.instrument.values = vec![0.0, 0.0, 2.0, 3.0];
        let grid = Grid {
            name: "lock".into(),
            steps_per_beat: 4,
            tracks: vec![track],
            groups: vec![],
        };
        let settings = Settings::default();
        let log = settings.log.clone();
        let (mut engine, mut handle) = Engine::new(1_000.0, 120.0, &grid, vec![Box::new(settings)]);
        engine.click_enabled = false;
        let lock = Lock {
            fx: None,
            param: 2,
            value: 8.0,
            ..Default::default()
        };
        // Stopped, it's ignored.
        handle.send(Command::Lock { track: 0, lock });
        render_stereo(&mut engine, 10);
        engine.play();
        render_stereo(&mut engine, 150);
        handle.send(Command::Lock { track: 0, lock });
        render_stereo(&mut engine, 300);
        handle.send(Command::Stop);
        render_stereo(&mut engine, 10);
        let log = log.lock().unwrap();
        let what: Vec<&str> = log.iter().map(|(_, w)| w.as_str()).collect();
        assert_eq!(what, ["note", "2=8", "note", "2=2"]);
        // Heard from when it was sent, through step 3's note, until stopped.
        assert!((160..385).contains(&log[1].0), "{log:?}");
        assert_eq!(log[2].0, 385);
        assert_eq!(log[3].0, 460);
    }

    #[test]
    fn locks_hold_until_the_settings_next_lock() {
        let lock = |param, value| Lock {
            fx: None,
            param,
            value,
            ..Default::default()
        };
        let mut locked = Step::note(60);
        locked.locks.set(lock(2, 5.0));
        locked.locks.set(lock(3, 1.0));
        let mut trigless = Step::default();
        trigless.locks.set(lock(2, 7.0));
        let mut reset = Step::note(62);
        reset.locks.set(Lock {
            change: Change::Reset,
            ..lock(3, 0.0)
        });
        let mut track = Track {
            steps: vec![locked, Step::default(), reset, trigless],
            rules: vec![],
            ..lead()
        };
        track.instrument.values = vec![0.0, 0.0, 2.0, 3.0];
        let grid = Grid {
            name: "locks".into(),
            steps_per_beat: 4,
            tracks: vec![track],
            groups: vec![],
        };
        let settings = Settings::default();
        let log = settings.log.clone();
        let (mut engine, mut handle) = Engine::new(1_000.0, 120.0, &grid, vec![Box::new(settings)]);
        engine.click_enabled = false;
        engine.play();
        // 125 samples a step. While the lock holds, the mixer's change waits.
        render_stereo(&mut engine, 200);
        // The UI sees the locks in force.
        let held: Vec<Lock> = handle.status().locks(0).iter().copied().collect();
        assert_eq!(held, [lock(2, 5.0), lock(3, 1.0)]);
        handle.send(Command::SetParam {
            target: Target::Instrument(0),
            id: 2,
            value: 9.0,
        });
        handle.send(Command::SetParam {
            target: Target::Instrument(0),
            id: 1,
            value: 4.0,
        });
        render_stereo(&mut engine, 400);
        handle.send(Command::Stop);
        render_stereo(&mut engine, 10);
        assert!(handle.status().locks(0).is_empty());
        let log = log.lock().unwrap();
        let log: Vec<(usize, &str)> = (log.iter())
            .map(|(at, what)| (*at, what.as_str()))
            .collect();
        assert_eq!(
            log,
            [
                (0, "2=5"),
                (0, "3=1"),
                (0, "note"),
                // Unlocked, it goes through at once.
                (200, "1=4"),
                // Step 2 puts one setting back to the mixer's, and the other holds.
                (250, "3=3"),
                (250, "note"),
                // Step 3 locks with no note; step 0 locks both again.
                (375, "2=7"),
                (500, "2=5"),
                (500, "3=1"),
                (500, "note"),
                // Stopping ends them.
                (600, "2=9"),
                (600, "3=3"),
            ]
        );
    }

    #[test]
    fn a_slide_arrives_on_its_step() {
        let lock = |value, change| Lock {
            fx: None,
            param: 2,
            value,
            change,
        };
        let mut from = Step::note(60);
        from.locks.set(lock(0.0, Change::Set));
        let mut to = Step::default();
        to.locks.set(lock(8.0, Change::Slide));
        let mut track = Track {
            steps: vec![from, Step::default(), to, Step::default()],
            rules: vec![],
            ..lead()
        };
        track.instrument.values = vec![0.0, 0.0, 2.0, 3.0];
        let grid = Grid {
            name: "slide".into(),
            steps_per_beat: 4,
            tracks: vec![track],
            groups: vec![],
        };
        let settings = Settings::default();
        let log = settings.log.clone();
        let (mut engine, _handle) = Engine::new(1_000.0, 120.0, &grid, vec![Box::new(settings)]);
        engine.click_enabled = false;
        engine.play();
        // 125 samples a step, in blocks of 25: the slide takes the two steps to step 2.
        for _ in 0..12 {
            render_stereo(&mut engine, 25);
        }
        let log = log.lock().unwrap();
        let values: Vec<(usize, f32)> = (log.iter())
            .filter_map(|(at, w)| Some((*at, w.strip_prefix("2=")?.parse().ok()?)))
            .collect();
        assert_eq!(values.first(), Some(&(0, 0.0)));
        assert!(values.windows(2).all(|w| w[0].1 < w[1].1), "{values:?}");
        assert!(values.len() > 5, "{values:?}");
        let arrived = values.iter().find(|(_, v)| *v == 8.0).unwrap();
        assert!((225..=250).contains(&arrived.0), "{values:?}");
        // Then it holds.
        assert_eq!(values.last().unwrap().1, 8.0);
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

    /// Plays one track of `steps` steps at 600 frames a step, feeding the input a ramp, each
    /// frame's value its index, a block at a time as an input stream would.
    fn looping(steps: usize) -> (Engine, Handle, Producer<f32>) {
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![Track {
                steps: vec![Step::default(); steps],
                ..lead()
            }],
            groups: vec![],
        };
        let (mut engine, mut handle) = Engine::new(4800.0, 120.0, &grid, vec![silent()]);
        let (tx, rx) = RingBuffer::new(1 << 16);
        handle.send(Command::SetInput(rx));
        engine.set_latency(0);
        (engine, handle, tx)
    }

    fn feed(engine: &mut Engine, input: &mut Producer<f32>, from: usize, frames: usize) {
        for block in (from..from + frames).step_by(MAX_BLOCK) {
            for i in block..block + MAX_BLOCK {
                input.push(i as f32).unwrap();
                input.push(-(i as f32)).unwrap();
            }
            render_stereo(engine, MAX_BLOCK);
        }
    }

    #[test]
    fn takes_record_the_input_a_pass_at_a_time() {
        let (mut engine, mut handle, mut input) = looping(4);
        // The block waiting to be read is latency too; take it off, to line up exactly.
        handle.send(Command::SetLatencyOffset(-(MAX_BLOCK as i64)));
        handle.send(Command::Record(Take::new(0, 3000)));
        handle.send(Command::Record(Take::new(0, 3000)));
        engine.play();
        feed(&mut engine, &mut input, 0, 512);
        assert!(handle.status().recording());
        feed(&mut engine, &mut input, 512, 5120);
        let pass = |from: usize| (from..from + 2400).map(|i| i as f32).collect::<Vec<_>>();
        let first = handle.recorded().expect("a pass recorded");
        assert!(first.complete);
        assert_eq!(first.left, pass(0));
        assert_eq!(first.right[10], -10.0);
        let second = handle.recorded().expect("and the next straight after");
        assert_eq!(second.left, pass(2400));
        assert!(!handle.status().recording());
    }

    #[test]
    fn latency_records_what_was_played_along_later() {
        let (mut engine, mut handle, mut input) = looping(4);
        handle.send(Command::SetLatencyOffset(100 - MAX_BLOCK as i64));
        handle.send(Command::Record(Take::new(0, 3000)));
        engine.play();
        feed(&mut engine, &mut input, 0, 2816);
        let take = handle.recorded().unwrap();
        assert_eq!(take.left[..2], [100.0, 101.0]);
        assert_eq!(take.len(), 2400);
    }

    #[test]
    fn stopping_ends_recording_unfinished() {
        let (mut engine, mut handle, mut input) = looping(4);
        handle.send(Command::Record(Take::new(0, 3000)));
        engine.play();
        feed(&mut engine, &mut input, 0, 1024);
        handle.send(Command::Stop);
        feed(&mut engine, &mut input, 1024, 256);
        let take = handle.recorded().unwrap();
        assert!(!take.complete);
    }

    #[test]
    fn a_new_sample_plays_on_without_rebuilding() {
        let (mut engine, mut handle) = with_synth(48_000.0);
        let sample = Arc::new(Sample {
            rate: 48_000.0,
            left: vec![0.5; 100],
            right: vec![0.5; 100],
        });
        // A synth plays no file, so it's sent back.
        handle.send(Command::SetSample {
            index: 0,
            sample: Some(sample),
        });
        render_stereo(&mut engine, 64);
        assert!(matches!(handle.garbage.pop(), Ok(Garbage::Sample(Some(_)))));
    }
}
