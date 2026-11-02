//! Switched, the modular engine, after Wendy Carlos's *Switched-On Bach*: a stack of saws, each
//! hard-synced to a master of its own, through a four-pole ladder filter, in stereo.
//!
//! As in the test synth, all eight voices live in SIMD lanes and are computed together, and
//! anything expensive happens at control rate. Each of a voice's saws runs from a master phase at
//! its pitch; the saw itself runs faster, by `grit`, and the master restarts it each time it comes
//! round, which tears the wave into the sync sweep's harmonics. Every jump in a saw, its own or a
//! restart, is smoothed with a polyBLEP a sample late, so the stack stays clean as it's pushed.
//! The saws are summed left and right by where `width` puts them, and each side goes through its
//! own ladder: four one-pole stages with the last fed back, driven through a soft clipper, so the
//! resonance growls before it screams and then sings on its own.

use std::f32::consts::PI;

use lattice_core::InstKind;
use lattice_core::instrument::switched::{
    CONTOUR, GAIN, GRIT, LADDER, SCREAM, SNAP, SPREAD, STACK, WIDTH,
};
use wide::f32x8;

use crate::dsp::decay_coef;
use crate::param::ParamInfo;
use crate::processor::{Event, EventKind, Processor, Segment, Shape, segments};

const VOICES: usize = 8;
/// The most saws in a stack.
const SAWS: usize = 7;
/// Samples between control-rate updates (envelope stages, filter coefficients, sync).
const CONTROL_PERIOD: u64 = 16;
/// The attack heads for this level and stops at 1.0, like an analogue envelope.
const ATTACK_TARGET: f32 = 1.2;
/// Below this, a releasing voice is finished.
const SILENCE: f32 = 1e-4;
/// How far the spread detunes the stack's outermost saws, in cents either side.
const MAX_DETUNE: f32 = 50.0;
/// How much faster than its master a saw runs at full grit and the envelope open.
const MAX_SYNC: f32 = 4.0;
/// How far the envelope opens the ladder at full contour, in octaves.
const CONTOUR_OCTAVES: f32 = 7.0;
/// How far the ladder's cutoff follows the note, as a power of its pitch over middle C.
const KEY_TRACK: f32 = 0.5;
const MIDDLE_C: f32 = 261.63;
/// The ladder's feedback at full scream: past about 4, it rings on its own.
const MAX_FEEDBACK: f32 = 4.4;
/// How much the input's raised as the feedback goes up, to make up the low end resonance takes.
const MAKE_UP: f32 = 0.4;
/// The highest the ladder's cutoff goes, as a fraction of the sample rate.
const TOP: f32 = 0.3;
/// How much the feedback eases off as the cutoff rises, so the ladder rings as readily high as
/// low.
const EASE: f32 = 0.5;
/// The drive into the ladder at no scream and the most it adds at full.
const DRIVE: (f32, f32) = (1.2, 2.5);

/// One saw in a stack: semitones from the note, how far the spread detunes it (from -1 to 1),
/// where `width` pans it (from -1, left, to 1) and its level.
#[derive(Debug, Clone, Copy)]
struct Saw {
    semitones: f32,
    detune: f32,
    pan: f32,
    level: f32,
}

const fn saw(semitones: f32, detune: f32, pan: f32, level: f32) -> Saw {
    Saw {
        semitones,
        detune,
        pan,
        level,
    }
}

const ONE: [Saw; 1] = [saw(0.0, 0.0, 0.0, 1.0)];
const PAIR: [Saw; 2] = [saw(0.0, -0.5, -1.0, 0.71), saw(0.0, 0.5, 1.0, 0.71)];
/// Seven, evenly detuned and panned alternately, so each side gets the whole spread.
const SWARM: [Saw; 7] = [
    saw(0.0, -1.0, -1.0, 0.38),
    saw(0.0, -0.66, 0.66, 0.38),
    saw(0.0, -0.33, -0.33, 0.38),
    saw(0.0, 0.0, 0.0, 0.38),
    saw(0.0, 0.33, 0.33, 0.38),
    saw(0.0, 0.66, -0.66, 0.38),
    saw(0.0, 1.0, 1.0, 0.38),
];
/// An organ's stops: 16' in the middle, 8' and 4' pairs spread out.
const STOPS: [Saw; 5] = [
    saw(-12.0, 0.0, 0.0, 0.6),
    saw(0.0, -0.3, -0.7, 0.5),
    saw(0.0, 0.3, 0.7, 0.5),
    saw(12.0, -0.5, -1.0, 0.3),
    saw(12.0, 0.5, 1.0, 0.3),
];
const FIFTHS: [Saw; 5] = [
    saw(0.0, -0.3, -0.6, 0.5),
    saw(0.0, 0.3, 0.6, 0.5),
    saw(7.0, -0.2, -1.0, 0.4),
    saw(7.0, 0.2, 1.0, 0.4),
    saw(12.0, 0.0, 0.0, 0.4),
];
const STACKS: [&[Saw]; 5] = [&ONE, &PAIR, &SWARM, &STOPS, &FIFTHS];

