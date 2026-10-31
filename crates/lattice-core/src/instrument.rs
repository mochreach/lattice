//! Instruments as a song stores them: which one plays a track, and its settings.
//!
//! The drums (kick, snare, hat, clap and tom) are Rhythmicon, Lattice's first engine, named after the
//! first drum machine, which Leon Theremin built for Henry Cowell in 1931. Each plays one drum on
//! one track: C-4 plays it at its tuning, and other notes tune it up or down from there.
//!
//! The string is Monochord, the second engine, named after Pythagoras' one-stringed instrument
//! for studying pitch: a plucked or bowed string, modelled as a waveguide.

use serde::{Deserialize, Serialize};

use crate::device::{Curve, Device, Kind, ParamInfo, param};

/// The instrument on a track, and its settings.
pub type Instrument = Device<InstKind>;

impl Default for Instrument {
    fn default() -> Self {
        Self::new(InstKind::Synth)
    }
}

/// The note that plays a drum at its tuning.
pub const DRUM_ROOT: u8 = 60;

/// The instruments Lattice has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum InstKind {
    /// The plain test synth.
    #[default]
    Synth,
    Kick,
    Snare,
    Hat,
    Clap,
    Tom,
    String,
}

impl InstKind {
    pub const ALL: [InstKind; 7] = [
        InstKind::Synth,
        InstKind::String,
        InstKind::Kick,
        InstKind::Snare,
        InstKind::Hat,
        InstKind::Clap,
        InstKind::Tom,
    ];

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    pub const fn params(self) -> &'static [ParamInfo] {
        match self {
            InstKind::Synth => &synth::PARAMS,
            InstKind::Kick => &kick::PARAMS,
            InstKind::Snare => &snare::PARAMS,
            InstKind::Hat => &hat::PARAMS,
            InstKind::Clap => &clap::PARAMS,
            InstKind::Tom => &tom::PARAMS,
            InstKind::String => &string::PARAMS,
        }
    }
}

impl Kind for InstKind {
    fn name(self) -> &'static str {
        match self {
            InstKind::Synth => "synth",
            InstKind::Kick => "kick",
            InstKind::Snare => "snare",
            InstKind::Hat => "hat",
            InstKind::Clap => "clap",
            InstKind::Tom => "tom",
            InstKind::String => "string",
        }
    }

    fn params(self) -> &'static [ParamInfo] {
        InstKind::params(self)
    }
}

/// The test synth: oscillator, low-pass filter with an envelope, amplifier.
pub mod synth {
    use super::*;

    pub const WAVE: u32 = 0;
    pub const ATTACK: u32 = 1;
    pub const DECAY: u32 = 2;
    pub const SUSTAIN: u32 = 3;
    pub const RELEASE: u32 = 4;
    pub const CUTOFF: u32 = 5;
    pub const ENV: u32 = 6;
    pub const RESONANCE: u32 = 7;
    pub const GAIN: u32 = 8;

    pub const WAVES: &[&str] = &["saw", "square"];

    pub(super) const PARAMS: [ParamInfo; 9] = [
        param(WAVE, "wave", (0.0, 1.0, 0.0), "", Curve::Choice(WAVES)),
        param(ATTACK, "attack", (0.001, 2.0, 0.005), "s", Curve::Log),
        param(DECAY, "decay", (0.01, 4.0, 0.2), "s", Curve::Log),
        param(SUSTAIN, "sustain", (0.0, 1.0, 0.5), "", Curve::Linear),
        param(RELEASE, "release", (0.005, 4.0, 0.1), "s", Curve::Log),
        param(
            CUTOFF,
            "cutoff",
            (20.0, 16_000.0, 1_000.0),
            "Hz",
            Curve::Log,
        ),
        // Hz added to the cutoff at the envelope's peak.
        param(ENV, "env", (0.0, 10_000.0, 3_000.0), "Hz", Curve::Linear),
        param(RESONANCE, "resonance", (0.0, 0.95, 0.3), "", Curve::Linear),
        param(GAIN, "gain", (0.0, 1.0, 0.2), "", Curve::Linear),
    ];
}

/// A sine that drops in pitch as it starts, then rings out.
pub mod kick {
    use super::*;

