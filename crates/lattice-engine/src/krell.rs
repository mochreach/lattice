//! Krell, the harpsichord engine, after the vanished civilisation of *Forbidden Planet*, whose
//! film had the first all-electronic score, Louis and Bebe Barron's.
//!
//! Each key plucks a choir of strings, as a harpsichord's stops choose: one at pitch (8'), a
//! second at pitch plucked further from the end, one an octave up (4'), one an octave down (16')
//! and one two octaves up (2'). Each string is a waveguide, as Monochord's are, plucked by a
//! quill near its end, bright and thin, with a click as the quill snaps off it. The choirs can
//! drift out of tune with each other, each wavering slowly about where it's drifted, and each
//! sits somewhere different in the stereo field.
//!
//! While the key's held a string rings out; when it comes up the damper falls on the string,
//! dulling and stopping it, and the quill slides back past it with a quieter click.
//!
//! Under the strings is the body, a bank of resonant modes they all play into, which rings in
//! sympathy with the notes that land near its modes. Its lowest mode is the hull's pitch: as
//! wood, the rest are its harmonics, and soon still; as glass and metal plates, they spread out of
//! tune with each other and ring on.

use lattice_core::InstKind;
use lattice_core::instrument::harpsichord::{
    BODY, DAMPER, DECAY, DRIFT, GAIN, GLASS, HULL, QUILL, STOPS,
};

use crate::dsp::decay_coef;
use crate::param::ParamInfo;
use crate::processor::{Event, EventKind, Processor, Segment, segments};
use crate::rhythmicon::{Noise, SIXTY_DB, one_pole};
use crate::waveguide::{self, Course};

const VOICES: usize = 8;
/// The most strings a key plucks.
const CHOIRS: usize = 5;
/// The lowest pitch a string plays; lower notes play this.
const LOWEST_HZ: f32 = 20.0;
/// Samples between the choirs' steps in their wavering, and checks for strings that have
/// fallen silent.
const CONTROL_PERIOD: u32 = 32;
/// Below this, a string has finished and stops rendering.
const SILENCE: f32 = 1e-4;
/// How long a choked string takes to fade, in seconds to fall by 1/e.
const CHOKE: f32 = 0.003;
/// How long the hardest damper takes to stop a string, in seconds to fall by 60 dB.
const DAMPED: f32 = 0.04;
/// How much of a ringing string is left when it's plucked again.
const REPLUCK: f32 = 0.3;
/// The level of a pluck, as the RMS of the string's displacement.
const PLUCK_LEVEL: f32 = 0.35;
/// How little a soft note is quieter than a hard one: a harpsichord's quills pluck much the same
/// however the key's struck.
const VELOCITY: f32 = 0.3;
/// The hardest quill's click as it snaps off the string, the click as it slides back past it,
/// and how long each lasts, in seconds to fall by 1/e.
const CLICK_LEVEL: f32 = 0.5;
const RELEASE_LEVEL: f32 = 0.06;
const CLICK_TIME: f32 = 0.0015;
/// The loop's damping, which dulls each trip round the string, with the hardest quill and the
/// softest, and as the damper falls.
const BRIGHT: (f32, f32) = (0.03, 0.2);
const DULL: f32 = 0.5;
/// How far the choirs drift apart at the most, in cents.
const MAX_DRIFT: f32 = 30.0;
/// How far each choir wavers about where it's drifted, as a fraction of its drift.
const WOBBLE: f32 = 0.35;
/// The output, so a few notes together leave headroom.
const OUT: f32 = 0.7;
/// The body's modes.
const MODES: usize = 8;
/// The body's modes, as multiples of the hull, as wood (in tune) and as glass and metal plates.
const WOOD: [f32; MODES] = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
const PLATE: [f32; MODES] = [1.0, 1.59, 2.14, 2.83, 3.51, 4.37, 5.29, 6.24];
/// How long the body's lowest mode rings, in seconds to fall by 60 dB, as wood and as glass;
/// higher modes ring for less.
const RING: (f32, f32) = (0.4, 6.0);
/// Each mode's input, as a multiple of the square root of what it loses each sample: between
/// matching its loss, which only rings it up after a long while in tune, and a fixed gain, which
/// would ring up without end. So a quill's click sets it ringing, and a string in tune with it
/// more so.
const MODE_GAIN: f32 = 0.05;
/// The highest mode the body keeps, as a fraction of the sample rate.
const TOP_MODE: f32 = 0.45;

