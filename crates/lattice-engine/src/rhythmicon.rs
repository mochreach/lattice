//! Rhythmicon, the drum engine: a kick, a snare, a hat, a clap and a tom, one drum to a track.
//!
//! Each drum is a single voice, retriggered by every note, which ignores note-offs and rings out
//! by its decay. C-4 plays a drum at its tuning; other notes tune it up or down from there.
//! Settings take effect on the next hit, except decay, which also shortens or lengthens the one
//! ringing.

use std::f32::consts::TAU;

use lattice_core::instrument::{clap, hat, kick, snare, tom};
use lattice_core::{DRUM_ROOT, InstKind};

use crate::dsp::{Svf, decay_coef};
use crate::param::ParamInfo;
use crate::processor::{Event, EventKind, Processor, Segment, segments};

/// Below this, a drum has finished and stops rendering.
const SILENCE: f32 = 1e-4;
/// ln(1000): time constants per "time to fade by 60 dB".
pub(crate) const SIXTY_DB: f32 = 6.907_755;

/// Per-sample multiplier that fades by 60 dB over `seconds`.
fn fade(seconds: f32, sample_rate: f32) -> f32 {
    decay_coef(seconds / SIXTY_DB, sample_rate)
}

/// One drum's sound: what a hit starts, and each sample after.
trait Voice: Send {
    const KIND: InstKind;

    fn new(sample_rate: f32) -> Self;

    /// Called with each setting, once at the start and whenever one changes. `value` is in range.
    fn set(&mut self, id: u32, value: f32);

    /// Starts a hit. `tune` multiplies the drum's tuning; `velocity` is 0 to 1.
    fn trigger(&mut self, tune: f32, velocity: f32);

    /// The next sample, and whether the drum is still sounding.
    fn tick(&mut self) -> (f32, bool);
}

/// How long a choked drum takes to fade, in seconds to fall by 1/e: quick, but not a click.
const CHOKE: f32 = 0.003;

/// How long the jump between a ringing hit and the next takes to smooth over, in seconds to fall
/// by 1/e: too quick to hear as anything but the strike.
const JOIN: f32 = 0.002;

/// A drum as an instrument.
struct Drum<V> {
    voice: V,
    sounding: bool,
    /// Multiplies the output: 1, falling once choked.
    fade: f32,
    fade_coef: f32,
    choked: bool,
    /// The last sample out, so a hit while the drum rings can start from it.
    last: f32,
    /// Added to the output, falling to nothing: what a retrigger jumped by, smoothed over, so a
    /// drum struck again mid-ring, as in a roll, doesn't click on every hit.
    offset: f32,
    offset_coef: f32,
    /// The next sample is a retrigger's first, so `offset` is to be worked out from it.
    joining: bool,
}

impl<V: Voice> Drum<V> {
    /// Starts at `values`, one per parameter, by id.
    fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let mut drum = Self {
            voice: V::new(sample_rate),
            sounding: false,
            fade: 1.0,
            fade_coef: decay_coef(CHOKE, sample_rate),
            choked: false,
            last: 0.0,
            offset: 0.0,
            offset_coef: decay_coef(JOIN, sample_rate),
            joining: false,
        };
        for info in V::KIND.params() {
            let value = values
                .get(info.id as usize)
                .copied()
                .unwrap_or(info.default);
            drum.voice.set(info.id, info.clamp(value));
        }
        drum
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.sounding {
            left.fill(0.0);
            right.fill(0.0);
            self.last = 0.0;
            return;
        }
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if !self.sounding {
                (*l, *r) = (0.0, 0.0);
                self.last = 0.0;
                continue;
            }
            let (out, sounding) = self.voice.tick();
            let out = out * self.fade;
            if self.joining {
                self.offset = self.last - out;
                self.joining = false;
            }
            let out = out + self.offset;
            self.offset *= self.offset_coef;
            (*l, *r) = (out, out);
            self.last = out;
            self.sounding = sounding;
            if self.choked {
                self.fade *= self.fade_coef;
                self.sounding &= self.fade > SILENCE;
            }
        }
    }
}