/// The envelope `snap` makes, in seconds and levels, with the filter's own decay and sustain.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Envelope {
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    filter_decay: f32,
    filter_sustain: f32,
}

impl Envelope {
    /// From a pluck at 0, through an organ's held note at 0.5, to a slow swell at 1.
    fn snap(snap: f32) -> Self {
        if snap < 0.5 {
            let u = snap / 0.5;
            let decay = 0.12 * 16f32.powf(u);
            let sustain = u * u;
            Self {
                attack: 0.001,
                decay,
                sustain,
                release: 0.05 + 0.25 * u,
                filter_decay: 0.6 * decay,
                filter_sustain: 0.5 * sustain,
            }
        } else {
            let u = (snap - 0.5) / 0.5;
            Self {
                attack: 0.001 * 1_500f32.powf(u),
                decay: 2.0,
                sustain: 1.0,
                release: 0.3 + 1.2 * u,
                filter_decay: 1.0,
                filter_sustain: 0.5 + 0.5 * u,
            }
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
pub struct Switched {
    sample_rate: f32,
    stack: &'static [Saw],
    spread: f32,
    width: f32,
    grit: f32,
    ladder: f32,
    scream: f32,
    contour: f32,
    envelope: Envelope,
    gain: f32,

    // Derived from the settings: each saw's pitch over the note, and its level left and right.
    ratio: [f32; SAWS],
    left_level: [f32; SAWS],
    right_level: [f32; SAWS],
    attack_coef: f32,
    decay_coef: f32,
    release_coef: f32,
    filter_decay_coef: f32,
    feedback: f32,
    drive: f32,

    // Audio-rate state, one lane per voice.
    inc: f32x8,
    velocity: f32x8,
    env: f32x8,
    env_target: f32x8,
    env_coef: f32x8,
    filter_env: f32x8,
    filter_target: f32x8,
    filter_coef: f32x8,
    /// How much faster each voice's saws run than their masters.
    sync: f32x8,
    /// The ladder's one-pole coefficient for each voice.
    g: f32x8,
    master: [f32x8; SAWS],
    slave: [f32x8; SAWS],
    /// Each saw's last sample, held back to take the first half of the next jump's polyBLEP.
    held: [f32x8; SAWS],
    ladder_left: [f32x8; 4],
    ladder_right: [f32x8; 4],

    // Control state, one entry per voice.
    stage: [Stage; VOICES],
    pitch: [u8; VOICES],
    started: [u64; VOICES],
    glide_left: [u32; VOICES],
    glide_ratio: [f32; VOICES],
    glide_to: [f32; VOICES],
    last: Option<u8>,
    shape: Shape,
    notes_started: u64,
    silent: bool,
    clock: u64,
}

impl Switched {
    /// Starts at a song's modular settings, one value per parameter, by id.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let zero = f32x8::splat(0.0);
        let mut switched = Self {
            sample_rate,
            stack: &SWARM,
            spread: 0.0,
            width: 0.0,
            grit: 0.0,
            ladder: 1_000.0,
            scream: 0.0,
            contour: 0.0,
            envelope: Envelope::snap(0.0),
            gain: 0.0,
            ratio: [1.0; SAWS],
            left_level: [0.0; SAWS],
            right_level: [0.0; SAWS],
            attack_coef: 0.0,
            decay_coef: 0.0,
            release_coef: 0.0,
            filter_decay_coef: 0.0,
            feedback: 0.0,
            drive: 1.0,
            inc: zero,
            velocity: zero,
            env: zero,
            env_target: zero,
            env_coef: zero,
            filter_env: zero,
            filter_target: zero,
            filter_coef: zero,
            sync: f32x8::splat(1.0),
            g: zero,
            master: [zero; SAWS],
            slave: [zero; SAWS],
            held: [zero; SAWS],
            ladder_left: [zero; 4],
            ladder_right: [zero; 4],
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
        };
        for info in InstKind::Switched.params() {
            let value = values
                .get(info.id as usize)
                .copied()
                .unwrap_or(info.default);
            switched.set(info.id, info.clamp(value));
        }
        switched
    }

    /// Called with each setting, once at the start and whenever one changes. `value` is in range.
    fn set(&mut self, id: u32, value: f32) {
        match id {
            STACK => {
                // The last for a value past the end.
                if let Some(stack) = STACKS.get(value.round() as usize).or(STACKS.last()) {
                    self.stack = stack;
                }
            }
            SPREAD => self.spread = value,
            WIDTH => self.width = value,
            GRIT => self.grit = value,
            LADDER => self.ladder = value,
            SCREAM => self.scream = value,
            CONTOUR => self.contour = value,
            SNAP => self.envelope = Envelope::snap(value),
            GAIN => self.gain = value,
            _ => {}
        }
        self.derive();
    }

    /// Works out what follows from the settings. Notes sounding pick up new envelope times at
    /// their next stage.
    fn derive(&mut self) {
        let saws = self.stack.iter();
        let derived = self
            .ratio
            .iter_mut()
            .zip(self.left_level.iter_mut().zip(&mut self.right_level));
        for (saw, (ratio, (left, right))) in saws.zip(derived) {
            let cents = saw.semitones * 100.0 + saw.detune * self.spread * MAX_DETUNE;
            *ratio = 2f32.powf(cents / 1200.0);
            // Equal power, so a saw's as loud wherever it's panned.
            let pan = saw.pan * self.width;
            *left = saw.level * (0.5 * (1.0 - pan)).sqrt();
            *right = saw.level * (0.5 * (1.0 + pan)).sqrt();
        }
        let envelope = self.envelope;
        // Time constant for an exponential towards ATTACK_TARGET that passes 1.0 after `attack`.
        let attack_tau = envelope.attack / -(1.0 - 1.0 / ATTACK_TARGET).ln();
        self.attack_coef = decay_coef(attack_tau, self.sample_rate);
        self.decay_coef = decay_coef(envelope.decay, self.sample_rate);
        self.release_coef = decay_coef(envelope.release, self.sample_rate);
        self.filter_decay_coef = decay_coef(envelope.filter_decay, self.sample_rate);
        self.feedback = MAX_FEEDBACK * self.scream;
        self.drive = DRIVE.0 + DRIVE.1 * self.scream;
    }

    fn note_on(&mut self, pitch: u8, velocity: u8) {
        // A free voice, else the oldest released one, else the oldest of all.
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
        let restarted = *stage == Stage::Idle;

        self.notes_started += 1;
        *stage = Stage::Attack;
        *voice_pitch = pitch;
        *started = self.notes_started;
        set_lane(&mut self.inc, i, from);
        set_lane(&mut self.velocity, i, f32::from(velocity) / 127.0);
        set_lane(&mut self.env_target, i, ATTACK_TARGET);
        set_lane(&mut self.env_coef, i, self.attack_coef);
        set_lane(&mut self.filter_target, i, ATTACK_TARGET);
        set_lane(&mut self.filter_coef, i, self.attack_coef);
        if restarted {
            // A silent voice starts its saws spread through their cycles, as free-running
            // oscillators would be, so they don't all jump at once; one still sounding runs on.
            let saws = self
                .master
                .iter_mut()
                .zip(&mut self.slave)
                .zip(&mut self.held);
            for (k, ((master, slave), held)) in saws.enumerate() {
                let phase = (k as f32 * 0.382).fract();
                set_lane(master, i, phase);
                set_lane(slave, i, phase);
                set_lane(held, i, 2.0 * phase - 1.0);
            }
            for state in self.ladder_left.iter_mut().chain(&mut self.ladder_right) {
                set_lane(state, i, 0.0);
            }
        }
        self.silent = false;
    }

    fn note_off(&mut self, pitch: u8) {
        self.release(|voice_pitch| voice_pitch == pitch);
    }

    fn choke(&mut self) {
        self.release(|_| true);
    }

    /// Releases the voices sounding whose pitch passes `which`.
    fn release(&mut self, which: impl Fn(u8) -> bool) {
        let mut target = self.env_target.to_array();
        let mut coef = self.env_coef.to_array();
        let mut filter_target = self.filter_target.to_array();
        let mut filter_coef = self.filter_coef.to_array();
        let lanes =
            (target.iter_mut().zip(&mut coef)).zip(filter_target.iter_mut().zip(&mut filter_coef));
        let voices = self.stage.iter_mut().zip(&self.pitch);
        for ((stage, &pitch), ((target, coef), (filter_target, filter_coef))) in voices.zip(lanes) {
            if which(pitch) && matches!(stage, Stage::Attack | Stage::Decay) {
                *stage = Stage::Release;
                (*target, *coef) = (0.0, self.release_coef);
                (*filter_target, *filter_coef) = (0.0, self.release_coef);
            }
        }
        self.env_target = f32x8::new(target);
        self.env_coef = f32x8::new(coef);
        self.filter_target = f32x8::new(filter_target);
        self.filter_coef = f32x8::new(filter_coef);
    }

    /// Envelope stages, glides, the ladder's cutoff and the sync, every `CONTROL_PERIOD` samples.
    fn update_control(&mut self) {
        let mut env = self.env.to_array();
        let mut target = self.env_target.to_array();
        let mut coef = self.env_coef.to_array();
        let mut filter_target = self.filter_target.to_array();
        let mut filter_coef = self.filter_coef.to_array();
        let lanes = (env.iter_mut().zip(target.iter_mut().zip(&mut coef)))
            .zip(filter_target.iter_mut().zip(&mut filter_coef));
        let envelope = self.envelope;
        for (stage, ((env, (target, coef)), (filter_target, filter_coef))) in
            self.stage.iter_mut().zip(lanes)
        {
            match stage {
                Stage::Attack if *env >= 1.0 => {
                    *stage = Stage::Decay;
                    (*target, *coef) = (envelope.sustain, self.decay_coef);
                    (*filter_target, *filter_coef) =
                        (envelope.filter_sustain, self.filter_decay_coef);
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
        self.filter_target = f32x8::new(filter_target);
        self.filter_coef = f32x8::new(filter_coef);
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

        // The ladder opens with the filter's envelope, harder for harder notes, and follows the
        // keys; the saws run faster than their masters as it opens, by the grit.
        let rate = self.sample_rate;
        // Kept well under Nyquist, where the feedback's sample of delay would ring.
        let top = (TOP * rate).max(20.0);
        let filter_env = self.filter_env.to_array();
        let velocity = self.velocity.to_array();
        let inc = self.inc.to_array();
        let mut g = [0.0; VOICES];
        let mut sync = [1.0; VOICES];
        let voices = filter_env.iter().zip(&velocity).zip(&inc);
        for ((((&opened, &velocity), &inc), g), sync) in voices.zip(&mut g).zip(&mut sync) {
            let opened = opened.clamp(0.0, 1.0);
            let hz = (inc * rate).max(1.0);
            let octaves = CONTOUR_OCTAVES * self.contour * opened * (0.6 + 0.4 * velocity);
            let cutoff = self.ladder * (hz / MIDDLE_C).powf(KEY_TRACK) * 2f32.powf(octaves);
            let t = (PI * cutoff.clamp(20.0, top) / rate).tan();
            *g = t / (1.0 + t);
            *sync = 1.0 + (MAX_SYNC - 1.0) * self.grit * (0.35 + 0.65 * opened);
        }
        self.g = f32x8::new(g);
        self.sync = f32x8::new(sync);
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let zero = f32x8::splat(0.0);
        let one = f32x8::splat(1.0);
        let two = f32x8::splat(2.0);
        let half = f32x8::splat(0.5);
        // Past this, a saw's phase at its master's restart had already come round itself.
        let wrapped = f32x8::splat(1.001);
        let feedback = f32x8::splat(self.feedback);
        let drive = f32x8::splat(self.drive);
        let make_up = f32x8::splat(1.0 + MAKE_UP * self.feedback);
        let saws = self.stack.len();

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

            let inc = self.inc.max(f32x8::splat(1e-6));
            let (mut sum_left, mut sum_right) = (zero, zero);
            let state = (self.master.iter_mut().zip(&mut self.slave)).zip(&mut self.held);
            let levels = self
                .ratio
                .iter()
                .zip(&self.left_level)
                .zip(&self.right_level);
            for (((master, slave), held), ((&ratio, &gain_left), &gain_right)) in
                state.zip(levels).take(saws)
            {
                let dm = inc * f32x8::splat(ratio);
                let ds = dm * self.sync;
                let m = *master + dm;
                let s = *slave + ds;
                // The master coming round restarts the saw, from wherever it had got to; else the
                // saw may come round on its own. Either way it falls by `drop`, `since` of a
                // sample ago.
                let restart = m.simd_ge(one);
                let since_restart = (m - one) / dm;
                let at_restart = s - since_restart * ds;
                let at_restart = at_restart
                    .simd_gt(wrapped)
                    .bitselect(at_restart - one, at_restart);
                let round = s.simd_ge(one);
                let drop = restart.bitselect(two * at_restart, round.bitselect(two, zero));
                let since = restart.bitselect(since_restart, round.bitselect((s - one) / ds, zero));
                let m = restart.bitselect(m - one, m);
                let s = restart.bitselect(m * self.sync, round.bitselect(s - one, s));
                *master = m;
                *slave = s;
                // The polyBLEP: the sample before the jump, held back, takes the first half, and
                // this one the second.
                let step = half * drop;
                let out = *held - step * since * since;
                let after = one - since;
                *held = two * s - one + step * after * after;
                sum_left += out * f32x8::splat(gain_left);
                sum_right += out * f32x8::splat(gain_right);
            }

            self.env = self.env_target + (self.env - self.env_target) * self.env_coef;
            self.filter_env =
                self.filter_target + (self.filter_env - self.filter_target) * self.filter_coef;

            let amp = self.env.min(one) * self.velocity;
            let g = self.g;
            let feedback = feedback * (one - f32x8::splat(EASE) * g);
            let out_left = ladder(
                &mut self.ladder_left,
                sum_left * make_up,
                g,
                feedback,
                drive,
            );
            let out_right = ladder(
                &mut self.ladder_right,
                sum_right * make_up,
                g,
                feedback,
                drive,
            );
            *l = (out_left * amp).reduce_add() * self.gain;
            *r = (out_right * amp).reduce_add() * self.gain;
        }
    }
}

/// One sample through a ladder: the input less the last stage, fed back, driven through a soft
/// clipper, then four one-pole low-passes in a row. The clipper keeps it from running away
/// however hard it's fed back.
#[inline]
fn ladder(stages: &mut [f32x8; 4], x: f32x8, g: f32x8, feedback: f32x8, drive: f32x8) -> f32x8 {
    let [a, b, c, d] = stages;
    let u = soft_clip(drive * (x - feedback * *d));
    *a += g * (u - *a);
    *b += g * (*a - *b);
    *c += g * (*b - *c);
    *d += g * (*c - *d);
    *d
}

/// A rational tanh, close enough between -3 and 3 and flat at ±1 beyond.
#[inline]
fn soft_clip(x: f32x8) -> f32x8 {
    let x = x.max(f32x8::splat(-3.0)).min(f32x8::splat(3.0));
    let x2 = x * x;
    x * (f32x8::splat(27.0) + x2) / (f32x8::splat(27.0) + f32x8::splat(9.0) * x2)
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

impl Processor for Switched {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.render(l, r),
            Segment::Event(event) => match event.kind {
                EventKind::NoteOn { pitch, velocity } => self.note_on(pitch, velocity),
                EventKind::NoteOff { pitch } => self.note_off(pitch),
                EventKind::Choke => self.choke(),
                EventKind::Shape(shape) => self.shape = shape,
                EventKind::Param { id, value } => {
                    if let Some(info) = InstKind::Switched.params().get(id as usize) {
                        self.set(id, info.clamp(value));
                    }
                }
            },
        });
    }

    fn params(&self) -> &[ParamInfo] {
        InstKind::Switched.params()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn modular(settings: &[(u32, f32)]) -> Switched {
        let mut values = lattice_core::Instrument::new(InstKind::Switched).values;
        for &(id, value) in settings {
            values[id as usize] = value;
        }
        Switched::with_values(RATE, &values)
    }

    fn on(pitch: u8) -> Event {
        Event {
            offset: 0,
            kind: EventKind::NoteOn {
                pitch,
                velocity: 110,
            },
        }
    }

    fn play(m: &mut Switched, events: &[Event], seconds: f32) -> (Vec<f32>, Vec<f32>) {
        let n = (seconds * RATE) as usize;
        let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
        m.process(events, &mut l, &mut r);
        (l, r)
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|x| x * x).sum::<f32>() / x.len() as f32).sqrt()
    }

    /// The fundamental, by counting rising zero crossings of a low-passed copy.
    fn pitch(x: &[f32]) -> f32 {
        let mut y = 0.0;
        let smooth: Vec<f32> = x
            .iter()
            .map(|&x| {
                y += 0.02 * (x - y);
                y
            })
            .collect();
        let crossings: Vec<usize> = (1..smooth.len())
            .filter(|&i| smooth[i - 1] < 0.0 && smooth[i] >= 0.0)
            .collect();
        let (first, last) = (crossings[0], *crossings.last().unwrap());
        (crossings.len() - 1) as f32 * RATE / (last - first) as f32
    }

    #[test]
    fn silent_until_a_note_and_after_its_release() {
        let mut m = modular(&[]);
        let (l, _) = play(&mut m, &[], 0.1);
        assert!(l.iter().all(|&x| x == 0.0));
        let (l, r) = play(&mut m, &[on(57)], 0.3);
        assert!(rms(&l) > 0.01 && rms(&r) > 0.01);
        play(
            &mut m,
            &[Event {
                offset: 0,
                kind: EventKind::NoteOff { pitch: 57 },
            }],
            3.0,
        );
        assert!(m.silent);
    }

    #[test]
    fn one_saw_plays_in_tune_and_a_wide_stack_in_stereo() {
        let mut one = modular(&[(STACK, 0.0), (LADDER, 400.0), (CONTOUR, 0.0), (SNAP, 0.5)]);
        let (l, r) = play(&mut one, &[on(57)], 1.0);
        assert_eq!(l, r, "one saw is in the middle");
        let hz = pitch(&l[4_800..]);
        assert!((hz - 220.0).abs() < 1.0, "{hz}");

        let mut swarm = modular(&[(STACK, 2.0), (WIDTH, 1.0), (SPREAD, 0.5)]);
        let (l, r) = play(&mut swarm, &[on(57)], 0.5);
        let difference: Vec<f32> = l.iter().zip(&r).map(|(a, b)| a - b).collect();
        assert!(rms(&difference) > 0.2 * rms(&l), "the swarm spreads out");
    }

    #[test]
    fn grit_and_scream_brighten_it_and_nothing_runs_away() {
        // Brightness as the share of the signal a one-sample difference keeps.
        let bright = |x: &[f32]| {
            let d: Vec<f32> = x.windows(2).map(|w| w[1] - w[0]).collect();
            rms(&d) / rms(x)
        };
        let base = [(STACK, 0.0), (LADDER, 2_000.0), (CONTOUR, 0.0), (SNAP, 0.5)];
        let (plain, _) = play(&mut modular(&base), &[on(45)], 0.5);
        let (gritty, _) = play(
            &mut modular(&[base.as_slice(), &[(GRIT, 1.0)]].concat()),
            &[on(45)],
            0.5,
        );
        assert!(bright(&gritty) > 1.3 * bright(&plain));

        // Screaming, it sings at the cutoff on its own, and stays bounded.
        let mut screaming = modular(&[(SCREAM, 1.0), (LADDER, 1_000.0), (SNAP, 0.5)]);
        let (l, r) = play(&mut screaming, &[on(36), on(48), on(60), on(72)], 2.0);
        assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() < 2.0));
        assert!(rms(&l) > 0.02);
    }

    #[test]
    fn a_pluck_dies_away_while_held_and_a_swell_takes_its_time() {
        let mut pluck = modular(&[(SNAP, 0.0)]);
        let (l, _) = play(&mut pluck, &[on(57)], 1.0);
        assert!(rms(&l[43_200..]) < 0.05 * rms(&l[..4_800]));

        let mut swell = modular(&[(SNAP, 1.0)]);
        let (l, _) = play(&mut swell, &[on(57)], 2.0);
        assert!(rms(&l[..4_800]) < 0.2 * rms(&l[86_400..]));
    }

    #[test]
    fn a_glide_slides_from_the_last_pitch() {
        let mut m = modular(&[(STACK, 0.0)]);
        play(&mut m, &[on(57)], 0.1);
        let glide = Shape {
            glide: 4_800,
            ..Shape::default()
        };
        play(
            &mut m,
            &[
                Event {
                    offset: 0,
                    kind: EventKind::Shape(glide),
                },
                on(69),
            ],
            0.05,
        );
        let lane = m.pitch.iter().position(|&p| p == 69).unwrap();
        let hz = m.inc.to_array()[lane] * RATE;
        assert!((hz - 311.1).abs() < 3.0, "{hz}");
    }
}
