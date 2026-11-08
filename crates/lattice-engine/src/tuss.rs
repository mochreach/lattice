//! Tuss, the electric piano engine, after The Tuss, a name Richard D. James put records out
//! under in 2007, credited to a Cornish brother and sister no one ever met.
//!
//! Each key throws a hammer at a tine, a thin steel rod clamped at one end, which swings in front
//! of a magnetic pickup, as in a Rhodes. The tine is modal: its fundamental, which rings on for
//! seconds, and two of a clamped bar's upper modes, 6.27 and 17.55 times it, out of tune with
//! the note, which clang at the start of the note and fade fast. A soft felt hammer, or a soft
//! note, barely sets them going; a hard one rings them out as a bell. Each mode is a phasor, a
//! point turning round a circle a little smaller each turn, so it can be retuned as it rings
//! without jumping.
//!
//! The pickup hears the tine's swing through its field, which falls off either side of the
//! pickup's pole: `1 / (1 + u²)`, `u` the tine's distance across. A tine in line with the pole is
//! heard twice a swing, hollow, an octave up; one off to the side mostly once, round and full.
//! A tine struck hard swings far into the field's curve, so it barks, the harder the more.
//!
//! The notes can drift out of tune, each its own way, wavering together as a worn tape does, and
//! each sits in stereo the way it's drifted. They can play in other tunings than equal
//! temperament, built on any key.

use lattice_core::InstKind;
use lattice_core::instrument::tuss::{
    BARK, BELL, DAMPER, DECAY, DRIFT, GAIN, KEY, PICKUP, STRIKE, TUNING,
};
use lattice_core::tuning;

use crate::param::ParamInfo;
use crate::processor::{Event, EventKind, Processor, Segment, segments};
use crate::rhythmicon::{Noise, SIXTY_DB, one_pole};

const VOICES: usize = 8;
/// The tine's modes: its fundamental and two of a clamped bar's upper modes.
const MODES: usize = 3;
/// The modes' pitches, as multiples of the note: a bar clamped at one end, free at the other.
const RATIOS: [f32; MODES] = [1.0, 6.267, 17.55];
/// The upper modes' levels against each other, at full `bell` and a hard hammer.
const BELLS: [f32; MODES] = [0.0, 3.0, 1.2];
/// How long the upper modes ring, in seconds to fall by 60 dB, at most: they clang and are gone.
const BELL_RING: [f32; MODES] = [f32::INFINITY, 0.45, 0.09];
/// The note the decay is for, C-3; higher notes ring for less, an octave up three quarters as
/// long.
const DECAY_HZ: f32 = 130.8;
const DECAY_SLOPE: f32 = 0.4;
/// How long the hardest damper takes to stop a tine, in seconds to fall by 60 dB.
const DAMPED: f32 = 0.05;
/// How long the hammer touches the tine, in seconds, felt and hard: the shorter, the more of the
/// upper modes it rings. A hard note shortens it, a soft one lengthens it.
const CONTACT: (f32, f32) = (0.003, 0.0003);
/// How hard the pickup's driven, clean and barking.
const DRIVE: (f32, f32) = (0.5, 4.0);
/// The furthest the pickup sits from the tine's line, as a share of the field's width.
const OFFSET: f32 = 0.6;
/// How far the notes drift apart at the most, each its own way, and how far they waver all
/// together, in cents; how fast they waver, in Hz.
const MAX_DRIFT: f32 = 20.0;
const WOW: f32 = 8.0;
const WOW_HZ: f32 = 0.31;
/// How far a note drifted furthest sits from the middle in stereo, at full drift.
const SPREAD: f32 = 0.8;
/// Samples between retuning the modes as they waver, and checks for tines fallen silent.
const CONTROL_PERIOD: u32 = 32;
/// Below this, a tine has finished and stops rendering.
const SILENCE: f32 = 1e-4;
/// How long a choked tine takes to fade, in seconds to fall by 1/e.
const CHOKE: f32 = 0.003;
/// The output, so a few notes together leave headroom.
const OUT: f32 = 0.7;
/// The lowest pitch a tine plays; lower notes play this.
const LOWEST_HZ: f32 = 20.0;

/// One of the tine's modes: a phasor, turned and shrunk a little each sample.
#[derive(Debug, Clone, Copy, Default)]
struct Mode {
    re: f32,
    im: f32,
    /// The turn and shrink, as `r cos ω` and `r sin ω`.
    cos: f32,
    sin: f32,
}

