//! Mosaic, the bowed engine: a bowed string on a body of resonances that can be any material at
//! any size. It's named after IRCAM's modal synthesis program, first called MOSAIC and now
//! Modalys, which was made to build instruments that can't exist.
//!
//! Each voice is a waveguide string bowed by friction, as Monochord's is, but played as a
//! violinist plays it. A note starts with a bite of rosin as fast or slow as `bite` says. A note
//! that starts just as another ends is slurred: the bow stays on and the finger moves, gliding
//! for `glide`, so a legato line is one bow stroke. The same note again changes bow. Vibrato
//! comes in a moment into a note, as a player's does, so fast passages stay clean.
//!
//! The strings all sound through one body, shared by every voice, as a violin's four strings
//! share theirs. The body is 24 resonant modes, each a two-pole filter with its own pitch,
//! level, ring and place in the stereo field. `material` slides the modes between four sets: a
//! violin's measured resonances, a glass bowl's sparse pairs that beat, a metal plate's
//! out-of-tune partials, and the formants of a sung vowel. `size` scales them all. Behind it
//! all, five sympathetic strings tuned to the key's root and fifth ring when the body plays
//! their notes, as a viola d'amore's do.

use lattice_core::InstKind;
use lattice_core::instrument::mosaic::{
    BITE, BOW, GAIN, GLIDE, HALO, KEY, MATERIAL, RING, SIZE, VIBRATO,
};

use crate::dsp::decay_coef;
use crate::param::ParamInfo;
use crate::processor::{Event, EventKind, Processor, Segment, Shape, segments};
use crate::rhythmicon::{Noise, SIXTY_DB, one_pole};
use crate::waveguide::{self, Course, Line};
use wide::f32x8;

const VOICES: usize = 8;
/// The lowest pitch a string plays; lower notes play this.
const LOWEST_HZ: f32 = 20.0;
/// Samples between checks for strings that have fallen silent.
const CONTROL_PERIOD: u32 = 64;
/// Samples between steps of glides and vibrato, often enough not to be heard as steps.
const GLIDE_PERIOD: u32 = 16;
/// Below this, a string, or the body's ringing, has finished.
const SILENCE: f32 = 1e-4;
/// How long a choked string takes to fade, in seconds to fall by 1/e.
const CHOKE: f32 = 0.003;
/// How long the bow takes to lift, in seconds to fall by 1/e.
const LIFT_TIME: f32 = 0.04;
/// How long the bow takes to bite at the slowest and fastest `bite`, in seconds to rise by 1/e.
const BITE_TIME: (f32, f32) = (0.25, 0.003);
/// The bow's speed at the lightest bow and at the heaviest, and how hard it grips for that
/// speed, as the friction curve's slope times the speed: low grips harder, scratching, high
/// skates, breathing.
const BOW_SPEED: (f32, f32) = (0.04, 0.35);
const BOW_HOLD: (f32, f32) = (0.35, 0.15);

/// How much the bow's speed wavers, as a hand's does.
const BOW_WAVER: f32 = 0.05;
/// How much of the bow's speed is left, at the most, when it changes direction on a repeated
/// note, and how long the change takes: the bow slows into it and away again, as a jump in
/// speed would click.
const REBOW: f32 = 0.15;
const TURN_TIME: f32 = 0.004;
const TURN_HOLD: f32 = 0.012;
/// The breath of a bow skating lightly, through the body.
const AIR: f32 = 0.04;
/// The scratch of rosin as the bow bites, at the hardest bite, how long it lasts, and where its
/// noise is loudest, in Hz; and how much of it there is when the bow changes direction.
const SCRATCH_LEVEL: f32 = 0.3;
const SCRATCH_HZ: f32 = 3_000.0;
const SCRATCH_TURN: f32 = 0.4;
const SCRATCH_TIME: f32 = 0.012;
/// A string bowed at a steady speed takes a while to settle into its motion, so a hard bite
/// starts faster, as a player's bow does: this much faster, at the hardest, dying away over
/// this long, in seconds to fall by 1/e.
const ACCENT: f32 = 3.0;
const ACCENT_TIME: f32 = 0.03;
/// A hard bite starts the string moving, so the bow speaks at once.
const KICK_LEVEL: f32 = 0.25;
/// Where the bowed sound starts to fall away, 6 dB an octave, as a multiple of the note, and
/// the level that makes up for it, as in Monochord.
const BOW_TILT: f32 = 1.5;
const BOW_LIFT: f32 = 2.0;
/// What's left of a string's wave each time round, the same for every note, so low strings
/// ring longer, as on a violin, and each note's as easy to bow; and its bridge's damping.
const STRING_LOSS: f32 = 0.97;
const DAMPING: f32 = 0.3;
/// Vibrato: how long into a note it starts, and how long it takes to come in fully.
const VIBRATO_DELAY: f32 = 0.12;
const VIBRATO_RISE: f32 = 0.4;
/// The dry string, mixed with the body.
const DRY: f32 = 0.25;
/// The output's level, so a few strings together leave headroom.
const LEVEL: f32 = 0.7;

/// Ring times, in seconds to fall 60 dB, are `Q * RING_Q / Hz`.
const RING_Q: f32 = SIXTY_DB / std::f32::consts::PI;
/// The longest ring a mode may have, so none rings forever.
const LONGEST_RING: f32 = 60.0;

const MODES: usize = 24;
/// The modes, in lanes of eight.
const LANES: usize = MODES / 8;

