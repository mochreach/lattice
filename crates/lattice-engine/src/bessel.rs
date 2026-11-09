//! Bessel, the FM synthesiser, after Friedrich Bessel, whose functions give the strength of each
//! partial that frequency modulation makes.
//!
//! Four operators, each a sine oscillator with its own envelope. An operator that's heard is a
//! carrier; one that isn't bends another's phase, a modulator. A sine bent by a sine at `ratio`
//! times its pitch sprouts sidebands either side of it, `ratio` times the note apart, as strong
//! as Bessel's functions of the modulation index say: a little bending adds a few, warm; a lot
//! adds many, bright and metallic. Ratios that are whole numbers keep the sidebands on the
//! note's harmonics; others scatter them between, as a bell's are. The modulator's envelope
//! moves the brightness through the note, which is what makes FM sound alive. Strictly it's
//! phase modulation, as Yamaha's were too: the same sidebands, and steady in pitch however hard
//! it's bent.
//!
//! The algorithms wire the operators together, always a higher one into a lower, so each sample
//! they run from 4 down to 1. Operator 4 can modulate itself, its last two outputs averaged into
//! its phase: a sine, through something like a saw, to noise. An operator's pitch can follow the
//! note or stay put (`track`), which with the pitch envelope makes drums: a sine swept down is a
//! kick, a high fixed pair clanging is a cowbell, feedback noise is a snare's rattle.

use lattice_core::InstKind;
use lattice_core::instrument::bessel::{
    ALGORITHM, ATTACK, DECAY, DETUNE, FEEDBACK, GAIN, KEY, KEYSCALE, LEVEL, OPS, PER_OP, PITCH,
    RATIO, RELEASE, SUSTAIN, SWEEP, TRACK, TUNING, VELOCITY, WAVE,
};
use lattice_core::tuning;

use wide::f32x8;

use crate::param::ParamInfo;
use crate::processor::{Event, EventKind, Processor, Segment, segments};
use crate::rhythmicon::SIXTY_DB;

const VOICES: usize = 8;
/// The deepest a modulator at full level bends its carrier's phase, in cycles: 4π radians, as
/// deep as a DX7's.
const MAX_INDEX: f32 = 2.0;
/// The deepest operator 4 bends itself, in cycles, at full feedback: past about half of this,
/// it's noise.
const MAX_FEEDBACK: f32 = 0.8;
/// C-4, which a fixed operator's ratio multiplies, and key scaling's middle.
const REF_HZ: f32 = 261.625_58;
const REF_NOTE: f32 = 60.0;
/// Samples between moving the pitch envelope and retuning, and checking for voices fallen
/// silent.
const CONTROL_PERIOD: u32 = 32;
/// The pitch envelope's length, in its time constants.
const SWEPT: f32 = 7.0;
/// Below this, a carrier's envelope has finished.
const SILENCE: f32 = 1e-4;
/// How long a choked voice takes to fade, in seconds to fall by 1/e.
const CHOKE: f32 = 0.003;
/// The output, so a few notes together leave headroom.
const OUT: f32 = 0.5;
/// The highest an operator plays, as a share of the sample rate, short of folding back down.
const HIGHEST: f32 = 0.45;

/// How the operators connect: for each, the operators modulating it, a bit each, operator 1 the
/// lowest; and which are heard.
#[derive(Debug, Clone, Copy)]
struct Algorithm {
    mods: [u8; OPS],
    carriers: u8,
}

