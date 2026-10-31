//! Monochord, the string engine: a plucked or bowed string, after Pythagoras' one-stringed
//! instrument for studying pitch.
//!
//! Each voice is a waveguide (an extended Karplus–Strong string): a delay line one period long,
//! read with a fractional delay so it plays in tune, and a gentle low-pass in its loop, so high
//! harmonics die sooner than low ones, as on a real string. A pluck fills the line with a burst
//! of filtered noise, shaped by where along the string it's plucked; a bow feeds noise in for as
//! long as the note is held. Notes ring out by their decay after a note-off, which only lifts
//! the bow; plucking a string that's still ringing damps it first, as a finger would.

use lattice_core::InstKind;
use lattice_core::instrument::string::{BOW, DECAY, MODEL, PLUCK, POSITION, TONE};

use crate::dsp::decay_coef;
use crate::param::ParamInfo;
use crate::processor::{Event, EventKind, Processor, Segment, segments};
use crate::rhythmicon::{Noise, SIXTY_DB, one_pole};

const VOICES: usize = 8;
/// The lowest pitch a string plays; lower notes play this.
const LOWEST_HZ: f32 = 20.0;
/// Samples between checks for strings that have fallen silent.
const CONTROL_PERIOD: u32 = 64;
/// Below this, a string has finished and stops rendering.
const SILENCE: f32 = 1e-4;
/// How long a choked string takes to fade, in seconds to fall by 1/e.
const CHOKE: f32 = 0.003;
/// The bow's noise is low-passed this many times above the note, so it rosins the string
/// rather than hissing over it.
const BOW_BAND: f32 = 4.0;
/// How long the bow takes to bite, or to lift, in seconds to move by 1/e.
const BOW_TIME: f32 = 0.06;
/// How much of a ringing string is left when it's plucked again.
const REPLUCK: f32 = 0.3;
/// The level of a full-velocity pluck, as the RMS of the string's displacement.
const PLUCK_LEVEL: f32 = 0.35;
/// The level a full bow holds the string at.
const BOW_LEVEL: f32 = 0.8;
/// Each string's output, so a few together leave headroom.
const GAIN: f32 = 0.35;

