//! Monochord, the string engine: a plucked or bowed string, after Pythagoras' one-stringed
//! instrument for studying pitch.
//!
//! Each voice is a waveguide string, as in the Synthesis ToolKit's `Bowed`: two delay lines that
//! meet where the bow sits, one running to the nut and back, the other to the bridge and back.
//! Reflections invert the waves at each end, and the bridge's end has a gentle low-pass and a
//! loss, so high harmonics die sooner than low ones, as on a real string. The bridge's line is
//! read with a fractional delay, so the string plays in tune.
//!
//! A pluck fills both lines with a burst of filtered noise, shaped by where along the string it's
//! plucked, and a pick that hits hard adds its own click. A bow, while the note's held, drags the
//! string along by friction until it slips back, again and again: the sawtooth motion of a real
//! bowed string. Notes ring out by their decay after a note-off, which only lifts the bow;
//! plucking a string that's still ringing damps it first, as a finger would.

use lattice_core::InstKind;
use lattice_core::instrument::monochord::{BOW, DECAY, MODEL, PLUCK, POSITION, TONE};

use crate::dsp::decay_coef;
use crate::param::ParamInfo;
use crate::processor::{Event, EventKind, Processor, Segment, Shape, segments};
use crate::rhythmicon::{Noise, SIXTY_DB, one_pole};
use crate::waveguide::{self, Course};

const VOICES: usize = 8;
/// The lowest pitch a string plays; lower notes play this.
const LOWEST_HZ: f32 = 20.0;
/// Samples between checks for strings that have fallen silent.
const CONTROL_PERIOD: u32 = 64;
/// Samples between a gliding string's steps in pitch, often enough not to be heard as steps.
const GLIDE_PERIOD: u32 = 16;
/// Below this, a string has finished and stops rendering.
const SILENCE: f32 = 1e-4;
/// How long a choked string takes to fade, in seconds to fall by 1/e.
const CHOKE: f32 = 0.003;
/// How long the bow takes to bite, or to lift, in seconds to move by 1/e.
const BOW_TIME: f32 = 0.03;
/// The bow's speed at the lightest bow and at the heaviest, in the string's units.
const BOW_SPEED: (f32, f32) = (0.06, 0.3);
/// The bow's pressure at the lightest and the heaviest, as the friction curve's slope: low skates
/// and breathes, high grips and scratches.
const BOW_GRIP: (f32, f32) = (2.0, 4.2);
/// How much the bow's speed wavers, as a hand's does.
const BOW_WAVER: f32 = 0.05;
/// Where the bowed sound starts to fall away, 6 dB an octave, as a multiple of the note: the
/// string's own sawtooth, rather than the pulses of its velocity at the bridge.
const BOW_TILT: f32 = 1.5;
/// Makes up the level the tilt takes.
const BOW_LIFT: f32 = 2.0;
/// How much of a ringing string is left when it's plucked again.
const REPLUCK: f32 = 0.3;
/// The level of a full-velocity pluck, as the RMS of the string's displacement.
const PLUCK_LEVEL: f32 = 0.35;
/// The level of the hardest pick's click, and how long it lasts, in seconds to fall by 1/e.
const CLICK_LEVEL: f32 = 0.8;
const CLICK_TIME: f32 = 0.0015;
/// Each string's output, so a few together leave headroom.
const GAIN: f32 = 0.35;

/// What sets one kind of string apart.
#[derive(Debug)]
struct StringModel {
    /// The hardest pluck's brightness, as a fraction of the way from the fundamental to the top.
    pick: f32,
    /// The loop's damping with the tone all the way up, from 0 (none) to 0.5 (the most).
    bright: f32,
    /// Strings to a course, sounding together.
    courses: usize,
    /// How far the second string of a course is tuned above the first, in cents.
    detune: f32,
    /// How far a loud string shortens as it hits the bridge, as a fraction of its length: a
    /// little gives the falling pitch of a hard pluck, more a sitar's buzz.
    buzz: f32,
}