/// In [`lattice_core::instrument::bessel::ALGORITHMS`]' order.
const ALGORITHMS: [Algorithm; 8] = [
    // stack: 4→3→2→1.
    Algorithm {
        mods: [0b0010, 0b0100, 0b1000, 0],
        carriers: 0b0001,
    },
    // branch: 4→3→1, 2→1.
    Algorithm {
        mods: [0b0110, 0, 0b1000, 0],
        carriers: 0b0001,
    },
    // crown: 2, 3 and 4 into 1.
    Algorithm {
        mods: [0b1110, 0, 0, 0],
        carriers: 0b0001,
    },
    // pairs: 4→3 and 2→1, both heard.
    Algorithm {
        mods: [0b0010, 0, 0b1000, 0],
        carriers: 0b0101,
    },
    // fan: 4 into 1, 2 and 3.
    Algorithm {
        mods: [0b1000, 0b1000, 0b1000, 0],
        carriers: 0b0111,
    },
    // chain: 4→3→2, and 1 alone.
    Algorithm {
        mods: [0, 0b0100, 0b1000, 0],
        carriers: 0b0011,
    },
    // trio: 4→3, heard with 1 and 2.
    Algorithm {
        mods: [0, 0, 0b1000, 0],
        carriers: 0b0111,
    },
    // organ: all four heard.
    Algorithm {
        mods: [0, 0, 0, 0],
        carriers: 0b1111,
    },
];

/// `sin(2πx)` in every lane, `x` in cycles, as a polynomial: accurate to about 1e-5, much quicker
/// than `sin`.
#[inline(always)]
fn sine(x: f32x8) -> f32x8 {
    let quarter = f32x8::splat(0.25);
    // Into -½ to ½, then folded into -¼ to ¼, where the polynomial's good.
    let x = x - x.round();
    let half = (x.simd_lt(f32x8::ZERO)).bitselect(f32x8::splat(-0.5), f32x8::HALF);
    let x = x.abs().simd_gt(quarter).bitselect(half - x, x);
    let t = x * f32x8::splat(std::f32::consts::TAU);
    let t2 = t * t;
    let p = f32x8::splat(1.0 / 362_880.0);
    let p = p.mul_add(t2, f32x8::splat(-1.0 / 5040.0));
    let p = p.mul_add(t2, f32x8::splat(1.0 / 120.0));
    let p = p.mul_add(t2, f32x8::splat(-1.0 / 6.0));
    let p = p.mul_add(t2, f32x8::ONE);
    t * p
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Attack,
    /// Falling to the sustain level, and holding there.
    Decay,
    Release,
}

/// An operator's settings, as set.
#[derive(Debug, Clone, Copy, Default)]
struct OpSettings {
    ratio: f32,
    detune: f32,
    level: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    velocity: f32,
    track: f32,
    wave: u8,
}

/// A value for each voice, side by side.
type Lanes = f32x8;

/// Puts `x` in voice `i`'s lane.
fn put(lanes: &mut Lanes, i: usize, x: f32) {
    let mut array = lanes.to_array();
    if let Some(lane) = array.get_mut(i) {
        *lane = x;
    }
    *lanes = f32x8::new(array);
}

/// What's in voice `i`'s lane.
fn get(lanes: Lanes, i: usize) -> f32 {
    lanes.to_array().get(i).copied().unwrap_or(0.0)
}

/// One operator in every voice, a lane each, so the voices run side by side.
#[derive(Debug, Clone, Copy)]
struct Op {
    /// In cycles, and cycles a sample.
    phase: Lanes,
    inc: Lanes,
    /// The envelope, which each sample becomes `target + (env - target) * keep + rise`, until
    /// it's risen to 1.
    env: Lanes,
    target: Lanes,
    keep: Lanes,
    rise: Lanes,
    /// What it falls to, and the share kept each sample falling, once it's risen.
    sustain: Lanes,
    fall: Lanes,
    stage: [Stage; VOICES],
    /// How loud it is if it's heard, or how far it bends, in cycles, if it modulates.
    amount: Lanes,
}

impl Op {
    const SILENT: Self = Self {
        phase: f32x8::ZERO,
        inc: f32x8::ZERO,
        env: f32x8::ZERO,
        target: f32x8::ZERO,
        keep: f32x8::ZERO,
        rise: f32x8::ZERO,
        sustain: f32x8::ZERO,
        fall: f32x8::ZERO,
        stage: [Stage::Release; VOICES],
        amount: f32x8::ZERO,
    };