impl Mode {
    #[inline]
    fn tick(&mut self) -> f32 {
        (self.re, self.im) = (
            self.cos * self.re - self.sin * self.im,
            self.sin * self.re + self.cos * self.im,
        );
        self.im
    }
}

#[derive(Debug, Clone)]
struct Voice {
    modes: [Mode; MODES],
    pitch: u8,
    /// Its pitch in the tuning, before it drifts.
    hz: f32,
    sounding: bool,
    /// The key is down, so the damper is off the tine.
    held: bool,
    /// Which way, and how far, it drifts, as a fraction of the most.
    wander: f32,
    /// Its place in the wavering, so the notes don't all waver in step.
    phase: f32,
    /// Its gains to the left and the right.
    pan: (f32, f32),
    /// Its DC blocker, as `(last input, last output)`: the pickup hears a tine swinging nearer
    /// the pole for longer than one swinging away, so what it hears isn't centred on nothing.
    dc: (f32, f32),
    /// Fades a choked tine out.
    fade: f32,
    choked: bool,
    started: u64,
    /// The loudest output since the last check, for telling when it's fallen silent.
    peak: f32,
}

#[derive(Debug, Clone)]
pub struct Tuss {
    sample_rate: f32,
    voices: [Voice; VOICES],
    strike: f32,
    bell: f32,
    decay: f32,
    damper: f32,
    drift: f32,
    tuning: usize,
    key: usize,
    gain: f32,
    /// The pickup: how hard it's driven, how far it sits off the tine's line, and what brings
    /// its loudest output to 1.
    drive: f32,
    offset: f32,
    norm: f32,
    /// Where the wavering has got to, in cycles.
    wow: f32,
    noise: Noise,
    fade_coef: f32,
    strikes: u64,
    clock: u32,
    dc_coef: f32,
}

/// The pickup's field, `u` across from its pole.
#[inline]
fn field(u: f32) -> f32 {
    1.0 / (1.0 + u * u)
}