/// One choir of strings.
#[derive(Debug)]
struct Choir {
    /// Its place in the choirs' wavering.
    id: usize,
    /// Its pitch against the note's, in semitones.
    pitch: f32,
    /// Where the quill plucks it, as a fraction of the string.
    position: f32,
    level: f32,
    /// From -1 (left) to 1 (right).
    pan: f32,
    /// Which way, and how far, it drifts, as a fraction of the most.
    drift: f32,
    /// How fast it wavers, in Hz.
    wobble: f32,
}

const SIXTEEN: Choir = Choir {
    id: 0,
    pitch: -12.0,
    position: 0.15,
    level: 0.7,
    pan: -0.4,
    drift: -0.6,
    wobble: 0.11,
};
const EIGHT: Choir = Choir {
    id: 1,
    pitch: 0.0,
    position: 0.11,
    level: 1.0,
    pan: 0.0,
    drift: 0.0,
    wobble: 0.17,
};
/// The second 8', plucked further from the end, so rounder.
const BACK: Choir = Choir {
    id: 2,
    pitch: 0.0,
    position: 0.2,
    level: 0.8,
    pan: 0.45,
    drift: 1.0,
    wobble: 0.23,
};
const FOUR: Choir = Choir {
    id: 3,
    pitch: 12.0,
    position: 0.09,
    level: 0.55,
    pan: -0.6,
    drift: 0.5,
    wobble: 0.29,
};
const TWO: Choir = Choir {
    id: 4,
    pitch: 24.0,
    position: 0.08,
    level: 0.35,
    pan: 0.7,
    drift: -1.0,
    wobble: 0.13,
};

/// The choirs each setting of the stops plucks.
const STOP_CHOIRS: [&[Choir]; 5] = [
    &[EIGHT],
    &[EIGHT, BACK],
    &[EIGHT, FOUR],
    &[SIXTEEN, EIGHT, FOUR],
    &[SIXTEEN, EIGHT, BACK, FOUR, TWO],
];

#[derive(Debug, Clone)]
struct Voice {
    strings: [Course; CHOIRS],
    /// The choirs it plucked, from the stops when it was played.
    choirs: &'static [Choir],
    /// Each string's gains to the left and the right.
    pans: [(f32, f32); CHOIRS],
    pitch: u8,
    hz: f32,
    sounding: bool,
    /// The key is down, so the damper is off the strings.
    held: bool,
    velocity: f32,
    /// The loop's damping, with the damper on or off.
    damping: f32,
    /// The quill's click: how loud it starts, where it's got to, and the last noise, which it's
    /// the difference of, so it's all top.
    click: f32,
    click_level: f32,
    click_last: f32,
    /// Fades a choked string out.
    fade: f32,
    choked: bool,
    started: u64,
    /// The loudest output since the last check, for telling when it's fallen silent.
    peak: f32,
}

/// One of the body's modes: a two-pole resonator.
#[derive(Debug, Clone, Copy, Default)]
struct Mode {
    a1: f32,
    a2: f32,
    /// The input's gain, which brings the mode's peak to about 1.
    b: f32,
    y1: f32,
    y2: f32,
}

impl Mode {
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b * x + self.a1 * self.y1 - self.a2 * self.y2;
        (self.y2, self.y1) = (self.y1, y);
        y
    }
}