    /// The kind of kick, which the other settings work within: short and clicky, an 808's long
    /// boom, or a 909's hard, driven thump.
    pub const MODEL: u32 = 0;
    /// Its pitch, once the drop is over, at [`DRUM_ROOT`].
    pub const TUNE: u32 = 1;
    /// How far and how hard the pitch drops at the start: a soft thud to a sharp click.
    pub const PUNCH: u32 = 2;
    /// Time to fade by 60 dB.
    pub const DECAY: u32 = 3;
    /// Saturation, off centre so it adds even harmonics: it fattens the tail and squares it off.
    pub const DRIVE: u32 = 4;

    pub const MODELS: &[&str] = &["tight", "boom", "hard"];

    pub(super) const PARAMS: [ParamInfo; 5] = [
        param(MODEL, "model", (0.0, 2.0, 1.0), "", Curve::Choice(MODELS)),
        param(TUNE, "tune", (30.0, 120.0, 50.0), "Hz", Curve::Log),
        param(PUNCH, "punch", (0.0, 1.0, 0.5), "", Curve::Linear),
        param(DECAY, "decay", (0.05, 2.0, 0.45), "s", Curve::Log),
        param(DRIVE, "drive", (0.0, 1.0, 0.35), "", Curve::Linear),
    ];
}

/// Two tuned modes for the shell, and a burst of noise for the wires.
pub mod snare {
    use super::*;

    /// The kind of snare: thin and short, full and weighty, or a 909's bright crack.
    pub const MODEL: u32 = 0;
    /// The shell's lower mode at [`DRUM_ROOT`].
    pub const TUNE: u32 = 1;
    pub const DECAY: u32 = 2;
    /// How much of the wires' noise there is against the shell.
    pub const SNAP: u32 = 3;
    /// Brightness of the noise.
    pub const TONE: u32 = 4;

    pub const MODELS: &[&str] = &["tight", "fat", "crack"];

    pub(super) const PARAMS: [ParamInfo; 5] = [
        param(MODEL, "model", (0.0, 2.0, 1.0), "", Curve::Choice(MODELS)),
        param(TUNE, "tune", (100.0, 400.0, 180.0), "Hz", Curve::Log),
        param(DECAY, "decay", (0.05, 1.0, 0.22), "s", Curve::Log),
        param(SNAP, "snap", (0.0, 1.0, 0.6), "", Curve::Linear),
        param(TONE, "tone", (1_000.0, 12_000.0, 5_000.0), "Hz", Curve::Log),
    ];
}

/// Six detuned square waves, as in the 808's cymbals, and some noise, high-passed.
pub mod hat {
    use super::*;

    /// The kind of hat: the squares mixed in pairs for a clangy metal, summed for an 808's
    /// ring, or mostly noise for a bright sizzle.
    pub const MODEL: u32 = 0;
    /// The lowest of the six at [`DRUM_ROOT`]; the others keep their ratios to it.
    pub const TUNE: u32 = 1;
    /// Short for a closed hat, long for an open one.
    pub const DECAY: u32 = 2;
    /// Where the high-pass sits: lower lets more of the metal through.
    pub const TONE: u32 = 3;
    /// Noise against the metal.
    pub const NOISE: u32 = 4;

    pub const MODELS: &[&str] = &["metal", "ring", "sizzle"];

    pub(super) const PARAMS: [ParamInfo; 5] = [
        param(MODEL, "model", (0.0, 2.0, 1.0), "", Curve::Choice(MODELS)),
        param(TUNE, "tune", (100.0, 800.0, 205.0), "Hz", Curve::Log),
        param(DECAY, "decay", (0.01, 2.0, 0.06), "s", Curve::Log),
        param(TONE, "tone", (2_000.0, 14_000.0, 7_000.0), "Hz", Curve::Log),
        param(NOISE, "noise", (0.0, 1.0, 0.3), "", Curve::Linear),
    ];
}

/// Bursts of band-passed noise in quick succession, as hands don't quite clap together, then a
/// tail.
pub mod clap {
    use super::*;

    /// The kind of clap: one tight snap, an 808's handful of claps and a roomy tail, or a crowd
    /// clapping out of time.
    pub const MODEL: u32 = 0;
    /// Where the noise's band sits at [`DRUM_ROOT`].
    pub const TUNE: u32 = 1;
    /// How long the tail rings.
    pub const DECAY: u32 = 2;
    /// Time between the bursts.
    pub const SPREAD: u32 = 3;
    /// The tail against the bursts.
    pub const ROOM: u32 = 4;

