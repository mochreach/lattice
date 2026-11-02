//! A phaser: the sound through a chain of first-order all-pass stages, which leave its level alone
//! but turn its phase, more for higher frequencies. Mixed back with itself, the frequencies the
//! chain turns half a cycle cancel, a notch for each pair of stages. An LFO sweeps the stages'
//! corner, geometrically about 630 Hz, by up to three octaves either way, and some of the chain's
//! output goes back into it, sharpening the notches and lifting peaks between them. The right
//! side's LFO is a quarter of a cycle on from the left's, so the sweep moves across the stereo
//! field.

use std::f32::consts::{PI, TAU};

use lattice_core::FxKind;

use crate::param::{ParamInfo, Smoothed};
use crate::processor::{Event, EventKind, Processor, Segment, segments};

pub use lattice_core::fx::phaser::{DEPTH, FEEDBACK, MIX, RATE, STAGES};

const PARAMS: &[ParamInfo] = FxKind::Phaser.params();

/// The stages each setting has.
const STAGE_COUNTS: [usize; 4] = [4, 6, 8, 12];
const MAX_STAGES: usize = 12;
/// The middle of the sweep, and how far it goes at full depth, in octaves either way.
const CENTRE: f32 = 630.0;
const SPAN: f32 = 3.0;
/// The highest the corner goes, as a fraction of the sample rate.
const TOP: f32 = 0.45;

pub struct Phaser {
    sample_rate: f32,
    stages: usize,
    rate: f32,
    depth: Smoothed,
    feedback: Smoothed,
    mix: Smoothed,
    /// Each side's stages' states.
    states: [[f32; MAX_STAGES]; 2],
    /// Each side's chain's last output, for the feedback.
    last: [f32; 2],
    /// The LFO, from 0 to 1.
    phase: f32,
}

impl Phaser {
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let mut phaser = Self {
            sample_rate,
            stages: 4,
            rate: 0.0,
            depth: Smoothed::new(0.0, sample_rate),
            feedback: Smoothed::new(0.0, sample_rate),
            mix: Smoothed::new(0.0, sample_rate),
            states: [[0.0; MAX_STAGES]; 2],
            last: [0.0; 2],
            phase: 0.0,
        };
        // Each parameter from `values`, or its default where they stop short.
        for info in PARAMS {
            let value = values.get(info.id as usize).copied();
            phaser.set(info.id, value.unwrap_or(info.default));
        }
        for s in [&mut phaser.depth, &mut phaser.feedback, &mut phaser.mix] {
            s.reset(s.target());
        }
        phaser
    }

    fn set(&mut self, id: u32, value: f32) {
        let Some(info) = PARAMS.get(id as usize) else {
            return;
        };
        let value = info.clamp(value);
        match id {
            STAGES => {
                // The most for a value past the end.
                let count = STAGE_COUNTS.get(value.round() as usize);
                self.stages = count.copied().unwrap_or(MAX_STAGES);
            }
            RATE => self.rate = value,
            DEPTH => self.depth.set(value),
            FEEDBACK => self.feedback.set(value),
            _ => self.mix.set(value),
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.phase = (self.phase + self.rate / self.sample_rate).fract();
            let depth = self.depth.tick();
            let feedback = self.feedback.tick();
            let mix = self.mix.tick();
            let sides = self.states.iter_mut().zip(&mut self.last).zip([l, r]);
            for (side, ((states, last), x)) in sides.enumerate() {
                let lfo = (TAU * (self.phase + 0.25 * side as f32)).sin();
                let corner = (CENTRE * (depth * SPAN * lfo).exp2()).min(TOP * self.sample_rate);
                let t = (PI * corner / self.sample_rate).tan();
                let a = (t - 1.0) / (t + 1.0);
                let mut y = *x + feedback * *last;
                for state in states.iter_mut().take(self.stages) {
                    let out = a * y + *state;
                    *state = y - a * out;
                    y = out;
                }
                *last = y;
                *x += (y - *x) * mix;
            }
        }
    }
}

impl Processor for Phaser {
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
    use crate::rhythmicon::Noise;

    const RATE_HZ: f32 = 48_000.0;

    fn phaser(settings: &[(u32, f32)]) -> Phaser {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        for &(id, value) in settings {
            values[id as usize] = value;
        }
        Phaser::with_values(RATE_HZ, &values)
    }

    /// The loudest a sine at `hz` comes through, once settled.
    fn level(settings: &[(u32, f32)], hz: f32) -> f32 {
        let mut p = phaser(settings);
        let mut left: Vec<f32> = (0..9_600)
            .map(|i| (TAU * hz * i as f32 / RATE_HZ).sin())
            .collect();
        let mut right = left.clone();
        p.process(&[], &mut left, &mut right);
        left[4_800..].iter().fold(0.0, |m, x| m.max(x.abs()))
    }

    #[test]
    fn notches_where_the_stages_turn_half_a_cycle() {
        // Held still at the centre, four stages turn 262 Hz (0.414 of it) half a cycle.
        let still = [(DEPTH, 0.0), (FEEDBACK, 0.0), (MIX, 0.5)];
        assert!(level(&still, 261.0) < 0.05, "{}", level(&still, 261.0));
        assert!(level(&still, 40.0) > 0.9);
        // Mixed out, it's untouched.
        assert!(level(&[(MIX, 0.0)], 261.0) > 0.99);
    }

    #[test]
    fn stays_steady_at_its_most_extreme() {
        let mut p = phaser(&[(STAGES, 3.0), (RATE, 10.0), (DEPTH, 1.0), (FEEDBACK, 0.9)]);
        let mut noise = Noise(7);
        let mut left: Vec<f32> = (0..96_000).map(|_| noise.next()).collect();
        let mut right = left.clone();
        p.process(&[], &mut left, &mut right);
        let peak = left
            .iter()
            .chain(&right)
            .fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak.is_finite() && peak < 20.0, "{peak}");
    }
}