    /// A sample of every voice's operator, bent by `bend`, in `wave`: what it puts out, before
    /// its amount. Every voice, sounding or not, so they run together.
    #[inline(always)]
    fn run(&mut self, bend: Lanes, wave: impl Fn(Lanes) -> Lanes) -> Lanes {
        let y = wave(self.phase + bend) * self.env;
        let phase = self.phase + self.inc;
        self.phase = phase - phase.floor();
        let env = (self.env - self.target).mul_add(self.keep, self.target) + self.rise;
        let risen = env.simd_ge(f32x8::ONE);
        self.env = risen.bitselect(f32x8::ONE, env);
        self.target = risen.bitselect(self.sustain, self.target);
        self.keep = risen.bitselect(self.fall, self.keep);
        self.rise = risen.bitselect(f32x8::ZERO, self.rise);
        y
    }
}

#[derive(Debug, Clone, Copy)]
struct Voice {
    pitch: u8,
    /// Its pitch in the tuning.
    hz: f32,
    /// How hard it was played, 0 to 1.
    velocity: f32,
    /// Seconds since it was struck, for the pitch envelope.
    age: f32,
    sounding: bool,
    held: bool,
    choked: bool,
    started: u64,
}

#[derive(Debug, Clone)]
pub struct Bessel {
    sample_rate: f32,
    voices: [Voice; VOICES],
    ops: [Op; OPS],
    settings: [OpSettings; OPS],
    /// Operator 4's last two outputs, for its feedback.
    last: (Lanes, Lanes),
    /// Each voice's level, and the share of it kept each sample, less than all once choked.
    fade: Lanes,
    fading: Lanes,
    algorithm: Algorithm,
    feedback: f32,
    pitch: f32,
    sweep: f32,
    keyscale: f32,
    tuning: usize,
    key: usize,
    gain: f32,
    fade_coef: f32,
    strikes: u64,
    clock: u32,
}