impl<V: Voice> Processor for Drum<V> {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.render(l, r),
            Segment::Event(event) => match event.kind {
                EventKind::NoteOn { pitch, velocity } => {
                    let semitones = f32::from(pitch) - f32::from(DRUM_ROOT);
                    self.voice
                        .trigger(2f32.powf(semitones / 12.0), f32::from(velocity) / 127.0);
                    self.joining = self.sounding;
                    self.offset = 0.0;
                    self.sounding = true;
                    self.fade = 1.0;
                    self.choked = false;
                }
                // A drum rings out whatever, and has no file to reverse or pitch to glide.
                EventKind::NoteOff { .. } | EventKind::Shape(_) => {}
                EventKind::Choke => self.choked = self.sounding,
                EventKind::Param { id, value } => {
                    if let Some(info) = V::KIND.params().get(id as usize) {
                        self.voice.set(id, info.clamp(value));
                    }
                }
            },
        });
    }

    fn params(&self) -> &[ParamInfo] {
        V::KIND.params()
    }
}

type Kick = Drum<KickVoice>;
type Snare = Drum<SnareVoice>;
type Hat = Drum<HatVoice>;
type Clap = Drum<ClapVoice>;
type Tom = Drum<TomVoice>;

/// The drum for `kind`, starting at `values`. `None` for an instrument that isn't a drum.
pub fn build(kind: InstKind, sample_rate: f32, values: &[f32]) -> Option<Box<dyn Processor>> {
    Some(match kind {
        InstKind::Kick => Box::new(Kick::with_values(sample_rate, values)),
        InstKind::Snare => Box::new(Snare::with_values(sample_rate, values)),
        InstKind::Hat => Box::new(Hat::with_values(sample_rate, values)),
        InstKind::Clap => Box::new(Clap::with_values(sample_rate, values)),
        InstKind::Tom => Box::new(Tom::with_values(sample_rate, values)),
        InstKind::Synth
        | InstKind::Switched
        | InstKind::Monochord
        | InstKind::Krell
        | InstKind::Sampler
        | InstKind::Chop => return None,
    })
}

/// White noise from a 32-bit xorshift: cheap, and the same every run.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Noise(pub(crate) u32);

impl Noise {
    pub(crate) fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// One-pole low-pass coefficient for a cutoff.
pub(crate) fn one_pole(hz: f32, sample_rate: f32) -> f32 {
    1.0 - (-TAU * hz.min(0.45 * sample_rate) / sample_rate).exp()
}

/// Longest the kick holds its level before it starts to decay.
const MAX_HOLD: f32 = 0.08;

/// What a kick model changes; the settings work within it.
struct KickModel {
    /// The fraction of the decay the body holds at full level before decaying.
    hold: f32,
    /// Multiplies the decay.
    decay: f32,
    /// Multiply how far and how fast the pitch drops.
    depth: f32,
    sweep: f32,
    /// Level of the noise click.
    click: f32,
    /// Added to the drive.
    drive: f32,
    level: f32,
}

/// By [`kick::MODELS`]: short and clicky; an 808's long boom; a 909's hard, driven thump.
const KICKS: [KickModel; 3] = [
    KickModel {
        hold: 0.0,
        decay: 0.6,
        depth: 0.8,
        sweep: 0.5,
        click: 1.6,
        drive: 0.0,
        level: 0.8,
    },
    KickModel {
        hold: 0.2,
        decay: 1.0,
        depth: 1.0,
        sweep: 1.0,
        click: 1.0,
        drive: 0.0,
        level: 0.75,
    },
    KickModel {
        hold: 0.1,
        decay: 0.8,
        depth: 1.4,
        sweep: 0.6,
        click: 2.2,
        drive: 0.35,
        level: 0.8,
    },
];

/// The model a choice setting picks, from `models`: the last for a value past the end, and none
/// only if there are none.
fn model<T>(models: &'static [T], value: f32) -> Option<&'static T> {
    models.get(value as usize).or(models.last())
}

struct KickVoice {
    sample_rate: f32,
    model: &'static KickModel,
    tune: f32,
    punch: f32,
    /// The decay setting, in seconds; the model scales it.
    decay: f32,
    amp_coef: f32,
    /// Samples the body holds at full level before decaying, like an 808's.
    hold_len: u32,
    drive: f32,
    /// Hz once the drop is over, for the current hit.
    freq: f32,
    /// How far above `freq` the hit starts, as a multiple of it.
    depth: f32,
    sweep: f32,
    sweep_coef: f32,
    phase: f32,
    amp: f32,
    hold: u32,
    /// The noise burst at the start of a hit, and its level.
    click: f32,
    click_coef: f32,
    noise: Noise,
    /// DC blocker state, as asymmetric drive shifts the waveform off centre.
    dc: [f32; 2],
    dc_coef: f32,
    velocity: f32,
}

impl Voice for KickVoice {
    const KIND: InstKind = InstKind::Kick;

    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            model: &KICKS[1],
            tune: 50.0,
            punch: 0.5,
            decay: 0.45,
            amp_coef: 0.0,
            drive: 0.0,
            freq: 50.0,
            depth: 0.0,
            sweep: 0.0,
            sweep_coef: 0.0,
            phase: 0.0,
            amp: 0.0,
            hold_len: 0,
            hold: 0,
            click: 0.0,
            click_coef: decay_coef(0.002, sample_rate),
            noise: Noise(0x2545_f491),
            dc: [0.0; 2],
            dc_coef: 1.0 - TAU * 15.0 / sample_rate,
            velocity: 0.0,
        }
    }

