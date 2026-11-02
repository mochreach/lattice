//! The sampler: plays a sound file, loaded off the audio thread and shared with every sampler
//! playing it. See [`lattice_core::instrument::sampler`] for its settings.

use std::sync::Arc;

use lattice_core::instrument::sampler::*;
use lattice_core::{DRUM_ROOT, InstKind};

use crate::param::{ParamInfo, Smoothed};
use crate::processor::{Event, EventKind, Processor, Segment, Shape, segments};

/// How many parameters it has.
const COUNT: usize = InstKind::Sampler.params().len();

/// A sound file's audio, in stereo.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    /// The rate it was recorded at, which may differ from the engine's.
    pub rate: f32,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl Sample {
    pub fn len(&self) -> usize {
        self.left.len()
    }

    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }

    /// Between frames, by cubic (Hermite) interpolation, silent outside the file.
    #[inline]
    fn at(&self, pos: f64) -> (f32, f32) {
        let i = pos.floor() as isize;
        let t = (pos - i as f64) as f32;
        let get = |ch: &[f32], j: isize| {
            if j < 0 {
                0.0
            } else {
                ch.get(j as usize).copied().unwrap_or(0.0)
            }
        };
        let hermite = |ch: &[f32]| {
            let (y0, y1, y2, y3) = (get(ch, i - 1), get(ch, i), get(ch, i + 1), get(ch, i + 2));
            let c1 = 0.5 * (y2 - y0);
            let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
            let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
            ((c3 * t + c2) * t + c1) * t + y1
        };
        (hermite(&self.left), hermite(&self.right))
    }
}

const VOICES: usize = 16;
/// Faded over this at the region's ends and on a choke, so nothing clicks.
const FADE_SECONDS: f32 = 0.002;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Idle,
    Playing,
    Releasing,
}

#[derive(Debug, Clone, Copy)]
struct Voice {
    stage: Stage,
    pitch: u8,
    /// When it started, for stealing the oldest.
    started: u64,
    /// Where in the file, in frames, and how far it moves each output sample (negative in
    /// reverse).
    pos: f64,
    rate: f64,
    /// The region it plays, in frames.
    from: f64,
    to: f64,
    looping: bool,
    gain: f32,
    /// The envelope's level, rising by `attack` a sample and falling by `release`.
    level: f32,
    attack: f32,
    release: f32,
}

impl Voice {
    const IDLE: Voice = Voice {
        stage: Stage::Idle,
        pitch: 0,
        started: 0,
        pos: 0.0,
        rate: 1.0,
        from: 0.0,
        to: 0.0,
        looping: false,
        gain: 0.0,
        level: 0.0,
        attack: 1.0,
        release: 1.0,
    };
}

/// A note to play from a file.
pub(crate) struct Play {
    pub pitch: u8,
    /// The region, in frames: played from `from`, or back from `to` if `rate` is negative.
    pub from: f64,
    pub to: f64,
    /// Frames moved an output sample.
    pub rate: f64,
    pub looping: bool,
    pub gain: f32,
    /// Seconds.
    pub attack: f32,
    pub release: f32,
}

/// Voices playing a sound file: the sampler's and chop's.
pub(crate) struct Player {
    sample_rate: f32,
    pub sample: Option<Arc<Sample>>,
    voices: [Voice; VOICES],
    notes: u64,
}

impl Player {
    pub fn new(sample_rate: f32, sample: Option<Arc<Sample>>) -> Self {
        Self {
            sample_rate,
            sample: sample.filter(|s| !s.is_empty()),
            voices: [Voice::IDLE; VOICES],
            notes: 0,
        }
    }

    /// Plays `sample` in place of the one it was, which `sample` then holds. Notes sounding carry
    /// on at the same place in the new one. Send `None` rather than an empty file.
    pub fn swap(&mut self, sample: &mut Option<Arc<Sample>>) {
        std::mem::swap(&mut self.sample, sample);
    }

    /// How far the file moves an output sample at its own pitch.
    pub fn speed(&self) -> f64 {
        self.sample
            .as_ref()
            .map_or(1.0, |s| f64::from(s.rate / self.sample_rate))
    }

