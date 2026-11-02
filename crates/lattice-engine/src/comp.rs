//! A compressor. Both channels share one gain, so the stereo image holds still, and it's worked
//! out from the louder of the two, so peaks are caught.

use lattice_core::{FxKind, db_to_gain};

use crate::dsp::decay_coef;
use crate::param::{ParamInfo, Smoothed};
use crate::processor::{Event, EventKind, Processor, Segment, segments};

pub use lattice_core::fx::comp::{ATTACK, MAKEUP, MIX, RATIO, RELEASE, THRESHOLD};

const PARAMS: &[ParamInfo] = FxKind::Comp.params();

/// The knee's width: compression eases in across this many dB around the threshold.
const KNEE: f32 = 6.0;
/// Quieter than this counts as silence, which keeps the log finite.
const FLOOR: f32 = 1e-6;

pub struct Comp {
    sample_rate: f32,
    threshold: f32,
    /// The fraction of the level above the threshold that's taken off: 1 - 1 / ratio.
    slope: f32,
    attack: f32,
    release: f32,
    makeup: Smoothed,
    mix: Smoothed,
    /// dB being taken off, following the level with the attack and release.
    reduction: f32,
}

impl Comp {
    pub fn new(sample_rate: f32) -> Self {
        let mut comp = Self {
            sample_rate,
            threshold: 0.0,
            slope: 0.0,
            attack: 0.0,
            release: 0.0,
            makeup: Smoothed::new(1.0, sample_rate),
            mix: Smoothed::new(1.0, sample_rate),
            reduction: 0.0,
        };
        for info in PARAMS {
            comp.set(info.id, info.default);
        }
        comp.settle();
        comp
    }

    /// Starts at `values` (one per parameter, by id) rather than gliding to them.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let mut comp = Self::new(sample_rate);
        for (id, &value) in values.iter().enumerate() {
            comp.set(id as u32, value);
        }
        comp.settle();
        comp
    }

    fn settle(&mut self) {
        self.makeup.reset(self.makeup.target());
        self.mix.reset(self.mix.target());
    }

    fn set(&mut self, id: u32, value: f32) {
        let Some(info) = PARAMS.get(id as usize) else {
            return;
        };
        let value = info.clamp(value);
        match id {
            THRESHOLD => self.threshold = value,
            RATIO => self.slope = 1.0 - 1.0 / value,
            ATTACK => self.attack = decay_coef(value, self.sample_rate),
            RELEASE => self.release = decay_coef(value, self.sample_rate),
            MAKEUP => self.makeup.set(db_to_gain(value)),
            _ => self.mix.set(value),
        }
    }

    /// dB to take off a signal at `level` dB, with a soft knee.
    fn target(&self, level: f32) -> f32 {
        let over = level - self.threshold;
        if over <= -KNEE / 2.0 {
            0.0
        } else if over < KNEE / 2.0 {
            let into = over + KNEE / 2.0;
            self.slope * into * into / (2.0 * KNEE)
        } else {
            self.slope * over
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let makeup = self.makeup.tick();
            let mix = self.mix.tick();
            let level = 20.0 * l.abs().max(r.abs()).max(FLOOR).log10();
            let target = self.target(level);
            // Clamping down follows the attack; letting go follows the release.
            let coef = if target > self.reduction {
                self.attack
            } else {
                self.release
            };
            self.reduction = target + (self.reduction - target) * coef;
            let gain = db_to_gain(-self.reduction) * makeup;
            for x in [l, r] {
                *x += (*x * gain - *x) * mix;
            }
        }
    }
}

impl Processor for Comp {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left.len(), |segment| match segment {
            Segment::Render(range) => {
                // The channels are the same length, so both have the range; if one didn't,
                // there'd be no right way to render it.
                if let (Some(l), Some(r)) = (left.get_mut(range.clone()), right.get_mut(range)) {
                    self.render(l, r);
                }
            }
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

    const RATE: f32 = 48_000.0;

    fn comp(set: &[(u32, f32)]) -> Comp {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        for &(id, value) in set {
            values[id as usize] = value;
        }
        Comp::with_values(RATE, &values)
    }

    /// The level out, once settled, of a steady signal at `db`.
    fn steady(comp: &mut Comp, db: f32) -> f32 {
        let x = db_to_gain(db);
        let (mut left, mut right) = (vec![x; RATE as usize], vec![-x; RATE as usize]);
        comp.process(&[], &mut left, &mut right);
        assert_eq!(
            left.last().map(|l| -l),
            right.last().copied(),
            "channels move together"
        );
        20.0 * left.last().unwrap().abs().log10()
    }

    #[test]
    fn signals_over_the_threshold_are_turned_down_by_the_ratio() {
        let mut c = comp(&[(THRESHOLD, -20.0), (RATIO, 4.0)]);
        // 12 dB over comes out 3 dB over.
        let out = steady(&mut c, -8.0);
        assert!((out - -17.0).abs() < 0.1, "{out}");
    }

    #[test]
    fn quiet_signals_pass_untouched_apart_from_makeup() {
        let mut c = comp(&[(THRESHOLD, -20.0), (MAKEUP, 6.0)]);
        let out = steady(&mut c, -40.0);
        assert!((out - -34.0).abs() < 0.01, "{out}");
    }

    #[test]
    fn mix_blends_back_the_dry_signal() {
        let mut c = comp(&[(THRESHOLD, -48.0), (RATIO, 20.0), (MIX, 0.0)]);
        assert!(steady(&mut c, -6.0).abs() - 6.0 < 0.01);
    }

    #[test]
    fn it_clamps_down_with_the_attack_and_lets_go_with_the_release() {
        let mut c = comp(&[
            (THRESHOLD, -20.0),
            (RATIO, 20.0),
            (ATTACK, 0.001),
            (RELEASE, 0.1),
        ]);
        let loud = db_to_gain(-2.0);
        let mut block = |x: f32, len: usize| {
            let (mut left, mut right) = (vec![x; len], vec![x; len]);
            c.process(&[], &mut left, &mut right);
            left
        };
        let hit = block(loud, 480);
        assert!(hit[0] > 0.7, "the first sample gets through");
        assert!(hit[479] < 0.2, "clamped within 10 ms");
        // Straight after, a quiet signal is still held down, then comes back.
        let after = block(0.01, 48_000);
        assert!(after[0] < 0.005);
        assert!((after[47_999] - 0.01).abs() < 1e-4);
    }
}