    fn set(&mut self, id: u32, value: f32) {
        match id {
            kick::MODEL => {
                if let Some(model) = model(&KICKS, value) {
                    self.model = model;
                }
            }
            kick::TUNE => self.tune = value,
            kick::PUNCH => self.punch = value,
            kick::DECAY => self.decay = value,
            _ => self.drive = value,
        }
        let decay = self.decay * self.model.decay;
        self.amp_coef = fade(decay, self.sample_rate);
        self.hold_len = ((decay * self.model.hold).min(MAX_HOLD) * self.sample_rate) as u32;
    }

    fn trigger(&mut self, tune: f32, velocity: f32) {
        self.freq = self.tune * tune;
        // A soft thud starts two octaves up and falls slowly; a hard click starts higher and
        // falls fast.
        self.depth = (3.0 + 9.0 * self.punch) * self.model.depth;
        let time = (0.03 - 0.022 * self.punch) * self.model.sweep;
        self.sweep_coef = decay_coef(time, self.sample_rate);
        self.sweep = 1.0;
        self.phase = 0.0;
        self.amp = 1.0;
        self.hold = self.hold_len;
        self.click = 1.0;
        self.velocity = velocity;
    }

    fn tick(&mut self) -> (f32, bool) {
        let hz = self.freq * (1.0 + self.depth * self.sweep);
        self.phase += hz / self.sample_rate;
        self.phase -= self.phase.floor();
        self.sweep *= self.sweep_coef;
        let click = self.noise.next() * self.click * (0.1 + 0.4 * self.punch) * self.model.click;
        self.click *= self.click_coef;
        let x = (TAU * self.phase).sin() * self.amp + click;
        if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.amp *= self.amp_coef;
        }
        // Saturation, pushed off centre so it adds even harmonics, which carry the kick on
        // small speakers. Scaled so the larger, negative side of a full-scale sine peaks at 1.
        let drive = (self.drive + self.model.drive).min(1.0);
        let gain = 1.0 + 9.0 * drive;
        let bias = 0.4 * drive;
        let shaped =
            ((x * gain + bias).tanh() - bias.tanh()) / ((gain - bias).tanh() + bias.tanh());
        let out = shaped - self.dc[0] + self.dc_coef * self.dc[1];
        self.dc = [shaped, out];
        (out * self.velocity * self.model.level, self.amp > SILENCE)
    }
}

/// The second mode of a circular membrane, relative to the first.
const SHELL_RATIO: f32 = 1.59;

/// Every drum, for the tests.
#[cfg(test)]
const DRUMS: [InstKind; 5] = [
    InstKind::Kick,
    InstKind::Snare,
    InstKind::Hat,
    InstKind::Clap,
    InstKind::Tom,
];

/// What a snare model changes; the settings work within it.
struct SnareModel {
    /// Drive on the shell, which thickens it.
    drive: f32,
    /// Multiplies how long the shell rings, against the decay.
    body: f32,
    /// How far above its tuning the shell starts.
    sweep: f32,
    /// Where the wires' noise is high-passed: low keeps the snare's weight.
    wires_low: f32,
    /// Level of the wires against the shell.
    wires: f32,
    level: f32,
}