    pub fn start(&mut self, play: Play) {
        if play.to - play.from < 2.0 {
            return;
        }
        let per_sample = |seconds: f32| 1.0 / (seconds * self.sample_rate).max(1.0);
        // A free voice, else the oldest releasing, else the oldest. The first of equals wins.
        let free = |v: &Voice| match v.stage {
            Stage::Idle => (0, 0),
            Stage::Releasing => (1, v.started),
            Stage::Playing => (2, v.started),
        };
        // There are always voices, so there's always one to take.
        let Some(voice) = self.voices.iter_mut().min_by_key(|v| free(v)) else {
            return;
        };
        self.notes += 1;
        *voice = Voice {
            stage: Stage::Playing,
            pitch: play.pitch,
            started: self.notes,
            pos: if play.rate < 0.0 {
                play.to - 1.0
            } else {
                play.from
            },
            rate: play.rate,
            from: play.from,
            to: play.to,
            looping: play.looping,
            gain: play.gain,
            level: 0.0,
            attack: per_sample(play.attack),
            release: per_sample(play.release),
        };
    }

    /// Lets go of the voices playing `pitch`.
    pub fn release(&mut self, pitch: u8) {
        for v in &mut self.voices {
            if v.stage == Stage::Playing && v.pitch == pitch {
                v.stage = Stage::Releasing;
            }
        }
    }

    pub fn choke(&mut self) {
        let fade = 1.0 / (FADE_SECONDS * self.sample_rate).max(1.0);
        for v in &mut self.voices {
            if v.stage != Stage::Idle {
                v.stage = Stage::Releasing;
                v.release = v.release.max(fade);
            }
        }
    }

    pub fn render(&mut self, gain: &mut Smoothed, left: &mut [f32], right: &mut [f32]) {
        let Some(sample) = &self.sample else {
            return;
        };
        let fade = f64::from(FADE_SECONDS * self.sample_rate);
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let gain = gain.tick();
            let (mut sum_l, mut sum_r) = (0.0, 0.0);
            for v in &mut self.voices {
                if v.stage == Stage::Idle {
                    continue;
                }
                if v.looping {
                    // Round the region, in either direction.
                    let span = v.to - v.from;
                    if v.pos >= v.to {
                        v.pos -= span;
                    } else if v.pos < v.from {
                        v.pos += span;
                    }
                } else if v.pos >= v.to || v.pos < v.from {
                    v.stage = Stage::Idle;
                    continue;
                }
                v.level = match v.stage {
                    Stage::Playing => (v.level + v.attack).min(1.0),
                    _ => v.level - v.release,
                };
                if v.level <= 0.0 && v.stage == Stage::Releasing {
                    v.stage = Stage::Idle;
                    continue;
                }
                // Faded in and out at the region's ends, unless it loops round them.
                let edge = if v.looping {
                    1.0
                } else {
                    let left_to_play = if v.rate > 0.0 {
                        v.to - v.pos
                    } else {
                        v.pos - v.from
                    } / v.rate.abs();
                    (left_to_play / fade).min(1.0) as f32
                };
                let (a, b) = sample.at(v.pos);
                let g = v.gain * v.level * edge;
                sum_l += a * g;
                sum_r += b * g;
                v.pos += v.rate;
            }
            *l = sum_l * gain;
            *r = sum_r * gain;
        }
    }
}

pub struct Sampler {
    player: Player,
    values: [f32; COUNT],
    gain: Smoothed,
    /// How the next note plays.
    shape: Shape,
}

