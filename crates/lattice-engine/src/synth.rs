//! A plain test synth: oscillator → low-pass filter → amplifier.
//!
//! This isn't one of Lattice's flagship engines. It's here to exercise the engine and to set the
//! pattern for the real ones: all eight voices live in SIMD lanes (structure of arrays) and are
//! computed together, and anything expensive is updated at control rate, not per sample.

use std::f32::consts::PI;

use lattice_core::InstKind;
use lattice_core::instrument::synth::{
    ATTACK, CUTOFF, DECAY, ENV, GAIN, RELEASE, RESONANCE, SUSTAIN, WAVE,
};
use wide::f32x8;

use crate::dsp::{AdsrParams, Wave, decay_coef};
use crate::param::ParamInfo;
use crate::processor::{Event, EventKind, Processor, Segment, Shape, segments};

const VOICES: usize = 8;
/// Samples between control-rate updates (envelope stages, filter coefficients).
const CONTROL_PERIOD: u64 = 16;
/// The attack heads for this level and stops at 1.0, like an analogue envelope, so it reaches
/// full level in finite time without a per-sample branch.
const ATTACK_TARGET: f32 = 1.2;
/// Below this, a releasing voice is finished.
const SILENCE: f32 = 1e-4;

#[derive(Debug, Clone, Copy)]
pub struct Patch {
    pub wave: Wave,
    pub amp: AdsrParams,
    /// Filter cutoff in Hz with the envelope closed.
    pub cutoff: f32,
    /// Hz added to the cutoff at full envelope.
    pub env_amount: f32,
    pub resonance: f32,
    pub gain: f32,
}

impl Patch {
    /// The patch a song's synth settings describe, one value per parameter, by id.
    pub fn from_values(values: &[f32]) -> Self {
        let mut patch = Self {
            wave: Wave::Saw,
            amp: AdsrParams {
                attack: 0.0,
                decay: 0.0,
                sustain: 0.0,
                release: 0.0,
            },
            cutoff: 0.0,
            env_amount: 0.0,
            resonance: 0.0,
            gain: 0.0,
        };
        for info in InstKind::Synth.params() {
            let value = values
                .get(info.id as usize)
                .copied()
                .unwrap_or(info.default);
            patch.set(info.id, info.clamp(value));
        }
        patch
    }

