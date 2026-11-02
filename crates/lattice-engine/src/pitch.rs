//! A pitch shifter, the classic delay-line kind: the sound is written into a short delay line and
//! read back by two heads, each sweeping the delay across a window, faster or slower than the
//! writing, which shifts the pitch. As one head jumps back to the start of the window, the other
//! is halfway across, and each fades in and out (a squared sine, so the two always sum to one), so
//! the jumps aren't heard.
//!
//! It smears transients a little, being made of overlapping grains, which suits what it's for:
//! the octave in a shimmer reverb's feedback.

use std::f32::consts::PI;

use lattice_core::FxKind;

use crate::param::{ParamInfo, Smoothed};
use crate::processor::{Event, EventKind, Processor, Segment, segments};

pub use lattice_core::fx::pitch::{FINE, MAX_WINDOW, MIX, PITCH, WINDOW};

const PARAMS: &[ParamInfo] = FxKind::Pitch.params();

/// The shortest delay a head reads at, in samples, so it never reads where it's being written.
const MIN_DELAY: f32 = 1.0;

pub struct Pitch {
    sample_rate: f32,
    lines: [Vec<f32>; 2],
    write: usize,
    /// In semitones, the pitch and fine together.
    shift: Smoothed,
    pitch: f32,
    fine: f32,
    /// In samples.
    window: Smoothed,
    mix: Smoothed,
    /// Where the first head is across the window, from 0 to 1; the second is half a window on.
    phase: f32,
}

impl Pitch {
    /// Allocates the delay lines, so build it off the audio thread.
    pub fn new(sample_rate: f32) -> Self {
        let len = (MAX_WINDOW * sample_rate) as usize + 4;
        let mut pitch = Self {
            sample_rate,
            lines: [vec![0.0; len], vec![0.0; len]],
            write: 0,
            shift: Smoothed::new(0.0, sample_rate),
            pitch: 0.0,
            fine: 0.0,
            window: Smoothed::new(0.0, sample_rate),
            mix: Smoothed::new(0.0, sample_rate),
            phase: 0.0,
        };
        for info in PARAMS {
            pitch.set(info.id, info.default);
        }
        pitch.settle();
        pitch
    }

    /// Starts at `values` (one per parameter, by id) rather than gliding to them.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let mut pitch = Self::new(sample_rate);
        for (id, &value) in values.iter().enumerate() {
            pitch.set(id as u32, value);
        }
        pitch.settle();
        pitch
    }

    fn settle(&mut self) {
        for smoothed in [&mut self.shift, &mut self.window, &mut self.mix] {
            smoothed.reset(smoothed.target());
        }
    }

    fn set(&mut self, id: u32, value: f32) {
        let Some(info) = PARAMS.get(id as usize) else {
            return;
        };
        let value = info.clamp(value);
        match id {
            PITCH => self.pitch = value.round(),
            FINE => self.fine = value,
            WINDOW => self.window.set(value * self.sample_rate),
            _ => self.mix.set(value),
        }
        self.shift.set(self.pitch + self.fine / 100.0);
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "the lines are the same length, never empty, and `write` is wrapped to it"
    )]
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = self.lines[0].len();
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let ratio = (self.shift.tick() / 12.0).exp2();
            let window = self.window.tick();
            let mix = self.mix.tick();
            // A head reading faster than the writing closes in on it, so its delay shrinks.
            self.phase = (self.phase + (1.0 - ratio) / window).rem_euclid(1.0);
            let heads = [self.phase, (self.phase + 0.5) % 1.0];
            let write = self.write;
            for (line, x) in self.lines.iter_mut().zip([l, r]) {
                line[write] = *x;
                let shifted: f32 = heads
                    .iter()
                    .map(|&h| (PI * h).sin().powi(2) * read(line, write, MIN_DELAY + h * window))
                    .sum();
                *x += (shifted - *x) * mix;
            }
            self.write = (write + 1) % len;
        }
    }
}

/// `line`, `delay` samples back from the last written, at `write`, between samples.
#[inline]
#[expect(
    clippy::indexing_slicing,
    reason = "`at` and `before` are wrapped to the line's length, and the line's never empty"
)]
fn read(line: &[f32], write: usize, delay: f32) -> f32 {
    let len = line.len();
    let back = delay.floor();
    let frac = delay - back;
    let at = (write + len - back as usize % len) % len;
    let before = (at + len - 1) % len;
    line[at] + (line[before] - line[at]) * frac
}

impl Processor for Pitch {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.render(l, r),
            Segment::Event(event) => {
                if let EventKind::Param { id, value } = event.kind {
                    self.set(id, value);
                }
            }
        });
    }

    fn params(&self) -> &[ParamInfo] {
        PARAMS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const RATE: f32 = 48_000.0;

    /// Half a second of a sine at `hz` through a shifter `semitones` up, as the left channel.
    fn shifted(hz: f32, semitones: f32) -> Vec<f32> {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        values[PITCH as usize] = semitones;
        let mut pitch = Pitch::with_values(RATE, &values);
        let len = RATE as usize / 2;
        let mut left: Vec<f32> = (0..len)
            .map(|i| (TAU * hz * i as f32 / RATE).sin())
            .collect();
        let mut right = left.clone();
        pitch.process(&[], &mut left, &mut right);
        assert_eq!(left, right);
        left
    }

    /// Its frequency, by counting upward zero crossings after the first window.
    fn frequency(audio: &[f32]) -> f32 {
        let settled = &audio[(0.1 * RATE) as usize..];
        let ups = settled.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0);
        ups.count() as f32 / (settled.len() as f32 / RATE)
    }

    #[test]
    fn an_octave_up_doubles_the_frequency() {
        let up = frequency(&shifted(220.0, 12.0));
        assert!((up - 440.0).abs() < 440.0 * 0.03, "{up}");
        let down = frequency(&shifted(440.0, -12.0));
        assert!((down - 220.0).abs() < 220.0 * 0.03, "{down}");
    }

    #[test]
    fn unshifted_it_passes_the_sound_through() {
        let audio = shifted(220.0, 0.0);
        assert!((frequency(&audio) - 220.0).abs() < 220.0 * 0.03);
        let peak = audio[RATE as usize / 4..]
            .iter()
            .fold(0.0f32, |m, x| m.max(x.abs()));
        assert!((peak - 1.0).abs() < 0.05, "{peak}");
    }
}