    pub const MODELS: &[&str] = &["tight", "room", "crowd"];

    pub(super) const PARAMS: [ParamInfo; 5] = [
        param(MODEL, "model", (0.0, 2.0, 1.0), "", Curve::Choice(MODELS)),
        param(TUNE, "tune", (500.0, 3_000.0, 1_100.0), "Hz", Curve::Log),
        param(DECAY, "decay", (0.05, 1.5, 0.3), "s", Curve::Log),
        param(SPREAD, "spread", (0.003, 0.03, 0.011), "s", Curve::Log),
        param(ROOM, "room", (0.0, 1.0, 0.5), "", Curve::Linear),
    ];
}

/// A tuned membrane: a sine that bends down onto its pitch, with a touch of noise for the stick.
pub mod tom {
    use super::*;

    /// The kind of tom: short and dry, an 808's round boom, or a 909's harder, ringing shell.
    pub const MODEL: u32 = 0;
    /// Its pitch at [`DRUM_ROOT`], once the bend is over.
    pub const TUNE: u32 = 1;
    pub const DECAY: u32 = 2;
    /// How far above its pitch it starts.
    pub const BEND: u32 = 3;
    /// The stick's noise against the shell.
    pub const NOISE: u32 = 4;

    pub const MODELS: &[&str] = &["tight", "round", "hard"];

    pub(super) const PARAMS: [ParamInfo; 5] = [
        param(MODEL, "model", (0.0, 2.0, 1.0), "", Curve::Choice(MODELS)),
        param(TUNE, "tune", (50.0, 400.0, 120.0), "Hz", Curve::Log),
        param(DECAY, "decay", (0.05, 2.0, 0.4), "s", Curve::Log),
        param(BEND, "bend", (0.0, 1.0, 0.4), "", Curve::Linear),
        param(NOISE, "noise", (0.0, 1.0, 0.2), "", Curve::Linear),
    ];
}

/// A string, plucked or bowed: a waveguide whose loop sets its pitch, and whose losses set how
/// long and how brightly it rings.
pub mod string {
    use super::*;

    /// The kind of string: warm nylon, a pair of bright steel strings a few cents apart, as on a
    /// twelve-string, or a sitar's, buzzing against its curved bridge.
    pub const MODEL: u32 = 0;
    /// What plucks it, from a soft thumb, nearly a sine, to a hard pick, bright and clicking.
    pub const PLUCK: u32 = 1;
    /// Where along the string it's plucked: near the bridge is thin and nasal, the middle hollow.
    pub const POSITION: u32 = 2;
    /// Time for the lowest notes to fade by 60 dB; high notes, as on a real string, die sooner.
    pub const DECAY: u32 = 3;
    /// How much of the high end survives each trip along the string.
    pub const TONE: u32 = 4;
    /// Bowing while the note is held, which keeps the string sounding: light skates over it, heavy
    /// grips and grits. Plucks fade out as it's turned up.
    pub const BOW: u32 = 5;

    pub const MODELS: &[&str] = &["nylon", "steel", "sitar"];

    pub(super) const PARAMS: [ParamInfo; 6] = [
        param(MODEL, "model", (0.0, 2.0, 0.0), "", Curve::Choice(MODELS)),
        param(PLUCK, "pluck", (0.0, 1.0, 0.5), "", Curve::Linear),
        param(POSITION, "position", (0.02, 0.5, 0.15), "", Curve::Linear),
        param(DECAY, "decay", (0.2, 20.0, 3.0), "s", Curve::Log),
        param(TONE, "tone", (0.0, 1.0, 0.6), "", Curve::Linear),
        param(BOW, "bow", (0.0, 1.0, 0.0), "", Curve::Linear),
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_in_range_and_ids_are_indices() {
        for kind in InstKind::ALL {
            for (i, p) in kind.params().iter().enumerate() {
                assert_eq!(p.id as usize, i, "{} {}", kind.name(), p.name);
                assert!((p.min..=p.max).contains(&p.default), "{}", p.name);
            }
            assert_eq!(InstKind::named(kind.name()), Some(kind));
        }
    }
}