/// By [`snare::MODELS`]: thin and short; full and weighty; a 909's bright crack.
const SNARES: [SnareModel; 3] = [
    SnareModel {
        drive: 1.0,
        body: 0.5,
        sweep: 0.3,
        wires_low: 900.0,
        wires: 2.4,
        level: 0.7,
    },
    SnareModel {
        drive: 1.8,
        body: 0.9,
        sweep: 0.5,
        wires_low: 180.0,
        wires: 2.0,
        level: 0.62,
    },
    SnareModel {
        drive: 3.0,
        body: 0.6,
        sweep: 1.0,
        wires_low: 400.0,
        wires: 2.8,
        level: 0.5,
    },
];

struct SnareVoice {
    sample_rate: f32,
    model: &'static SnareModel,
    tune: f32,
    /// The decay setting, in seconds.
    decay: f32,
    snap: f32,
    body_coef: f32,
    noise_coef: f32,
    lowpass: f32,
    highpass: f32,
    freq: f32,
    sweep: f32,
    sweep_coef: f32,
    phases: [f32; 2],
    body: f32,
    rattle: f32,
    noise: Noise,
    low: f32,
    high: f32,
    velocity: f32,
}

impl Voice for SnareVoice {
    const KIND: InstKind = InstKind::Snare;

    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            model: &SNARES[1],
            tune: 180.0,
            decay: 0.22,
            snap: 0.6,
            body_coef: 0.0,
            noise_coef: 0.0,
            lowpass: 0.0,
            highpass: 0.0,
            freq: 180.0,
            sweep: 0.0,
            sweep_coef: decay_coef(0.01, sample_rate),
            phases: [0.0; 2],
            body: 0.0,
            rattle: 0.0,
            noise: Noise(0x1234_5678),
            low: 0.0,
            high: 0.0,
            velocity: 0.0,
        }
    }

    fn set(&mut self, id: u32, value: f32) {
        match id {
            snare::MODEL => {
                if let Some(model) = model(&SNARES, value) {
                    self.model = model;
                }
            }
            snare::TUNE => self.tune = value,
            snare::DECAY => self.decay = value,
            snare::SNAP => self.snap = value,
            _ => self.lowpass = one_pole(value, self.sample_rate),
        }
        // The shell dies away before the wires do.
        self.body_coef = fade(self.decay * self.model.body, self.sample_rate);
        self.noise_coef = fade(self.decay, self.sample_rate);
        self.highpass = one_pole(self.model.wires_low, self.sample_rate);
    }

    fn trigger(&mut self, tune: f32, velocity: f32) {
        self.freq = self.tune * tune;
        self.sweep = 1.0;
        self.phases = [0.0; 2];
        self.body = 1.0;
        self.rattle = 1.0;
        self.velocity = velocity;
    }

    fn tick(&mut self) -> (f32, bool) {
        let hz = self.freq * (1.0 + self.model.sweep * self.sweep);
        self.sweep *= self.sweep_coef;
        for (phase, ratio) in self.phases.iter_mut().zip([1.0, SHELL_RATIO]) {
            *phase += hz * ratio / self.sample_rate;
            *phase -= phase.floor();
        }
        let shell = 0.6 * (TAU * self.phases[0]).sin() + 0.4 * (TAU * self.phases[1]).sin();
        let drive = self.model.drive;
        let shell = (shell * self.body * drive).tanh() / drive.tanh();

        // Noise, low-passed by the tone, then high-passed.
        self.low += self.lowpass * (self.noise.next() - self.low);
        self.high += self.highpass * (self.low - self.high);
        let wires = (self.low - self.high) * self.model.wires;

        let out = shell * (1.0 - 0.35 * self.snap) + wires * self.rattle * self.snap;
        self.body *= self.body_coef;
        self.rattle *= self.noise_coef;
        (
            out * self.velocity * self.model.level,
            self.body.max(self.rattle) > SILENCE,
        )
    }
}

/// The 808's six cymbal oscillators, relative to the lowest (205.3 Hz).
const METAL: [f32; 6] = [1.0, 1.4827, 1.8003, 2.5460, 2.6303, 3.8967];

/// What a hat model changes; the settings work within it.
struct HatModel {
    /// Multiply the squares in pairs, as the 606's and 909's do, rather than summing them.
    pairs: bool,
    /// Where the noise setting starts from: the noise is this, plus the setting's share of the
    /// rest.
    noise: f32,
    /// High-pass stages, at 6 dB/octave each.
    stages: usize,
    level: f32,
}

