//! Chop: a sound file cut into equal pieces, one to a step. Each note plays its piece from where
//! it starts until the note ends, so a loop follows the tempo without stretching. See
//! [`lattice_core::instrument::chop`] for its settings.

use std::sync::Arc;

use lattice_core::InstKind;
use lattice_core::instrument::chop::*;

use crate::param::{ParamInfo, Smoothed};
use crate::processor::{Event, EventKind, Processor, Segment, Shape, segments};
use crate::sampler::{Play, Player, Sample};

/// How many parameters it has.
const COUNT: usize = InstKind::Chop.params().len();

/// A piece fades in over this, so it never clicks on.
const ATTACK_SECONDS: f32 = 0.000_5;

pub struct Chop {
    player: Player,
    values: [f32; COUNT],
    gain: Smoothed,
    /// How the next note plays.
    shape: Shape,
}

impl Chop {
    pub fn new(sample_rate: f32, values: &[f32], sample: Option<Arc<Sample>>) -> Self {
        let mut all = [0.0; COUNT];
        for (v, info) in all.iter_mut().zip(InstKind::Chop.params()) {
            *v = info.default;
        }
        for (v, &value) in all.iter_mut().zip(values) {
            *v = value;
        }
        let mut chop = Self {
            player: Player::new(sample_rate, sample),
            gain: Smoothed::new(0.0, sample_rate),
            values: all,
            shape: Shape::default(),
        };
        chop.gain.reset(chop.value(GAIN));
        chop
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "only ever given the chop's own parameter ids, constants below COUNT"
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
        let pieces = self.value(PIECES).round().max(1.0) as usize;
        let width = (to - from) / pieces as f64;
        let start = from + width * piece(pitch, pieces) as f64;
        let rate = f64::from(2f32.powf(self.value(TUNE) / 12.0)) * self.player.speed();
        let shape = std::mem::take(&mut self.shape);
        let into = width * shape.along();
        // Played on past the piece's end while the note lasts, to the region's; backwards from
        // its end towards the region's start. An offset starts it that far into the piece.
        let play = if (self.value(DIRECTION) >= 0.5) != shape.reverse {
            Play {
                from,
                to: start + width - into,
                rate: -rate,
                ..self.play(pitch, velocity)
            }
        } else {
            Play {
                from: start + into,
                to,
                rate,
                ..self.play(pitch, velocity)
            }
        };
        self.player.start(play);
    }

    /// What every note plays with.
    fn play(&self, pitch: u8, velocity: u8) -> Play {
        Play {
            pitch,
            from: 0.0,
            to: 0.0,
            rate: 1.0,
            looping: false,
            gain: f32::from(velocity) / 127.0,
            attack: ATTACK_SECONDS,
            release: self.value(RELEASE),
            join: true,
        }
    }
}