    fn set(&mut self, id: u32, value: f32) {
        match id {
            WAVE => self.wave = if value < 0.5 { Wave::Saw } else { Wave::Square },
            ATTACK => self.amp.attack = value,
            DECAY => self.amp.decay = value,
            SUSTAIN => self.amp.sustain = value,
            RELEASE => self.amp.release = value,
            CUTOFF => self.cutoff = value,
            ENV => self.env_amount = value,
            RESONANCE => self.resonance = value,
            GAIN => self.gain = value,
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Idle,
    Attack,
    Decay,
    Release,
}

#[derive(Debug, Clone)]
pub struct Synth {
    patch: Patch,
    sample_rate: f32,

    // Audio-rate state: one lane per voice.
    phase: f32x8,
    inc: f32x8,
    env: f32x8,
    env_target: f32x8,
    env_coef: f32x8,
    velocity: f32x8,
    ic1: f32x8,
    ic2: f32x8,
    a1: f32x8,
    a2: f32x8,
    a3: f32x8,

    // Control state, one entry per voice.
    stage: [Stage; VOICES],
    pitch: [u8; VOICES],
    started: [u64; VOICES],
    /// A voice gliding: control periods to go, the step its pitch takes each, and where it
    /// ends up, as phase increments.
    glide_left: [u32; VOICES],
    glide_ratio: [f32; VOICES],
    glide_to: [f32; VOICES],
    /// The pitch last played, which a glide starts from.
    last: Option<u8>,
    /// How the next note plays.
    shape: Shape,
    notes_started: u64,
    /// Every voice is idle, so rendering can be skipped until the next note.
    silent: bool,
    /// Samples rendered so far. Control updates happen on multiples of `CONTROL_PERIOD`, so
    /// output doesn't depend on how the host splits buffers.
    clock: u64,

    // Derived from the patch.
    attack_coef: f32,
    decay_coef: f32,
    release_coef: f32,
    damping: f32,
}

impl Synth {
    pub fn new(patch: Patch, sample_rate: f32) -> Self {
        let zero = f32x8::splat(0.0);
        let mut synth = Self {
            patch,
            sample_rate,
            phase: zero,
            inc: zero,
            env: zero,
            env_target: zero,
            env_coef: zero,
            velocity: zero,
            ic1: zero,
            ic2: zero,
            a1: zero,
            a2: zero,
            a3: zero,
            stage: [Stage::Idle; VOICES],
            pitch: [0; VOICES],
            started: [0; VOICES],
            glide_left: [0; VOICES],
            glide_ratio: [1.0; VOICES],
            glide_to: [0.0; VOICES],
            last: None,
            shape: Shape::default(),
            notes_started: 0,
            silent: true,
            clock: 0,
            attack_coef: 0.0,
            decay_coef: 0.0,
            release_coef: 0.0,
            damping: 0.0,
        };
        synth.derive();
        synth
    }

    /// Starts at a song's synth settings, one value per parameter, by id.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        Self::new(Patch::from_values(values), sample_rate)
    }

    /// Works out the coefficients that follow from the patch. Notes already sounding pick up
    /// the new envelope times at their next stage.
    fn derive(&mut self) {
        let patch = &self.patch;
        // Time constant for an exponential towards ATTACK_TARGET that passes 1.0 after `attack`.
        let attack_tau = patch.amp.attack.max(1e-4) / -(1.0 - 1.0 / ATTACK_TARGET).ln();
        self.attack_coef = decay_coef(attack_tau, self.sample_rate);
        self.decay_coef = decay_coef(patch.amp.decay, self.sample_rate);
        self.release_coef = decay_coef(patch.amp.release, self.sample_rate);
        self.damping = 2.0 - 2.0 * patch.resonance.clamp(0.0, 0.98);
    }

    fn note_on(&mut self, pitch: u8, velocity: u8) {
        // A free voice, else the oldest released one, else the oldest of all. The first of
        // equals wins.
        let free = |(stage, &started): (&Stage, &u64)| match stage {
            Stage::Idle => (0, 0),
            Stage::Release => (1, started),
            Stage::Attack | Stage::Decay => (2, started),
        };
        let voices = self.stage.iter().zip(&self.started).enumerate();
        // There are always voices, so there's always one to take.
        let Some((i, _)) = voices.min_by_key(|&(_, voice)| free(voice)) else {
            return;
        };
        let (Some(stage), Some(voice_pitch), Some(started)) = (
            self.stage.get_mut(i),
            self.pitch.get_mut(i),
            self.started.get_mut(i),
        ) else {
            return;
        };
        let (Some(left), Some(ratio), Some(to)) = (
            self.glide_left.get_mut(i),
            self.glide_ratio.get_mut(i),
            self.glide_to.get_mut(i),
        ) else {
            return;
        };
        *to = midi_to_hz(pitch) / self.sample_rate;
        // Gliding, it starts at the last pitch and steps evenly, in pitch, to its own.
        let shape = std::mem::take(&mut self.shape);
        let from = match self.last {
            Some(last) if shape.glide > 0 => {
                let periods = shape.glide.div_ceil(CONTROL_PERIOD as u32).max(1);
                let from = midi_to_hz(last) / self.sample_rate;
                *left = periods;
                *ratio = (*to / from).powf(1.0 / periods as f32);
                from
            }
            _ => {
                *left = 0;
                *to
            }
        };
        self.last = Some(pitch);

        self.notes_started += 1;
        *stage = Stage::Attack;
        *voice_pitch = pitch;
        *started = self.notes_started;
        set_lane(&mut self.inc, i, from);
        set_lane(&mut self.velocity, i, f32::from(velocity) / 127.0);
        set_lane(&mut self.env_target, i, ATTACK_TARGET);
        set_lane(&mut self.env_coef, i, self.attack_coef);
        self.silent = false;
    }

    fn note_off(&mut self, pitch: u8) {
        self.release(|voice_pitch| voice_pitch == pitch);
    }

    /// Releases every voice.
    fn choke(&mut self) {
        self.release(|_| true);
    }

    /// Releases the voices sounding whose pitch passes `which`.
    fn release(&mut self, which: impl Fn(u8) -> bool) {
        let mut target = self.env_target.to_array();
        let mut coef = self.env_coef.to_array();
        let lanes = target.iter_mut().zip(&mut coef);
        for ((stage, &pitch), (target, coef)) in self.stage.iter_mut().zip(&self.pitch).zip(lanes) {
            if which(pitch) && matches!(stage, Stage::Attack | Stage::Decay) {
                *stage = Stage::Release;
                *target = 0.0;
                *coef = self.release_coef;
            }
        }
        self.env_target = f32x8::new(target);
        self.env_coef = f32x8::new(coef);
    }

    /// Envelope stage changes and filter coefficients, every `CONTROL_PERIOD` samples.
    fn update_control(&mut self) {
        let mut env = self.env.to_array();
        let mut target = self.env_target.to_array();
        let mut coef = self.env_coef.to_array();
        let lanes = env.iter_mut().zip(target.iter_mut().zip(&mut coef));
        for (stage, (env, (target, coef))) in self.stage.iter_mut().zip(lanes) {
            match stage {
                Stage::Attack if *env >= 1.0 => {
                    *stage = Stage::Decay;
                    *target = self.patch.amp.sustain;
                    *coef = self.decay_coef;
                }
                Stage::Release if *env < SILENCE => {
                    *stage = Stage::Idle;
                    *env = 0.0;
                }
                _ => {}
            }
        }
        self.env = f32x8::new(env);
        self.env_target = f32x8::new(target);
        self.env_coef = f32x8::new(coef);
        self.silent = self.stage.iter().all(|&s| s == Stage::Idle);

        if self.glide_left.iter().any(|&left| left > 0) {
            let mut inc = self.inc.to_array();
            let glides = self.glide_left.iter_mut().zip(&self.glide_ratio);
            for ((inc, (left, ratio)), to) in inc.iter_mut().zip(glides).zip(&self.glide_to) {
                if *left > 0 {
                    *left -= 1;
                    *inc = if *left == 0 { *to } else { *inc * ratio };
                }
            }
            self.inc = f32x8::new(inc);
        }

        // TPT state-variable filter coefficients for each voice's current cutoff.
        // Kept over the bottom, as `clamp` panics otherwise, at an absurdly low rate.
        let nyquist_ish = (0.45 * self.sample_rate).max(20.0);
        let g = f32x8::new(env.map(|e| {
            let cutoff = self.patch.cutoff + self.patch.env_amount * e.min(1.0);
            (PI * cutoff.clamp(20.0, nyquist_ish) / self.sample_rate).tan()
        }));
        let one = f32x8::splat(1.0);
        self.a1 = one / (one + g * (g + f32x8::splat(self.damping)));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let one = f32x8::splat(1.0);
        let two = f32x8::splat(2.0);
        let half = f32x8::splat(0.5);
        let gain = self.patch.gain;

        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.clock.is_multiple_of(CONTROL_PERIOD) {
                self.update_control();
            }
            self.clock += 1;
            if self.silent {
                *l = 0.0;
                *r = 0.0;
                continue;
            }

            let t = self.phase;
            let dt = self.inc.max(f32x8::splat(1e-6));
            let x = match self.patch.wave {
                Wave::Saw => two * t - one - poly_blep(t, dt),
                Wave::Square => {
                    let naive = t.simd_lt(half).bitselect(one, -one);
                    naive + poly_blep(t, dt) - poly_blep(fract(t + half), dt)
                }
            };
            self.phase = fract(t + dt);

            self.env = self.env_target + (self.env - self.env_target) * self.env_coef;

            let v3 = x - self.ic2;
            let v1 = self.a1 * self.ic1 + self.a2 * v3;
            let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
            self.ic1 = two * v1 - self.ic1;
            self.ic2 = two * v2 - self.ic2;

            let out = (v2 * self.env.min(one) * self.velocity).reduce_add() * gain;
            *l = out;
            *r = out;
        }
    }
}