/// By [`hat::MODELS`]: clangy metal; an 808's ring; a bright, noisy sizzle.
const HATS: [HatModel; 3] = [
    HatModel {
        pairs: true,
        noise: 0.0,
        stages: 2,
        level: 6.0,
    },
    HatModel {
        pairs: false,
        noise: 0.0,
        stages: 2,
        level: 6.0,
    },
    HatModel {
        pairs: false,
        noise: 0.6,
        stages: 3,
        level: 6.0,
    },
];

struct HatVoice {
    sample_rate: f32,
    model: &'static HatModel,
    tune: f32,
    amp_coef: f32,
    highpass: f32,
    noise_mix: f32,
    incs: [f32; 6],
    phases: [f32; 6],
    noise: Noise,
    /// One-pole low-passes, each subtracted for a stage of high-pass.
    lows: [f32; 3],
    amp: f32,
    velocity: f32,
}

impl Voice for HatVoice {
    const KIND: InstKind = InstKind::Hat;

    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            model: &HATS[1],
            tune: 205.0,
            amp_coef: 0.0,
            highpass: 0.0,
            noise_mix: 0.3,
            incs: [0.0; 6],
            // Spread out, so the six don't all switch together on the first sample.
            phases: [0.0, 0.17, 0.39, 0.58, 0.71, 0.93],
            noise: Noise(0x9e37_79b9),
            lows: [0.0; 3],
            amp: 0.0,
            velocity: 0.0,
        }
    }

    fn set(&mut self, id: u32, value: f32) {
        match id {
            hat::MODEL => {
                if let Some(model) = model(&HATS, value) {
                    self.model = model;
                }
            }
            hat::TUNE => self.tune = value,
            hat::DECAY => self.amp_coef = fade(value, self.sample_rate),
            hat::TONE => self.highpass = one_pole(value, self.sample_rate),
            _ => self.noise_mix = value,
        }
    }

    fn trigger(&mut self, tune: f32, velocity: f32) {
        for (inc, ratio) in self.incs.iter_mut().zip(METAL) {
            *inc = self.tune * tune * ratio / self.sample_rate;
        }
        self.amp = 1.0;
        self.velocity = velocity;
    }

    fn tick(&mut self) -> (f32, bool) {
        let mut squares = [0.0; 6];
        for ((square, phase), inc) in squares.iter_mut().zip(&mut self.phases).zip(self.incs) {
            *phase += inc;
            *phase -= phase.floor();
            *square = if *phase < 0.5 { 1.0 } else { -1.0 };
        }
        let metal = if self.model.pairs {
            // Multiplying squares is XOR-ing them: sum and difference tones, and a harsher clang.
            let [a, b, c, d, e, f] = squares;
            (a * b + c * d + e * f) / 3.0
        } else {
            squares.iter().sum::<f32>() / 6.0
        };
        let noise = self.model.noise + (1.0 - self.model.noise) * self.noise_mix;
        let source = metal * (1.0 - noise) + self.noise.next() * noise;
        // High-pass: what's left after taking away the lows, stage by stage.
        let mut x = source;
        for low in self.lows.iter_mut().take(self.model.stages) {
            *low += self.highpass * (x - *low);
            x -= *low;
        }
        let out = x * self.amp;
        self.amp *= self.amp_coef;
        (out * self.velocity * self.model.level, self.amp > SILENCE)
    }
}

/// What a clap model changes; the settings work within it.
struct ClapModel {
    bursts: u32,
    /// How far each gap may stray from the spread, as a fraction of it.
    jitter: f32,
    /// How fast each burst dies, in seconds to fall by 1/e.
    burst: f32,
    /// The band's narrowness.
    resonance: f32,
    /// Multiplies the room setting.
    tail: f32,
    level: f32,
}

/// By [`clap::MODELS`]: one tight snap; an 808's four claps and a roomy tail; a crowd, out of
/// time.
const CLAPS: [ClapModel; 3] = [
    ClapModel {
        bursts: 2,
        jitter: 0.0,
        burst: 0.004,
        resonance: 0.5,
        tail: 0.5,
        level: 2.6,
    },
    ClapModel {
        bursts: 4,
        jitter: 0.15,
        burst: 0.005,
        resonance: 0.6,
        tail: 1.0,
        level: 3.0,
    },
    ClapModel {
        bursts: 9,
        jitter: 0.9,
        burst: 0.007,
        resonance: 0.35,
        tail: 1.2,
        level: 1.7,
    },
];

