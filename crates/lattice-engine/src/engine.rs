//! The sequencer and mixer, driven entirely from the audio callback.

use lattice_core::{Grid, Step, Track, realise_into};

use crate::click::Click;
use crate::clock::Clock;
use crate::denormal;
use crate::instrument::{Event, EventKind, Instrument};

const BEATS_PER_BAR: u64 = 4;
/// Largest block rendered in one go. Longer host buffers are split.
const MAX_BLOCK: usize = 256;
/// Events per track per block; more are dropped rather than allocating.
const MAX_EVENTS: usize = 128;
/// Notes a track can hold at once before the oldest is cut.
const MAX_HELD: usize = 16;
const CLICK_PITCH: u8 = 84;
const CLICK_ACCENT_PITCH: u8 = 96;

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
    left: Vec<f32>,
    right: Vec<f32>,
}

pub struct Engine {
    clock: Clock,
    steps_per_beat: u64,
    step_index: u64,
    tracks: Vec<TrackPlayer>,
    click: Click,
    click_events: Vec<Event>,
    click_left: Vec<f32>,
    click_right: Vec<f32>,
    pub click_enabled: bool,
}

impl Engine {
    /// `instruments[i]` plays `grid.tracks[i]`. Allocates, so build it off the audio thread.
    pub fn new(
        sample_rate: f32,
        bpm: f64,
        grid: &Grid,
        instruments: Vec<Box<dyn Instrument>>,
    ) -> Self {
        assert_eq!(
            grid.tracks.len(),
            instruments.len(),
            "one instrument per track"
        );
        let tracks = grid
            .tracks
            .iter()
            .zip(instruments)
            .map(|(track, instrument)| TrackPlayer {
                track: track.clone(),
                instrument,
                steps: Vec::with_capacity(track.steps.len()),
                playhead: 0,
                pass: 0,
                events: Vec::with_capacity(MAX_EVENTS),
                held: Vec::with_capacity(MAX_HELD),
                left: vec![0.0; MAX_BLOCK],
                right: vec![0.0; MAX_BLOCK],
            })
            .collect();
        Self {
            clock: Clock::new(f64::from(sample_rate), bpm, grid.steps_per_beat),
            steps_per_beat: u64::from(grid.steps_per_beat.max(1)),
            step_index: 0,
            tracks,
            click: Click::new(sample_rate),
            click_events: Vec::with_capacity(MAX_EVENTS),
            click_left: vec![0.0; MAX_BLOCK],
            click_right: vec![0.0; MAX_BLOCK],
            click_enabled: true,
        }
    }

    /// Renders the next stretch of stereo audio. Real-time safe: no allocation, no locks.
    ///
    /// Steps become events stamped with their exact sample offset, so timing is sample-accurate
    /// whatever the buffer size.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        denormal::flush_to_zero();
        for (l, r) in left.chunks_mut(MAX_BLOCK).zip(right.chunks_mut(MAX_BLOCK)) {
            self.process_block(l, r);
        }
    }

    fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len();
        for t in &mut self.tracks {
            t.events.clear();
        }
        self.click_events.clear();

        // Sequence: turn every step starting in this block into events.
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

        // Render and mix.
        left.fill(0.0);
        right.fill(0.0);
        for t in &mut self.tracks {
            let (tl, tr) = (&mut t.left[..len], &mut t.right[..len]);
            t.instrument.process(&t.events, tl, tr);
            mix_into(left, tl);
            mix_into(right, tr);
        }
        let (cl, cr) = (&mut self.click_left[..len], &mut self.click_right[..len]);
        self.click.process(&self.click_events, cl, cr);
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
            push(
                &mut self.click_events,
                Event {
                    offset,
                    kind: EventKind::NoteOn {
                        pitch,
                        velocity: 100,
                    },
                },
            );
        }
        self.step_index += 1;

        for t in &mut self.tracks {
            if t.track.steps.is_empty() {
                continue;
            }
            // Note-offs first, so a voice can be reused by this step's note.
            t.held.retain_mut(|h| {
                h.steps_left -= 1;
                if h.steps_left == 0 {
                    push(&mut t.events, note_off(offset, h.pitch));
                }
                h.steps_left > 0
            });

            if t.playhead == 0 {
                realise_into(&t.track, t.pass, &mut t.steps);
            }
            if let Some(note) = t.steps[t.playhead].note {
                // Retriggering a held pitch, or out of room: end the old note first.
                let existing = t.held.iter().position(|h| h.pitch == note.pitch);
                let full = (t.held.len() == MAX_HELD).then_some(0);
                if let Some(i) = existing.or(full) {
                    push(&mut t.events, note_off(offset, t.held[i].pitch));
                    t.held.remove(i);
                }
                push(
                    &mut t.events,
                    Event {
                        offset,
                        kind: EventKind::NoteOn {
                            pitch: note.pitch,
                            velocity: note.velocity,
                        },
                    },
                );
                t.held.push(Held {
                    pitch: note.pitch,
                    steps_left: note.length.max(1),
                });
            }

            t.playhead += 1;
            if t.playhead == t.steps.len() {
                t.playhead = 0;
                t.pass += 1;
            }
        }
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
        Engine::new(sample_rate, bpm, &grid, vec![])
    }

    fn with_synth(sample_rate: f32) -> Engine {
        let track = Track {
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
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![track],
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
        Engine::new(sample_rate, 133.0, &grid, vec![synth])
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
        let reference = render(&mut with_synth(48_000.0), 1, 100_000);
        assert!(reference.iter().any(|x| x.abs() > 0.05));
        for block in [7, 64, 333, 4096] {
            assert_eq!(
                render(&mut with_synth(48_000.0), block, 100_000),
                reference,
                "block {block}"
            );
        }
    }
}