/// What sets one kind of string apart.
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
    // Nylon: a soft attack and a warm, fairly quick-fading top.
    StringModel {
        pick: 0.4,
        bright: 0.25,
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

/// One string: a delay line and its loop filter.
#[derive(Debug, Clone)]
struct Course {
    line: Box<[f32]>,
    /// Where the next sample goes.
    write: usize,
    /// The delay, in samples, that tunes the loop: its period less the loop filter's delay.
    delay: f32,
    /// The period, in samples.
    period: f32,
    /// What's left after each trip round the loop.
    loss: f32,
    /// How much of a bow's noise goes in each sample, to make up for the loss.
    feed: f32,
    /// The sample read last time round, for the loop filter.
    last: f32,
}

impl Course {
    fn new(len: usize) -> Self {
        Self {
            line: vec![0.0; len].into_boxed_slice(),
            write: 0,
            delay: 2.0,
            period: 2.0,
            loss: 0.0,
            feed: 0.0,
            last: 0.0,
        }
    }

    /// The line `delay` samples back, read with third-order Lagrange interpolation, which keeps
    /// the high harmonics a linear read would dull. `delay` is at least 2.
    #[inline]
    fn read(&self, delay: f32) -> f32 {
        let mask = self.line.len() - 1;
        let whole = delay as usize;
        // The four taps sit at delays whole - 1 to whole + 2, so `d` is between 1 and 2.
        let d = delay - whole as f32 + 1.0;
        let at = |k: usize| self.line[(self.write + mask + 1 - (whole - 1 + k)) & mask];
        let (dm1, dm2, dm3) = (d - 1.0, d - 2.0, d - 3.0);
        -dm1 * dm2 * dm3 / 6.0 * at(0) + d * dm2 * dm3 / 2.0 * at(1) - d * dm1 * dm3 / 2.0 * at(2)
            + d * dm1 * dm2 / 6.0 * at(3)
    }

    /// One trip round the loop: reads the string at `delay`, filters it by `damping` and `loss`,
    /// adds `input`, and returns what was read.
    #[inline]
    fn tick(&mut self, delay: f32, damping: f32, loss: f32, input: f32) -> f32 {
        let x = self.read(delay);
        let y = loss * ((1.0 - damping) * x + damping * self.last);
        self.last = x;
        let mask = self.line.len() - 1;
        self.line[self.write] = y + input;
        self.write = (self.write + 1) & mask;
        x
    }
}

#[derive(Debug, Clone)]
struct Voice {
    courses: [Course; 2],
    /// Strings sounding, from the model when it was plucked.
    strings: usize,
    pitch: u8,
    sounding: bool,
    /// The note is held, so the bow, if any, keeps going.
    held: bool,
    velocity: f32,
    /// The loop filter's damping.
    damping: f32,
    buzz: f32,
    /// The bow's current level, gliding towards the bow setting while held, to 0 after.
    bow: f32,
    /// The bow's noise, low-passed.
    rosin: f32,
    /// The low-pass's coefficient, and the gain that makes up for what it takes out.
    rosin_coef: f32,
    rosin_gain: f32,
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
    model: usize,
    pluck: f32,
    position: f32,
    decay: f32,
    tone: f32,
    bow: f32,
    noise: Noise,
    /// One period of a pluck, worked out before it goes on the string.
    scratch: Box<[f32]>,
    bow_coef: f32,
    fade_coef: f32,
    plucks: u64,
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
            sounding: false,
            held: false,
            velocity: 0.0,
            damping: 0.5,
            buzz: 0.0,
            bow: 0.0,
            rosin: 0.0,
            rosin_coef: 1.0,
            rosin_gain: 1.0,
            fade: 1.0,
            choked: false,
            started: 0,
            peak: 0.0,
        };
        let mut string = Self {
            sample_rate,
            voices: std::array::from_fn(|_| voice.clone()),
            model: 0,
            pluck: 0.0,
            position: 0.0,
            decay: 1.0,
            tone: 0.0,
            bow: 0.0,
            noise: Noise(0x5eed_1e55),
            // A period, and as much again for the pluck position's notch.
            scratch: vec![0.0; 2 * len].into_boxed_slice(),
            bow_coef: decay_coef(BOW_TIME, sample_rate),
            fade_coef: decay_coef(CHOKE, sample_rate),
            plucks: 0,
            clock: 0,
            dc: [(0.0, 0.0); 2],
            dc_coef: 1.0 - one_pole(10.0, sample_rate),
        };
        for info in InstKind::String.params() {
            let value = values
                .get(info.id as usize)
                .copied()
                .unwrap_or(info.default);
            string.set(info.id, info.clamp(value));
        }
        string
    }

    /// Called with each setting, once at the start and whenever one changes. `value` is in range.
    /// Decay and tone change the strings ringing too; the rest wait for the next pluck.
    fn set(&mut self, id: u32, value: f32) {
        match id {
            MODEL => self.model = (value.round() as usize).min(STRINGS.len() - 1),
            PLUCK => self.pluck = value,
            POSITION => self.position = value,
            DECAY => self.decay = value,
            TONE => self.tone = value,
            BOW => self.bow = value,
            _ => {}
        }
        if matches!(id, DECAY | TONE) {
            for i in 0..VOICES {
                if self.voices[i].sounding {
                    self.tune(i);
                }
            }
        }
    }

    /// The loop's losses and delays for voice `i`'s pitch, from the decay and tone.
    fn tune(&mut self, i: usize) {
        let model = &STRINGS[self.model];
        let damping = model.bright + (0.5 - model.bright) * (1.0 - self.tone);
        let voice = &mut self.voices[i];
        voice.damping = damping;
        // Lost per sample, so every string fades over the decay whatever its period.
        let loss = (-SIXTY_DB / (self.decay * self.sample_rate)).exp();
        let hz = midi_to_hz(voice.pitch).max(LOWEST_HZ);
        voice.rosin_coef = one_pole(hz * BOW_BAND, self.sample_rate);
        // A one-pole low-pass leaves white noise with a/(2 - a) of its power.
        voice.rosin_gain = ((2.0 - voice.rosin_coef) / voice.rosin_coef).sqrt();
        for (c, course) in voice.courses.iter_mut().enumerate() {
            let cents = if c == 0 { 0.0 } else { model.detune };
            course.period = self.sample_rate / (hz * 2f32.powf(cents / 1200.0));
            // The loop filter delays by about `damping` samples; the line makes up the rest.
            course.delay = (course.period - damping).max(2.0);
            course.loss = loss.powf(course.period);
            // A bow feeds in noise at the rate the loop loses energy, so it holds the string at
            // a steady level whatever the decay.
            course.feed = (1.0 - course.loss * course.loss).sqrt();
        }
    }

    fn note_on(&mut self, pitch: u8, velocity: u8) {
        // The string already playing this note, else a silent one, else the oldest.
        let i = (0..VOICES)
            .find(|&i| self.voices[i].sounding && self.voices[i].pitch == pitch)
            .or_else(|| (0..VOICES).find(|&i| !self.voices[i].sounding))
            .unwrap_or_else(|| {
                (0..VOICES)
                    .min_by_key(|&i| self.voices[i].started)
                    .unwrap_or(0)
            });
        let ringing = self.voices[i].sounding && self.voices[i].pitch == pitch;
        let model = &STRINGS[self.model];
        self.plucks += 1;
        let voice = &mut self.voices[i];
        voice.pitch = pitch;
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
        self.tune(i);
        self.pluck(i, if ringing { REPLUCK } else { 0.0 });
    }

    /// Plucks voice `i`: one period of noise, low-passed by how hard the pluck is and notched by
    /// where it lands, goes on each of its strings, over what's left of the string times `keep`.
    /// A bow takes over from the pluck as it's turned up.
    fn pluck(&mut self, i: usize, keep: f32) {
        let model = &STRINGS[self.model];
        let voice = &self.voices[i];
        let level = PLUCK_LEVEL * voice.velocity * (1.0 - self.bow);
        let period = voice.courses[0].period;
        let len = (period.round() as usize).clamp(2, self.scratch.len() / 2);
        let hz = self.sample_rate / period;
        // Harder plucks, and harder-played notes, are brighter.
        let hardness = self.pluck * (0.7 + 0.3 * voice.velocity);
        let top = 0.45 * self.sample_rate;
        let cutoff = hz * 2.0 * (top / (hz * 2.0)).max(1.0).powf(hardness * model.pick);
        let smooth = one_pole(cutoff, self.sample_rate);

        // Plucking a fraction `position` along cancels the harmonics with a node there.
        let gap = ((self.position * period).round() as usize).clamp(1, len - 1);
        let burst = &mut self.scratch[..len + gap];
        let mut y = 0.0;
        // A period's worth first, so the filter has settled by the one that's kept.
        for _ in 0..2 {
            for x in burst[..len].iter_mut() {
                y += smooth * (self.noise.next() - y);
                *x = y;
            }
        }
        // The period, less itself `gap` samples on, going round.
        burst.copy_within(..gap, len);
        for k in 0..len {
            burst[k] = burst[k + gap] - burst[k];
        }
        let burst = &self.scratch[..len];
        let mean = burst.iter().sum::<f32>() / len as f32;
        let rms = (burst.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / len as f32)
            .sqrt()
            .max(1e-9);
        let scale = level / rms;

        let voice = &mut self.voices[i];
        for course in &mut voice.courses[..voice.strings] {
            let mask = course.line.len() - 1;
            // Enough of the line for every read until the pluck has gone round once.
            let span = (course.delay as usize + 4).min(course.line.len());
            for k in 0..span {
                let at = (course.write + mask + 1 - span + k) & mask;
                let x = (self.scratch[k % len] - mean) * scale;
                course.line[at] = course.line[at] * keep + x;
            }
            course.last *= keep;
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
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.clock.is_multiple_of(CONTROL_PERIOD) {
                self.update_control();
            }
            self.clock = self.clock.wrapping_add(1);
            let (mut out_l, mut out_r) = (0.0, 0.0);
            for voice in &mut self.voices {
                if !voice.sounding {
                    continue;
                }
                let target = if voice.held {
                    self.bow * voice.velocity
                } else {
                    0.0
                };
                voice.bow = target + (voice.bow - target) * self.bow_coef;
                let strings = voice.strings;
                let bow = BOW_LEVEL * voice.bow;
                if bow > 0.0 {
                    voice.rosin += voice.rosin_coef * (self.noise.next() - voice.rosin);
                }
                let rosin = bow * voice.rosin * voice.rosin_gain;
                let mut sum = [0.0; 2];
                for (c, course) in voice.courses[..strings].iter_mut().enumerate() {
                    let input = rosin * course.feed;
                    // A loud string hits the bridge and plays shorter.
                    let delay =
                        (course.delay * (1.0 - voice.buzz * course.last.abs().min(1.0))).max(2.0);
                    sum[c] = course.tick(delay, voice.damping, course.loss, input);
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
        segments(events, left.len(), |segment| match segment {
            Segment::Render(range) => self.render(&mut left[range.clone()], &mut right[range]),
            Segment::Event(event) => match event.kind {
                EventKind::NoteOn { pitch, velocity } => self.note_on(pitch, velocity),
                EventKind::NoteOff { pitch } => self.note_off(pitch),
                EventKind::Choke => self.choke(),
                EventKind::Param { id, value } => {
                    if let Some(info) = InstKind::String.params().get(id as usize) {
                        self.set(id, info.clamp(value));
                    }
                }
            },
        });
    }

    fn params(&self) -> &[ParamInfo] {
        InstKind::String.params()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn string(settings: &[(u32, f32)]) -> Monochord {
        let mut values = lattice_core::Instrument::new(InstKind::String).values;
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
        let off = Event {
            offset: 0,
            kind: EventKind::NoteOff { pitch: 57 },
        };
        play(&mut m, &[off], 2.0);
        assert!(m.voices.iter().all(|v| !v.sounding));
    }

    #[test]
    fn every_model_stays_steady_at_its_most_extreme() {
        for model in 0..3 {
            let mut m = string(&[
                (MODEL, model as f32),
                (PLUCK, 1.0),
                (DECAY, 20.0),
                (TONE, 1.0),
                (POSITION, 0.02),
            ]);
            let chord: Vec<Event> = [28, 40, 52, 64, 76, 88, 100, 112, 124]
                .into_iter()
                .map(on)
                .collect();
            let x = play(&mut m, &chord, 4.0);
            let peak = x.iter().fold(0.0f32, |p, x| p.max(x.abs()));
            assert!(peak.is_finite() && peak < 4.0, "model {model}: {peak}");
            assert!(
                rms(&x[168_000..]) <= rms(&x[..24_000]),
                "model {model} grows"
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