impl Sampler {
    pub fn new(sample_rate: f32, values: &[f32], sample: Option<Arc<Sample>>) -> Self {
        let mut all = [0.0; COUNT];
        for (v, info) in all.iter_mut().zip(InstKind::Sampler.params()) {
            *v = info.default;
        }
        for (v, &value) in all.iter_mut().zip(values) {
            *v = value;
        }
        let mut sampler = Self {
            player: Player::new(sample_rate, sample),
            gain: Smoothed::new(0.0, sample_rate),
            values: all,
            shape: Shape::default(),
        };
        sampler.gain.reset(sampler.value(GAIN));
        sampler
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "only ever given the sampler's own parameter ids, constants below COUNT"
    )]
    fn value(&self, id: u32) -> f32 {
        self.values[id as usize]
    }

    fn note_on(&mut self, pitch: u8, velocity: u8) {
        let Some(sample) = &self.player.sample else {
            return;
        };
        let len = sample.len() as f64;
        let (mut from, mut to) = (
            f64::from(self.value(START)) * len,
            f64::from(self.value(END)) * len,
        );
        if to < from {
            std::mem::swap(&mut from, &mut to);
        }
        let slices = self.value(SLICES).round() as i32 + 1;
        let offset = i32::from(pitch) - i32::from(DRUM_ROOT);
        let semitones = if slices > 1 {
            // Each note a slice, wrapping round, at the sample's own pitch.
            let width = (to - from) / f64::from(slices);
            from += width * f64::from(offset.rem_euclid(slices));
            to = from + width;
            0.0
        } else {
            offset as f32
        } + self.value(TUNE);
        let mut rate = f64::from(2f32.powf(semitones / 12.0)) * self.player.speed();
        let shape = std::mem::take(&mut self.shape);
        // Started part way in, at the far end of what it plays.
        let into = (to - from) * shape.along();
        if (self.value(DIRECTION) >= 0.5) != shape.reverse {
            rate = -rate;
            to -= into;
        } else {
            from += into;
        }
        self.player.start(Play {
            pitch,
            from,
            to,
            rate,
            looping: self.value(MODE).round() as u32 == 2,
            gain: f32::from(velocity) / 127.0,
            attack: self.value(ATTACK),
            release: self.value(RELEASE),
        });
    }

    fn note_off(&mut self, pitch: u8) {
        // A one-shot plays on to the end of its region.
        if self.value(MODE).round() as u32 != 0 {
            self.player.release(pitch);
        }
    }
}

