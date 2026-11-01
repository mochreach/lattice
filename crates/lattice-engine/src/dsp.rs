//! Building blocks: band-limited oscillators, envelopes and filters.

use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wave {
    Saw,
    Square,
}

/// PolyBLEP oscillator: naive waveforms with their discontinuities smoothed, which removes most
/// of the aliasing. Phase runs from 0 to 1.
#[derive(Debug, Clone, Default)]
pub struct Osc {
    phase: f32,
    inc: f32,
}

impl Osc {
    pub fn set_freq(&mut self, hz: f32, sample_rate: f32) {
        self.inc = hz / sample_rate;
    }

    pub fn next(&mut self, wave: Wave) -> f32 {
        let (t, dt) = (self.phase, self.inc);
        let out = match wave {
            Wave::Saw => 2.0 * t - 1.0 - poly_blep(t, dt),
            Wave::Square => {
                let naive = if t < 0.5 { 1.0 } else { -1.0 };
                naive + poly_blep(t, dt) - poly_blep((t + 0.5).fract(), dt)
            }
        };
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        out
    }
}

/// Correction for a unit step at phase 0, spread over one sample either side.
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        2.0 * t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

/// Envelope times in seconds; `sustain` is a level from 0 to 1.
#[derive(Debug, Clone, Copy)]
pub struct AdsrParams {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Linear attack, exponential decay and release. Driven by explicit trigger and release events,
/// so back-to-back notes always retrigger.
#[derive(Debug, Clone, Default)]
pub struct Adsr {
    stage: Stage,
    level: f32,
}

impl Adsr {
    /// Starts the attack from the current level, so retriggering a sounding note doesn't click.
    pub fn trigger(&mut self) {
        self.stage = Stage::Attack;
    }

    pub fn release(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    pub fn is_idle(&self) -> bool {
        self.stage == Stage::Idle
    }

    pub fn next(&mut self, p: &AdsrParams, sample_rate: f32) -> f32 {
        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                self.level += 1.0 / (p.attack.max(1e-4) * sample_rate);
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level =
                    p.sustain + (self.level - p.sustain) * decay_coef(p.decay, sample_rate);
                if (self.level - p.sustain).abs() < 1e-4 {
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => self.level = p.sustain,
            Stage::Release => {
                self.level *= decay_coef(p.release, sample_rate);
                if self.level < 1e-4 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.level
    }
}

/// Per-sample multiplier for an exponential decay with time constant `seconds`.
pub fn decay_coef(seconds: f32, sample_rate: f32) -> f32 {
    (-1.0 / (seconds.max(1e-4) * sample_rate)).exp()
}

/// Topology-preserving-transform state-variable filter (after Andrew Simper). Stays stable while
/// the cutoff is modulated every sample.
#[derive(Debug, Clone, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// Low-pass output. `resonance` runs from 0 to just under 1.
    pub fn lowpass(&mut self, input: f32, cutoff: f32, resonance: f32, sample_rate: f32) -> f32 {
        self.tick(input, cutoff, resonance, sample_rate).1
    }

    /// Band-pass output, at unity gain at the centre whatever the resonance.
    pub fn bandpass(&mut self, input: f32, centre: f32, resonance: f32, sample_rate: f32) -> f32 {
        let k = damping(resonance);
        self.tick(input, centre, resonance, sample_rate).0 * k
    }

    /// The band-pass and low-pass outputs.
    fn tick(&mut self, input: f32, cutoff: f32, resonance: f32, sample_rate: f32) -> (f32, f32) {
        let (low, band, _) = self.outputs(input, Self::g(cutoff, sample_rate), damping(resonance));
        (band, low)
    }

    /// The coefficient for `cutoff`, worth working out once for several filters.
    pub fn g(cutoff: f32, sample_rate: f32) -> f32 {
        (PI * cutoff.clamp(20.0, 0.45 * sample_rate) / sample_rate).tan()
    }

    /// The low-, band- and high-pass outputs, for a coefficient from [`Svf::g`] and a damping
    /// from [`damping`]. The band-pass's gain at its centre is `1 / k`.
    pub fn outputs(&mut self, input: f32, g: f32, k: f32) -> (f32, f32, f32) {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = input - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, input - k * v1 - v2)
    }
}

/// The filter's damping, 2 (none) down to 0.04, for a resonance from 0 to just under 1.
pub fn damping(resonance: f32) -> f32 {
    2.0 - 2.0 * resonance.clamp(0.0, 0.98)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oscillators_stay_in_range() {
        for wave in [Wave::Saw, Wave::Square] {
            let mut osc = Osc::default();
            osc.set_freq(3_000.0, 48_000.0);
            for _ in 0..48_000 {
                let x = osc.next(wave);
                assert!((-1.05..=1.05).contains(&x), "{wave:?}: {x}");
            }
        }
    }

    #[test]
    fn envelope_runs_through_its_stages() {
        let p = AdsrParams {
            attack: 0.01,
            decay: 0.05,
            sustain: 0.5,
            release: 0.05,
        };
        let mut env = Adsr::default();
        env.trigger();
        let peak = (0..480).map(|_| env.next(&p, 48_000.0)).fold(0.0, f32::max);
        assert_eq!(peak, 1.0);
        for _ in 0..48_000 {
            env.next(&p, 48_000.0);
        }
        assert_eq!(env.next(&p, 48_000.0), 0.5);
        env.release();
        for _ in 0..48_000 {
            env.next(&p, 48_000.0);
        }
        assert!(env.is_idle());
    }

    #[test]
    fn lowpass_passes_dc_and_cuts_highs() {
        let mut f = Svf::default();
        let dc = (0..4_800)
            .map(|_| f.lowpass(1.0, 1_000.0, 0.0, 48_000.0))
            .last()
            .unwrap();
        assert!((dc - 1.0).abs() < 1e-3);

        let mut f = Svf::default();
        let mut osc = Osc::default();
        osc.set_freq(10_000.0, 48_000.0);
        let peak = (0..4_800)
            .map(|_| {
                f.lowpass(osc.next(Wave::Square), 200.0, 0.0, 48_000.0)
                    .abs()
            })
            .skip(2_400)
            .fold(0.0, f32::max);
        assert!(peak < 0.01, "{peak}");
    }

    #[test]
    fn bandpass_passes_its_centre_at_unity() {
        let level = |hz: f32| {
            let mut f = Svf::default();
            (0..9_600)
                .map(|i| {
                    let x = (2.0 * PI * hz * i as f32 / 48_000.0).sin();
                    f.bandpass(x, 1_000.0, 0.8, 48_000.0).abs()
                })
                .skip(4_800)
                .fold(0.0, f32::max)
        };
        assert!((level(1_000.0) - 1.0).abs() < 0.01, "{}", level(1_000.0));
        assert!(level(100.0) < 0.1, "{}", level(100.0));
    }
}