const STRINGS: [StringModel; 3] = [
    // Nylon: a softer attack and a warm, fairly quick-fading top.
    StringModel {
        pick: 0.75,
        bright: 0.2,
        courses: 1,
        detune: 0.0,
        buzz: 0.0,
    },
    // Steel: bright, in pairs a few cents apart, so they shimmer as they beat.
    StringModel {
        pick: 1.0,
        bright: 0.04,
        courses: 2,
        detune: 7.0,
        buzz: 0.004,
    },
    // Sitar: bright, and buzzing against its curved bridge (jawari).
    StringModel {
        pick: 1.0,
        bright: 0.02,
        courses: 1,
        detune: 0.0,
        buzz: 0.06,
    },
];

#[derive(Debug, Clone)]
struct Voice {
    courses: [Course; 2],
    /// Strings sounding, from the model when it was plucked.
    strings: usize,
    pitch: u8,
    /// The pitch it's tuned to, gliding towards the note's.
    hz: f32,
    /// Gliding: `GLIDE_PERIOD`s to go, and the step in pitch each takes.
    glide_left: u32,
    glide_ratio: f32,
    sounding: bool,
    /// The note is held, so the bow, if any, keeps going.
    held: bool,
    velocity: f32,
    /// The bridge's low-pass's damping.
    damping: f32,
    buzz: f32,
    /// How far the bow is on the string, gliding to 1 while the note's held and to 0 after.
    bow: f32,
    /// The bowed sound's tilt: a low-pass for each string, and its coefficient.
    tilt: [f32; 2],
    tilt_coef: f32,
    /// The pick's click: how loud it starts, where it's got to, and the last noise, which it's
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

#[derive(Debug, Clone)]
pub struct Monochord {
    sample_rate: f32,
    voices: [Voice; VOICES],
    model: &'static StringModel,
    pluck: f32,
    position: f32,
    decay: f32,
    tone: f32,
    bow: f32,
    noise: Noise,
    /// One period of a pluck, worked out before it goes on the string.
    scratch: Box<[f32]>,
    bow_coef: f32,
    click_coef: f32,
    fade_coef: f32,
    plucks: u64,
    /// The pitch last played, which a glide starts from.
    last: Option<u8>,
    /// How the next note plays.
    shape: Shape,
    clock: u32,
    /// The output's DC blocker, one per side, as `(last input, last output)`.
    dc: [(f32, f32); 2],
    dc_coef: f32,
}

impl Monochord {
    /// Starts at a song's string settings, one value per parameter, by id. Allocates the delay
    /// lines, so call it off the audio thread.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        // Room for the lowest note's period and the interpolator's taps.
        let len = ((sample_rate / LOWEST_HZ) as usize + 8).next_power_of_two();
        let voice = Voice {
            courses: [Course::new(len), Course::new(len)],
            strings: 1,
            pitch: 0,
            hz: LOWEST_HZ,
            glide_left: 0,
            glide_ratio: 1.0,
            sounding: false,
            held: false,
            velocity: 0.0,
            damping: 0.5,
            buzz: 0.0,
            bow: 0.0,
            tilt: [0.0; 2],
            tilt_coef: 1.0,
            click: 0.0,
            click_level: 0.0,
            click_last: 0.0,
            fade: 1.0,
            choked: false,
            started: 0,
            peak: 0.0,
        };
        let mut string = Self {
            sample_rate,
            voices: std::array::from_fn(|_| voice.clone()),
            model: &STRINGS[0],
            pluck: 0.0,
            position: 0.0,
            decay: 1.0,
            tone: 0.0,
            bow: 0.0,
            noise: Noise(0x5eed_1e55),
            // A period, and as much again for the pluck position's notch.
            scratch: vec![0.0; 2 * len].into_boxed_slice(),
            bow_coef: decay_coef(BOW_TIME, sample_rate),
            click_coef: decay_coef(CLICK_TIME, sample_rate),
            fade_coef: decay_coef(CHOKE, sample_rate),
            plucks: 0,
            last: None,
            shape: Shape::default(),
            clock: 0,
            dc: [(0.0, 0.0); 2],
            dc_coef: 1.0 - one_pole(10.0, sample_rate),
        };
        for info in InstKind::Monochord.params() {
            let value = values
                .get(info.id as usize)
                .copied()
                .unwrap_or(info.default);
            string.set(info.id, info.clamp(value));
        }
        string
    }

