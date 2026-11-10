//! Instruments as a song stores them: which one plays a track, and its settings.
//!
//! The drums (kick, snare, hat, clap and tom) are Rhythmicon, Lattice's first engine, named after the
//! first drum machine, which Leon Theremin built for Henry Cowell in 1931. Each plays one drum on
//! one track: C-4 plays it at its tuning, and other notes tune it up or down from there.
//!
//! The sampler plays a sound file: whole and pitched by the note, or chopped into slices, one
//! to a note.
//!
//! Chop cuts a sound file into equal pieces, one to a step, so a loop can be rearranged a step
//! at a time: each note plays a piece, from where it starts, until the note ends.
//!
//! The string is Monochord, the second engine, named after Pythagoras' one-stringed instrument
//! for studying pitch: a plucked or bowed string, modelled as a waveguide.
//!
//! The modular is Switched, after Wendy Carlos's *Switched-On Bach*: a stack of saws, hard-synced
//! for grit, through a screaming ladder filter, in stereo.
//!
//! The harpsichord is Krell, after the vanished civilisation of *Forbidden Planet*, whose film
//! had the first all-electronic score, Louis and Bebe Barron's: choirs of quill-plucked strings
//! at 16', 8', 4' and 2', which can drift apart, over a body of plates and glass that rings in
//! sympathy.
//!
//! The electric piano is Tuss, after The Tuss, the name Richard D. James put out records under
//! in 2007, as two Cornish siblings who never existed: a tine struck by a hammer, ringing in
//! front of a magnetic pickup that barks when it's struck hard, which can drift out of tune and
//! play in other tunings than equal temperament.
//!
//! The FM synthesiser is Bessel, after Friedrich Bessel, whose functions give the strength of
//! each partial frequency modulation makes: four operators, sine oscillators that bend each
//! other's phase, in one of eight algorithms, as in Yamaha's four-operator synthesisers.
//!
//! The bowed engine is Mosaic, after IRCAM's modal synthesis program, first called MOSAIC and
//! now Modalys, made for instruments that can't exist: a bowed string, as a violin's, whose body
//! can be any material at any size, from a violin's wood through glass and metal to a singing
//! voice, with sympathetic strings ringing behind it.

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
    Monochord,
    Sampler,
    Chop,
    Switched,
    Krell,
    Tuss,
    Bessel,
    Mosaic,
    /// Not an instrument of Lattice's own: notes and settings go out over MIDI.
    Midi,
}

impl InstKind {
    pub const ALL: [InstKind; 15] = [
        InstKind::Synth,
        InstKind::Switched,
        InstKind::Bessel,
        InstKind::Monochord,
        InstKind::Mosaic,
        InstKind::Krell,
        InstKind::Tuss,
        InstKind::Sampler,
        InstKind::Chop,
        InstKind::Kick,
        InstKind::Snare,
        InstKind::Hat,
        InstKind::Clap,
        InstKind::Tom,
        InstKind::Midi,
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
            InstKind::Monochord => &monochord::PARAMS,
            InstKind::Sampler => &sampler::PARAMS,
            InstKind::Chop => &chop::PARAMS,
            InstKind::Switched => &switched::PARAMS,
            InstKind::Krell => &krell::PARAMS,
            InstKind::Tuss => &tuss::PARAMS,
            InstKind::Bessel => &bessel::PARAMS,
            InstKind::Mosaic => &mosaic::PARAMS,
            InstKind::Midi => &midi::PARAMS,
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
            InstKind::Monochord => "monochord",
            InstKind::Sampler => "sampler",
            InstKind::Chop => "chop",
            InstKind::Switched => "switched",
            InstKind::Krell => "krell",
            InstKind::Tuss => "tuss",
            InstKind::Bessel => "bessel",
            InstKind::Mosaic => "mosaic",
            InstKind::Midi => "midi",
        }
    }

    fn params(self) -> &'static [ParamInfo] {
        InstKind::params(self)
    }
}

/// A MIDI track: its notes go out over MIDI on its channel, and its settings are the
/// channel, the pitch bend and the 128 controllers, each sent as it changes. A controller at -1
/// isn't sent at all, so a track only sends what it's been given.
pub mod midi {
    use super::*;