impl Processor for Chop {
    fn swap_sample(&mut self, sample: &mut Option<Arc<Sample>>) -> bool {
        self.player.swap(sample);
        true
    }

    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.player.render(&mut self.gain, l, r),
            Segment::Event(event) => match event.kind {
                EventKind::NoteOn { pitch, velocity } => self.note_on(pitch, velocity),
                EventKind::NoteOff { pitch } => self.player.release(pitch),
                EventKind::Choke => self.player.choke(),
                EventKind::Shape(shape) => self.shape = shape,
                EventKind::Program(_) => {}
                EventKind::Param { id, value } => {
                    let info = InstKind::Chop.params().get(id as usize);
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
        InstKind::Chop.params()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{DRUM_ROOT, Instrument};

    /// A ramp from 0 to 1 over `len` frames, at 1 kHz.
    fn ramp(len: usize) -> Arc<Sample> {
        let left: Vec<f32> = (0..len).map(|i| i as f32 / len as f32).collect();
        Arc::new(Sample {
            rate: 1_000.0,
            right: left.clone(),
            left,
        })
    }

    fn chop(settings: &[(u32, f32)]) -> Chop {
        let mut instrument = Instrument::new(InstKind::Chop);
        instrument.values[GAIN as usize] = 1.0;
        instrument.values[PIECES as usize] = 4.0;
        for &(id, value) in settings {
            instrument.values[id as usize] = value;
        }
        Chop::new(1_000.0, &instrument.values, Some(ramp(100)))
    }

    fn event(kind: EventKind) -> Event {
        Event { offset: 0, kind }
    }

    /// Plays `pitch` for `held` frames, then lets go, rendering `frames` in all.
    fn play(c: &mut Chop, pitch: u8, held: usize, frames: usize) -> Vec<f32> {
        let mut left = vec![0.0; frames];
        let mut right = vec![0.0; frames];
        let on = event(EventKind::NoteOn {
            pitch,
            velocity: 127,
        });
        let off = Event {
            offset: held as u32,
            kind: EventKind::NoteOff { pitch },
        };
        c.process(&[on, off], &mut left, &mut right);
        left
    }

    #[test]
    fn each_note_plays_its_piece_from_its_start() {
        // The third of four pieces starts at 50.
        let out = play(&mut chop(&[]), DRUM_ROOT + 2, 25, 40);
        assert!((out[5] - 0.55).abs() < 0.01, "{}", out[5]);
        assert_eq!(out[39], 0.0, "and stops once the note's ended");
        // Wrapping round: the fifth note is the first piece again.
        let out = play(&mut chop(&[]), DRUM_ROOT + 4, 25, 10);
        assert!((out[5] - 0.05).abs() < 0.01, "{}", out[5]);
    }

    #[test]
    fn a_longer_note_plays_on_into_the_next_piece() {
        let out = play(&mut chop(&[]), DRUM_ROOT, 50, 60);
        assert!((out[40] - 0.4).abs() < 0.01, "{}", out[40]);
    }

    #[test]
    fn reversed_it_plays_its_piece_backwards_from_its_end() {
        let out = play(&mut chop(&[(DIRECTION, 1.0)]), DRUM_ROOT + 1, 25, 30);
        assert!((out[5] - 0.44).abs() < 0.01, "{}", out[5]);
    }

    #[test]
    fn a_notes_shape_turns_its_piece_round_or_starts_it_in() {
        let mut c = chop(&[]);
        let shape = Shape {
            offset: 8,
            ..Shape::default()
        };
        // Half way into the second piece, from 37.5.
        c.process(&[event(EventKind::Shape(shape))], &mut [], &mut []);
        let out = play(&mut c, DRUM_ROOT + 1, 25, 30);
        assert!((out[5] - 0.425).abs() < 0.01, "{}", out[5]);
        // Back from half way.
        let back = Shape {
            reverse: true,
            ..shape
        };
        c.process(&[event(EventKind::Shape(back))], &mut [], &mut []);
        let out = play(&mut c, DRUM_ROOT + 1, 25, 30);
        assert!((out[5] - 0.325).abs() < 0.01, "{}", out[5]);
        // And the next as it is.
        let out = play(&mut c, DRUM_ROOT + 1, 25, 30);
        assert!((out[5] - 0.3).abs() < 0.01, "{}", out[5]);
    }

    #[test]
    fn pieces_one_after_the_other_play_as_one_sound_rather_than_over_each_other() {
        let mut c = chop(&[]);
        let mut left = vec![0.0; 50];
        let mut right = vec![0.0; 50];
        let on = |offset, pitch| Event {
            offset,
            kind: EventKind::NoteOn {
                pitch,
                velocity: 127,
            },
        };
        let off = |offset, pitch| Event {
            offset,
            kind: EventKind::NoteOff { pitch },
        };
        let events = [on(0, DRUM_ROOT), off(25, DRUM_ROOT), on(25, DRUM_ROOT + 1)];
        c.process(&events, &mut left, &mut right);
        // The ramp goes on through the second piece's start, not doubled as the first fades.
        for (i, x) in left.iter().enumerate().skip(5) {
            assert!((x - i as f32 / 100.0).abs() < 0.015, "{i}: {x}");
        }
    }

    #[test]
    fn a_repeated_piece_starts_again() {
        let mut c = chop(&[]);
        play(&mut c, DRUM_ROOT, 25, 25);
        let out = play(&mut c, DRUM_ROOT, 25, 10);
        assert!((out[5] - 0.05).abs() < 0.01, "{}", out[5]);
    }

    #[test]
    fn without_a_file_its_silent() {
        let mut c = Chop::new(1_000.0, &[], None);
        assert!(play(&mut c, DRUM_ROOT, 5, 10).iter().all(|&x| x == 0.0));
    }
}