impl Processor for Synth {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.render(l, r),
            Segment::Event(event) => match event.kind {
                EventKind::NoteOn { pitch, velocity } => self.note_on(pitch, velocity),
                EventKind::NoteOff { pitch } => self.note_off(pitch),
                EventKind::Choke => self.choke(),
                EventKind::Shape(shape) => self.shape = shape,
                EventKind::Param { id, value } => {
                    if let Some(info) = InstKind::Synth.params().get(id as usize) {
                        self.patch.set(id, info.clamp(value));
                        self.derive();
                    }
                }
            },
        });
    }

    fn params(&self) -> &[ParamInfo] {
        InstKind::Synth.params()
    }
}

/// PolyBLEP correction for a unit step at phase 0, across all lanes at once.
#[inline]
fn poly_blep(t: f32x8, dt: f32x8) -> f32x8 {
    let one = f32x8::splat(1.0);
    let a = t / dt;
    let before = a + a - a * a - one;
    let b = (t - one) / dt;
    let after = b * b + b + b + one;
    t.simd_lt(dt).bitselect(
        before,
        t.simd_gt(one - dt).bitselect(after, f32x8::splat(0.0)),
    )
}

#[inline]
fn fract(x: f32x8) -> f32x8 {
    let wrapped = x - x.floor();
    // Guard against rounding up to exactly 1.0.
    wrapped
        .simd_ge(f32x8::splat(1.0))
        .bitselect(f32x8::splat(0.0), wrapped)
}

