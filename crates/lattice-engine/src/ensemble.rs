//! An ensemble, as on the string machines of the seventies: the sound written into a short delay
//! line and read back by three heads, each a few milliseconds behind and swept by two LFOs, a
//! slow one for the chorus and one nine times faster for the shimmer, each head a third of a
//! cycle on from the last. The right side's heads sit a sixth of a cycle from the left's, so the
//! two sides waver differently and the sound spreads across the stereo field.

use std::f32::consts::TAU;

use lattice_core::FxKind;

use crate::param::{ParamInfo, Smoothed};
use crate::pitch::read;
use crate::processor::{Event, EventKind, Processor, Segment, segments};

pub use lattice_core::fx::ensemble::{DEPTH, MIX, RATE};

const PARAMS: &[ParamInfo] = FxKind::Ensemble.params();

/// The heads' delay at rest, and how far each LFO swings it at full depth, in seconds.
const BASE: f32 = 0.007;
const SLOW: f32 = 0.003;
const FAST: f32 = 0.0004;
/// How much faster the shimmer goes than the chorus.
const FAST_RATIO: f32 = 9.0;
const HEADS: usize = 3;

pub struct Ensemble {
    sample_rate: f32,
    lines: [Vec<f32>; 2],
    write: usize,
    rate: f32,
    depth: Smoothed,
    mix: Smoothed,
    /// Where the slow and the fast LFOs are, from 0 to 1.
    slow: f32,
    fast: f32,
}

impl Ensemble {
    /// Allocates the delay lines, so build it off the audio thread.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let len = ((BASE + SLOW + FAST) * sample_rate) as usize + 4;
        let mut ensemble = Self {
            sample_rate,
            lines: [vec![0.0; len], vec![0.0; len]],
            write: 0,
            rate: 0.0,
            depth: Smoothed::new(0.0, sample_rate),
            mix: Smoothed::new(0.0, sample_rate),
            slow: 0.0,
            fast: 0.0,
        };
        // Each parameter from `values`, or its default where they stop short.
        for info in PARAMS {
            let value = values.get(info.id as usize).copied();
            ensemble.set(info.id, value.unwrap_or(info.default));
        }
        for s in [&mut ensemble.depth, &mut ensemble.mix] {
            s.reset(s.target());
        }
        ensemble
    }

    fn set(&mut self, id: u32, value: f32) {
        let Some(info) = PARAMS.get(id as usize) else {
            return;
        };
        let value = info.clamp(value);
        match id {
            RATE => self.rate = value,
            DEPTH => self.depth.set(value),
            _ => self.mix.set(value),
        }
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "the lines are the same length, never empty, and `write` is wrapped to it"
    )]
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = self.lines[0].len();
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.slow = (self.slow + self.rate / self.sample_rate).fract();
            self.fast = (self.fast + FAST_RATIO * self.rate / self.sample_rate).fract();
            let depth = self.depth.tick();
            let mix = self.mix.tick();
            let write = self.write;
            for (side, (line, x)) in self.lines.iter_mut().zip([l, r]).enumerate() {
                line[write] = *x;
                let mut wet = 0.0;
                for head in 0..HEADS {
                    let offset = head as f32 / HEADS as f32 + side as f32 / 6.0;
                    let swing = SLOW * (TAU * (self.slow + offset)).sin()
                        + FAST * (TAU * (self.fast + offset)).sin();
                    wet += read(line, write, (BASE + depth * swing) * self.sample_rate);
                }
                wet /= HEADS as f32;
                *x += (wet - *x) * mix;
            }
            self.write = (write + 1) % len;
        }
    }
}

impl Processor for Ensemble {
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

    const RATE_HZ: f32 = 48_000.0;

    fn ensemble(settings: &[(u32, f32)]) -> Ensemble {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        for &(id, value) in settings {
            values[id as usize] = value;
        }
        Ensemble::with_values(RATE_HZ, &values)
    }

    fn sine(hz: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| (TAU * hz * i as f32 / RATE_HZ).sin())
            .collect()
    }

    #[test]
    fn spreads_one_sound_across_both_sides() {
        let mut e = ensemble(&[(MIX, 1.0), (DEPTH, 1.0)]);
        let mut left = sine(440.0, 48_000);
        let mut right = left.clone();
        e.process(&[], &mut left, &mut right);
        let side: f32 = left[4_800..]
            .iter()
            .zip(&right[4_800..])
            .map(|(l, r)| (l - r).powi(2))
            .sum();
        let mid: f32 = left[4_800..].iter().map(|l| l * l).sum();
        assert!(side > 0.05 * mid, "{side} {mid}");
        let peak = left
            .iter()
            .chain(&right)
            .fold(0.0f32, |p, x| p.max(x.abs()));
        assert!(peak <= 1.0 + 1e-3, "{peak}");
    }

    #[test]
    fn mixed_out_it_is_untouched() {
        let mut e = ensemble(&[(MIX, 0.0)]);
        let dry = sine(440.0, 4_800);
        let (mut left, mut right) = (dry.clone(), dry.clone());
        e.process(&[], &mut left, &mut right);
        assert_eq!(left, dry);
    }
}
