//! The sequencer and mixer, driven entirely from the audio callback.
//!
//! The UI thread talks to the engine through a [`Handle`]: commands go over a lock-free queue,
//! and playback position comes back through atomics in [`Status`]. Nothing on the audio side
//! blocks or allocates. Replaced patterns are sent back to the UI thread to be freed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use lattice_core::{Grid, MAX_STEPS, Step, Track, realise_into};
use rtrb::{Consumer, Producer, PushError, RingBuffer};

use crate::click::Click;
use crate::clock::Clock;
use crate::denormal;
use crate::instrument::{Event, EventKind, Instrument};

pub const MAX_TRACKS: usize = 16;
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
    /// Audition a note on a track's instrument.
    Preview {
        track: usize,
        pitch: u8,
        velocity: u8,
    },
}

/// Playback state the audio thread publishes for the UI.
pub struct Status {
    playing: AtomicBool,
    /// Per track: `pass << 32 | step`, or `u64::MAX` when stopped. One atomic, so the step and
    /// pass are always read together.
    positions: [AtomicU64; MAX_TRACKS],
}

impl Status {
    fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            positions: std::array::from_fn(|_| AtomicU64::new(u64::MAX)),
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

    fn set_position(&self, track: usize, step: usize, pass: u32) {
        self.positions[track].store(u64::from(pass) << 32 | step as u64, Ordering::Relaxed);
    }

    fn set_stopped(&self) {
        self.playing.store(false, Ordering::Relaxed);
        for p in &self.positions {
            p.store(u64::MAX, Ordering::Relaxed);
        }
    }
}

/// The UI thread's end of the engine.
pub struct Handle {
    commands: Producer<Command>,
    garbage: Consumer<Box<Track>>,
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

    /// Frees patterns the audio thread has finished with. Call regularly.
    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
    }
}

#[derive(Debug, Clone, Copy)]
struct Held {
    pitch: u8,
    steps_left: u16,
}

struct TrackPlayer {
    track: Track,
    instrument: Box<dyn Instrument>,
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
}

pub struct Engine {
    clock: Clock,
    playing: bool,
    steps_per_beat: u64,
    step_index: u64,
    tracks: Vec<TrackPlayer>,
    click: Click,
    click_events: Vec<Event>,
    click_left: Vec<f32>,
    click_right: Vec<f32>,
    click_enabled: bool,
    preview_samples: usize,
    commands: Consumer<Command>,
    garbage: Producer<Box<Track>>,
    status: Arc<Status>,
}

impl Engine {
    /// `instruments[i]` plays `grid.tracks[i]`. Starts stopped. Allocates, so build it off the
    /// audio thread, then move the engine into the audio callback and keep the handle.
    pub fn new(
        sample_rate: f32,
        bpm: f64,
        grid: &Grid,
        instruments: Vec<Box<dyn Instrument>>,
    ) -> (Self, Handle) {
        assert_eq!(
            grid.tracks.len(),
            instruments.len(),
            "one instrument per track"
        );
        assert!(grid.tracks.len() <= MAX_TRACKS, "too many tracks");
        assert!(grid.tracks.iter().all(|t| t.steps.len() <= MAX_STEPS));

        let tracks = grid
            .tracks
            .iter()
            .zip(instruments)
            .map(|(track, instrument)| TrackPlayer {
                track: track.clone(),
                instrument,
                steps: Vec::with_capacity(MAX_STEPS),
                playhead: 0,
                pass: 0,
                events: Vec::with_capacity(MAX_EVENTS),
                held: Vec::with_capacity(MAX_HELD),
                preview: None,
                left: vec![0.0; MAX_BLOCK],
                right: vec![0.0; MAX_BLOCK],
            })
            .collect();
        let (command_tx, command_rx) = RingBuffer::new(COMMAND_QUEUE);
        let (garbage_tx, garbage_rx) = RingBuffer::new(COMMAND_QUEUE);
        let status = Arc::new(Status::new());

        let engine = Self {
            clock: Clock::new(f64::from(sample_rate), bpm, grid.steps_per_beat),
            playing: false,
            steps_per_beat: u64::from(grid.steps_per_beat.max(1)),
            step_index: 0,
            tracks,
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
                    if let Err(PushError::Full(track)) = self.garbage.push(track) {
                        // Never free on the audio thread; leaking is the lesser evil.
                        std::mem::forget(track);
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
            }
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
        for t in &mut self.tracks {
            sort_by_offset(&mut t.events);
            let (tl, tr) = (&mut t.left[..len], &mut t.right[..len]);
            t.instrument.process(&t.events, tl, tr);
            t.events.clear();
            mix_into(left, tl);
            mix_into(right, tr);
        }
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
            self.status.set_position(index, t.playhead, t.pass);
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
        let synth: Box<dyn Instrument> = Box::new(Synth::new(patch, sample_rate));
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