/// A body's modes, as (pitch in Hz, level, Q).
type Material = [(f32, f32, f32); MODES];

/// The four materials, in `material`'s order: each mode slides to the same mode of the next.
const MATERIALS: [Material; 4] = [
    // Wood: a violin's, measured: the air in it (A0), the plates' first bending (B1- and B1+),
    // the dense modes of the middle, and the bridge's hill, which makes a violin bright.
    [
        (275.0, 0.55, 18.0),
        (405.0, 0.25, 25.0),
        (460.0, 0.85, 30.0),
        (530.0, 1.00, 30.0),
        (610.0, 0.50, 30.0),
        (700.0, 0.55, 30.0),
        (780.0, 0.45, 32.0),
        (860.0, 0.50, 32.0),
        (950.0, 0.40, 34.0),
        (1040.0, 0.45, 34.0),
        (1150.0, 0.35, 34.0),
        (1280.0, 0.30, 32.0),
        (1420.0, 0.30, 30.0),
        (1600.0, 0.32, 28.0),
        (1800.0, 0.35, 26.0),
        (2050.0, 0.45, 24.0),
        (2350.0, 0.55, 22.0),
        (2650.0, 0.55, 22.0),
        (2950.0, 0.45, 22.0),
        (3350.0, 0.30, 22.0),
        (3850.0, 0.20, 22.0),
        (4400.0, 0.14, 22.0),
        (5100.0, 0.09, 22.0),
        (6000.0, 0.06, 22.0),
    ],
    // Glass: a bowl's modes, each a pair a few cents apart that beat, as a wine glass's do when
    // it's not quite round, and a few of the stem's between.
    [
        (262.0, 0.50, 700.0),
        (430.0, 1.00, 900.0),
        (431.0, 0.80, 900.0),
        (701.0, 0.45, 700.0),
        (1032.0, 0.80, 860.0),
        (1035.0, 0.64, 860.0),
        (1363.0, 0.35, 700.0),
        (1892.0, 0.60, 820.0),
        (1898.0, 0.48, 820.0),
        (2408.0, 0.30, 700.0),
        (2967.0, 0.45, 780.0),
        (2976.0, 0.36, 780.0),
        (3526.0, 0.20, 700.0),
        (4257.0, 0.32, 740.0),
        (4270.0, 0.26, 740.0),
        (5031.0, 0.15, 700.0),
        (5762.0, 0.22, 700.0),
        (5780.0, 0.18, 700.0),
        (6708.0, 0.10, 700.0),
        (7482.0, 0.15, 660.0),
        (7505.0, 0.12, 660.0),
        (8514.0, 0.08, 700.0),
        (9417.0, 0.10, 620.0),
        (9446.0, 0.08, 620.0),
    ],
    // Metal: a plate's partials, crowding and stretching out of tune as they rise, as a gong's.
    [
        (101.0, 0.60, 500.0),
        (180.0, 1.00, 700.0),
        (265.0, 0.85, 650.0),
        (376.0, 0.90, 700.0),
        (461.0, 0.70, 600.0),
        (497.0, 0.75, 650.0),
        (565.0, 0.60, 600.0),
        (668.0, 0.65, 600.0),
        (763.0, 0.50, 550.0),
        (842.0, 0.55, 550.0),
        (977.0, 0.45, 500.0),
        (1102.0, 0.42, 500.0),
        (1197.0, 0.38, 480.0),
        (1332.0, 0.35, 450.0),
        (1494.0, 0.30, 420.0),
        (1638.0, 0.28, 400.0),
        (1836.0, 0.24, 380.0),
        (2070.0, 0.20, 350.0),
        (2322.0, 0.18, 320.0),
        (2556.0, 0.15, 300.0),
        (2862.0, 0.12, 280.0),
        (3186.0, 0.10, 260.0),
        (3528.0, 0.08, 240.0),
        (3978.0, 0.06, 220.0),
    ],
    // A voice: a singer's chest, then the five formants of a sung "ah", each a few broad modes
    // across its width.
    [
        (300.0, 0.45, 4.0),
        (450.0, 0.40, 6.0),
        (687.0, 0.65, 12.0),
        (718.0, 0.91, 12.0),
        (750.0, 1.00, 12.0),
        (782.0, 0.91, 12.0),
        (813.0, 0.65, 12.0),
        (1073.0, 0.39, 15.0),
        (1112.0, 0.55, 15.0),
        (1150.0, 0.60, 15.0),
        (1188.0, 0.55, 15.0),
        (1227.0, 0.39, 15.0),
        (2488.0, 0.29, 23.0),
        (2544.0, 0.41, 23.0),
        (2600.0, 0.45, 23.0),
        (2656.0, 0.41, 23.0),
        (2712.0, 0.29, 23.0),
        (3260.0, 0.16, 24.0),
        (3330.0, 0.23, 24.0),
        (3400.0, 0.25, 24.0),
        (3470.0, 0.23, 24.0),
        (3540.0, 0.16, 24.0),
        (4100.0, 0.12, 24.0),
        (4300.0, 0.12, 24.0),
    ],
];

/// How loud each material's body is, so they all sound about as loud as each other.
const LOUDNESS: [f32; 4] = [1.0, 2.9, 3.7, 0.5];

