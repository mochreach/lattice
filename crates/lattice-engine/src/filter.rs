//! A resonant state-variable filter, low-, band- or high-pass, with an LFO sweeping its cutoff
//! up and down by octaves.

use std::f32::consts::TAU;

use lattice_core::FxKind;

use crate::dsp::{Svf, damping};
use crate::param::{ParamInfo, Smoothed};
use crate::processor::{Event, EventKind, Processor, Segment, segments};

pub use lattice_core::fx::filter::{CUTOFF, MIX, RATE, RESONANCE, SWEEP, TYPE};

const PARAMS: &[ParamInfo] = FxKind::Filter.params();

pub struct Filter {
    sample_rate: f32,
    filters: [Svf; 2],
    kind: u32,
    /// In octaves above 20 Hz, so it glides evenly through the range.
    cutoff: Smoothed,
    resonance: Smoothed,
    sweep: Smoothed,
    mix: Smoothed,
    rate: f32,
    /// The LFO's phase, from 0 to 1.
    phase: f32,
}

/// Octaves above 20 Hz.
fn octaves(hz: f32) -> f32 {
    (hz / 20.0).log2()
}

impl Filter {
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let default = |id: u32| PARAMS[id as usize].default;
        let mut filter = Self {
            sample_rate,
            filters: Default::default(),
            kind: 0,
            cutoff: Smoothed::new(octaves(default(CUTOFF)), sample_rate),
            resonance: Smoothed::new(default(RESONANCE), sample_rate),
            sweep: Smoothed::new(default(SWEEP), sample_rate),
            mix: Smoothed::new(default(MIX), sample_rate),
            rate: default(RATE),
            phase: 0.0,
        };
        for (id, &value) in values.iter().enumerate() {
            filter.set(id as u32, value);
        }
        for s in [
            &mut filter.cutoff,
            &mut filter.resonance,
            &mut filter.sweep,
            &mut filter.mix,
        ] {
            s.reset(s.target());
        }
        filter
    }

    fn set(&mut self, id: u32, value: f32) {
        let Some(info) = PARAMS.get(id as usize) else {
            return;
        };
        let value = info.clamp(value);
        match id {
            TYPE => self.kind = value as u32,
            CUTOFF => self.cutoff.set(octaves(value)),
            RESONANCE => self.resonance.set(value),
            RATE => self.rate = value,
            SWEEP => self.sweep.set(value),
            _ => self.mix.set(value),
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let lfo = (self.phase * TAU).sin();
            self.phase = (self.phase + self.rate / self.sample_rate).fract();
            let cutoff = 20.0 * (self.cutoff.tick() + lfo * self.sweep.tick()).exp2();
            let g = Svf::g(cutoff, self.sample_rate);
            let k = damping(self.resonance.tick());
            let mix = self.mix.tick();
            for (x, f) in [&mut *l, &mut *r].into_iter().zip(&mut self.filters) {
                let (low, band, high) = f.outputs(*x, g, k);
                let y = match self.kind {
                    0 => low,
                    // At unity gain at its centre, whatever the resonance.
                    1 => band * k,
                    _ => high,
                };
                *x += (y - *x) * mix;
            }
        }
    }
}

impl Processor for Filter {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left.len(), |segment| match segment {
            Segment::Render(range) => self.render(&mut left[range.clone()], &mut right[range]),
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

    /// The loudest a sine at `hz` comes through, once settled.
    fn level(settings: &[(u32, f32)], hz: f32) -> f32 {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        for &(id, value) in settings {
            values[id as usize] = value;
        }
        let mut filter = Filter::with_values(48_000.0, &values);
        let mut left: Vec<f32> = (0..9_600)
            .map(|i| (TAU * hz * i as f32 / 48_000.0).sin())
            .collect();
        let mut right = left.clone();
        filter.process(&[], &mut left, &mut right);
        left[4_800..].iter().fold(0.0, |m, x| m.max(x.abs()))
    }

    #[test]
    fn each_type_passes_its_own_band() {
        let low = [(TYPE, 0.0), (CUTOFF, 1_000.0)];
        assert!(level(&low, 100.0) > 0.95 && level(&low, 10_000.0) < 0.05);
        let high = [(TYPE, 2.0), (CUTOFF, 1_000.0)];
        assert!(level(&high, 10_000.0) > 0.95 && level(&high, 100.0) < 0.05);
        let band = [(TYPE, 1.0), (CUTOFF, 1_000.0), (RESONANCE, 0.8)];
        assert!((level(&band, 1_000.0) - 1.0).abs() < 0.02);
        assert!(level(&band, 100.0) < 0.1);
        // Mixed out, it's untouched.
        assert!(level(&[(CUTOFF, 100.0), (MIX, 0.0)], 10_000.0) > 0.99);
    }

    #[test]
    fn the_lfo_sweeps_the_cutoff() {
        // A 100 Hz cutoff swept four octaves up lets 1 kHz through at the top of each sweep.
        let still = [(CUTOFF, 100.0)];
        let swept = [(CUTOFF, 100.0), (SWEEP, 4.0), (RATE, 10.0)];
        assert!(level(&still, 1_000.0) < 0.05);
        assert!(level(&swept, 1_000.0) > 0.5);
    }
}