    /// Called with each setting, once at the start and whenever one changes. `value` is in range.
    /// Decay, tone and bow change the strings sounding too; the rest wait for the next pluck.
    fn set(&mut self, id: u32, value: f32) {
        match id {
            MODEL => {
                // The last for a value past the end.
                if let Some(model) = STRINGS.get(value.round() as usize).or(STRINGS.last()) {
                    self.model = model;
                }
            }
            PLUCK => self.pluck = value,
            POSITION => self.position = value,
            DECAY => self.decay = value,
            TONE => self.tone = value,
            BOW => self.bow = value,
            _ => {}
        }
        if matches!(id, DECAY | TONE) {
            for voice in self.voices.iter_mut().filter(|v| v.sounding) {
                Self::tune(voice, self.model, self.tone, self.decay, self.sample_rate);
            }
        }
    }

    /// The loop's losses and delays for `voice`'s pitch, from the decay and tone.
    fn tune(voice: &mut Voice, model: &StringModel, tone: f32, decay: f32, sample_rate: f32) {
        let damping = model.bright + (0.5 - model.bright) * (1.0 - tone);
        voice.damping = damping;
        // Lost per sample, so every string fades over the decay whatever its period.
        let loss = (-SIXTY_DB / (decay * sample_rate)).exp();
        let hz = voice.hz.max(LOWEST_HZ);
        voice.tilt_coef = one_pole(hz * BOW_TILT, sample_rate);
        for (c, course) in voice.courses.iter_mut().enumerate() {
            let cents = if c == 0 { 0.0 } else { model.detune };
            let period = sample_rate / (hz * 2f32.powf(cents / 1200.0));
            course.tune(period, damping);
            course.loss = loss.powf(period);
        }
    }

    fn note_on(&mut self, pitch: u8, velocity: u8) {
        // The string already playing this note, else a silent one, else the oldest. The first of
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
        let ringing = voice.sounding && voice.pitch == pitch;
        let model = self.model;
        self.plucks += 1;
        voice.pitch = pitch;
        // Gliding, it's plucked at the last pitch and slides evenly, in pitch, to its own.
        let to = midi_to_hz(pitch);
        let shape = std::mem::take(&mut self.shape);
        (voice.hz, voice.glide_left) = match self.last {
            Some(last) if shape.glide > 0 => {
                let steps = shape.glide.div_ceil(GLIDE_PERIOD).max(1);
                let from = midi_to_hz(last);
                voice.glide_ratio = (to / from).powf(1.0 / steps as f32);
                (from, steps)
            }
            _ => (to, 0),
        };
        self.last = Some(pitch);
        voice.velocity = f32::from(velocity) / 127.0;
        voice.strings = model.courses;
        voice.buzz = model.buzz;
        voice.sounding = true;
        voice.held = true;
        voice.choked = false;
        voice.fade = 1.0;
        voice.started = self.plucks;
        // Not silent before it's had a chance to sound.
        voice.peak = 1.0;
        if !ringing {
            voice.bow = 0.0;
        }
        // The harder the pick, the louder its click; a thumb makes none.
        voice.click = 1.0;
        voice.click_level =
            CLICK_LEVEL * self.pluck * self.pluck * voice.velocity * (1.0 - self.bow);
        Self::tune(voice, model, self.tone, self.decay, self.sample_rate);
        self.pluck(i, if ringing { REPLUCK } else { 0.0 });
    }