/// The sympathetic strings, in semitones from the key's root above middle C, and how far each
/// sits to the right, from -1 to 1.
const HALO_STRINGS: [(f32, f32); 5] = [
    (-12.0, 0.0),
    (0.0, -0.6),
    (7.0, 0.6),
    (12.0, -0.3),
    (19.0, 0.3),
];
/// How long the sympathetic strings ring, in seconds to fall 60 dB.
const HALO_RING: f32 = 4.0;
/// The sympathetic strings' damping, as the string's, and the delay it adds, in samples.
const HALO_DAMPING: f32 = 0.3;
/// How loud the sympathetic strings are. Each is driven by the body in proportion to what its
/// loop loses, so held at its own pitch it rings twice as loud as the body at most, whatever its
/// pitch: driven by a fixed amount, a high string's many more trips round its loop a second made
/// it ring far louder than a low one, and than the violin itself.
const HALO_LEVEL: f32 = 2.0;

#[derive(Debug, Clone)]
struct Voice {
    course: Course,
    pitch: u8,
    /// The pitch it's tuned to, gliding towards the note's.
    hz: f32,
    /// Gliding: `GLIDE_PERIOD`s to go, and the step in pitch each takes.
    glide_left: u32,
    glide_ratio: f32,
    sounding: bool,
    held: bool,
    /// Let go at this instant, so a note starting now is slurred onto it.
    released: bool,
    velocity: f32,
    /// How fast the bow's moving, as a fraction of the note's full speed, chasing the velocity
    /// while the note's held and 0 after.
    bow: f32,
    /// How fast it bites, as a one-pole's coefficient.
    bite_coef: f32,
    /// The extra speed the bow starts a note with, as a multiple of its own, dying away.
    accent: f32,
    /// Changing direction: how far into the change the bow is, from 0 to 1, chasing `turning`,
    /// which jumps to 1 and dies away.
    turn: f32,
    turning: f32,
    /// The bow's scratch as it bites: where it's got to, and its noise, a band around
    /// `SCRATCH_HZ`: a low-pass, the difference of it from the last, and a low-pass of that.
    scratch: f32,
    scratch_band: [f32; 3],
    /// The bowed sound's tilt: a low-pass, and its coefficient.
    tilt: f32,
    tilt_coef: f32,
    /// Vibrato: `GLIDE_PERIOD`s since the note started, how far round it is, and its wander.
    age: u32,
    phase: f32,
    drift: f32,
    fade: f32,
    choked: bool,
    started: u64,
    peak: f32,
}

/// Eight of the body's modes, side by side: each a two-pole resonator, `y = b x + a1 y1 +
/// a2 y2`, and where it sits in the stereo field.
#[derive(Debug, Clone, Copy, Default)]
struct Modes {
    b: f32x8,
    a1: f32x8,
    a2: f32x8,
    y1: f32x8,
    y2: f32x8,
    left: f32x8,
    right: f32x8,
}

/// A sympathetic string: a comb filter, tuned, with a gentle low-pass in its loop.
#[derive(Debug, Clone)]
struct Sympathetic {
    line: Line,
    delay: f32,
    feedback: f32,
    drive: f32,
    last: f32,
    left: f32,
    right: f32,
}

#[derive(Debug, Clone)]
pub struct Mosaic {
    sample_rate: f32,
    voices: [Voice; VOICES],
    modes: [Modes; LANES],
    halo_strings: [Sympathetic; 5],
    material: f32,
    size: f32,
    bow: f32,
    bite: f32,
    vibrato: f32,
    halo: f32,
    key: f32,
    glide: f32,
    ring: f32,
    gain: f32,
    /// How loud the body is, from the material.
    loudness: f32,
    noise: Noise,
    /// One period of the kick a hard bite gives the string.
    kick: Box<[f32]>,
    lift_coef: f32,
    accent_coef: f32,
    turn_coef: f32,
    turning_coef: f32,
    scratch_coef: f32,
    scratch_smooth: f32,
    fade_coef: f32,
    notes: u64,
    /// The pitch last played, which a gesture's glide starts from.
    last: Option<u8>,
    shape: Shape,
    clock: u32,
    /// The body's and the sympathetic strings' loudest since the last check, so they ring out
    /// after the strings have stopped, and then stop too.
    ringing: f32,
    tail: bool,
    dc: [(f32, f32); 2],
    dc_coef: f32,
}