struct ClapVoice {
    sample_rate: f32,
    model: &'static ClapModel,
    tune: f32,
    room: f32,
    /// In samples.
    spread: f32,
    burst_coef: f32,
    tail_coef: f32,
    freq: f32,
    filter: Svf,
    noise: Noise,
    /// Samples since the hit, and when the next burst is due.
    time: f32,
    next_burst: f32,
    bursts_left: u32,
    burst: f32,
    tail: f32,
    /// The tail fades in over the bursts, this many samples.
    onset: f32,
    velocity: f32,
}

impl Voice for ClapVoice {
    const KIND: InstKind = InstKind::Clap;

    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            model: &CLAPS[1],
            tune: 1_100.0,
            room: 0.5,
            spread: 0.0,
            burst_coef: 0.0,
            tail_coef: 0.0,
            freq: 1_100.0,
            filter: Svf::default(),
            noise: Noise(0x0bad_cafe),
            time: 0.0,
            next_burst: 0.0,
            bursts_left: 0,
            burst: 0.0,
            tail: 0.0,
            onset: 1.0,
            velocity: 0.0,
        }
    }

    fn set(&mut self, id: u32, value: f32) {
        match id {
            clap::MODEL => {
                if let Some(model) = model(&CLAPS, value) {
                    self.model = model;
                }
            }
            clap::TUNE => self.tune = value,
            clap::DECAY => self.tail_coef = fade(value, self.sample_rate),
            clap::SPREAD => self.spread = value * self.sample_rate,
            _ => self.room = value,
        }
        self.burst_coef = decay_coef(self.model.burst, self.sample_rate);
    }

    fn trigger(&mut self, tune: f32, velocity: f32) {
        self.freq = self.tune * tune;
        self.time = 0.0;
        self.next_burst = 0.0;
        self.bursts_left = self.model.bursts;
        self.tail = 1.0;
        self.onset = (self.spread * self.model.bursts as f32).max(1.0);
        self.velocity = velocity;
    }

    fn tick(&mut self) -> (f32, bool) {
        if self.bursts_left > 0 && self.time >= self.next_burst {
            self.burst = 1.0;
            self.bursts_left -= 1;
            let jitter = 1.0 + self.model.jitter * self.noise.next();
            self.next_burst += self.spread * jitter;
        }
        let tail = self.tail * self.room * self.model.tail * (self.time / self.onset).min(1.0);
        let band = self.filter.bandpass(
            self.noise.next(),
            self.freq,
            self.model.resonance,
            self.sample_rate,
        );
        let out = band * (self.burst + 0.5 * tail);
        self.burst *= self.burst_coef;
        self.tail *= self.tail_coef;
        self.time += 1.0;
        let sounding = self.bursts_left > 0 || self.burst.max(self.tail) > SILENCE;
        (out * self.velocity * self.model.level, sounding)
    }
}

/// What a tom model changes; the settings work within it.
struct TomModel {
    /// Multiply the decay and the bend.
    decay: f32,
    bend: f32,
    /// Seconds for the bend to fall by 1/e.
    sweep: f32,
    /// Multiplies the noise setting.
    noise: f32,
    /// Level of a second shell mode, a fifth and a bit up.
    shell: f32,
    drive: f32,
    level: f32,
}

/// By [`tom::MODELS`]: short and dry; an 808's round boom; a 909's harder, ringing shell.
const TOMS: [TomModel; 3] = [
    TomModel {
        decay: 0.5,
        bend: 0.6,
        sweep: 0.015,
        noise: 1.5,
        shell: 0.0,
        drive: 1.0,
        level: 0.8,
    },
    TomModel {
        decay: 1.0,
        bend: 1.0,
        sweep: 0.04,
        noise: 1.0,
        shell: 0.0,
        drive: 1.0,
        level: 0.8,
    },
    TomModel {
        decay: 0.8,
        bend: 1.3,
        sweep: 0.02,
        noise: 1.3,
        shell: 0.35,
        drive: 2.5,
        level: 0.75,
    },
];

struct TomVoice {
    sample_rate: f32,
    model: &'static TomModel,
    tune: f32,
    decay: f32,
    bend: f32,
    noise_level: f32,
    amp_coef: f32,
    sweep_coef: f32,
    stick_coef: f32,
    freq: f32,
    sweep: f32,
    phases: [f32; 2],
    amp: f32,
    stick: f32,
    noise: Noise,
    low: f32,
    velocity: f32,
}

