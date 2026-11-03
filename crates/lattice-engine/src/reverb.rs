//! A plate reverb after Jon Dattorro's ("Effect Design, Part 1", 1997): the input is low-passed,
//! smeared by four all-passes, and fed into a tank of two halves, each feeding the other, which
//! rings as it goes round. The output is tapped from several points in the tank, differently for
//! each side, which spreads it in stereo.

use std::f32::consts::TAU;

use lattice_core::FxKind;

use crate::param::{ParamInfo, Smoothed};
use crate::processor::{Event, EventKind, Processor, Segment, segments};

pub use lattice_core::fx::reverb::{DECAY, LOWCUT, MAX_PREDELAY, MIX, PREDELAY, SIZE, TONE};

const PARAMS: &[ParamInfo] = FxKind::Reverb.params();

/// The rate Dattorro's delay lengths are given at.
const DESIGN_RATE: f32 = 29_761.0;
/// How far the size scales the tank's lengths, from smallest to largest.
const SCALES: (f32, f32) = (0.4, 1.6);
/// How far the tank's first all-passes wander, in samples at the design rate, and how fast.
const EXCURSION: f32 = 16.0;
const WANDER_HZ: f32 = 0.7;

/// A circular buffer, read some way behind where it's written.
struct Line {
    buffer: Vec<f32>,
    write: usize,
}

impl Line {
    fn new(len: usize) -> Self {
        Self {
            // Room for a read at least a sample back, between two samples.
            buffer: vec![0.0; len.max(3)],
            write: 0,
        }
    }

    /// `delay` samples back, between samples.
    #[inline]
    #[expect(
        clippy::indexing_slicing,
        reason = "the buffer's never empty, and both indices are wrapped to its length"
    )]
    fn read(&self, delay: f32) -> f32 {
        let len = self.buffer.len();
        let back = self.write as f32 + len as f32 - delay.clamp(1.0, (len - 2) as f32);
        let i = back as usize;
        let frac = back - i as f32;
        let (a, b) = (self.buffer[i % len], self.buffer[(i + 1) % len]);
        a + (b - a) * frac
    }

    #[inline]
    #[expect(
        clippy::indexing_slicing,
        reason = "`write` is kept wrapped to the buffer's length"
    )]
    fn write(&mut self, x: f32) {
        self.buffer[self.write] = x;
        self.write = (self.write + 1) % self.buffer.len();
    }

    /// As a delay `delay` samples long: writes `x` and returns what comes out.
    #[inline]
    fn delay(&mut self, x: f32, delay: f32) -> f32 {
        let out = self.read(delay);
        self.write(x);
        out
    }

    /// As an all-pass `delay` samples long, with gain `g`.
    #[inline]
    fn allpass(&mut self, x: f32, delay: f32, g: f32) -> f32 {
        let delayed = self.read(delay);
        let w = x - g * delayed;
        self.write(w);
        delayed + g * w
    }
}

/// A delay or all-pass in the design: its line and its length at the design rate.
struct Stage {
    line: Line,
    length: f32,
}

/// One half of the tank.
struct Half {
    /// The wandering all-pass, the delay, the all-pass and the delay, in order.
    wander: Stage,
    first: Stage,
    allpass: Stage,
    second: Stage,
    /// The damping low-pass's state.
    low: f32,
}

pub struct Reverb {
    sample_rate: f32,
    /// Samples per sample at the design rate, before the size.
    rate_scale: f32,
    predelay: Line,
    /// The input's low-pass, so the plate doesn't hiss: its coefficient and state.
    bandwidth: f32,
    input_low: f32,
    /// The input's high-pass (the low cut): its coefficient, and the low end it takes away.
    lowcut: f32,
    input_bass: f32,
    diffusers: [Stage; 4],
    halves: [Half; 2],
    /// What each half feeds the other, from the last sample.
    feedback: [f32; 2],
    phase: f32,
    size: Smoothed,
    decay: Smoothed,
    damping: f32,
    predelay_time: Smoothed,
    mix: Smoothed,
}