impl Tuss {
    /// Starts at a song's electric piano settings, one value per parameter, by id.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let voice = Voice {
            modes: [Mode::default(); MODES],
            pitch: 0,
            hz: LOWEST_HZ,
            sounding: false,
            held: false,
            wander: 0.0,
            phase: 0.0,
            pan: (1.0, 1.0),
            dc: (0.0, 0.0),
            fade: 1.0,
            choked: false,
            started: 0,
            peak: 0.0,
        };
        let mut tuss = Self {
            sample_rate,
            voices: std::array::from_fn(|_| voice.clone()),
            strike: 0.0,
            bell: 0.0,
            decay: 1.0,
            damper: 0.0,
            drift: 0.0,
            tuning: 0,
            key: 0,
            gain: 0.0,
            drive: DRIVE.0,
            offset: 0.0,
            norm: 1.0,
            wow: 0.0,
            noise: Noise(0x7055_c0de),
            fade_coef: crate::dsp::decay_coef(CHOKE, sample_rate),
            strikes: 0,
            clock: 0,
            dc_coef: 1.0 - one_pole(10.0, sample_rate),
        };
        for info in InstKind::Tuss.params() {
            let value = values
                .get(info.id as usize)
                .copied()
                .unwrap_or(info.default);
            tuss.set(info.id, info.clamp(value));
        }
        tuss
    }

    /// Called with each setting, once at the start and whenever one changes. `value` is in range.
    /// The hammer and bell wait for the next note; the rest change the tines already ringing.
    fn set(&mut self, id: u32, value: f32) {
        match id {
            STRIKE => self.strike = value,
            BELL => self.bell = value,
            PICKUP => self.offset = OFFSET * value,
            BARK => self.drive = DRIVE.0 * (DRIVE.1 / DRIVE.0).powf(value),
            DECAY => self.decay = value,
            DAMPER => self.damper = value,
            DRIFT => self.drift = value,
            TUNING => self.tuning = value.round() as usize,
            KEY => self.key = value.round() as usize,
            GAIN => self.gain = value,
            _ => {}
        }
        if matches!(id, PICKUP | BARK) {
            self.norm = self.loudest();
        }
        if matches!(id, TUNING | KEY) {
            for voice in &mut self.voices {
                voice.hz = Self::pitch_hz(voice.pitch, self.tuning, self.key);
            }
        }
        if matches!(id, DECAY | DAMPER | DRIFT | TUNING | KEY) {
            self.retune();
        }
    }

    /// The pickup's loudest output, from a tine swinging as far as a hard note swings it: what's
    /// heard is how far the field's moved from where it is at rest. The field's strongest on the
    /// pole's line, so swinging across it counts too.
    fn loudest(&self) -> f32 {
        let rest = field(self.offset);
        let ends = [self.offset + self.drive, self.offset - self.drive];
        let mut most = ends
            .iter()
            .map(|&u| (rest - field(u)).abs())
            .fold(0.0, f32::max);
        if self.offset < self.drive {
            most = most.max(1.0 - rest);
        }
        most.max(1e-6)
    }

    /// A note's pitch in a tuning, in Hz.
    fn pitch_hz(pitch: u8, tuning: usize, key: usize) -> f32 {
        let cents = tuning::offset(pitch, tuning, key);
        440.0 * 2f32.powf((f32::from(pitch) - 69.0 + cents / 100.0) / 12.0)
    }

    /// Sets voice `i`'s modes' pitches and losses, as it's drifted and wavered, held or damped.
    fn tune(&mut self, i: usize) {
        let rate = self.sample_rate;
        let (decay, damper, drift, wow) = (self.decay, self.damper, self.drift, self.wow);
        let Some(voice) = self.voices.get_mut(i) else {
            return;
        };
        let waver = (std::f32::consts::TAU * (wow + voice.phase)).sin();
        let cents = drift * (MAX_DRIFT * voice.wander + WOW * waver);
        let hz = (voice.hz * 2f32.powf(cents / 1200.0)).max(LOWEST_HZ);
        let ring = decay * (DECAY_HZ / hz).powf(DECAY_SLOPE).min(1.0);
        let damped = decay * (DAMPED / decay).powf(damper);
        for ((mode, ratio), bell) in voice.modes.iter_mut().zip(RATIOS).zip(BELL_RING) {
            let mut time = ring.min(bell);
            if !voice.held {
                time = time.min(damped);
            }
            let r = (-SIXTY_DB / (time * rate)).exp();
            let w = std::f32::consts::TAU * hz * ratio / rate;
            (mode.cos, mode.sin) = (r * w.cos(), r * w.sin());
        }
    }

    /// Retunes the tines ringing, after a setting's changed or as they waver.
    fn retune(&mut self) {
        for i in 0..VOICES {
            if self.voices.get(i).is_some_and(|v| v.sounding) {
                self.tune(i);
            }
        }
    }

    fn note_on(&mut self, pitch: u8, velocity: u8) {
        // The tine already ringing for this note, else a silent one, else the oldest. The first
        // of equals wins.
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
        if !restruck {
            voice.modes = [Mode::default(); MODES];
            voice.dc = (0.0, 0.0);
            voice.wander = self.noise.next();
            voice.phase = (self.noise.next() + 1.0) * 0.5;
        }
        voice.pitch = pitch;
        voice.hz = Self::pitch_hz(pitch, self.tuning, self.key);
        // Equal power, so a note's as loud wherever it sits.
        let angle = (SPREAD * self.drift * voice.wander + 1.0) * std::f32::consts::FRAC_PI_4;
        voice.pan = (angle.cos(), angle.sin());
        voice.sounding = true;
        voice.held = true;
        voice.choked = false;
        voice.fade = 1.0;
        voice.started = self.strikes;
        // Not silent before it's had a chance to sound.
        voice.peak = 1.0;

        // The hammer: a hard note swings the tine further, and touches it for less time, so it
        // rings more of the upper modes.
        let v = f32::from(velocity) / 127.0;
        let level = v.powf(1.5);
        let contact = CONTACT.0 * (CONTACT.1 / CONTACT.0).powf(self.strike) * (1.6 - v);
        let top = 0.45 * self.sample_rate;
        // How much of a mode at `hz` a hammer touching for `contact` rings, against the
        // fundamental's.
        let fundamental = 1.0 + (std::f32::consts::PI * voice.hz * contact).powi(2);
        for ((mode, ratio), bell) in voice.modes.iter_mut().zip(RATIOS).zip(BELLS) {
            let hz = voice.hz * ratio;
            if hz > top {
                continue;
            }
            let reach = fundamental / (1.0 + (std::f32::consts::PI * hz * contact).powi(2));
            let share = if ratio == 1.0 {
                1.0
            } else {
                bell * self.bell * reach
            };
            mode.re += level * share;
        }
        self.tune(i);
    }

    /// The key comes up: the damper falls on the tine.
    fn note_off(&mut self, pitch: u8) {
        for i in 0..VOICES {
            let Some(voice) = self.voices.get_mut(i) else {
                continue;
            };
            if voice.pitch != pitch || !voice.held || !voice.sounding {
                continue;
            }
            voice.held = false;
            self.tune(i);
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

    /// Every `CONTROL_PERIOD` samples: the notes waver, and tines that have fallen silent stop.
    fn update_control(&mut self) {
        self.wow = (self.wow + WOW_HZ * CONTROL_PERIOD as f32 / self.sample_rate).fract();
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
        if self.voices.iter().all(|v| !v.sounding) {
            left.fill(0.0);
            right.fill(0.0);
            return;
        }
        let (rest, drive, offset) = (field(self.offset), self.drive, self.offset);
        let scale = OUT * self.gain / self.norm;
        let dc_coef = self.dc_coef;
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
                let swing: f32 = voice.modes.iter_mut().map(Mode::tick).sum();
                let heard = rest - field(offset + drive * swing);
                let (x1, y1) = voice.dc;
                let centred = heard - x1 + dc_coef * y1;
                voice.dc = (heard, centred);
                voice.peak = voice.peak.max(swing.abs());
                let x = centred * voice.fade;
                if voice.choked {
                    voice.fade *= self.fade_coef;
                }
                out_l += x * voice.pan.0;
                out_r += x * voice.pan.1;
            }
            (*l, *r) = (out_l * scale, out_r * scale);
        }
    }
}