impl Bessel {
    /// Starts at a song's FM settings, one value per parameter, by id.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let voice = Voice {
            pitch: 0,
            hz: REF_HZ,
            velocity: 0.0,
            age: 0.0,
            sounding: false,
            held: false,
            choked: false,
            started: 0,
        };
        let mut bessel = Self {
            sample_rate,
            voices: [voice; VOICES],
            ops: [Op::SILENT; OPS],
            settings: [OpSettings::default(); OPS],
            last: (f32x8::ZERO, f32x8::ZERO),
            fade: f32x8::ONE,
            fading: f32x8::ONE,
            algorithm: ALGORITHMS[3],
            feedback: 0.0,
            pitch: 0.0,
            sweep: 0.05,
            keyscale: 0.0,
            tuning: 0,
            key: 0,
            gain: 0.0,
            fade_coef: crate::dsp::decay_coef(CHOKE, sample_rate),
            strikes: 0,
            clock: 0,
        };
        for info in InstKind::Bessel.params() {
            let value = values
                .get(info.id as usize)
                .copied()
                .unwrap_or(info.default);
            bessel.set(info.id, info.clamp(value));
        }
        bessel
    }

    /// Called with each setting, once at the start and whenever one changes. `value` is in range.
    /// Voices already playing take it up at once.
    fn set(&mut self, id: u32, value: f32) {
        match id {
            ALGORITHM => {
                if let Some(&algorithm) = ALGORITHMS.get(value.round() as usize) {
                    self.algorithm = algorithm;
                }
            }
            FEEDBACK => self.feedback = value,
            PITCH => self.pitch = value,
            SWEEP => self.sweep = value,
            KEYSCALE => self.keyscale = value,
            TUNING => self.tuning = value.round() as usize,
            KEY => self.key = value.round() as usize,
            GAIN => self.gain = value,
            _ => {
                let Some(offset) = id.checked_sub(8) else {
                    return;
                };
                let Some(op) = self.settings.get_mut((offset / PER_OP) as usize) else {
                    return;
                };
                match offset % PER_OP {
                    RATIO => op.ratio = value,
                    DETUNE => op.detune = value,
                    LEVEL => op.level = value,
                    ATTACK => op.attack = value,
                    DECAY => op.decay = value,
                    SUSTAIN => op.sustain = value,
                    RELEASE => op.release = value,
                    VELOCITY => op.velocity = value,
                    TRACK => op.track = value,
                    WAVE => op.wave = value.round() as u8,
                    _ => {}
                }
            }
        }
        if matches!(id, TUNING | KEY) {
            for voice in &mut self.voices {
                voice.hz = Self::pitch_hz(voice.pitch, self.tuning, self.key);
            }
        }
        for i in 0..VOICES {
            if self.voices.get(i).is_some_and(|v| v.sounding) {
                self.tune(i);
            }
        }
    }

    /// A note's pitch in a tuning, in Hz.
    fn pitch_hz(pitch: u8, tuning: usize, key: usize) -> f32 {
        let cents = tuning::offset(pitch, tuning, key);
        440.0 * 2f32.powf((f32::from(pitch) - 69.0 + cents / 100.0) / 12.0)
    }

    /// Sets voice `i`'s operators' pitches, envelopes and levels, from the settings, the note,
    /// how hard it was played and how far the pitch envelope has fallen.
    fn tune(&mut self, i: usize) {
        let rate = self.sample_rate;
        let carriers = self.algorithm.carriers;
        let heard = carriers.count_ones().max(1) as f32;
        let Some(voice) = self.voices.get(i) else {
            return;
        };
        // The pitch envelope is all but over after seven of its time constants, so it ends there.
        let sweep = if voice.age < SWEPT * self.sweep {
            self.pitch * (-voice.age / self.sweep).exp()
        } else {
            0.0
        };
        // Octaves above C-4: key scaling shortens the envelopes and softens the modulators of
        // notes above it, and lengthens those below.
        let octaves = (f32::from(voice.pitch) - REF_NOTE) / 12.0;
        let times = 2f32.powf(-octaves * self.keyscale);
        let brightness = 2f32.powf(-octaves * self.keyscale * 0.5).min(1.0);
        let swept = 2f32.powf(sweep / 12.0);
        for (n, (op, set)) in self.ops.iter_mut().zip(&self.settings).enumerate() {
            let tracked = REF_HZ * (voice.hz / REF_HZ).powf(set.track);
            let hz = set.ratio * 2f32.powf(set.detune / 1200.0) * tracked * swept;
            put(&mut op.inc, i, (hz / rate).min(HIGHEST));
            let rise = 1.0 / (set.attack * times * rate).max(1.0);
            let fall = (-SIXTY_DB / (set.decay * times * rate)).exp();
            let fade = (-SIXTY_DB / (set.release * times * rate)).exp();
            put(&mut op.sustain, i, set.sustain);
            put(&mut op.fall, i, fall);
            let (target, keep, rise) = match op.stage.get(i) {
                Some(Stage::Attack) => (0.0, 1.0, rise),
                Some(Stage::Decay) => (set.sustain, fall, 0.0),
                _ => (0.0, fade, 0.0),
            };
            put(&mut op.target, i, target);
            put(&mut op.keep, i, keep);
            put(&mut op.rise, i, rise);
            let touch = 1.0 - set.velocity + set.velocity * voice.velocity;
            let level = set.level * set.level * touch;
            let amount = if carriers >> n & 1 == 1 {
                level / heard
            } else {
                level * MAX_INDEX * brightness
            };
            put(&mut op.amount, i, amount);
        }
    }

    /// Moves voice `i`'s operators' envelopes on to `stage`.
    fn stage(&mut self, i: usize, stage: Stage) {
        for op in &mut self.ops {
            if let Some(s) = op.stage.get_mut(i) {
                *s = stage;
            }
        }
        self.tune(i);
    }

    fn note_on(&mut self, pitch: u8, velocity: u8) {
        // The voice already playing this note, else a silent one, else the oldest. The first of
        // equals wins.
        let free = |v: &Voice| match (v.sounding, v.pitch == pitch) {
            (true, true) => (0, 0),
            (false, _) => (1, 0),
            (true, false) => (2, v.started),
        };
        let voices = self.voices.iter_mut().enumerate();
        // There are always voices, so there's always one to take.
        let Some((i, voice)) = voices.min_by_key(|(_, v)| free(v)) else {
            return;
        };
        self.strikes += 1;
        let restruck = voice.sounding && voice.pitch == pitch && !voice.choked;
        *voice = Voice {
            pitch,
            hz: Self::pitch_hz(pitch, self.tuning, self.key),
            velocity: (f32::from(velocity) / 127.0).powf(1.5),
            age: 0.0,
            sounding: true,
            held: true,
            choked: false,
            started: self.strikes,
        };
        if !restruck {
            // From the start of the cycle and silence, so every note, and every drum hit, starts
            // the same. Played again, it rises from wherever it had fallen to, so it doesn't
            // click.
            for op in &mut self.ops {
                put(&mut op.phase, i, 0.0);
                put(&mut op.env, i, 0.0);
            }
            put(&mut self.last.0, i, 0.0);
            put(&mut self.last.1, i, 0.0);
        }
        put(&mut self.fade, i, 1.0);
        put(&mut self.fading, i, 1.0);
        self.stage(i, Stage::Attack);
    }

    /// An arpeggio's next note: the latest voice held at `from` goes on at `to`, its operators'
    /// envelopes where they are; with none, `to` starts afresh.
    fn retune(&mut self, from: u8, to: u8, velocity: u8) {
        let (tuning, key) = (self.tuning, self.key);
        let held = (self.voices.iter_mut())
            .filter(|v| v.pitch == from && v.held && v.sounding)
            .max_by_key(|v| v.started);
        match held {
            Some(voice) => {
                voice.pitch = to;
                voice.hz = Self::pitch_hz(to, tuning, key);
            }
            None => self.note_on(to, velocity),
        }
    }

    fn note_off(&mut self, pitch: u8) {
        for i in 0..VOICES {
            let Some(voice) = self.voices.get_mut(i) else {
                continue;
            };
            if voice.pitch == pitch && voice.held && voice.sounding {
                voice.held = false;
                self.stage(i, Stage::Release);
            }
        }
    }

    fn choke(&mut self) {
        for (i, voice) in self.voices.iter_mut().enumerate() {
            if voice.sounding {
                voice.choked = true;
                voice.held = false;
                put(&mut self.fading, i, self.fade_coef);
            }
        }
    }

    /// Every `CONTROL_PERIOD` samples: envelopes that have risen fall, the pitch envelope moves
    /// on, and voices whose carriers have all faded stop.
    fn update_control(&mut self) {
        let step = CONTROL_PERIOD as f32 / self.sample_rate;
        let carriers = self.algorithm.carriers;
        let sweeping = self.pitch != 0.0;
        for i in 0..VOICES {
            let Some(voice) = self.voices.get_mut(i) else {
                continue;
            };
            if !voice.sounding {
                continue;
            }
            voice.age += step;
            let mut faded = true;
            for (n, op) in self.ops.iter_mut().enumerate() {
                let (env, rise) = (get(op.env, i), get(op.rise, i));
                let Some(stage) = op.stage.get_mut(i) else {
                    continue;
                };
                if *stage == Stage::Attack && rise == 0.0 {
                    *stage = Stage::Decay;
                }
                if carriers >> n & 1 == 1 && (*stage == Stage::Attack || env >= SILENCE) {
                    faded = false;
                }
            }
            let choked = voice.choked && get(self.fade, i) < SILENCE;
            voice.sounding = !faded && !choked;
            if !voice.sounding {
                // Silent, in case another voice keeps the rest running.
                for op in &mut self.ops {
                    put(&mut op.env, i, 0.0);
                    put(&mut op.rise, i, 0.0);
                    put(&mut op.target, i, 0.0);
                }
                put(&mut self.fade, i, 0.0);
            } else if sweeping && voice.age - step < SWEPT * self.sweep {
                // Once more after the pitch envelope ends, to land on the note.
                self.tune(i);
            }
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.voices.iter().all(|v| !v.sounding) {
            left.fill(0.0);
            right.fill(0.0);
            return;
        }
        let scale = OUT * self.gain;
        let feedback = f32x8::splat(self.feedback * self.feedback * MAX_FEEDBACK * 0.5);
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.clock.is_multiple_of(CONTROL_PERIOD) {
                self.update_control();
            }
            self.clock = self.clock.wrapping_add(1);
            let Algorithm { mods, carriers } = self.algorithm;
            // Each operator's output in every voice, from 4 down, as the algorithms only
            // modulate down.
            let mut outs = [f32x8::ZERO; OPS];
            let mut heard = f32x8::ZERO;
            let ops = self.ops.iter_mut().zip(&self.settings).zip(mods);
            for (n, ((op, set), modulators)) in ops.enumerate().rev() {
                let mut bend = (outs.iter().enumerate())
                    .filter(|(m, _)| modulators >> m & 1 == 1)
                    .fold(f32x8::ZERO, |bend, (_, &out)| bend + out);
                if n == OPS - 1 {
                    bend += feedback * (self.last.0 + self.last.1);
                }
                let y = match set.wave {
                    // Its top half, the bottom flattened.
                    1 => op.run(bend, |x| sine(x).max(f32x8::ZERO) - f32x8::FRAC_1_PI),
                    // Both halves turned up, an octave higher.
                    2 => op.run(bend, |x| sine(x).abs() - f32x8::FRAC_2_PI),
                    // A whole sine in the first half of the cycle, then nothing.
                    3 => op.run(bend, |x| {
                        let first = (x - x.floor()).simd_lt(f32x8::HALF);
                        first.bitselect(sine(x + x), f32x8::ZERO)
                    }),
                    _ => op.run(bend, sine),
                };
                if n == OPS - 1 {
                    self.last = (y, self.last.0);
                }
                let out = y * op.amount;
                if carriers >> n & 1 == 1 {
                    heard += out;
                }
                if let Some(o) = outs.get_mut(n) {
                    *o = out;
                }
            }
            let x = (heard * self.fade).reduce_add();
            self.fade *= self.fading;
            let x = x * scale;
            (*l, *r) = (x, x);
        }
    }
}