impl Mosaic {
    /// Starts at a song's settings, one value per parameter, by id. Allocates the delay lines,
    /// so call it off the audio thread.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let len = ((sample_rate / LOWEST_HZ) as usize + 8).next_power_of_two();
        let voice = Voice {
            course: Course::new(len),
            pitch: 0,
            hz: LOWEST_HZ,
            glide_left: 0,
            glide_ratio: 1.0,
            sounding: false,
            held: false,
            released: false,
            velocity: 0.0,
            bow: 0.0,
            bite_coef: 0.0,
            accent: 0.0,
            turn: 0.0,
            turning: 0.0,
            scratch: 0.0,
            scratch_band: [0.0; 3],
            tilt: 0.0,
            tilt_coef: 1.0,
            age: 0,
            phase: 0.0,
            drift: 0.0,
            fade: 1.0,
            choked: false,
            started: 0,
            peak: 0.0,
        };
        // The lowest sympathetic string is an octave below the highest key's root, C# to B.
        let halo_len = ((sample_rate / 120.0) as usize + 8).next_power_of_two();
        let sympathetic = |&(_, pan): &(f32, f32)| {
            let (left, right) = pan_gains(pan);
            Sympathetic {
                line: Line::new(halo_len),
                delay: 100.0,
                feedback: 0.0,
                drive: 0.0,
                last: 0.0,
                left,
                right,
            }
        };
        let mut modes = [Modes::default(); LANES];
        for (lane, modes) in modes.iter_mut().enumerate() {
            // Spread about the field, each a good way from the last, the lowest near the middle.
            let pan = |k: usize| {
                let i = (lane * 8 + k) as f32;
                pan_gains((i * 2.4).sin() * (0.2 + 0.6 * i / MODES as f32))
            };
            modes.left = f32x8::from(std::array::from_fn::<f32, 8, _>(|k| pan(k).0));
            modes.right = f32x8::from(std::array::from_fn::<f32, 8, _>(|k| pan(k).1));
        }
        let mut mosaic = Self {
            sample_rate,
            voices: std::array::from_fn(|_| voice.clone()),
            modes,
            halo_strings: HALO_STRINGS.each_ref().map(sympathetic),
            material: 0.0,
            size: 1.0,
            bow: 0.5,
            bite: 0.5,
            vibrato: 0.0,
            halo: 0.0,
            key: 0.0,
            glide: 0.0,
            ring: 0.3,
            gain: 0.5,
            loudness: 1.0,
            noise: Noise(0x5eed_ba55),
            kick: vec![0.0; 2 * len].into_boxed_slice(),
            lift_coef: decay_coef(LIFT_TIME, sample_rate),
            accent_coef: decay_coef(ACCENT_TIME, sample_rate),
            turn_coef: 1.0 - decay_coef(TURN_TIME, sample_rate),
            turning_coef: decay_coef(TURN_HOLD, sample_rate),
            scratch_coef: decay_coef(SCRATCH_TIME, sample_rate),
            scratch_smooth: one_pole(SCRATCH_HZ, sample_rate),
            fade_coef: decay_coef(CHOKE, sample_rate),
            notes: 0,
            last: None,
            shape: Shape::default(),
            clock: 0,
            ringing: 0.0,
            tail: false,
            dc: [(0.0, 0.0); 2],
            dc_coef: 1.0 - one_pole(10.0, sample_rate),
        };
        for info in InstKind::Mosaic.params() {
            let value = values
                .get(info.id as usize)
                .copied()
                .unwrap_or(info.default);
            mosaic.set(info.id, info.clamp(value));
        }
        mosaic.shape_body();
        mosaic.tune_halo();
        mosaic
    }

    /// Called with each setting, once at the start and whenever one changes. `value` is in range.
    fn set(&mut self, id: u32, value: f32) {
        match id {
            MATERIAL => self.material = value,
            SIZE => self.size = value,
            BOW => self.bow = value,
            BITE => self.bite = value,
            VIBRATO => self.vibrato = value,
            HALO => self.halo = value,
            KEY => self.key = value.round(),
            GLIDE => self.glide = value,
            RING => self.ring = value,
            GAIN => self.gain = value,
            _ => {}
        }
        match id {
            MATERIAL | SIZE | RING => self.shape_body(),
            KEY => self.tune_halo(),
            _ => {}
        }
    }

    /// The body's modes, from the material, size and ring. A body that's playing changes as it
    /// plays, so sliding the material morphs one into the other.
    fn shape_body(&mut self) {
        // `material` is in range, so both are materials; the last's next is itself.
        let from = (self.material.floor() as usize).min(MATERIALS.len() - 1);
        let to = (from + 1).min(MATERIALS.len() - 1);
        let t = self.material - from as f32;
        let (Some(a), Some(b)) = (MATERIALS.get(from), MATERIALS.get(to)) else {
            return;
        };
        let loud = |i: usize| LOUDNESS.get(i).copied().unwrap_or(1.0);
        self.loudness = loud(from) + (loud(to) - loud(from)) * t;
        // A bigger body is lower and rings longer, but only as the square root of its size.
        let longer = 0.25 * 100f32.powf(self.ring);
        let ring = longer * self.size.sqrt();
        // A mode rung shorter passes a wider band, and so more of the string, which this makes
        // up for. One rung longer passes less, but is made only a little louder, or a harmonic
        // landing on it would build up, over its long ring, to far louder than the rest.
        let makeup = if longer < 1.0 {
            longer.powf(0.8)
        } else {
            longer.powf(0.25)
        };
        let nyquist = 0.45 * self.sample_rate;
        let mut coefs = [(0.0, 0.0, 0.0); MODES];
        for ((mode, &(fa, ga, qa)), &(fb, gb, qb)) in coefs.iter_mut().zip(a).zip(b) {
            // Pitch and Q slide evenly in log, level in line.
            let hz = fa * (fb / fa).powf(t) / self.size;
            let q = qa * (qb / qa).powf(t);
            let level = ga + (gb - ga) * t;
            let seconds = (q * RING_Q / hz * ring).min(LONGEST_RING);
            // Silent past the top, or below the lowest a string plays.
            if !(LOWEST_HZ..nyquist).contains(&hz) {
                continue;
            }
            let theta = std::f32::consts::TAU * hz / self.sample_rate;
            let r = (-SIXTY_DB / (seconds * self.sample_rate)).exp();
            // A peak of the mode's level at its pitch, made a little louder the narrower it
            // is, so a glass's few sharp modes are heard as well as wood's many broad ones.
            let b = level * (1.0 - r) * 2.0 * theta.sin() * (q / 20.0).sqrt().max(1.0) * makeup;
            *mode = (b, 2.0 * r * theta.cos(), -r * r);
        }
        for (modes, coefs) in self.modes.iter_mut().zip(coefs.as_chunks::<8>().0) {
            modes.b = f32x8::from(coefs.map(|c| c.0));
            modes.a1 = f32x8::from(coefs.map(|c| c.1));
            modes.a2 = f32x8::from(coefs.map(|c| c.2));
        }
    }

    /// The sympathetic strings, tuned to the key's root and fifth.
    fn tune_halo(&mut self) {
        let root = 60.0 + self.key;
        let rate = self.sample_rate;
        for (string, &(offset, _)) in self.halo_strings.iter_mut().zip(&HALO_STRINGS) {
            let hz = 440.0 * 2f32.powf((root + offset - 69.0) / 12.0);
            let period = rate / hz;
            // The loop's low-pass delays it a little, which the line makes up.
            string.delay = (period - HALO_DAMPING).max(2.0);
            string.feedback = (-SIXTY_DB * period / (HALO_RING * rate)).exp();
            string.drive = 1.0 - string.feedback;
        }
    }

    /// Tunes `voice`'s string to `hz` afresh.
    fn tune(voice: &mut Voice, sample_rate: f32) {
        let hz = voice.hz.max(LOWEST_HZ);
        let period = sample_rate / hz;
        voice.course.tune(period, DAMPING);
        voice.course.loss = STRING_LOSS;
        voice.tilt_coef = one_pole(hz * BOW_TILT, sample_rate);
    }

    /// The voice to slur a note at `pitch` onto: one let go of just now, the nearest in pitch.
    fn slurred(&self, pitch: u8) -> Option<usize> {
        let voices = self.voices.iter().enumerate();
        let released = voices.filter(|(_, v)| v.released && v.sounding);
        released
            .min_by_key(|(_, v)| v.pitch.abs_diff(pitch))
            .map(|(i, _)| i)
    }

    fn note_on(&mut self, pitch: u8, velocity: u8) {
        let velocity = f32::from(velocity) / 127.0;
        let shape = std::mem::take(&mut self.shape);
        let from = self.last.replace(pitch);
        self.notes += 1;
        if let Some(i) = self.slurred(pitch) {
            self.slur(i, pitch, velocity, shape.glide);
            return;
        }
        // The string already sounding this note, else a silent one, else the oldest.
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
        let ringing = voice.sounding;
        voice.pitch = pitch;
        let to = midi_to_hz(pitch);
        // A gesture's glide starts from the last note played, as a shift of the hand.
        (voice.hz, voice.glide_left) = match from {
            Some(last) if shape.glide > 0 => {
                let steps = shape.glide.div_ceil(GLIDE_PERIOD).max(1);
                let from = midi_to_hz(last);
                voice.glide_ratio = (to / from).powf(1.0 / steps as f32);
                (from, steps)
            }
            _ => (to, 0),
        };
        voice.velocity = velocity;
        voice.sounding = true;
        voice.held = true;
        voice.released = false;
        voice.choked = false;
        voice.fade = 1.0;
        voice.started = self.notes;
        voice.peak = 1.0;
        voice.age = 0;
        voice.drift = 0.0;
        (voice.turn, voice.turning) = (0.0, 0.0);
        if !ringing {
            voice.bow = 0.0;
            voice.tilt = 0.0;
        }
        let seconds = BITE_TIME.0 * (BITE_TIME.1 / BITE_TIME.0).powf(self.bite);
        voice.bite_coef = 1.0 - decay_coef(seconds, self.sample_rate);
        voice.scratch = SCRATCH_LEVEL * self.bite * velocity;
        voice.accent = ACCENT * self.bite;
        Self::tune(voice, self.sample_rate);
        if !ringing {
            self.kick(i, KICK_LEVEL * self.bite * velocity);
        }
    }

    /// Keeps the bow on voice `i` and moves the finger to `pitch`. The same pitch again changes
    /// the bow's direction instead.
    fn slur(&mut self, i: usize, pitch: u8, velocity: f32, glide: u32) {
        let (rate, bite) = (self.sample_rate, self.bite);
        let glide = match glide {
            0 => (self.glide * rate) as u32,
            glide => glide,
        };
        let Some(voice) = self.voices.get_mut(i) else {
            return;
        };
        voice.held = true;
        voice.released = false;
        voice.velocity = velocity;
        voice.started = self.notes;
        if voice.pitch == pitch {
            voice.turning = 1.0;
            voice.scratch = SCRATCH_TURN * SCRATCH_LEVEL * bite * velocity;
            return;
        }
        voice.pitch = pitch;
        voice.age = 0;
        let to = midi_to_hz(pitch);
        let steps = glide / GLIDE_PERIOD;
        if steps == 0 {
            voice.hz = to;
            voice.glide_left = 0;
            Self::tune(voice, rate);
        } else {
            voice.glide_ratio = (to / voice.hz.max(LOWEST_HZ)).powf(1.0 / steps as f32);
            voice.glide_left = steps;
        }
    }

    /// Starts voice `i`'s string moving with a bright burst, so a hard bite speaks at once.
    fn kick(&mut self, i: usize, level: f32) {
        if level <= 0.0 {
            return;
        }
        let Some(voice) = self.voices.get_mut(i) else {
            return;
        };
        let period = voice.course.period;
        let len = (period.round() as usize).clamp(2, self.kick.len() / 2);
        let gap = ((0.127 * period).round() as usize).clamp(1, len - 1);
        let smooth = one_pole(0.3 * self.sample_rate, self.sample_rate);
        let Some(wave) = waveguide::pluck(&mut self.kick, &mut self.noise, len, gap, smooth, level)
        else {
            return;
        };
        voice.course.place(wave, 1.0);
    }

    fn note_off(&mut self, pitch: u8) {
        for voice in &mut self.voices {
            if voice.pitch == pitch && voice.held {
                voice.held = false;
                voice.released = true;
            }
        }
    }

    /// Glides the string playing `from` to `to`, as a slur.
    fn retune(&mut self, from: u8, to: u8, velocity: u8) {
        self.note_off(from);
        self.note_on(to, velocity);
    }

    fn choke(&mut self) {
        for voice in &mut self.voices {
            if voice.sounding {
                voice.choked = true;
                voice.held = false;
                voice.released = false;
            }
        }
    }

    /// Every `GLIDE_PERIOD` samples: gliding strings take their next step in pitch, and the
    /// vibrato moves on.
    fn glide(&mut self) {
        let rate = self.sample_rate;
        let step = GLIDE_PERIOD as f32 / rate;
        let delay = (VIBRATO_DELAY / step) as u32;
        let rise = VIBRATO_RISE / step;
        let depth = 100.0 * self.vibrato * self.vibrato.sqrt();
        let speed = 5.2 + 3.5 * self.vibrato * self.vibrato;
        for voice in self.voices.iter_mut().filter(|v| v.sounding) {
            if voice.glide_left > 0 {
                voice.glide_left -= 1;
                voice.hz = match voice.glide_left {
                    0 => midi_to_hz(voice.pitch),
                    _ => voice.hz * voice.glide_ratio,
                };
                Self::tune(voice, rate);
            }
            if depth <= 0.0 || !voice.held {
                continue;
            }
            voice.age = voice.age.saturating_add(1);
            // A hand's vibrato wanders a little in speed.
            voice.drift = (voice.drift + 0.02 * self.noise.next()).clamp(-1.0, 1.0);
            voice.phase = (voice.phase + speed * (1.0 + 0.1 * voice.drift) * step).fract();
            let into = voice.age.saturating_sub(delay) as f32 / rise;
            let cents = depth * into.min(1.0) * (std::f32::consts::TAU * voice.phase).sin();
            let hz = voice.hz.max(LOWEST_HZ) * 2f32.powf(cents / 1200.0);
            voice.course.stretch(rate / hz, DAMPING);
        }
    }

    /// Every `CONTROL_PERIOD` samples: strings that have fallen silent stop, and once they all
    /// have, so does the body when it's rung out.
    fn update_control(&mut self) {
        for voice in &mut self.voices {
            if voice.sounding {
                let faded = voice.choked && voice.fade < SILENCE;
                voice.sounding =
                    !faded && (voice.peak > SILENCE || voice.held || voice.bow > SILENCE);
                voice.peak = 0.0;
            }
        }
        let strings = self.voices.iter().any(|v| v.sounding);
        let was = self.tail;
        self.tail = strings || self.ringing > SILENCE;
        self.ringing = 0.0;
        if was && !self.tail {
            self.silence();
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        // A note starting after this isn't slurred onto one let go before it.
        for voice in &mut self.voices {
            voice.released = false;
        }
        if !self.tail && self.voices.iter().all(|v| !v.sounding) {
            left.fill(0.0);
            right.fill(0.0);
            return;
        }
        let lerp = |(lo, hi): (f32, f32)| lo + (hi - lo) * self.bow;
        let speed = lerp(BOW_SPEED);
        // The friction curve's slope, which a lower value makes grip harder: tied to the speed,
        // or a fast bow with a firm grip falls out of the string's Helmholtz motion into
        // weaker ones that start or not depending on how the note began.
        let grip = lerp(BOW_HOLD) / speed;
        let air = AIR * (1.0 - self.bow) * (1.0 - self.bow);
        let body_level = self.loudness;
        let halo = self.halo;
        let gain = LEVEL * 2.0 * self.gain;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.clock.is_multiple_of(CONTROL_PERIOD) {
                self.update_control();
            }
            if self.clock.is_multiple_of(GLIDE_PERIOD) {
                self.glide();
            }
            self.clock = self.clock.wrapping_add(1);
            // The strings, together, as they push on the bridge.
            let mut bridge = 0.0;
            for voice in &mut self.voices {
                if !voice.sounding {
                    continue;
                }
                let (target, coef) = if voice.held {
                    (voice.velocity, voice.bite_coef)
                } else {
                    (0.0, 1.0 - self.lift_coef)
                };
                voice.bow += coef * (target - voice.bow);
                let bow = (voice.bow > 1e-3).then(|| {
                    let waver = 1.0 + BOW_WAVER * self.noise.next();
                    let speed = speed * (1.0 + voice.accent) * (1.0 - (1.0 - REBOW) * voice.turn);
                    // Grip goes as the speed does, as the bow's set up to.
                    (speed * voice.bow * waver, grip / (1.0 + voice.accent))
                });
                voice.accent *= self.accent_coef;
                voice.turn += self.turn_coef * (voice.turning - voice.turn);
                voice.turning *= self.turning_coef;
                let x = voice.course.tick(DAMPING, 0.0, bow);
                voice.tilt += voice.tilt_coef * (x - voice.tilt);
                let mut out = BOW_LIFT * voice.tilt;
                if voice.scratch > SILENCE {
                    let [low, last, band] = &mut voice.scratch_band;
                    *low += self.scratch_smooth * (self.noise.next() - *low);
                    *band += self.scratch_smooth * (*low - *last - *band);
                    *last = *low;
                    // The band's level is about a fifth of the noise's.
                    out += 5.0 * voice.scratch * *band;
                    voice.scratch *= self.scratch_coef;
                }
                out += air * voice.bow * self.noise.next();
                let out = out * voice.fade;
                if voice.choked {
                    voice.fade *= self.fade_coef;
                }
                voice.peak = voice.peak.max(out.abs());
                bridge += out;
            }
            let (mut out_l, mut out_r) = (DRY * bridge, DRY * bridge);
            let (mut body, mut body_l, mut body_r) = (f32x8::ZERO, f32x8::ZERO, f32x8::ZERO);
            let x = f32x8::splat(bridge);
            for modes in &mut self.modes {
                let y = modes.b * x + modes.a1 * modes.y1 + modes.a2 * modes.y2;
                (modes.y2, modes.y1) = (modes.y1, y);
                body_l += modes.left * y;
                body_r += modes.right * y;
                body += y;
            }
            out_l += body_level * body_l.reduce_add();
            out_r += body_level * body_r.reduce_add();
            let body = body.reduce_add();
            if halo > 0.0 {
                for string in &mut self.halo_strings {
                    let back = string.line.read_between(string.delay);
                    let damped = (1.0 - HALO_DAMPING) * back + HALO_DAMPING * string.last;
                    string.last = back;
                    string
                        .line
                        .write(string.drive * body + string.feedback * damped);
                    out_l += HALO_LEVEL * halo * string.left * back;
                    out_r += HALO_LEVEL * halo * string.right * back;
                }
            }
            self.ringing = self.ringing.max(out_l.abs()).max(out_r.abs());
            for (out, (x1, y1)) in [&mut out_l, &mut out_r].into_iter().zip(&mut self.dc) {
                let y = *out - *x1 + self.dc_coef * *y1;
                (*x1, *y1) = (*out, y);
                *out = y * gain;
            }
            (*l, *r) = (out_l, out_r);
        }
    }

    /// Clears what's left ringing in the body and sympathetic strings once it's too quiet to
    /// hear, so they start from silence.
    fn silence(&mut self) {
        for modes in &mut self.modes {
            (modes.y1, modes.y2) = (f32x8::ZERO, f32x8::ZERO);
        }
        for string in &mut self.halo_strings {
            string.line.clear();
            string.last = 0.0;
        }
        self.dc = [(0.0, 0.0); 2];
    }
}