impl Reverb {
    /// Allocates the lines for the largest size, so build it off the audio thread.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let rate_scale = sample_rate / DESIGN_RATE;
        let stage = |length: f32, wanders: bool| {
            let extra = if wanders { EXCURSION } else { 0.0 };
            Stage {
                line: Line::new(((length + extra) * rate_scale * SCALES.1) as usize + 4),
                length,
            }
        };
        let half = |wander, first, allpass, second| Half {
            wander: stage(wander, true),
            first: stage(first, false),
            allpass: stage(allpass, false),
            second: stage(second, false),
            low: 0.0,
        };
        let mut reverb = Self {
            sample_rate,
            rate_scale,
            predelay: Line::new((MAX_PREDELAY * sample_rate) as usize + 4),
            bandwidth: 1.0 - (-TAU * 10_000.0 / sample_rate).exp(),
            input_low: 0.0,
            lowcut: 0.0,
            input_bass: 0.0,
            diffusers: [
                stage(142.0, false),
                stage(107.0, false),
                stage(379.0, false),
                stage(277.0, false),
            ],
            halves: [
                half(672.0, 4_453.0, 1_800.0, 3_720.0),
                half(908.0, 4_217.0, 2_656.0, 3_163.0),
            ],
            feedback: [0.0; 2],
            phase: 0.0,
            size: Smoothed::new(0.0, sample_rate),
            decay: Smoothed::new(0.0, sample_rate),
            damping: 0.0,
            predelay_time: Smoothed::new(0.0, sample_rate),
            mix: Smoothed::new(0.0, sample_rate),
        };
        // Each parameter from `values`, or its default where they stop short.
        for info in PARAMS {
            let value = values.get(info.id as usize).copied();
            reverb.set(info.id, value.unwrap_or(info.default));
        }
        for s in [
            &mut reverb.size,
            &mut reverb.decay,
            &mut reverb.predelay_time,
            &mut reverb.mix,
        ] {
            s.reset(s.target());
        }
        reverb
    }

    fn set(&mut self, id: u32, value: f32) {
        let Some(info) = PARAMS.get(id as usize) else {
            return;
        };
        let value = info.clamp(value);
        match id {
            SIZE => self.size.set(value),
            DECAY => self.decay.set(value),
            TONE => self.damping = 1.0 - (-TAU * value / self.sample_rate).exp(),
            PREDELAY => self.predelay_time.set(value * self.sample_rate),
            LOWCUT => self.lowcut = 1.0 - (-TAU * value / self.sample_rate).exp(),
            _ => self.mix.set(value),
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let size = self.size.tick();
            let decay = self.decay.tick();
            let mix = self.mix.tick();
            // Samples at this rate for a length at the design rate.
            let scale = self.rate_scale * (SCALES.0 + (SCALES.1 - SCALES.0) * size);
            let wander = (self.phase * TAU).sin() * EXCURSION * self.rate_scale;
            self.phase = (self.phase + WANDER_HZ / self.sample_rate).fract();

            // The input: pre-delayed, low-passed and diffused.
            let input = 0.5 * (*l + *r);
            let x = self
                .predelay
                .delay(input, self.predelay_time.tick().max(1.0));
            self.input_low += self.bandwidth * (x - self.input_low);
            self.input_bass += self.lowcut * (self.input_low - self.input_bass);
            let mut x = self.input_low - self.input_bass;
            for (i, d) in self.diffusers.iter_mut().enumerate() {
                let g = if i < 2 { 0.75 } else { 0.625 };
                // The input diffusers keep their length whatever the size, as in a real plate.
                x = d.line.allpass(x, d.length * self.rate_scale, g);
            }

            // Round the tank: each half takes the input and what the other half sent last.
            let decay_diffusion = (decay + 0.15).clamp(0.25, 0.5);
            // Each half takes what the other sent, and they wander in opposite directions.
            let [fed_a, fed_b] = self.feedback;
            let inputs = [(fed_b, wander), (fed_a, -wander)];
            let halves = self.halves.iter_mut().zip(&mut self.feedback);
            for ((half, feedback), (fed, swing)) in halves.zip(inputs) {
                let mut y = x + fed;
                y = half
                    .wander
                    .line
                    .allpass(y, half.wander.length * scale + swing, -0.7);
                y = half.first.line.delay(y, half.first.length * scale);
                half.low += self.damping * (y - half.low);
                y = half.low * decay;
                y = half
                    .allpass
                    .line
                    .allpass(y, half.allpass.length * scale, decay_diffusion);
                y = half.second.line.delay(y, half.second.length * scale);
                *feedback = y * decay;
            }

            // Tapped from both halves, differently for each side.
            let tap = |stage: &Stage, at: f32| stage.line.read(at * scale);
            let [a, b] = &self.halves;
            let out_l = tap(&b.first, 266.0) + tap(&b.first, 2_974.0) - tap(&b.allpass, 1_913.0)
                + tap(&b.second, 1_996.0)
                - tap(&a.first, 1_990.0)
                - tap(&a.allpass, 187.0)
                - tap(&a.second, 1_066.0);
            let out_r = tap(&a.first, 353.0) + tap(&a.first, 3_627.0) - tap(&a.allpass, 1_228.0)
                + tap(&a.second, 2_673.0)
                - tap(&b.first, 2_111.0)
                - tap(&b.allpass, 335.0)
                - tap(&b.second, 121.0);
            *l += (0.6 * out_l - *l) * mix;
            *r += (0.6 * out_r - *r) * mix;
        }
    }
}