    pub const CHANNEL: u32 = 0;
    pub const BEND: u32 = 1;
    /// The first controller's id: controller `n` is `CC + n`.
    pub const CC: u32 = 2;

    /// The controllers' names, by number.
    const NAMES: [&str; 128] = [
        "cc0", "cc1", "cc2", "cc3", "cc4", "cc5", "cc6", "cc7", "cc8", "cc9", "cc10", "cc11",
        "cc12", "cc13", "cc14", "cc15", "cc16", "cc17", "cc18", "cc19", "cc20", "cc21", "cc22",
        "cc23", "cc24", "cc25", "cc26", "cc27", "cc28", "cc29", "cc30", "cc31", "cc32", "cc33",
        "cc34", "cc35", "cc36", "cc37", "cc38", "cc39", "cc40", "cc41", "cc42", "cc43", "cc44",
        "cc45", "cc46", "cc47", "cc48", "cc49", "cc50", "cc51", "cc52", "cc53", "cc54", "cc55",
        "cc56", "cc57", "cc58", "cc59", "cc60", "cc61", "cc62", "cc63", "cc64", "cc65", "cc66",
        "cc67", "cc68", "cc69", "cc70", "cc71", "cc72", "cc73", "cc74", "cc75", "cc76", "cc77",
        "cc78", "cc79", "cc80", "cc81", "cc82", "cc83", "cc84", "cc85", "cc86", "cc87", "cc88",
        "cc89", "cc90", "cc91", "cc92", "cc93", "cc94", "cc95", "cc96", "cc97", "cc98", "cc99",
        "cc100", "cc101", "cc102", "cc103", "cc104", "cc105", "cc106", "cc107", "cc108", "cc109",
        "cc110", "cc111", "cc112", "cc113", "cc114", "cc115", "cc116", "cc117", "cc118", "cc119",
        "cc120", "cc121", "cc122", "cc123", "cc124", "cc125", "cc126", "cc127",
    ];