impl Processor for Bessel {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.render(l, r),
            Segment::Event(event) => match event.kind {
                EventKind::NoteOn { pitch, velocity } => self.note_on(pitch, velocity),
                EventKind::NoteOff { pitch } => self.note_off(pitch),
                EventKind::Retune { from, to, velocity } => self.retune(from, to, velocity),
                EventKind::Choke => self.choke(),
                // Its notes don't glide: the pitch envelope sweeps instead.
                EventKind::Shape(_) | EventKind::Program(_) => {}
                EventKind::Param { id, value } => {
                    if let Some(info) = InstKind::Bessel.params().get(id as usize) {
                        self.set(id, info.clamp(value));
                    }
                }
            },
        });
    }

    fn params(&self) -> &[ParamInfo] {
        InstKind::Bessel.params()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::instrument::bessel::op;

    const RATE: f32 = 48_000.0;

    fn fm(settings: &[(u32, f32)]) -> Bessel {
        let mut values = lattice_core::Instrument::new(InstKind::Bessel).values;
        for &(id, value) in settings {
            values[id as usize] = value;
        }
        Bessel::with_values(RATE, &values)
    }

    /// Operator 1 alone, a plain sine, held: the organ, the others silent.
    fn sine_only(extra: &[(u32, f32)]) -> Vec<(u32, f32)> {
        let mut s = vec![
            (ALGORITHM, 7.0),
            (op(1, LEVEL), 1.0),
            (op(1, SUSTAIN), 1.0),
            (op(2, LEVEL), 0.0),
            (op(3, LEVEL), 0.0),
            (op(4, LEVEL), 0.0),
        ];
        s.extend_from_slice(extra);
        s
    }

    fn on(pitch: u8, velocity: u8) -> Event {
        Event {
            offset: 0,
            kind: EventKind::NoteOn { pitch, velocity },
        }
    }

    fn off(pitch: u8) -> Event {
        Event {
            offset: 0,
            kind: EventKind::NoteOff { pitch },
        }
    }

    fn play(b: &mut Bessel, events: &[Event], seconds: f32) -> Vec<f32> {
        let len = (seconds * RATE) as usize;
        let (mut l, mut r) = (vec![0.0; len], vec![0.0; len]);
        b.process(events, &mut l, &mut r);
        l
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|x| x * x).sum::<f32>() / x.len() as f32).sqrt()
    }

    /// How strongly `x` has `hz` in it.
    fn strength(x: &[f32], hz: f32) -> f32 {
        let w = std::f32::consts::TAU * hz / RATE;
        let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (n, x)| {
            (re + x * (w * n as f32).cos(), im + x * (w * n as f32).sin())
        });
        (re * re + im * im).sqrt() / x.len() as f32
    }

    /// The pitch, from the lag that best matches the signal with itself.
    fn pitch(x: &[f32], around: f32) -> f32 {
        let period = RATE / around;
        let lags = (period * 0.9) as usize..(period * 1.1) as usize;
        let score = |lag: usize| -> f32 { x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum() };
        let best = lags
            .clone()
            .max_by(|&a, &b| score(a).total_cmp(&score(b)))
            .unwrap();
        let (a, b, c) = (score(best - 1), score(best), score(best + 1));
        let lag = best as f32 + 0.5 * (a - c) / (a - 2.0 * b + c);
        RATE / lag
    }

    fn cents(heard: f32, hz: f32) -> f32 {
        1200.0 * (heard / hz).log2()
    }

    fn hz(note: u8) -> f32 {
        440.0 * 2f32.powf((note as f32 - 69.0) / 12.0)
    }

    #[test]
    fn the_sine_is_a_sine() {
        for i in -2000..2000 {
            let x = i as f32 / 997.0;
            let want = (std::f32::consts::TAU * x).sin();
            let got = sine(f32x8::splat(x)).to_array()[0];
            assert!((got - want).abs() < 2e-5, "{x}: {got} {want}");
        }
    }

    #[test]
    fn plays_in_tune_and_in_its_tunings() {
        for note in [36, 57, 81] {
            let mut b = fm(&sine_only(&[]));
            let l = play(&mut b, &[on(note, 100)], 0.5);
            let heard = pitch(&l[4_800..], hz(note));
            assert!(cents(heard, hz(note)).abs() < 2.0, "{note}: {heard}");
        }
        // Just intonation on A: C♯ is a pure major third, 14 cents under equal temperament.
        let mut b = fm(&sine_only(&[(TUNING, 1.0), (KEY, 9.0)]));
        let l = play(&mut b, &[on(73, 100)], 0.5);
        let off = cents(pitch(&l[4_800..], hz(73)), hz(73));
        assert!((off + 13.7).abs() < 2.0, "{off}");
    }

    #[test]
    fn a_modulator_adds_sidebands_as_deep_as_its_level() {
        // 2→1 at 1:1: the more operator 2 bends operator 1, the stronger the harmonics.
        let harmonics = |level: f32| {
            let mut b = fm(&[
                (ALGORITHM, 3.0),
                (op(1, LEVEL), 1.0),
                (op(1, SUSTAIN), 1.0),
                (op(2, LEVEL), level),
                (op(2, SUSTAIN), 1.0),
                (op(3, LEVEL), 0.0),
            ]);
            let l = play(&mut b, &[on(57, 127)], 0.3);
            let x = &l[4_800..];
            let more = (2..16).map(|h| strength(x, 220.0 * h as f32)).sum::<f32>();
            (strength(x, 220.0), more)
        };
        let (one, more) = harmonics(0.0);
        assert!(more < 0.001 * one, "a sine alone: {one} {more}");
        let (one, more) = harmonics(0.5);
        assert!(more > 0.2 * one, "bent, harmonics: {one} {more}");
        let (all, most) = harmonics(1.0);
        assert!(most / all > more / one, "deeper, brighter");
    }

    #[test]
    fn a_fixed_operator_keeps_its_pitch_and_the_pitch_envelope_sweeps() {
        let fixed = sine_only(&[(op(1, TRACK), 0.0), (op(1, RATIO), 2.0)]);
        for note in [40, 60, 80] {
            let mut b = fm(&fixed);
            let l = play(&mut b, &[on(note, 100)], 0.3);
            let heard = pitch(&l[4_800..], 2.0 * REF_HZ);
            assert!(cents(heard, 2.0 * REF_HZ).abs() < 2.0, "{note}: {heard}");
        }
        // Two octaves up, falling: it starts high and lands on the note.
        let mut b = fm(&sine_only(&[(PITCH, 24.0), (SWEEP, 0.03)]));
        let l = play(&mut b, &[on(48, 127)], 1.0);
        let early = strength(&l[..960], 4.0 * hz(48)) + strength(&l[..960], 3.0 * hz(48));
        assert!(early > strength(&l[..960], hz(48)), "it starts high");
        let late = pitch(&l[14_400..], hz(48));
        assert!(cents(late, hz(48)).abs() < 3.0, "{late}");
    }

    #[test]
    fn feedback_brightens_it_into_noise() {
        let bright = |feedback: f32| {
            let mut b = fm(&[
                (ALGORITHM, 7.0),
                (FEEDBACK, feedback),
                (op(1, LEVEL), 0.0),
                (op(2, LEVEL), 0.0),
                (op(3, LEVEL), 0.0),
                (op(4, RATIO), 1.0),
                (op(4, LEVEL), 1.0),
                (op(4, SUSTAIN), 1.0),
            ]);
            let l = play(&mut b, &[on(45, 127)], 0.3);
            let x = &l[4_800..];
            (2..8).map(|h| strength(x, 110.0 * h as f32)).sum::<f32>() / strength(x, 110.0)
        };
        assert!(bright(0.0) < 0.01, "{}", bright(0.0));
        assert!(bright(0.6) > 0.3, "{}", bright(0.6));
    }

    #[test]
    fn the_envelopes_shape_it_and_it_stops() {
        // No sustain: it fades while held.
        let mut b = fm(&sine_only(&[(op(1, SUSTAIN), 0.0), (op(1, DECAY), 0.3)]));
        assert!(play(&mut b, &[], 0.1).iter().all(|&x| x == 0.0));
        let l = play(&mut b, &[on(60, 100)], 1.0);
        assert!(rms(&l[..4_800]) > 50.0 * rms(&l[43_200..]));
        // Held, it sustains; let go, it releases, and goes silent.
        let mut b = fm(&sine_only(&[(op(1, RELEASE), 0.1)]));
        let l = play(&mut b, &[on(60, 100)], 0.5);
        assert!(rms(&l[19_200..]) > 0.02);
        let l = play(&mut b, &[off(60)], 0.3);
        assert!(rms(&l[9_600..]) < 1e-3);
        play(&mut b, &[], 0.2);
        assert!(play(&mut b, &[], 0.1).iter().all(|&x| x == 0.0));
        // Velocity: softer notes, quieter.
        let loud = |velocity| {
            let mut b = fm(&sine_only(&[(op(1, VELOCITY), 1.0)]));
            rms(&play(&mut b, &[on(60, velocity)], 0.2))
        };
        assert!(loud(127) > 3.0 * loud(40));
    }

    #[test]
    fn stays_steady_at_its_most_extreme() {
        for algorithm in 0..8 {
            let mut settings = vec![
                (ALGORITHM, algorithm as f32),
                (FEEDBACK, 1.0),
                (PITCH, 48.0),
                (SWEEP, 2.0),
                (GAIN, 1.0),
            ];
            for n in 1..=4 {
                settings.extend([
                    (op(n, RATIO), 32.0),
                    (op(n, LEVEL), 1.0),
                    (op(n, SUSTAIN), 1.0),
                    (op(n, WAVE), (n % 4) as f32),
                ]);
            }
            let mut b = fm(&settings);
            let chord: Vec<Event> = [21, 36, 60, 84, 96, 108, 115, 127]
                .into_iter()
                .map(|n| on(n, 127))
                .collect();
            let l = play(&mut b, &chord, 1.0);
            let peak = l.iter().fold(0.0f32, |p, x| p.max(x.abs()));
            assert!(
                peak.is_finite() && peak <= 8.0 * OUT * 1.7,
                "{algorithm}: {peak}"
            );
            let choke = Event {
                offset: 0,
                kind: EventKind::Choke,
            };
            let l = play(&mut b, &[choke], 0.1);
            assert!(rms(&l[2_400..]) < 1e-4, "{algorithm}");
        }
    }
}