impl Processor for Tuss {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.render(l, r),
            Segment::Event(event) => match event.kind {
                EventKind::NoteOn { pitch, velocity } => self.note_on(pitch, velocity),
                EventKind::NoteOff { pitch } => self.note_off(pitch),
                EventKind::Choke => self.choke(),
                // A tine can't glide.
                EventKind::Shape(_) | EventKind::Program(_) => {}
                EventKind::Param { id, value } => {
                    if let Some(info) = InstKind::Tuss.params().get(id as usize) {
                        self.set(id, info.clamp(value));
                    }
                }
            },
        });
    }

    fn params(&self) -> &[ParamInfo] {
        InstKind::Tuss.params()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn piano(settings: &[(u32, f32)]) -> Tuss {
        let mut values = lattice_core::Instrument::new(InstKind::Tuss).values;
        for &(id, value) in settings {
            values[id as usize] = value;
        }
        Tuss::with_values(RATE, &values)
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

    /// `seconds` of output after `events`, both sides.
    fn play(t: &mut Tuss, events: &[Event], seconds: f32) -> (Vec<f32>, Vec<f32>) {
        let len = (seconds * RATE) as usize;
        let (mut l, mut r) = (vec![0.0; len], vec![0.0; len]);
        t.process(events, &mut l, &mut r);
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

    fn cents(heard: f32, hz: f32) -> f32 {
        1200.0 * (heard / hz).log2()
    }

    #[test]
    fn plays_in_tune_and_rings_while_held() {
        for note in [36, 57, 81] {
            let mut t = piano(&[(DRIFT, 0.0)]);
            let (l, _) = play(&mut t, &[on(note, 100)], 1.0);
            let hz = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
            let heard = pitch(&l[9_600..33_600], hz);
            assert!(cents(heard, hz).abs() < 3.0, "{note}: {heard} Hz, not {hz}");
            assert!(rms(&l[43_200..]) > 0.003, "{note} still rings");
        }
    }

    #[test]
    fn plays_in_other_tunings_on_its_key() {
        // Just intonation on A: its major third, C♯, is a pure 5/4, 14 cents under equal
        // temperament's, and A itself stays at 440.
        let heard = |note: u8, tuning: f32| {
            let mut t = piano(&[(DRIFT, 0.0), (TUNING, tuning), (KEY, 9.0)]);
            let (l, _) = play(&mut t, &[on(note, 100)], 0.6);
            let hz = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
            cents(pitch(&l[9_600..], hz), hz)
        };
        assert!(heard(69, 1.0).abs() < 2.0);
        assert!((heard(73, 1.0) + 13.7).abs() < 2.0, "{}", heard(73, 1.0));
        assert!(heard(73, 0.0).abs() < 2.0);
    }

    #[test]
    fn silent_until_played_and_the_damper_stops_it() {
        let mut t = piano(&[(DAMPER, 1.0)]);
        assert!(play(&mut t, &[], 0.1).0.iter().all(|&x| x == 0.0));
        let (l, _) = play(&mut t, &[on(60, 100)], 0.3);
        assert!(rms(&l) > 0.01);
        let (l, _) = play(&mut t, &[off(60)], 0.3);
        assert!(rms(&l[9_600..]) < 1e-3, "{}", rms(&l[9_600..]));
        play(&mut t, &[], 1.0);
        assert!(play(&mut t, &[], 0.1).0.iter().all(|&x| x == 0.0));

        // With the damper off, it rings on after the key's up.
        let mut t = piano(&[(DAMPER, 0.0)]);
        play(&mut t, &[on(60, 100)], 0.3);
        let (l, _) = play(&mut t, &[off(60)], 0.3);
        assert!(rms(&l[9_600..]) > 0.01);
    }

    #[test]
    fn a_hard_hammer_rings_the_bell() {
        let bell = |strike: f32, bell: f32, velocity: u8| {
            let mut t = piano(&[(STRIKE, strike), (BELL, bell), (DRIFT, 0.0), (BARK, 0.0)]);
            let (l, _) = play(&mut t, &[on(57, velocity)], 0.1);
            strength(&l[..2_400], 220.0 * 6.267) / strength(&l[..2_400], 220.0)
        };
        assert!(bell(1.0, 1.0, 127) > 5.0 * bell(0.0, 1.0, 127));
        assert!(
            bell(0.5, 1.0, 127) > 2.0 * bell(0.5, 1.0, 40),
            "harder notes ring it more"
        );
        assert!(bell(1.0, 0.0, 127) < 0.01, "no bell, no clang");
        // And it's gone soon after.
        let mut t = piano(&[(STRIKE, 1.0), (BELL, 1.0), (DRIFT, 0.0)]);
        let (l, _) = play(&mut t, &[on(57, 127)], 1.5);
        let late = &l[60_000..];
        assert!(strength(late, 220.0 * 6.267) < 0.01 * strength(late, 220.0));
    }

    #[test]
    fn the_pickup_in_line_hears_an_octave_and_hard_notes_bark() {
        let harmonics = |pickup: f32, bark: f32, velocity: u8| {
            let mut t = piano(&[(PICKUP, pickup), (BARK, bark), (BELL, 0.0), (DRIFT, 0.0)]);
            let (l, _) = play(&mut t, &[on(45, velocity)], 0.5);
            let x = &l[4_800..];
            (strength(x, 110.0), strength(x, 220.0), strength(x, 330.0))
        };
        let (one, two, _) = harmonics(0.0, 0.3, 100);
        assert!(two > 10.0 * one, "in line, it's an octave up: {one} {two}");
        let (one, two, _) = harmonics(1.0, 0.3, 100);
        assert!(one > two, "off to the side, the note: {one} {two}");
        let growl = |velocity| {
            let (one, two, three) = harmonics(1.0, 1.0, velocity);
            (two + three) / one
        };
        assert!(growl(127) > 2.0 * growl(30), "{} {}", growl(127), growl(30));
    }

    #[test]
    fn drift_detunes_the_notes_and_spreads_them() {
        let notes = |drift: f32| {
            let mut t = piano(&[(DRIFT, drift)]);
            let chord: Vec<Event> = [57, 61, 64, 68].into_iter().map(|n| on(n, 100)).collect();
            let (l, r) = play(&mut t, &chord, 1.0);
            let side: Vec<f32> = l.iter().zip(&r).map(|(l, r)| l - r).collect();
            rms(&side[24_000..]) / rms(&l[24_000..])
        };
        assert!(notes(0.0) < 1e-3, "in tune, they're in the middle");
        assert!(notes(1.0) > 0.05, "{}", notes(1.0));
    }

    #[test]
    fn stays_steady_at_its_most_extreme() {
        let mut t = piano(&[
            (STRIKE, 1.0),
            (BELL, 1.0),
            (PICKUP, 0.0),
            (BARK, 1.0),
            (DECAY, 20.0),
            (DAMPER, 0.0),
            (DRIFT, 1.0),
            (TUNING, 5.0),
            (GAIN, 1.0),
        ]);
        let chord: Vec<Event> = [21, 36, 48, 60, 72, 84, 96, 108]
            .into_iter()
            .map(|n| on(n, 127))
            .collect();
        let (l, r) = play(&mut t, &chord, 4.0);
        let peak = l.iter().chain(&r).fold(0.0f32, |p, x| p.max(x.abs()));
        // Eight notes as loud as they come, so up to eight times one.
        assert!(peak.is_finite() && peak < 8.0 * OUT, "{peak}");
        assert!(rms(&l[96_000..]) > 1e-3, "it rings on");
        // Choked, it stops.
        let choke = Event {
            offset: 0,
            kind: EventKind::Choke,
        };
        let (l, _) = play(&mut t, &[choke], 0.1);
        assert!(rms(&l[2_400..]) < 1e-4);
    }
}