    #[expect(
        clippy::indexing_slicing,
        reason = "worked out as it compiles, so out of bounds is an error then, not a panic"
    )]
    pub(super) const PARAMS: [ParamInfo; 130] = {
        let mut params = [param(CHANNEL, "channel", (1.0, 16.0, 1.0), "", Curve::Whole); 130];
        params[1] = param(BEND, "bend", (-1.0, 1.0, 0.0), "", Curve::Linear);
        let mut n = 0;
        while n < 128 {
            params[CC as usize + n] = param(
                CC + n as u32,
                NAMES[n],
                (-1.0, 127.0, -1.0),
                "",
                Curve::Midi,
            );
            n += 1;
        }
        params
    };

    /// The id of controller `number`.
    pub fn cc(number: u8) -> u32 {
        CC + u32::from(number.min(127))
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

/// Switched, the modular: a stack of saws, each hard-synced to a master at its own pitch, through
/// a four-pole ladder filter that screams as its resonance goes up, with one envelope for both.
pub mod switched {
    use super::*;

    /// What's stacked: one saw, a pair a little apart, a swarm of seven, organ stops (16', 8' and
    /// 4') or fifths. Each spreads across the stereo field as far as `width` takes it.
    pub const STACK: u32 = 0;
    /// How far the stack's saws are detuned from each other, up to half a semitone either side.
    pub const SPREAD: u32 = 1;
    /// How far the stack spreads left and right.
    pub const WIDTH: u32 = 2;
    /// Hard sync: each saw restarted by a master at the note, running up to four times faster
    /// as the envelope opens, for the tearing sync sweep.
    pub const GRIT: u32 = 3;
    /// The ladder's cutoff at the note middle C, before the envelope opens it; it follows the
    /// keys half way.
    pub const LADDER: u32 = 4;
    /// Resonance, driven harder as it goes up, until the ladder sings on its own.
    pub const SCREAM: u32 = 5;
    /// How far the envelope opens the ladder, up to seven octaves.
    pub const CONTOUR: u32 = 6;
    /// The envelope's shape: a pluck at the bottom, an organ's held note half way, a slow swell
    /// at the top.
    pub const SNAP: u32 = 7;
    pub const GAIN: u32 = 8;

    pub const STACKS: &[&str] = &["one", "pair", "swarm", "stops", "fifths"];

    pub(super) const PARAMS: [ParamInfo; 9] = [
        param(STACK, "stack", (0.0, 4.0, 2.0), "", Curve::Choice(STACKS)),
        param(SPREAD, "spread", (0.0, 1.0, 0.3), "", Curve::Linear),
        param(WIDTH, "width", (0.0, 1.0, 0.6), "", Curve::Linear),
        param(GRIT, "grit", (0.0, 1.0, 0.0), "", Curve::Linear),
        param(LADDER, "ladder", (20.0, 16_000.0, 900.0), "Hz", Curve::Log),
        param(SCREAM, "scream", (0.0, 1.0, 0.3), "", Curve::Linear),
        param(CONTOUR, "contour", (0.0, 1.0, 0.45), "", Curve::Linear),
        param(SNAP, "snap", (0.0, 1.0, 0.35), "", Curve::Linear),
        param(GAIN, "gain", (0.0, 1.0, 0.25), "", Curve::Linear),
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
pub mod monochord {
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

/// Krell's harpsichord: choirs of strings plucked by quills, damped as the key comes up, over a
/// body that rings in sympathy.
pub mod krell {
    use super::*;

    /// The choirs of strings each key plucks, as an organ's stops: one at pitch (8'), two, one
    /// at pitch and one an octave up (4'), an octave down (16') with those, or all five, up to
    /// two octaves up (2'). Each sits somewhere different in the stereo field.
    pub const STOPS: u32 = 0;
    /// How far the choirs drift out of tune with each other, and waver, up to a third of a
    /// semitone: a little is a real harpsichord's shimmer, a lot something stranger.
    pub const DRIFT: u32 = 1;
    /// What plucks the strings, from soft leather to a hard metal plectrum, brighter and clicking.
    pub const QUILL: u32 = 2;
    /// Time for the lowest notes to fade by 60 dB while the key's held; high notes die sooner.
    pub const DECAY: u32 = 3;
    /// How quickly the dampers stop a string when its key comes up, from not at all to at once.
    pub const DAMPER: u32 = 4;
    /// How loudly the body rings in sympathy with the strings.
    pub const BODY: u32 = 5;
    /// The body's lowest mode: tuned to the song's key, it rings with the notes in it.
    pub const HULL: u32 = 6;
    /// The body, from wood, its modes in tune and soon still, to glass and metal plates, out of
    /// tune with each other and ringing on.
    pub const GLASS: u32 = 7;
    pub const GAIN: u32 = 8;

    pub const STOPS_NAMES: &[&str] = &["8'", "8' 8'", "8' 4'", "16' 8' 4'", "16' to 2'"];

    pub(super) const PARAMS: [ParamInfo; 9] = [
        param(
            STOPS,
            "stops",
            (0.0, 4.0, 1.0),
            "",
            Curve::Choice(STOPS_NAMES),
        ),
        param(DRIFT, "drift", (0.0, 1.0, 0.15), "", Curve::Linear),
        param(QUILL, "quill", (0.0, 1.0, 0.7), "", Curve::Linear),
        param(DECAY, "decay", (0.2, 20.0, 4.0), "s", Curve::Log),
        param(DAMPER, "damper", (0.0, 1.0, 0.8), "", Curve::Linear),
        param(BODY, "body", (0.0, 1.0, 0.3), "", Curve::Linear),
        param(HULL, "hull", (40.0, 2_000.0, 196.0), "Hz", Curve::Log),
        param(GLASS, "glass", (0.0, 1.0, 0.5), "", Curve::Linear),
        param(GAIN, "gain", (0.0, 1.0, 0.5), "", Curve::Linear),
    ];
}

/// Tuss's electric piano: a hammer strikes a tine, which rings in front of a magnetic pickup,
/// damped as the key comes up.
pub mod tuss {
    use super::*;

    /// The hammer, from soft felt, round and dark, to hard, bringing out the tine's bell. Hard
    /// notes strike harder still.
    pub const STRIKE: u32 = 0;
    /// How loud the tine's clangorous upper modes are, which ring out of tune with the note and
    /// fade fast: the bell at the start of each note.
    pub const BELL: u32 = 1;
    /// Where the pickup sits: in line with the tine, hollow, hearing it twice a swing, an octave
    /// up, to off to one side, round and full.
    pub const PICKUP: u32 = 2;
    /// How hard the pickup is driven: clean, to growling on hard notes, as the tine swings far
    /// into its field.
    pub const BARK: u32 = 3;
    /// Time for the lowest notes to fade by 60 dB while the key's held; high notes die sooner.
    pub const DECAY: u32 = 4;
    /// How quickly the damper stops a tine when its key comes up, from not at all to at once.
    pub const DAMPER: u32 = 5;
    /// How far each note drifts out of tune, its own way, and wavers, as a tape stretched and
    /// worn: each sits in stereo the way it's drifted.
    pub const DRIFT: u32 = 6;
    /// The tuning the notes play in, from [`crate::tuning::TUNINGS`].
    pub const TUNING: u32 = 7;
    /// The key note the tuning's built on.
    pub const KEY: u32 = 8;
    pub const GAIN: u32 = 9;

    pub(super) const PARAMS: [ParamInfo; 10] = [
        param(STRIKE, "strike", (0.0, 1.0, 0.5), "", Curve::Linear),
        param(BELL, "bell", (0.0, 1.0, 0.4), "", Curve::Linear),
        param(PICKUP, "pickup", (0.0, 1.0, 0.7), "", Curve::Linear),
        param(BARK, "bark", (0.0, 1.0, 0.3), "", Curve::Linear),
        param(DECAY, "decay", (0.2, 20.0, 6.0), "s", Curve::Log),
        param(DAMPER, "damper", (0.0, 1.0, 0.7), "", Curve::Linear),
        param(DRIFT, "drift", (0.0, 1.0, 0.1), "", Curve::Linear),
        param(
            TUNING,
            "tuning",
            (0.0, 5.0, 0.0),
            "",
            Curve::Choice(crate::tuning::TUNINGS),
        ),
        param(
            KEY,
            "key",
            (0.0, 11.0, 0.0),
            "",
            Curve::Choice(crate::tuning::KEYS),
        ),
        param(GAIN, "gain", (0.0, 1.0, 0.5), "", Curve::Linear),
    ];
}

/// Mosaic, the bowed engine: a bow on a string, as a violin's, and the string on a body of
/// resonances that can be any material at any size.
pub mod mosaic {
    use super::*;

    /// What the body's made of, sliding between them: 0 wood, a violin's own resonances; 1 glass,
    /// sparse, pure and high; 2 metal, stretched out of tune and ringing on; 3 a voice, a singer's
    /// vowel, so the string sings.
    pub const MATERIAL: u32 = 0;
    /// How big the body is, from a toy (0.25) to a cathedral (8): bigger bodies are lower and
    /// ring longer.
    pub const SIZE: u32 = 1;
    /// How hard the bow presses: from skating airily over the string, ghostly and breathy, to
    /// pressed in till it crunches and growls.
    pub const BOW: u32 = 2;
    /// How a note starts: from a slow swell to an instant, articulate bite with a scratch of rosin.
    pub const BITE: u32 = 3;
    /// A player's vibrato, coming in a moment after the note starts so fast notes stay clean;
    /// turned up, wider and faster than any hand.
    pub const VIBRATO: u32 = 4;
    /// How loudly the sympathetic strings ring behind it, tuned to the key's root and fifth.
    pub const HALO: u32 = 5;
    /// The key the sympathetic strings are tuned to.
    pub const KEY: u32 = 6;
    /// How long a slurred note takes to slide to its pitch: a finger's change, or a portamento.
    pub const GLIDE: u32 = 7;
    /// How long the body rings on, from dry to a glass hall that keeps sounding.
    pub const RING: u32 = 8;
    pub const GAIN: u32 = 9;

    /// The materials, in order: `material` slides between neighbours.
    pub const MATERIALS: &[&str] = &["wood", "glass", "metal", "voice"];

    pub(super) const PARAMS: [ParamInfo; 10] = [
        param(MATERIAL, "material", (0.0, 3.0, 0.0), "", Curve::Linear),
        param(SIZE, "size", (0.25, 8.0, 1.0), "", Curve::Log),
        param(BOW, "bow", (0.0, 1.0, 0.5), "", Curve::Linear),
        param(BITE, "bite", (0.0, 1.0, 0.6), "", Curve::Linear),
        param(VIBRATO, "vibrato", (0.0, 1.0, 0.3), "", Curve::Linear),
        param(HALO, "halo", (0.0, 1.0, 0.2), "", Curve::Linear),
        param(
            KEY,
            "key",
            (0.0, 11.0, 2.0),
            "",
            Curve::Choice(crate::tuning::KEYS),
        ),
        param(GLIDE, "glide", (0.0, 0.5, 0.015), "s", Curve::Linear),
        param(RING, "ring", (0.0, 1.0, 0.3), "", Curve::Linear),
        param(GAIN, "gain", (0.0, 1.0, 0.5), "", Curve::Linear),
    ];
}

/// Bessel's FM synthesiser: four operators, each a sine oscillator with its own envelope, bending
/// each other's phase in one of eight algorithms. Operator `n`'s settings are named with its
/// number, `ratio1` to `wave4`, and `op(n, RATIO)` finds their ids.
pub mod bessel {
    use super::*;

    /// How the operators modulate each other, from [`ALGORITHMS`].
    pub const ALGORITHM: u32 = 0;
    /// How much operator 4 modulates itself: a sine, through a saw, to noise.
    pub const FEEDBACK: u32 = 1;
    /// How far, in semitones, every operator's pitch starts from the note's, falling back to it
    /// over `sweep`: a kick's thump, a tom's fall, a laser's zap.
    pub const PITCH: u32 = 2;
    /// How long the pitch takes to fall back, in seconds to fall by 1/e.
    pub const SWEEP: u32 = 3;
    /// How much higher notes are duller and die sooner, as a real instrument's do.
    pub const KEYSCALE: u32 = 4;
    /// The tuning the notes play in, from [`crate::tuning::TUNINGS`], and its key note.
    pub const TUNING: u32 = 5;
    pub const KEY: u32 = 6;
    pub const GAIN: u32 = 7;

    /// An operator's settings, by their offset among its own: operator `n`'s are at
    /// [`op`]`(n, …)`.
    ///
    /// Its pitch as a multiple of the note's (with `track` at 1) or of C-4's (at 0).
    pub const RATIO: u32 = 0;
    /// Its pitch nudged, in cents.
    pub const DETUNE: u32 = 1;
    /// How loud it is, if it's heard; how much it bends another's phase, if it modulates.
    pub const LEVEL: u32 = 2;
    /// Its envelope: rising to full, falling to `sustain`, and fading once the note's off.
    pub const ATTACK: u32 = 3;
    pub const DECAY: u32 = 4;
    pub const SUSTAIN: u32 = 5;
    pub const RELEASE: u32 = 6;
    /// How much softer notes make it quieter, or bend less.
    pub const VELOCITY: u32 = 7;
    /// How far its pitch follows the note: 1 all the way, 0 not at all, fixed, for drums.
    pub const TRACK: u32 = 8;
    /// Its waveform, from [`WAVES`].
    pub const WAVE: u32 = 9;
    /// How many settings each operator has, after the voice's own.
    pub const PER_OP: u32 = 10;
    pub const OPS: usize = 4;

    pub const RATIO1: u32 = 8;
    pub const DETUNE1: u32 = 9;
    pub const LEVEL1: u32 = 10;
    pub const ATTACK1: u32 = 11;
    pub const DECAY1: u32 = 12;
    pub const SUSTAIN1: u32 = 13;
    pub const RELEASE1: u32 = 14;
    pub const VELOCITY1: u32 = 15;
    pub const TRACK1: u32 = 16;
    pub const WAVE1: u32 = 17;
    pub const RATIO2: u32 = 18;
    pub const DETUNE2: u32 = 19;
    pub const LEVEL2: u32 = 20;
    pub const ATTACK2: u32 = 21;
    pub const DECAY2: u32 = 22;
    pub const SUSTAIN2: u32 = 23;
    pub const RELEASE2: u32 = 24;
    pub const VELOCITY2: u32 = 25;
    pub const TRACK2: u32 = 26;
    pub const WAVE2: u32 = 27;
    pub const RATIO3: u32 = 28;
    pub const DETUNE3: u32 = 29;
    pub const LEVEL3: u32 = 30;
    pub const ATTACK3: u32 = 31;
    pub const DECAY3: u32 = 32;
    pub const SUSTAIN3: u32 = 33;
    pub const RELEASE3: u32 = 34;
    pub const VELOCITY3: u32 = 35;
    pub const TRACK3: u32 = 36;
    pub const WAVE3: u32 = 37;
    pub const RATIO4: u32 = 38;
    pub const DETUNE4: u32 = 39;
    pub const LEVEL4: u32 = 40;
    pub const ATTACK4: u32 = 41;
    pub const DECAY4: u32 = 42;
    pub const SUSTAIN4: u32 = 43;
    pub const RELEASE4: u32 = 44;
    pub const VELOCITY4: u32 = 45;
    pub const TRACK4: u32 = 46;
    pub const WAVE4: u32 = 47;

    /// The id of operator `n`'s (1 to 4) setting `offset`, such as `op(2, LEVEL)`.
    pub const fn op(n: u32, offset: u32) -> u32 {
        8 + (n - 1) * PER_OP + offset
    }

    /// The algorithms, as in Yamaha's four-operator synthesisers though not in their order:
    /// `stack` 4→3→2→1; `branch` 4→3→1 and 2→1; `crown` 2, 3 and 4 all into 1; `pairs` 4→3 and
    /// 2→1, both heard; `fan` 4 into each of 1, 2 and 3; `chain` 4→3→2, and 1 alone; `trio` 4→3,
    /// heard with 1 and 2; `organ` all four heard, none modulating.
    pub const ALGORITHMS: &[&str] = &[
        "stack", "branch", "crown", "pairs", "fan", "chain", "trio", "organ",
    ];
    /// The waveforms: a sine; its top half only, the bottom flattened; both halves turned up,
    /// an octave higher; and half a sine twice as fast, then nothing, bright and nasal.
    pub const WAVES: &[&str] = &["sine", "half", "abs", "pulse"];

    pub(super) const PARAMS: [ParamInfo; 48] = [
        param(
            ALGORITHM,
            "algorithm",
            (0.0, 7.0, 3.0),
            "",
            Curve::Choice(ALGORITHMS),
        ),
        param(FEEDBACK, "feedback", (0.0, 1.0, 0.0), "", Curve::Linear),
        param(PITCH, "pitch", (-48.0, 48.0, 0.0), "st", Curve::Linear),
        param(SWEEP, "sweep", (0.002, 2.0, 0.05), "s", Curve::Log),
        param(KEYSCALE, "keyscale", (0.0, 1.0, 0.3), "", Curve::Linear),
        param(
            TUNING,
            "tuning",
            (0.0, 5.0, 0.0),
            "",
            Curve::Choice(crate::tuning::TUNINGS),
        ),
        param(
            KEY,
            "key",
            (0.0, 11.0, 0.0),
            "",
            Curve::Choice(crate::tuning::KEYS),
        ),
        param(GAIN, "gain", (0.0, 1.0, 0.5), "", Curve::Linear),
        param(RATIO1, "ratio1", (0.125, 32.0, 1.0), "", Curve::Log),
        param(DETUNE1, "detune1", (-100.0, 100.0, 0.0), "c", Curve::Linear),
        param(LEVEL1, "level1", (0.0, 1.0, 0.9), "", Curve::Linear),
        param(ATTACK1, "attack1", (0.0005, 10.0, 0.002), "s", Curve::Log),
        param(DECAY1, "decay1", (0.005, 20.0, 2.0), "s", Curve::Log),
        param(SUSTAIN1, "sustain1", (0.0, 1.0, 0.3), "", Curve::Linear),
        param(RELEASE1, "release1", (0.005, 20.0, 0.3), "s", Curve::Log),
        param(VELOCITY1, "velocity1", (0.0, 1.0, 0.3), "", Curve::Linear),
        param(TRACK1, "track1", (0.0, 1.0, 1.0), "", Curve::Linear),
        param(WAVE1, "wave1", (0.0, 3.0, 0.0), "", Curve::Choice(WAVES)),
        param(RATIO2, "ratio2", (0.125, 32.0, 1.0), "", Curve::Log),
        param(DETUNE2, "detune2", (-100.0, 100.0, 0.0), "c", Curve::Linear),
        param(LEVEL2, "level2", (0.0, 1.0, 0.55), "", Curve::Linear),
        param(ATTACK2, "attack2", (0.0005, 10.0, 0.002), "s", Curve::Log),
        param(DECAY2, "decay2", (0.005, 20.0, 1.2), "s", Curve::Log),
        param(SUSTAIN2, "sustain2", (0.0, 1.0, 0.2), "", Curve::Linear),
        param(RELEASE2, "release2", (0.005, 20.0, 0.3), "s", Curve::Log),
        param(VELOCITY2, "velocity2", (0.0, 1.0, 0.7), "", Curve::Linear),
        param(TRACK2, "track2", (0.0, 1.0, 1.0), "", Curve::Linear),
        param(WAVE2, "wave2", (0.0, 3.0, 0.0), "", Curve::Choice(WAVES)),
        param(RATIO3, "ratio3", (0.125, 32.0, 1.0), "", Curve::Log),
        param(DETUNE3, "detune3", (-100.0, 100.0, 0.0), "c", Curve::Linear),
        param(LEVEL3, "level3", (0.0, 1.0, 0.6), "", Curve::Linear),
        param(ATTACK3, "attack3", (0.0005, 10.0, 0.002), "s", Curve::Log),
        param(DECAY3, "decay3", (0.005, 20.0, 1.5), "s", Curve::Log),
        param(SUSTAIN3, "sustain3", (0.0, 1.0, 0.2), "", Curve::Linear),
        param(RELEASE3, "release3", (0.005, 20.0, 0.3), "s", Curve::Log),
        param(VELOCITY3, "velocity3", (0.0, 1.0, 0.5), "", Curve::Linear),
        param(TRACK3, "track3", (0.0, 1.0, 1.0), "", Curve::Linear),
        param(WAVE3, "wave3", (0.0, 3.0, 0.0), "", Curve::Choice(WAVES)),
        param(RATIO4, "ratio4", (0.125, 32.0, 14.0), "", Curve::Log),
        param(DETUNE4, "detune4", (-100.0, 100.0, 0.0), "c", Curve::Linear),
        param(LEVEL4, "level4", (0.0, 1.0, 0.4), "", Curve::Linear),
        param(ATTACK4, "attack4", (0.0005, 10.0, 0.002), "s", Curve::Log),
        param(DECAY4, "decay4", (0.005, 20.0, 0.08), "s", Curve::Log),
        param(SUSTAIN4, "sustain4", (0.0, 1.0, 0.0), "", Curve::Linear),
        param(RELEASE4, "release4", (0.005, 20.0, 0.3), "s", Curve::Log),
        param(VELOCITY4, "velocity4", (0.0, 1.0, 0.8), "", Curve::Linear),
        param(TRACK4, "track4", (0.0, 1.0, 1.0), "", Curve::Linear),
        param(WAVE4, "wave4", (0.0, 3.0, 0.0), "", Curve::Choice(WAVES)),
    ];
}

/// Plays a sound file. C-4 plays it as recorded, and other notes pitch it up or down, unless
/// it's sliced: then each note from C-4 up plays the next slice, at its own pitch.
pub mod sampler {
    use super::*;

    /// How notes play it: through to the end of the region whatever their length, until the note
    /// ends, or round the region until it ends.
    pub const MODE: u32 = 0;
    /// Where the region played starts and ends, as fractions of the file.
    pub const START: u32 = 1;
    pub const END: u32 = 2;
    /// How many equal slices the region is cut into.
    pub const SLICES: u32 = 3;
    /// Semitones up or down.
    pub const TUNE: u32 = 4;
    pub const ATTACK: u32 = 5;
    pub const RELEASE: u32 = 6;
    pub const DIRECTION: u32 = 7;
    pub const GAIN: u32 = 8;

    pub const MODES: &[&str] = &["one-shot", "gate", "loop"];
    pub const DIRECTIONS: &[&str] = &["forward", "reverse"];
    pub const SLICE_COUNTS: &[&str] = &[
        "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
    ];

    pub(super) const PARAMS: [ParamInfo; 9] = [
        param(MODE, "mode", (0.0, 2.0, 0.0), "", Curve::Choice(MODES)),
        param(START, "start", (0.0, 1.0, 0.0), "", Curve::Linear),
        param(END, "end", (0.0, 1.0, 1.0), "", Curve::Linear),
        param(
            SLICES,
            "slices",
            (0.0, 15.0, 0.0),
            "",
            Curve::Choice(SLICE_COUNTS),
        ),
        param(TUNE, "tune", (-24.0, 24.0, 0.0), "st", Curve::Linear),
        param(ATTACK, "attack", (0.000_5, 2.0, 0.000_5), "s", Curve::Log),
        param(RELEASE, "release", (0.005, 4.0, 0.03), "s", Curve::Log),
        param(
            DIRECTION,
            "direction",
            (0.0, 1.0, 0.0),
            "",
            Curve::Choice(DIRECTIONS),
        ),
        param(GAIN, "gain", (0.0, 1.0, 0.7), "", Curve::Linear),
    ];
}

/// Chop: a sound file cut into equal pieces, one to a step. A note plays a piece, C-4 the first
/// and each note up the next, wrapping round, from where the piece starts, at the file's speed,
/// until the note ends: so it follows the tempo with no stretching, playing on into the next
/// piece if the song's slower than the recording.
pub mod chop {
    use super::*;

    /// How many pieces the region is cut into: as many as the track has steps, when it's loaded.
    pub const PIECES: u32 = 0;
    /// Where the region cut up starts and ends, as fractions of the file.
    pub const START: u32 = 1;
    pub const END: u32 = 2;
    /// Semitones up or down, faster or slower.
    pub const TUNE: u32 = 3;
    /// Forwards, or each piece backwards from its end: lock it on a step to turn one piece round.
    pub const DIRECTION: u32 = 4;
    pub const RELEASE: u32 = 5;
    pub const GAIN: u32 = 6;

    pub const DIRECTIONS: &[&str] = &["forward", "reverse"];
    pub const MAX_PIECES: usize = 256;

    pub(super) const PARAMS: [ParamInfo; 7] = [
        param(
            PIECES,
            "pieces",
            (1.0, MAX_PIECES as f32, 16.0),
            "",
            Curve::Whole,
        ),
        param(START, "start", (0.0, 1.0, 0.0), "", Curve::Linear),
        param(END, "end", (0.0, 1.0, 1.0), "", Curve::Linear),
        param(TUNE, "tune", (-24.0, 24.0, 0.0), "st", Curve::Linear),
        param(
            DIRECTION,
            "direction",
            (0.0, 1.0, 0.0),
            "",
            Curve::Choice(DIRECTIONS),
        ),
        param(RELEASE, "release", (0.002, 1.0, 0.005), "s", Curve::Log),
        param(GAIN, "gain", (0.0, 1.0, 0.7), "", Curve::Linear),
    ];

    /// The piece `pitch` plays, of `pieces`.
    pub fn piece(pitch: u8, pieces: usize) -> usize {
        (i32::from(pitch) - i32::from(DRUM_ROOT)).rem_euclid(pieces.max(1) as i32) as usize
    }

    /// The note that plays piece `piece`.
    pub fn pitch(piece: usize) -> u8 {
        (usize::from(DRUM_ROOT) + piece).min(127) as u8
    }
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