impl Processor for Reverb {
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

    /// An impulse through a fully wet reverb at 48 kHz: the energy in each tenth of a second.
    fn tail(settings: &[(u32, f32)], seconds: usize) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        values[MIX as usize] = 1.0;
        for &(id, value) in settings {
            values[id as usize] = value;
        }
        let mut reverb = Reverb::with_values(48_000.0, &values);
        let len = 48_000 * seconds;
        let mut left = vec![0.0; len];
        let mut right = vec![0.0; len];
        left[0] = 1.0;
        right[0] = 1.0;
        reverb.process(&[], &mut left, &mut right);
        let energy = left
            .chunks(4_800)
            .map(|c| c.iter().map(|x| x * x).sum())
            .collect();
        (energy, left, right)
    }

    /// Fed the same in both, the two sides are as loud as each other overall, though at any one
    /// pitch they differ, as each is tapped from different points in the tank.
    #[test]
    fn both_sides_are_as_loud() {
        let (_, left, right) = tail(&[], 4);
        let db = |v: &[f32]| 10.0 * v.iter().map(|x| x * x).sum::<f32>().log10();
        let (l, r) = (db(&left), db(&right));
        assert!((l - r).abs() < 1.0, "left {l:.2} dB, right {r:.2} dB");
    }

    #[test]
    fn rings_on_and_dies_away() {
        let (energy, left, right) = tail(&[], 4);
        assert!(energy[1] > 1e-4, "it rings: {}", energy[1]);
        assert!(energy[39] < energy[5] * 1e-3, "and fades");
        assert!(
            left.iter()
                .chain(&right)
                .all(|x| x.is_finite() && x.abs() < 2.0)
        );
        assert_ne!(left[10_000], right[10_000], "in stereo");
    }

    #[test]
    fn decay_sets_how_long() {
        let (short, ..) = tail(&[(DECAY, 0.2)], 2);
        let (long, ..) = tail(&[(DECAY, 0.9)], 2);
        assert!(long[15] > short[15] * 100.0, "{} {}", long[15], short[15]);
    }

    #[test]
    fn predelay_keeps_the_front_clear() {
        let (.., left, _) = tail(&[(PREDELAY, 0.1)], 1);
        assert!(left[..4_000].iter().all(|x| x.abs() < 1e-6));
    }

    #[test]
    fn the_low_cut_keeps_the_bass_out_of_the_plate() {
        // A second of a 60 Hz boom, then what rings on after it.
        let ring = |lowcut: f32| {
            let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
            values[MIX as usize] = 1.0;
            values[DECAY as usize] = 0.9;
            values[LOWCUT as usize] = lowcut;
            let mut reverb = Reverb::with_values(48_000.0, &values);
            let mut left: Vec<f32> = (0..96_000)
                .map(|i| {
                    if i < 48_000 {
                        (TAU * 60.0 * i as f32 / 48_000.0).sin()
                    } else {
                        0.0
                    }
                })
                .collect();
            let mut right = left.clone();
            reverb.process(&[], &mut left, &mut right);
            left[48_000..].iter().map(|x| x * x).sum::<f32>()
        };
        let (open, cut) = (ring(20.0), ring(300.0));
        assert!(cut < open * 0.1, "{cut} against {open}");
    }

    #[test]
    fn stays_stable_at_its_longest() {
        let (energy, ..) = tail(&[(DECAY, 0.98), (SIZE, 1.0), (TONE, 16_000.0)], 10);
        assert!(energy.iter().all(|e| e.is_finite()));
        assert!(energy[99] <= energy[10], "never grows");
    }
}