/// Equal-power gains for a place in the field, from -1 (left) to 1 (right).
fn pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (angle.cos(), angle.sin())
}

fn midi_to_hz(pitch: u8) -> f32 {
    440.0 * 2f32.powf((f32::from(pitch) - 69.0) / 12.0)
}

impl Processor for Mosaic {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.render(l, r),
            Segment::Event(event) => match event.kind {
                EventKind::NoteOn { pitch, velocity } => self.note_on(pitch, velocity),
                EventKind::NoteOff { pitch } => self.note_off(pitch),
                EventKind::Retune { from, to, velocity } => self.retune(from, to, velocity),
                EventKind::Choke => self.choke(),
                EventKind::Shape(shape) => self.shape = shape,
                EventKind::Program(_) => {}
                EventKind::Param { id, value } => {
                    if let Some(info) = InstKind::Mosaic.params().get(id as usize) {
                        self.set(id, info.clamp(value));
                    }
                }
            },
        });
    }

    fn params(&self) -> &[ParamInfo] {
        InstKind::Mosaic.params()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn mosaic(settings: &[(u32, f32)]) -> Mosaic {
        let mut values = lattice_core::Instrument::new(InstKind::Mosaic).values;
        for &(id, value) in settings {
            values[id as usize] = value;
        }
        Mosaic::with_values(RATE, &values)
    }

    fn on(pitch: u8) -> Event {
        Event {
            offset: 0,
            kind: EventKind::NoteOn {
                pitch,
                velocity: 100,
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
    fn play_both(m: &mut Mosaic, events: &[Event], seconds: f32) -> (Vec<f32>, Vec<f32>) {
        let len = (seconds * RATE) as usize;
        let (mut l, mut r) = (vec![0.0; len], vec![0.0; len]);
        m.process(events, &mut l, &mut r);
        (l, r)
    }

    fn play(m: &mut Mosaic, events: &[Event], seconds: f32) -> Vec<f32> {
        play_both(m, events, seconds).0
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

    fn cents(a: f32, b: f32) -> f32 {
        1200.0 * (a / b).log2()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|x| x * x).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn plays_in_tune_on_every_body() {
        for material in [0.0, 1.0, 2.0, 3.0] {
            for note in [55, 67, 79, 91] {
                let mut m = mosaic(&[(MATERIAL, material), (VIBRATO, 0.0), (HALO, 0.0)]);
                let x = play(&mut m, &[on(note)], 0.8);
                let hz = midi_to_hz(note);
                let heard = pitch(&x[28_800..38_400], hz);
                assert!(
                    cents(heard, hz).abs() < 8.0,
                    "material {material}, {note}: {heard} Hz, not {hz}"
                );
            }
        }
    }

    #[test]
    fn a_hard_bite_speaks_at_once_and_a_soft_one_swells() {
        let onset = |bite: f32| {
            let mut m = mosaic(&[(BITE, bite)]);
            let x = play(&mut m, &[on(69)], 1.0);
            let full = rms(&x[38_400..]);
            // How long until it's within 6 dB of where it settles.
            let at = x
                .chunks(240)
                .position(|c| rms(c) > full * 0.5)
                .unwrap_or(200);
            at as f32 * 240.0 / RATE
        };
        let (hard, soft) = (onset(1.0), onset(0.0));
        assert!(hard < 0.04, "a hard bite took {hard} s");
        assert!(soft > 0.15, "a soft bite took only {soft} s");
    }

    #[test]
    fn a_slur_moves_the_finger_without_lifting_the_bow() {
        let mut m = mosaic(&[(VIBRATO, 0.0), (GLIDE, 0.0), (HALO, 0.0)]);
        play(&mut m, &[on(69)], 0.5);
        let bow = m.voices.iter().map(|v| v.bow).fold(0.0, f32::max);
        // A note-off and a note-on together, as a legato line plays them.
        let x = play(&mut m, &[off(69), on(72)], 0.3);
        let bowed: Vec<_> = m.voices.iter().filter(|v| v.held).collect();
        assert_eq!(bowed.len(), 1);
        assert_eq!(bowed[0].pitch, 72);
        assert!(bowed[0].bow > bow * 0.8, "the bow lifted");
        let heard = pitch(&x[4_800..], midi_to_hz(72));
        assert!(cents(heard, midi_to_hz(72)).abs() < 8.0, "{heard} Hz");
        // With a gap between, it's a new bow, on a string of its own.
        play(&mut m, &[off(72)], 0.1);
        play(&mut m, &[on(76)], 0.01);
        let fresh = m.voices.iter().find(|v| v.held).unwrap();
        assert!(fresh.bow < 0.5 * bow);
    }

    #[test]
    fn rings_out_after_the_bow_lifts_and_then_stops() {
        let mut m = mosaic(&[(MATERIAL, 1.0), (RING, 0.5), (HALO, 1.0)]);
        play(&mut m, &[on(69)], 0.5);
        let after = play(&mut m, &[off(69)], 0.3);
        assert!(rms(&after[9_600..]) > 1e-3, "the glass didn't ring");
        play(&mut m, &[], 60.0);
        assert!(!m.tail && m.voices.iter().all(|v| !v.sounding));
        assert!(play(&mut m, &[], 0.1).iter().all(|&x| x == 0.0));
    }

    #[test]
    fn every_body_stays_steady_at_its_most_extreme() {
        for material in [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0] {
            for (bow, size, ring) in [(0.0, 0.25, 0.0), (1.0, 8.0, 1.0), (1.0, 0.25, 1.0)] {
                let mut m = mosaic(&[
                    (MATERIAL, material),
                    (BOW, bow),
                    (SIZE, size),
                    (RING, ring),
                    (BITE, 1.0),
                    (VIBRATO, 1.0),
                    (HALO, 1.0),
                    (GAIN, 1.0),
                ]);
                let chord: Vec<Event> = [28, 43, 55, 67, 79, 91, 103, 120]
                    .into_iter()
                    .map(on)
                    .collect();
                let x = play(&mut m, &chord, 4.0);
                let peak = x.iter().fold(0.0f32, |p, x| p.max(x.abs()));
                // Loud, all eight at once with everything up, but not running away.
                let (early, late) = (rms(&x[48_000..96_000]), rms(&x[144_000..]));
                assert!(
                    peak.is_finite() && peak < 20.0 && late < early * 1.5,
                    "material {material}, bow {bow}, size {size}, ring {ring}: {peak}, {early} \
                     then {late}"
                );
            }
        }
    }

    #[test]
    fn the_materials_are_about_as_loud_as_each_other() {
        let level = |material: f32| {
            let mut m = mosaic(&[(MATERIAL, material), (HALO, 0.0)]);
            let x = play(&mut m, &[on(67), on(74)], 1.0);
            rms(&x[24_000..])
        };
        let levels = [0.0, 1.0, 2.0, 3.0].map(level);
        let (lo, hi) = levels
            .iter()
            .fold((f32::MAX, 0.0f32), |(lo, hi), &l| (lo.min(l), hi.max(l)));
        assert!(hi < lo * 2.0, "{levels:?}");
    }

    #[test]
    fn the_body_is_stereo_and_choking_stops_it() {
        let mut m = mosaic(&[]);
        let (l, r) = play_both(&mut m, &[on(62)], 0.2);
        assert_ne!(l, r);
        let choke = Event {
            offset: 0,
            kind: EventKind::Choke,
        };
        play(&mut m, &[choke], 0.1);
        assert!(m.voices.iter().all(|v| !v.sounding));
    }
    #[test]
    fn the_halo_rings_with_its_own_notes_and_never_drowns_them() {
        let held = |pitch: u8, halo: f32| {
            let mut m = mosaic(&[(HALO, halo), (VIBRATO, 0.0)]);
            rms(&play(&mut m, &[on(pitch)], 2.0)[48_000..])
        };
        // In D, the strings are D3, D4, A4, D5 and A5. Held on any of them, the halo blooms, by
        // as much high as low; off them, on an E, it stays quiet.
        for pitch in [50, 62, 69, 74, 81] {
            let db = 20.0 * (held(pitch, 1.0) / held(pitch, 0.0)).log10();
            assert!((4.0..12.0).contains(&db), "{pitch}: {db} dB");
        }
        let db = 20.0 * (held(64, 1.0) / held(64, 0.0)).log10();
        assert!(db.abs() < 1.0, "E: {db} dB");
    }
}