impl Processor for Sampler {
    fn swap_sample(&mut self, sample: &mut Option<Arc<Sample>>) -> bool {
        self.player.swap(sample);
        true
    }

    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.player.render(&mut self.gain, l, r),
            Segment::Event(event) => match event.kind {
                EventKind::NoteOn { pitch, velocity } => self.note_on(pitch, velocity),
                EventKind::NoteOff { pitch } => self.note_off(pitch),
                EventKind::Choke => self.player.choke(),
                EventKind::Shape(shape) => self.shape = shape,
                EventKind::Param { id, value } => {
                    let info = InstKind::Sampler.params().get(id as usize);
                    if let (Some(info), Some(slot)) = (info, self.values.get_mut(id as usize)) {
                        *slot = info.clamp(value);
                        if id == GAIN {
                            self.gain.set(*slot);
                        }
                    }
                }
            },
        });
    }

    fn params(&self) -> &[ParamInfo] {
        InstKind::Sampler.params()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::Instrument;

    /// A ramp from 0 to 1 over `len` frames, at 1 kHz.
    fn ramp(len: usize) -> Arc<Sample> {
        let left: Vec<f32> = (0..len).map(|i| i as f32 / len as f32).collect();
        Arc::new(Sample {
            rate: 1_000.0,
            right: left.clone(),
            left,
        })
    }

    fn sampler(settings: &[(u32, f32)], sample: Arc<Sample>) -> Sampler {
        let mut instrument = Instrument::new(InstKind::Sampler);
        instrument.values[GAIN as usize] = 1.0;
        for &(id, value) in settings {
            instrument.values[id as usize] = value;
        }
        Sampler::new(1_000.0, &instrument.values, Some(sample))
    }

    fn play(s: &mut Sampler, pitch: u8, frames: usize) -> Vec<f32> {
        let mut left = vec![0.0; frames];
        let mut right = vec![0.0; frames];
        let on = Event {
            offset: 0,
            kind: EventKind::NoteOn {
                pitch,
                velocity: 127,
            },
        };
        s.process(&[on], &mut left, &mut right);
        left
    }

    #[test]
    fn plays_the_file_at_its_pitch_from_c4() {
        let mut s = sampler(&[], ramp(100));
        let out = play(&mut s, DRUM_ROOT, 120);
        assert!((out[50] - 0.5).abs() < 0.01, "{}", out[50]);
        assert_eq!(out[110], 0.0, "and stops at the end");
        // An octave up, twice as fast.
        let out = play(&mut sampler(&[], ramp(100)), DRUM_ROOT + 12, 60);
        assert!((out[25] - 0.5).abs() < 0.01);
        assert_eq!(out[55], 0.0);
    }

    #[test]
    fn slices_play_one_to_a_note() {
        let mut s = sampler(&[(SLICES, 3.0)], ramp(100));
        // The third of four slices: from 50 to 75.
        let out = play(&mut s, DRUM_ROOT + 2, 40);
        assert!((out[10] - 0.6).abs() < 0.01, "{}", out[10]);
        assert_eq!(out[30], 0.0);
    }

    #[test]
    fn reverse_plays_from_the_end() {
        let mut s = sampler(&[(DIRECTION, 1.0), (START, 0.5)], ramp(100));
        let out = play(&mut s, DRUM_ROOT, 60);
        assert!((out[10] - 0.89).abs() < 0.01, "{}", out[10]);
        assert_eq!(out[55], 0.0, "stops at the start");
    }

    #[test]
    fn a_notes_shape_turns_it_round_or_starts_it_in_for_that_note_alone() {
        let shaped = |s: &mut Sampler, shape: Shape| {
            let event = Event {
                offset: 0,
                kind: EventKind::Shape(shape),
            };
            s.process(&[event], &mut [], &mut []);
        };
        let mut s = sampler(&[], ramp(100));
        // A quarter of the way in, forwards.
        let offset = Shape {
            offset: 4,
            ..Shape::default()
        };
        shaped(&mut s, offset);
        let out = play(&mut s, DRUM_ROOT, 100);
        assert!((out[10] - 0.35).abs() < 0.01, "{}", out[10]);
        assert_eq!(out[80], 0.0, "and stops at the end");
        // The next, the first having ended, starts at the top again.
        let out = play(&mut s, DRUM_ROOT, 20);
        assert!((out[10] - 0.1).abs() < 0.01, "{}", out[10]);

        // Backwards, from a quarter of the way back from the end; or forwards, the instrument
        // being set to reverse.
        let back = Shape {
            reverse: true,
            ..offset
        };
        let mut s = sampler(&[], ramp(100));
        shaped(&mut s, back);
        let out = play(&mut s, DRUM_ROOT, 100);
        assert!((out[10] - 0.64).abs() < 0.01, "{}", out[10]);
        let mut s = sampler(&[(DIRECTION, 1.0)], ramp(100));
        shaped(&mut s, back);
        let out = play(&mut s, DRUM_ROOT, 20);
        assert!((out[10] - 0.35).abs() < 0.01, "{}", out[10]);
    }

    #[test]
    fn loops_while_held_and_gates_stop_on_release() {
        let mut s = sampler(&[(MODE, 2.0), (END, 0.2)], ramp(100));
        let out = play(&mut s, DRUM_ROOT, 100);
        assert!(out[95] > 0.0, "still going round");
        let mut s = sampler(&[(MODE, 1.0), (RELEASE, 0.005)], ramp(100));
        play(&mut s, DRUM_ROOT, 10);
        let (mut l, mut r) = (vec![0.0; 20], vec![0.0; 20]);
        let off = Event {
            offset: 0,
            kind: EventKind::NoteOff { pitch: DRUM_ROOT },
        };
        s.process(&[off], &mut l, &mut r);
        assert_eq!(l[19], 0.0, "released");
    }

    #[test]
    fn without_a_file_its_silent() {
        let mut s = Sampler::new(1_000.0, &[], None);
        assert!(play(&mut s, DRUM_ROOT, 10).iter().all(|&x| x == 0.0));
    }
}