    /// Plucks voice `i`: one period of noise, low-passed by how hard the pluck is and notched by
    /// where it lands, goes round each of its strings, over what's left of the string times
    /// `keep`. A bow takes over from the pluck as it's turned up.
    fn pluck(&mut self, i: usize, keep: f32) {
        let model = self.model;
        // `i` is the voice just picked.
        let Some(voice) = self.voices.get_mut(i) else {
            return;
        };
        let level = PLUCK_LEVEL * voice.velocity * (1.0 - self.bow);
        let period = voice.courses[0].period;
        let len = (period.round() as usize).clamp(2, self.scratch.len() / 2);
        let hz = self.sample_rate / period;
        // Harder plucks, and harder-played notes, are brighter: from just above the note, which
        // is nearly a sine, to the top.
        let hardness = self.pluck * (0.8 + 0.2 * voice.velocity) * model.pick;
        let top = 0.45 * self.sample_rate;
        let cutoff = hz * (top / hz).max(1.0).powf(hardness);
        let smooth = one_pole(cutoff, self.sample_rate);

        // Plucking a fraction `position` along cancels the harmonics with a node there.
        let gap = ((self.position * period).round() as usize).clamp(1, len - 1);
        let Some(wave) =
            waveguide::pluck(&mut self.scratch, &mut self.noise, len, gap, smooth, level)
        else {
            return;
        };
        let strings = voice.strings;
        for course in voice.courses.iter_mut().take(strings) {
            course.place(wave, keep);
        }
    }