#[derive(Debug, Clone)]
pub struct Krell {
    sample_rate: f32,
    voices: [Voice; VOICES],
    choirs: &'static [Choir],
    drift: f32,
    quill: f32,
    decay: f32,
    damper: f32,
    body: f32,
    hull: f32,
    glass: f32,
    gain: f32,
    modes: [Mode; MODES],
    /// Fades the body out when the track's choked, and back in at the next note.
    body_fade: f32,
    /// Where each choir has got to in its wavering, in cycles.
    wobble: [f32; CHOIRS],
    noise: Noise,
    /// One period of a pluck, worked out before it goes on a string.
    scratch: Box<[f32]>,
    click_coef: f32,
    fade_coef: f32,
    plucks: u64,
    clock: u32,
    /// The output's DC blocker, one per side, as `(last input, last output)`.
    dc: [(f32, f32); 2],
    dc_coef: f32,
}

impl Krell {
    /// Starts at a song's harpsichord settings, one value per parameter, by id. Allocates the
    /// delay lines, so call it off the audio thread.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        // Room for the lowest note's period and the interpolator's taps.
        let len = ((sample_rate / LOWEST_HZ) as usize + 8).next_power_of_two();
        let voice = Voice {
            strings: std::array::from_fn(|_| Course::new(len)),
            choirs: STOP_CHOIRS[0],
            pans: [(1.0, 1.0); CHOIRS],
            pitch: 0,
            hz: LOWEST_HZ,
            sounding: false,
            held: false,
            velocity: 0.0,
            damping: DULL,
            click: 0.0,
            click_level: 0.0,
            click_last: 0.0,
            fade: 1.0,
            choked: false,
            started: 0,
            peak: 0.0,
        };
        let mut krell = Self {
            sample_rate,
            voices: std::array::from_fn(|_| voice.clone()),
            choirs: STOP_CHOIRS[0],
            drift: 0.0,
            quill: 0.0,
            decay: 1.0,
            damper: 0.0,
            body: 0.0,
            hull: 196.0,
            glass: 0.0,
            gain: 0.0,
            modes: [Mode::default(); MODES],
            body_fade: 1.0,
            wobble: std::array::from_fn(|c| c as f32 * 0.37 % 1.0),
            noise: Noise(0x0c4e_11a5),
            // A period, and as much again for the pluck position's notch.
            scratch: vec![0.0; 2 * len].into_boxed_slice(),
            click_coef: decay_coef(CLICK_TIME, sample_rate),
            fade_coef: decay_coef(CHOKE, sample_rate),
            plucks: 0,
            clock: 0,
            dc: [(0.0, 0.0); 2],
            dc_coef: 1.0 - one_pole(10.0, sample_rate),
        };
        for info in InstKind::Harpsichord.params() {
            let value = values
                .get(info.id as usize)
                .copied()
                .unwrap_or(info.default);
            krell.set(info.id, info.clamp(value));
        }
        krell
    }

    /// Called with each setting, once at the start and whenever one changes. `value` is in range.
    /// The stops wait for the next note; the rest change the strings already sounding.
    fn set(&mut self, id: u32, value: f32) {
        match id {
            STOPS => {
                // The last for a value past the end.
                if let Some(choirs) = STOP_CHOIRS
                    .get(value.round() as usize)
                    .or(STOP_CHOIRS.last())
                {
                    self.choirs = choirs;
                }
            }
            DRIFT => self.drift = value,
            QUILL => self.quill = value,
            DECAY => self.decay = value,
            DAMPER => self.damper = value,
            BODY => self.body = value,
            HULL => self.hull = value,
            GLASS => self.glass = value,
            GAIN => self.gain = value,
            _ => {}
        }
        if matches!(id, HULL | GLASS) {
            self.tune_body();
        }
        if matches!(id, DRIFT | QUILL | DECAY | DAMPER) {
            self.retune();
        }
    }

    /// The body's modes, from the hull and the glass.
    fn tune_body(&mut self) {
        let rate = self.sample_rate;
        let modes = self.modes.iter_mut().zip(WOOD.iter().zip(&PLATE));
        for (k, (mode, (wood, plate))) in modes.enumerate() {
            let hz = self.hull * (wood + (plate - wood) * self.glass);
            let ring = (RING.0 * (RING.1 / RING.0).powf(self.glass)) / (1.0 + 0.25 * k as f32);
            let r = (-SIXTY_DB / (ring * rate)).exp();
            let w = std::f32::consts::TAU * hz / rate;
            mode.a1 = 2.0 * r * w.cos();
            mode.a2 = r * r;
            // A mode past the top would alias, so it's left silent.
            mode.b = if hz < TOP_MODE * rate {
                MODE_GAIN * (1.0 - r).sqrt() * 2.0 * w.sin()
            } else {
                0.0
            };
        }
    }

    /// The loop's damping, as the quill and, if it's down, the damper leave it.
    fn damping(&self, held: bool) -> f32 {
        let bright = BRIGHT.0 + (BRIGHT.1 - BRIGHT.0) * (1.0 - self.quill);
        if held {
            bright
        } else {
            bright + (DULL - bright) * self.damper
        }
    }

    /// What's left of a string each sample, from the decay and, if it's down, the damper.
    fn loss(&self, held: bool) -> f32 {
        let decay = if held {
            self.decay
        } else {
            // From the decay to the damper's, evenly in time's logarithm.
            self.decay * (DAMPED / self.decay).powf(self.damper)
        };
        (-SIXTY_DB / (decay * self.sample_rate)).exp()
    }

    /// Each choir's pitch against the note's, in semitones, as it's drifted and wavered.
    fn offset(&self, choir: &Choir) -> f32 {
        let phase = self.wobble.get(choir.id).copied().unwrap_or(0.0);
        let wobble = WOBBLE * (std::f32::consts::TAU * phase).sin();
        choir.pitch + MAX_DRIFT * self.drift * (choir.drift + wobble) / 100.0
    }

    /// Sets voice `i`'s strings' losses and pitches: tuned afresh if `pluck`, or stretched
    /// smoothly if they're sounding.
    fn tune(&mut self, i: usize, pluck: bool) {
        let Some(voice) = self.voices.get(i) else {
            return;
        };
        let (held, hz, choirs) = (voice.held, voice.hz, voice.choirs);
        let (damping, loss) = (self.damping(held), self.loss(held));
        let mut periods = [0.0; CHOIRS];
        for (period, choir) in periods.iter_mut().zip(choirs) {
            let hz = hz * 2f32.powf(self.offset(choir) / 12.0);
            *period = self.sample_rate / hz.max(LOWEST_HZ);
        }
        let Some(voice) = self.voices.get_mut(i) else {
            return;
        };
        voice.damping = damping;
        for (string, &period) in voice.strings.iter_mut().zip(&periods).take(choirs.len()) {
            if pluck {
                string.tune(period, damping);
            } else {
                string.stretch(period, damping);
            }
            string.loss = loss.powf(period);
        }
    }

    /// Retunes the strings sounding, after a setting's changed or as they waver.
    fn retune(&mut self) {
        for i in 0..VOICES {
            if self.voices.get(i).is_some_and(|v| v.sounding) {
                self.tune(i, false);
            }
        }
    }

    fn note_on(&mut self, pitch: u8, velocity: u8) {
        // The key already playing this note, else a silent one, else the oldest. The first of
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
        // A ringing string keeps some of itself, if it's the same strings plucked again.
        let keep =
            if voice.sounding && voice.pitch == pitch && std::ptr::eq(voice.choirs, self.choirs) {
                REPLUCK
            } else {
                0.0
            };
        self.plucks += 1;
        voice.pitch = pitch;
        voice.hz = midi_to_hz(pitch);
        voice.choirs = self.choirs;
        for (pan, choir) in voice.pans.iter_mut().zip(self.choirs) {
            // Equal power, so a string's as loud wherever it sits.
            let angle = (choir.pan + 1.0) * std::f32::consts::FRAC_PI_4;
            *pan = (angle.cos(), angle.sin());
        }
        voice.velocity = 1.0 - VELOCITY * (1.0 - f32::from(velocity) / 127.0);
        voice.sounding = true;
        voice.held = true;
        voice.choked = false;
        voice.fade = 1.0;
        voice.started = self.plucks;
        // Not silent before it's had a chance to sound.
        voice.peak = 1.0;
        voice.click = 1.0;
        voice.click_level = CLICK_LEVEL * self.quill * self.quill * voice.velocity;
        self.body_fade = 1.0;
        self.tune(i, true);
        self.pluck(i, keep);
    }

    /// Plucks voice `i`'s strings: one period of noise each, low-passed by the quill and notched
    /// by where it plucks, over what's left of the string times `keep`.
    fn pluck(&mut self, i: usize, keep: f32) {
        let Some(voice) = self.voices.get_mut(i) else {
            return;
        };
        let choirs = voice.choirs;
        // More stops are louder, but not as much louder as they'd be added up.
        let total = choirs.iter().map(|c| c.level * c.level).sum::<f32>().sqrt();
        let top = 0.45 * self.sample_rate;
        // A quill's always bright; a metal one is brighter still.
        let hardness = 0.55 + 0.45 * self.quill;
        for (string, choir) in voice.strings.iter_mut().zip(choirs) {
            let period = string.period;
            let len = (period.round() as usize).clamp(2, self.scratch.len() / 2);
            let hz = self.sample_rate / period;
            let cutoff = hz * (top / hz).max(1.0).powf(hardness);
            let smooth = one_pole(cutoff, self.sample_rate);
            let level = PLUCK_LEVEL * voice.velocity * choir.level / total.sqrt();
            let gap = ((choir.position * period).round() as usize).clamp(1, len - 1);
            let scratch = &mut self.scratch;
            if let Some(wave) = waveguide::pluck(scratch, &mut self.noise, len, gap, smooth, level)
            {
                string.place(wave, keep);
            }
        }
    }

    /// The key comes up: the damper falls on its strings, and the quill slides back past them.
    fn note_off(&mut self, pitch: u8) {
        for i in 0..VOICES {
            let Some(voice) = self.voices.get_mut(i) else {
                continue;
            };
            if voice.pitch != pitch || !voice.held || !voice.sounding {
                continue;
            }
            voice.held = false;
            voice.click = 1.0;
            voice.click_level = RELEASE_LEVEL * self.damper * voice.velocity;
            self.tune(i, false);
        }
    }

    fn choke(&mut self) {
        for voice in &mut self.voices {
            if voice.sounding {
                voice.choked = true;
                voice.held = false;
            }
        }
    }

    /// Every `CONTROL_PERIOD` samples: the choirs waver, and strings that have fallen silent
    /// stop.
    fn update_control(&mut self) {
        for (phase, choir) in self.wobble.iter_mut().zip(STOP_CHOIRS[4]) {
            *phase = (*phase + choir.wobble * CONTROL_PERIOD as f32 / self.sample_rate).fract();
        }
        if self.drift > 0.0 {
            self.retune();
        }
        for voice in &mut self.voices {
            if voice.sounding {
                let faded = voice.choked && voice.fade < SILENCE;
                voice.sounding = !faded && voice.peak > SILENCE;
                voice.peak = 0.0;
            }
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let ringing = self
            .modes
            .iter()
            .any(|m| m.y1.abs() + m.y2.abs() > SILENCE * 0.01);
        if self.voices.iter().all(|v| !v.sounding) && !ringing {
            left.fill(0.0);
            right.fill(0.0);
            self.dc = [(0.0, 0.0); 2];
            for mode in &mut self.modes {
                (mode.y1, mode.y2) = (0.0, 0.0);
            }
            return;
        }
        let body = self.body;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.clock.is_multiple_of(CONTROL_PERIOD) {
                self.update_control();
            }
            self.clock = self.clock.wrapping_add(1);
            let (mut out_l, mut out_r, mut mono) = (0.0, 0.0, 0.0);
            // Fading the body only once every string sounding has been choked.
            let (mut choked, mut any) = (true, false);
            for voice in &mut self.voices {
                if !voice.sounding {
                    continue;
                }
                choked &= voice.choked;
                any = true;
                let (mut a, mut b) = (0.0, 0.0);
                let strings = voice.strings.iter_mut().zip(&voice.pans);
                for (string, (pan_l, pan_r)) in strings.take(voice.choirs.len()) {
                    let x = string.tick(voice.damping, 0.0, None);
                    a += x * pan_l;
                    b += x * pan_r;
                }
                if voice.click > SILENCE {
                    let noise = self.noise.next();
                    let click = voice.click * voice.click_level * (noise - voice.click_last);
                    voice.click_last = noise;
                    voice.click *= self.click_coef;
                    a += click;
                    b += click;
                }
                let fade = voice.fade;
                if voice.choked {
                    voice.fade *= self.fade_coef;
                }
                voice.peak = voice.peak.max(a.abs()).max(b.abs());
                out_l += a * fade;
                out_r += b * fade;
                mono += (a + b) * fade;
            }
            if choked && any {
                self.body_fade *= self.fade_coef;
            }
            // The body's even modes ring on the left and its odd ones on the right, each a
            // little on the other side too.
            let (mut body_l, mut body_r) = (0.0, 0.0);
            for [even, odd] in self.modes.as_chunks_mut::<2>().0 {
                let (e, o) = (even.tick(mono), odd.tick(mono));
                body_l += e + 0.3 * o;
                body_r += o + 0.3 * e;
            }
            out_l += body * self.body_fade * body_l;
            out_r += body * self.body_fade * body_r;
            for (out, (x1, y1)) in [&mut out_l, &mut out_r].into_iter().zip(&mut self.dc) {
                let y = *out - *x1 + self.dc_coef * *y1;
                (*x1, *y1) = (*out, y);
                *out = y * OUT * self.gain;
            }
            (*l, *r) = (out_l, out_r);
        }
    }
}