impl Voice for TomVoice {
    const KIND: InstKind = InstKind::Tom;

    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            model: &TOMS[1],
            tune: 120.0,
            decay: 0.4,
            bend: 0.4,
            noise_level: 0.2,
            amp_coef: 0.0,
            sweep_coef: 0.0,
            stick_coef: decay_coef(0.008, sample_rate),
            freq: 120.0,
            sweep: 0.0,
            phases: [0.0; 2],
            amp: 0.0,
            stick: 0.0,
            noise: Noise(0x7f4a_7c15),
            low: 0.0,
            velocity: 0.0,
        }
    }

    fn set(&mut self, id: u32, value: f32) {
        match id {
            tom::MODEL => {
                if let Some(model) = model(&TOMS, value) {
                    self.model = model;
                }
            }
            tom::TUNE => self.tune = value,
            tom::DECAY => self.decay = value,
            tom::BEND => self.bend = value,
            _ => self.noise_level = value,
        }
        self.amp_coef = fade(self.decay * self.model.decay, self.sample_rate);
        self.sweep_coef = decay_coef(self.model.sweep, self.sample_rate);
    }

    fn trigger(&mut self, tune: f32, velocity: f32) {
        self.freq = self.tune * tune;
        self.sweep = 1.0;
        self.phases = [0.0; 2];
        self.amp = 1.0;
        self.stick = 1.0;
        self.velocity = velocity;
    }

    fn tick(&mut self) -> (f32, bool) {
        let hz = self.freq * (1.0 + 1.5 * self.bend * self.model.bend * self.sweep);
        self.sweep *= self.sweep_coef;
        for (phase, ratio) in self.phases.iter_mut().zip([1.0, SHELL_RATIO]) {
            *phase += hz * ratio / self.sample_rate;
            *phase -= phase.floor();
        }
        let shell = (TAU * self.phases[0]).sin() + self.model.shell * (TAU * self.phases[1]).sin();
        let drive = self.model.drive;
        let shell = (shell * self.amp * drive).tanh() / drive.tanh();
        // The stick: a short burst of noise, softened.
        self.low += 0.3 * (self.noise.next() - self.low);
        let stick = self.low * self.stick * self.noise_level * self.model.noise;
        self.amp *= self.amp_coef;
        self.stick *= self.stick_coef;
        let out = shell + stick;
        (out * self.velocity * self.model.level, self.amp > SILENCE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::Instrument;

    const RATE: f32 = 48_000.0;

    fn hit(pitch: u8) -> Event {
        Event {
            offset: 0,
            kind: EventKind::NoteOn {
                pitch,
                velocity: 127,
            },
        }
    }

    /// A second of one hit at C-4.
    fn render(kind: InstKind, values: &[f32]) -> Vec<f32> {
        let mut drum = build(kind, RATE, values).unwrap();
        let (mut left, mut right) = (vec![0.0; RATE as usize], vec![0.0; RATE as usize]);
        drum.process(&[hit(DRUM_ROOT)], &mut left, &mut right);
        assert_eq!(left, right);
        left
    }

    fn peak(audio: &[f32]) -> f32 {
        audio.iter().fold(0.0, |m, x| m.max(x.abs()))
    }

    #[test]
    fn every_drum_and_model_hits_at_a_sensible_level_and_stops() {
        for kind in DRUMS {
            for model in 0..3 {
                let mut values = Instrument::new(kind).values;
                // Every drum's model is its first setting.
                values[0] = model as f32;
                let audio = render(kind, &values);
                let p = peak(&audio);
                assert!((0.2..=1.0).contains(&p), "{kind:?} {model} peaks at {p}");
                assert!(audio.iter().all(|x| x.is_finite()));
                assert!(
                    peak(&audio[audio.len() - 1_000..]) < 1e-3,
                    "{kind:?} {model} is silent within a second"
                );
            }
        }
    }

    #[test]
    fn models_sound_different() {
        for kind in DRUMS {
            let audio: Vec<_> = (0..3)
                .map(|model| {
                    let mut values = Instrument::new(kind).values;
                    values[0] = model as f32;
                    render(kind, &values)
                })
                .collect();
            for (a, b) in [(0, 1), (1, 2), (0, 2)] {
                let diff = audio[a]
                    .iter()
                    .zip(&audio[b])
                    .map(|(x, y)| (x - y).abs())
                    .sum::<f32>();
                assert!(diff > 10.0, "{kind:?} {a} and {b} differ by {diff}");
            }
        }
    }

    #[test]
    fn a_choke_cuts_a_ringing_drum_quickly() {
        let kind = InstKind::Hat;
        let mut values = Instrument::new(kind).values;
        values[hat::DECAY as usize] = 1.5;
        let mut hat = build(kind, RATE, &values).unwrap();
        let choke = Event {
            offset: 100,
            kind: EventKind::Choke,
        };
        let (mut left, mut right) = (vec![0.0; 4_800], vec![0.0; 4_800]);
        hat.process(&[hit(DRUM_ROOT), choke], &mut left, &mut right);
        assert!(peak(&left[..100]) > 0.1);
        assert!(
            peak(&left[100..200]) > 0.01,
            "it fades rather than clicking off"
        );
        assert!(peak(&left[2_400..]) == 0.0, "silent within 50 ms");
        // The next hit plays in full.
        hat.process(&[hit(DRUM_ROOT)], &mut left, &mut right);
        assert!(peak(&left[..100]) > 0.1);
    }

    #[test]
    fn silent_until_hit() {
        let mut kick = Kick::with_values(RATE, &Instrument::new(InstKind::Kick).values);
        let (mut left, mut right) = (vec![1.0; 256], vec![1.0; 256]);
        kick.process(&[], &mut left, &mut right);
        assert!(left.iter().chain(&right).all(|&x| x == 0.0));
    }

    /// Crossings of zero going up, per second: roughly the frequency.
    fn rising_crossings(audio: &[f32]) -> usize {
        audio
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count()
    }

    #[test]
    fn the_kick_settles_on_its_tuning_and_notes_tune_it() {
        let values = Instrument::new(InstKind::Kick).values;
        let mut kick = Kick::with_values(RATE, &values);
        let tail = |kick: &mut Kick, pitch| {
            let (mut left, mut right) = (vec![0.0; 9_600], vec![0.0; 9_600]);
            kick.process(&[hit(pitch)], &mut left, &mut right);
            // From 0.1 s, after the drop: a tenth of a second.
            rising_crossings(&left[4_800..])
        };
        assert!((4..=6).contains(&tail(&mut kick, DRUM_ROOT)), "about 50 Hz");
        assert!(
            (9..=11).contains(&tail(&mut kick, DRUM_ROOT + 12)),
            "an octave up"
        );
    }

    #[test]
    fn decay_sets_how_long_it_rings() {
        let kind = InstKind::Hat;
        let mut values = Instrument::new(kind).values;
        let closed = render(kind, &values);
        values[hat::DECAY as usize] = 1.0;
        let open = render(kind, &values);
        let at = RATE as usize / 5;
        assert!(peak(&closed[at..at + 100]) < 1e-3);
        assert!(peak(&open[at..at + 100]) > 0.01);
    }

    #[test]
    fn param_events_change_the_next_hit() {
        let kind = InstKind::Snare;
        let mut snare = Snare::with_values(RATE, &Instrument::new(kind).values);
        let set = Event {
            offset: 0,
            kind: EventKind::Param {
                id: snare::SNAP,
                value: 5.0,
            },
        };
        let (mut left, mut right) = (vec![0.0; 64], vec![0.0; 64]);
        snare.process(&[set, hit(DRUM_ROOT)], &mut left, &mut right);
        assert_eq!(snare.voice.snap, 1.0, "clamped to its range");
    }

    #[test]
    fn a_drum_struck_again_while_it_rings_doesnt_click() {
        let mut values = Instrument::new(InstKind::Tom).values;
        // No stick, so only the shell's wave is left to jump.
        values[tom::NOISE as usize] = 0.0;
        let mut drum = build(InstKind::Tom, RATE, &values).unwrap();
        // A roll: hits a few milliseconds apart, each landing part way through the wave.
        let roll: Vec<_> = (0..7)
            .map(|i| Event {
                offset: i * 619,
                ..hit(DRUM_ROOT)
            })
            .collect();
        let (mut left, mut right) = (vec![0.0; 8_000], vec![0.0; 8_000]);
        drum.process(&roll, &mut left, &mut right);
        let steepest = left
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(left.iter().any(|x| x.abs() > 0.3), "it still hits");
        assert!(steepest < 0.05, "jumps by {steepest}");
    }
}