/// Sets one of `v`'s lanes; there's nothing to set past the last.
fn set_lane(v: &mut f32x8, lane: usize, value: f32) {
    let mut a = v.to_array();
    if let Some(x) = a.get_mut(lane) {
        *x = value;
        *v = f32x8::new(a);
    }
}

fn midi_to_hz(pitch: u8) -> f32 {
    440.0 * 2f32.powf((f32::from(pitch) - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch() -> Patch {
        Patch {
            wave: Wave::Saw,
            amp: AdsrParams {
                attack: 0.005,
                decay: 0.1,
                sustain: 0.5,
                release: 0.05,
            },
            cutoff: 1_000.0,
            env_amount: 3_000.0,
            resonance: 0.3,
            gain: 0.2,
        }
    }

    fn ev(offset: u32, kind: EventKind) -> Event {
        Event { offset, kind }
    }

    #[test]
    fn silent_until_a_note_then_silent_after_release() {
        let mut synth = Synth::new(patch(), 48_000.0);
        let (mut l, mut r) = (vec![1.0; 4_800], vec![1.0; 4_800]);
        synth.process(&[], &mut l, &mut r);
        assert!(l.iter().all(|&x| x == 0.0));

        synth.process(
            &[ev(
                100,
                EventKind::NoteOn {
                    pitch: 57,
                    velocity: 127,
                },
            )],
            &mut l,
            &mut r,
        );
        assert!(l[..100].iter().all(|&x| x == 0.0));
        assert!(l[100..].iter().any(|&x| x.abs() > 0.01));
        assert_eq!(l, r);

        synth.process(&[ev(0, EventKind::NoteOff { pitch: 57 })], &mut l, &mut r);
        for _ in 0..10 {
            synth.process(&[], &mut l, &mut r);
        }
        assert!(synth.silent);
        assert!(l.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn a_glide_slides_from_the_last_pitch_to_its_own() {
        let mut synth = Synth::new(patch(), 48_000.0);
        let (mut l, mut r) = (vec![0.0; 2_400], vec![0.0; 2_400]);
        let on = |pitch| {
            ev(
                0,
                EventKind::NoteOn {
                    pitch,
                    velocity: 100,
                },
            )
        };
        let glide = Shape {
            glide: 4_800,
            ..Shape::default()
        };
        synth.process(&[on(57)], &mut l, &mut r);
        synth.process(&[ev(0, EventKind::Shape(glide)), on(69)], &mut l, &mut r);
        let lane = synth.pitch.iter().position(|&p| p == 69).unwrap();
        let hz = |synth: &Synth| synth.inc.to_array()[lane] * 48_000.0;
        // From A3 to A4, half way after half the time, in pitch.
        assert!((hz(&synth) - 311.1).abs() < 2.0, "{}", hz(&synth));
        synth.process(&[], &mut l, &mut r);
        assert!((hz(&synth) - 440.0).abs() < 0.01, "{}", hz(&synth));
        // The next note starts at its own pitch.
        synth.process(&[on(57)], &mut l, &mut r);
        let lane = synth.pitch.iter().rposition(|&p| p == 57).unwrap();
        assert!((synth.inc.to_array()[lane] * 48_000.0 - 220.0).abs() < 0.01);
    }

    #[test]
    fn settings_come_from_the_song_and_param_events() {
        let mut values = lattice_core::Instrument::new(InstKind::Synth).values;
        values[WAVE as usize] = 1.0;
        values[CUTOFF as usize] = 440.0;
        let mut synth = Synth::with_values(48_000.0, &values);
        assert_eq!(synth.patch.wave, Wave::Square);
        assert_eq!(synth.patch.cutoff, 440.0);
        let set = ev(
            0,
            EventKind::Param {
                id: RESONANCE,
                value: 0.5,
            },
        );
        let (mut l, mut r) = (vec![0.0; 16], vec![0.0; 16]);
        synth.process(&[set], &mut l, &mut r);
        assert_eq!(synth.damping, 1.0);
    }

    #[test]
    fn steals_the_oldest_voice_when_full() {
        let mut synth = Synth::new(patch(), 48_000.0);
        let events: Vec<_> = (0..9)
            .map(|i| {
                ev(
                    i,
                    EventKind::NoteOn {
                        pitch: 60 + i as u8,
                        velocity: 100,
                    },
                )
            })
            .collect();
        let (mut l, mut r) = (vec![0.0; 64], vec![0.0; 64]);
        synth.process(&events, &mut l, &mut r);
        // The ninth note took the first note's voice.
        assert_eq!(synth.pitch[0], 68);
        assert!(synth.stage.iter().all(|&s| s != Stage::Idle));
    }
}