fn midi_to_hz(pitch: u8) -> f32 {
    440.0 * 2f32.powf((f32::from(pitch) - 69.0) / 12.0)
}

impl Processor for Krell {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.render(l, r),
            Segment::Event(event) => match event.kind {
                EventKind::NoteOn { pitch, velocity } => self.note_on(pitch, velocity),
                EventKind::NoteOff { pitch } => self.note_off(pitch),
                EventKind::Choke => self.choke(),
                // Its notes don't glide.
                EventKind::Shape(_) => {}
                EventKind::Param { id, value } => {
                    if let Some(info) = InstKind::Harpsichord.params().get(id as usize) {
                        self.set(id, info.clamp(value));
                    }
                }
            },
        });
    }

    fn params(&self) -> &[ParamInfo] {
        InstKind::Harpsichord.params()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn harpsichord(settings: &[(u32, f32)]) -> Krell {
        let mut values = lattice_core::Instrument::new(InstKind::Harpsichord).values;
        for &(id, value) in settings {
            values[id as usize] = value;
        }
        Krell::with_values(RATE, &values)
    }

    fn on(pitch: u8) -> Event {
        Event {
            offset: 0,
            kind: EventKind::NoteOn {
                pitch,
                velocity: 127,
            },
        }
    }

    fn off(pitch: u8) -> Event {
        Event {
            offset: 0,
            kind: EventKind::NoteOff { pitch },
        }
    }

    /// `seconds` of output after `events`, both sides.
    fn play(k: &mut Krell, events: &[Event], seconds: f32) -> (Vec<f32>, Vec<f32>) {
        let len = (seconds * RATE) as usize;
        let (mut l, mut r) = (vec![0.0; len], vec![0.0; len]);
        k.process(events, &mut l, &mut r);
        (l, r)
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

    #[test]
    fn plays_in_tune_and_rings_while_held() {
        for note in [36, 57, 81] {
            let mut k = harpsichord(&[(STOPS, 0.0), (DRIFT, 0.0), (BODY, 0.0)]);
            let (l, _) = play(&mut k, &[on(note)], 0.5);
            let hz = midi_to_hz(note);
            let heard = pitch(&l[4_800..16_800], hz);
            let cents = 1200.0 * (heard / hz).log2();
            assert!(cents.abs() < 3.0, "{note}: {heard} Hz, not {hz}");
            assert!(rms(&l[19_200..]) > 0.003, "{note} still rings");
        }
    }

    #[test]
    fn silent_until_played_and_the_damper_stops_it() {
        let mut k = harpsichord(&[(DAMPER, 1.0)]);
        assert!(play(&mut k, &[], 0.1).0.iter().all(|&x| x == 0.0));
        let (l, _) = play(&mut k, &[on(60)], 0.3);
        assert!(rms(&l) > 0.01);
        let (l, _) = play(&mut k, &[off(60)], 0.3);
        assert!(rms(&l[9_600..]) < 1e-3, "{}", rms(&l[9_600..]));
        play(&mut k, &[], 6.0);
        assert!(play(&mut k, &[], 0.1).0.iter().all(|&x| x == 0.0));

        // With the dampers off, it rings on after the key's up.
        let mut k = harpsichord(&[(DAMPER, 0.0)]);
        play(&mut k, &[on(60)], 0.3);
        let (l, _) = play(&mut k, &[off(60)], 0.3);
        assert!(rms(&l[9_600..]) > 0.01);
    }

    #[test]
    fn the_stops_add_octaves() {
        let octaves = |stops: f32| {
            let mut k = harpsichord(&[(STOPS, stops), (DRIFT, 0.0), (BODY, 0.0)]);
            let (l, _) = play(&mut k, &[on(57)], 0.5);
            let x = &l[4_800..];
            (strength(x, 110.0), strength(x, 220.0))
        };
        let (low, note) = octaves(0.0);
        assert!(low < note * 0.01, "{low} {note}");
        let (low, note) = octaves(3.0);
        assert!(low > note * 0.1, "{low} {note}");
    }

    #[test]
    fn drift_spreads_the_choirs_in_stereo() {
        let width = |drift: f32| {
            let mut k = harpsichord(&[(STOPS, 4.0), (DRIFT, drift), (BODY, 0.0)]);
            let (l, r) = play(&mut k, &[on(57)], 1.0);
            let side: Vec<f32> = l.iter().zip(&r).map(|(l, r)| l - r).collect();
            rms(&side[24_000..]) / rms(&l[24_000..])
        };
        assert!(width(0.0) > 0.1, "the choirs sit apart");
        // Drifting, they beat against each other, so they don't stay fixed in place.
        let (still, drifting) = (width(0.0), width(1.0));
        assert!(
            drifting.is_finite() && (drifting - still).abs() > 0.02,
            "{still} {drifting}"
        );
    }

    #[test]
    fn the_body_rings_with_notes_in_its_key_and_on_after_them() {
        let ring = |body: f32| {
            let mut k = harpsichord(&[(BODY, body), (HULL, 220.0), (GLASS, 1.0), (DAMPER, 1.0)]);
            play(&mut k, &[on(57)], 0.3);
            let (l, _) = play(&mut k, &[off(57)], 0.5);
            rms(&l[12_000..])
        };
        assert!(ring(1.0) > 10.0 * ring(0.0), "{} {}", ring(1.0), ring(0.0));
        // Choked, it stops.
        let mut k = harpsichord(&[(BODY, 1.0), (HULL, 220.0), (GLASS, 1.0)]);
        play(&mut k, &[on(57)], 0.3);
        let choke = Event {
            offset: 0,
            kind: EventKind::Choke,
        };
        let (l, _) = play(&mut k, &[choke], 0.1);
        assert!(rms(&l[2_400..]) < 1e-4);
    }

    #[test]
    fn stays_steady_at_its_most_extreme() {
        let mut k = harpsichord(&[
            (STOPS, 4.0),
            (DRIFT, 1.0),
            (QUILL, 1.0),
            (DECAY, 20.0),
            (DAMPER, 0.0),
            (BODY, 1.0),
            (HULL, 40.0),
            (GLASS, 1.0),
            (GAIN, 1.0),
        ]);
        let chord: Vec<Event> = [24, 36, 48, 60, 72, 84, 96, 108, 120]
            .into_iter()
            .map(on)
            .collect();
        let (l, r) = play(&mut k, &chord, 4.0);
        let peak = l.iter().chain(&r).fold(0.0f32, |p, x| p.max(x.abs()));
        assert!(peak.is_finite() && peak < 4.0, "{peak}");
        assert!(rms(&l[96_000..]) > 1e-3, "it rings on");
    }
}