    fn note_off(&mut self, pitch: u8) {
        for voice in &mut self.voices {
            if voice.pitch == pitch {
                voice.held = false;
            }
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

    /// Every `GLIDE_PERIOD` samples: gliding strings take their next step in pitch, ending on
    /// their note's.
    fn glide(&mut self) {
        let (model, tone, decay, rate) = (self.model, self.tone, self.decay, self.sample_rate);
        for voice in self.voices.iter_mut().filter(|v| v.glide_left > 0) {
            voice.glide_left -= 1;
            voice.hz = match voice.glide_left {
                0 => midi_to_hz(voice.pitch),
                _ => voice.hz * voice.glide_ratio,
            };
            Self::tune(voice, model, tone, decay, rate);
        }
    }

    /// Every `CONTROL_PERIOD` samples: strings that have fallen silent stop.
    fn update_control(&mut self) {
        for voice in &mut self.voices {
            if voice.sounding {
                let bowing = voice.held && self.bow > 0.0 || voice.bow > SILENCE;
                let faded = voice.choked && voice.fade < SILENCE;
                voice.sounding = !faded && (voice.peak > SILENCE || bowing);
                voice.peak = 0.0;
            }
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.voices.iter().all(|v| !v.sounding) {
            left.fill(0.0);
            right.fill(0.0);
            self.dc = [(0.0, 0.0); 2];
            return;
        }
        let lerp = |(lo, hi): (f32, f32)| lo + (hi - lo) * self.bow;
        let (speed, grip) = (lerp(BOW_SPEED), lerp(BOW_GRIP));
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.clock.is_multiple_of(CONTROL_PERIOD) {
                self.update_control();
            }
            if self.clock.is_multiple_of(GLIDE_PERIOD) {
                self.glide();
            }
            self.clock = self.clock.wrapping_add(1);
            let (mut out_l, mut out_r) = (0.0, 0.0);
            for voice in &mut self.voices {
                if !voice.sounding {
                    continue;
                }
                let target = if voice.held && self.bow > 0.0 {
                    1.0
                } else {
                    0.0
                };
                voice.bow = target + (voice.bow - target) * self.bow_coef;
                let bow = (voice.bow > 1e-3).then(|| {
                    let waver = 1.0 + BOW_WAVER * self.noise.next();
                    (speed * voice.bow * voice.velocity * waver, grip)
                });
                let strings = voice.strings;
                let mut sum = [0.0; 2];
                let courses = voice.courses.iter_mut().zip(&mut voice.tilt).zip(&mut sum);
                for ((course, tilt), sum) in courses.take(strings) {
                    let x = course.tick(voice.damping, voice.buzz, bow);
                    // Tilted as far as the bow's on the string; a pluck rings as it is.
                    *tilt += voice.tilt_coef * (x - *tilt);
                    *sum = x + voice.bow * (BOW_LIFT * *tilt - x);
                }
                let mut click = 0.0;
                if voice.click > SILENCE {
                    let noise = self.noise.next();
                    click = voice.click * voice.click_level * (noise - voice.click_last);
                    voice.click_last = noise;
                    voice.click *= self.click_coef;
                }
                let fade = voice.fade;
                if voice.choked {
                    voice.fade *= self.fade_coef;
                }
                let (a, b) = if strings == 2 {
                    // A pair of strings, spread a little apart.
                    (0.6 * sum[0] + 0.4 * sum[1], 0.4 * sum[0] + 0.6 * sum[1])
                } else {
                    (sum[0], sum[0])
                };
                let (a, b) = (a + click, b + click);
                voice.peak = voice.peak.max(a.abs()).max(b.abs());
                out_l += a * fade;
                out_r += b * fade;
            }
            for (out, (x1, y1)) in [&mut out_l, &mut out_r].into_iter().zip(&mut self.dc) {
                let y = *out - *x1 + self.dc_coef * *y1;
                (*x1, *y1) = (*out, y);
                *out = y * GAIN;
            }
            (*l, *r) = (out_l, out_r);
        }
    }
}

fn midi_to_hz(pitch: u8) -> f32 {
    440.0 * 2f32.powf((f32::from(pitch) - 69.0) / 12.0)
}

impl Processor for Monochord {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.render(l, r),
            Segment::Event(event) => match event.kind {
                EventKind::NoteOn { pitch, velocity } => self.note_on(pitch, velocity),
                EventKind::NoteOff { pitch } => self.note_off(pitch),
                // A string is plucked again.
                EventKind::Retune { from, to, velocity } => {
                    self.note_off(from);
                    self.note_on(to, velocity);
                }
                EventKind::Choke => self.choke(),
                EventKind::Shape(shape) => self.shape = shape,
                EventKind::Program(_) => {}
                EventKind::Param { id, value } => {
                    if let Some(info) = InstKind::Monochord.params().get(id as usize) {
                        self.set(id, info.clamp(value));
                    }
                }
            },
        });
    }

    fn params(&self) -> &[ParamInfo] {
        InstKind::Monochord.params()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn string(settings: &[(u32, f32)]) -> Monochord {
        let mut values = lattice_core::Instrument::new(InstKind::Monochord).values;
        for &(id, value) in settings {
            values[id as usize] = value;
        }
        Monochord::with_values(RATE, &values)
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

    /// `seconds` of output after `events`, left side.
    fn play(m: &mut Monochord, events: &[Event], seconds: f32) -> Vec<f32> {
        let len = (seconds * RATE) as usize;
        let (mut l, mut r) = (vec![0.0; len], vec![0.0; len]);
        m.process(events, &mut l, &mut r);
        l
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
        // Between samples, from the parabola through the best lag and its neighbours.
        let (a, b, c) = (score(best - 1), score(best), score(best + 1));
        let lag = best as f32 + 0.5 * (a - c) / (a - 2.0 * b + c);
        RATE / lag
    }

    fn cents(a: f32, b: f32) -> f32 {
        1200.0 * (a / b).log2()
    }

    #[test]
    fn a_glide_slides_from_the_last_pitch_to_its_own() {
        let mut m = string(&[]);
        play(&mut m, &[on(57)], 0.1);
        let glide = Event {
            offset: 0,
            kind: EventKind::Shape(Shape {
                glide: (0.2 * RATE) as u32,
                ..Shape::default()
            }),
        };
        // Plucked at A3, it's at A4 once the glide's over, and on its way between.
        let start = play(&mut m, &[glide, on(69)], 0.02);
        assert!(cents(pitch(&start, 225.0), 225.0).abs() < 30.0);
        let middle = play(&mut m, &[], 0.1);
        let between = pitch(&middle[2_400..], 330.0);
        assert!((260.0..400.0).contains(&between), "{between}");
        play(&mut m, &[], 0.1);
        let end = play(&mut m, &[], 0.05);
        assert!(cents(pitch(&end, 440.0), 440.0).abs() < 10.0);
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|x| x * x).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn plays_in_tune_from_low_to_high() {
        for pitch in [33, 45, 57, 69, 81, 93] {
            let mut m = string(&[]);
            let x = play(&mut m, &[on(pitch)], 0.5);
            let hz = midi_to_hz(pitch);
            let heard = pitch_of(&x[2_400..], hz);
            assert!(
                cents(heard, hz).abs() < 3.0,
                "{pitch}: {heard} Hz, not {hz}"
            );
        }
    }

    fn pitch_of(x: &[f32], hz: f32) -> f32 {
        pitch(&x[..x.len().min(12_000)], hz)
    }

    #[test]
    fn rings_out_and_stops_and_high_notes_die_sooner() {
        let mut low = string(&[(DECAY, 1.0)]);
        let mut high = string(&[(DECAY, 1.0)]);
        let lows = play(&mut low, &[on(40)], 0.6);
        let highs = play(&mut high, &[on(88)], 0.6);
        let fall = |x: &[f32]| rms(&x[24_000..28_800]) / rms(&x[..4_800]);
        assert!(fall(&highs) < fall(&lows) / 2.0);
        // A note-off doesn't stop a plucked string.
        let off = Event {
            offset: 0,
            kind: EventKind::NoteOff { pitch: 40 },
        };
        assert!(rms(&play(&mut low, &[off], 0.1)) > 1e-3);
        play(&mut low, &[], 3.0);
        assert!(low.voices.iter().all(|v| !v.sounding));
        assert!(play(&mut low, &[], 0.1).iter().all(|&x| x == 0.0));
    }

    #[test]
    fn a_bow_holds_the_string_until_the_note_ends() {
        let mut m = string(&[(BOW, 1.0), (DECAY, 0.5)]);
        let x = play(&mut m, &[on(57)], 2.0);
        let (early, late) = (rms(&x[9_600..19_200]), rms(&x[86_400..]));
        assert!(late > 0.02 && late > early * 0.7, "{early} then {late}");
        // The bow drags the string round at its own pitch.
        let heard = pitch_of(&x[48_000..], midi_to_hz(57));
        assert!(cents(heard, midi_to_hz(57)).abs() < 10.0, "{heard} Hz");
        let off = Event {
            offset: 0,
            kind: EventKind::NoteOff { pitch: 57 },
        };
        play(&mut m, &[off], 2.0);
        assert!(m.voices.iter().all(|v| !v.sounding));
    }

    #[test]
    fn every_model_stays_steady_at_its_most_extreme() {
        for (model, bow) in (0..3).flat_map(|m| [(m, 0.0), (m, 1.0)]) {
            let mut m = string(&[
                (MODEL, model as f32),
                (PLUCK, 1.0),
                (DECAY, 20.0),
                (TONE, 1.0),
                (POSITION, 0.02),
                (BOW, bow),
            ]);
            let chord: Vec<Event> = [28, 40, 52, 64, 76, 88, 100, 112, 124]
                .into_iter()
                .map(on)
                .collect();
            let x = play(&mut m, &chord, 4.0);
            let peak = x.iter().fold(0.0f32, |p, x| p.max(x.abs()));
            assert!(
                peak.is_finite() && peak < 4.0,
                "model {model}, bow {bow}: {peak}"
            );
            // Plucked, it only fades; bowed, it swells for a second or two, then holds.
            let (early, late) = (rms(&x[24_000..48_000]), rms(&x[168_000..]));
            let most = if bow > 0.0 { 2.5 } else { 1.0 };
            assert!(
                late <= early * most,
                "model {model}, bow {bow}: {early} then {late}"
            );
        }
    }

    #[test]
    fn steel_shimmers_in_stereo_and_choking_stops_it() {
        let mut m = string(&[(MODEL, 1.0)]);
        let (mut l, mut r) = (vec![0.0; 4_800], vec![0.0; 4_800]);
        m.process(&[on(52)], &mut l, &mut r);
        assert_ne!(l, r);
        let choke = Event {
            offset: 0,
            kind: EventKind::Choke,
        };
        play(&mut m, &[choke], 0.1);
        assert!(m.voices.iter().all(|v| !v.sounding));
    }
}
