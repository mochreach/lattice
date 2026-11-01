//! Effects as a song stores them: which effect, and its settings. The engine builds the
//! processors from these; the descriptions of each effect's parameters live here so the editor
//! and the engine agree on them.

use serde::{Deserialize, Serialize};

use crate::chain::{FxChain, FxLane};
use crate::device::{Curve, Device, Kind, ParamInfo, param};
use crate::song::MAX_DB;

/// Effects in one chain. The engine preallocates for this.
pub const MAX_EFFECTS: usize = 8;

/// The effects Lattice has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FxKind {
    Delay,
    Drive,
    Comp,
    Reverb,
    Filter,
    Pitch,
}

impl FxKind {
    pub const ALL: [FxKind; 6] = [
        FxKind::Delay,
        FxKind::Reverb,
        FxKind::Filter,
        FxKind::Drive,
        FxKind::Comp,
        FxKind::Pitch,
    ];

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    pub const fn params(self) -> &'static [ParamInfo] {
        match self {
            FxKind::Delay => &delay::PARAMS,
            FxKind::Drive => &drive::PARAMS,
            FxKind::Comp => &comp::PARAMS,
            FxKind::Reverb => &reverb::PARAMS,
            FxKind::Filter => &filter::PARAMS,
            FxKind::Pitch => &pitch::PARAMS,
        }
    }
}

impl Kind for FxKind {
    fn name(self) -> &'static str {
        match self {
            FxKind::Delay => "delay",
            FxKind::Drive => "drive",
            FxKind::Comp => "comp",
            FxKind::Reverb => "reverb",
            FxKind::Filter => "filter",
            FxKind::Pitch => "pitch",
        }
    }

    fn params(self) -> &'static [ParamInfo] {
        FxKind::params(self)
    }
}

/// The delay's parameters.
pub mod delay {
    use crate::device::{Curve, ParamInfo};

    pub const TIME: u32 = 0;
    pub const FEEDBACK: u32 = 1;
    pub const TONE: u32 = 2;
    pub const MIX: u32 = 3;
    /// The longest delay time, in seconds.
    pub const MAX_TIME: f32 = 2.0;

    pub(super) const PARAMS: [ParamInfo; 4] = [
        ParamInfo {
            id: TIME,
            name: "time",
            min: 0.01,
            max: MAX_TIME,
            default: 0.375,
            unit: "s",
            curve: Curve::Log,
        },
        ParamInfo {
            id: FEEDBACK,
            name: "feedback",
            min: 0.0,
            max: 0.95,
            default: 0.45,
            unit: "",
            curve: Curve::Linear,
        },
        ParamInfo {
            id: TONE,
            name: "tone",
            min: 200.0,
            max: 16_000.0,
            default: 4_000.0,
            unit: "Hz",
            curve: Curve::Log,
        },
        ParamInfo {
            id: MIX,
            name: "mix",
            min: 0.0,
            max: 1.0,
            default: 0.3,
            unit: "",
            curve: Curve::Linear,
        },
    ];
}

/// A saturator: soft clipping, a little off centre for even harmonics, then a low-pass to tame
/// the fizz.
pub mod drive {
    use super::*;

    /// Gain into the clipper, from clean to crushed.
    pub const DRIVE: u32 = 0;
    pub const TONE: u32 = 1;
    pub const MIX: u32 = 2;
    /// Out, to make up for or rein in what the drive adds.
    pub const LEVEL: u32 = 3;

    pub(super) const PARAMS: [ParamInfo; 4] = [
        param(DRIVE, "drive", (0.0, 1.0, 0.3), "", Curve::Linear),
        param(TONE, "tone", (500.0, 16_000.0, 8_000.0), "Hz", Curve::Log),
        param(MIX, "mix", (0.0, 1.0, 1.0), "", Curve::Linear),
        param(LEVEL, "level", (-24.0, 6.0, 0.0), "dB", Curve::Linear),
    ];
}

/// A compressor, both channels together. `mix` below 1 blends the dry signal back in, for
/// parallel compression.
pub mod comp {
    use super::*;

    pub const THRESHOLD: u32 = 0;
    pub const RATIO: u32 = 1;
    pub const ATTACK: u32 = 2;
    pub const RELEASE: u32 = 3;
    /// Gain after compressing.
    pub const MAKEUP: u32 = 4;
    pub const MIX: u32 = 5;

    pub(super) const PARAMS: [ParamInfo; 6] = [
        param(
            THRESHOLD,
            "threshold",
            (-48.0, 0.0, -18.0),
            "dB",
            Curve::Linear,
        ),
        param(RATIO, "ratio", (1.0, 20.0, 4.0), "", Curve::Log),
        param(ATTACK, "attack", (0.000_1, 0.1, 0.005), "s", Curve::Log),
        param(RELEASE, "release", (0.01, 1.0, 0.15), "s", Curve::Log),
        param(MAKEUP, "makeup", (0.0, 24.0, 0.0), "dB", Curve::Linear),
        param(MIX, "mix", (0.0, 1.0, 1.0), "", Curve::Linear),
    ];
}

/// A plate reverb, after Jon Dattorro's: a diffused input feeding a figure-of-eight tank.
pub mod reverb {
    use super::*;

    /// How big the plate is, which sets how far apart its echoes are.
    pub const SIZE: u32 = 0;
    /// How long it rings: how much survives each trip round the tank.
    pub const DECAY: u32 = 1;
    /// How much of the high end survives each trip.
    pub const TONE: u32 = 2;
    /// A gap before the reverb, keeping the sound's front clear of it.
    pub const PREDELAY: u32 = 3;
    pub const MIX: u32 = 4;

    /// The longest pre-delay, in seconds.
    pub const MAX_PREDELAY: f32 = 0.25;

    pub(super) const PARAMS: [ParamInfo; 5] = [
        param(SIZE, "size", (0.0, 1.0, 0.5), "", Curve::Linear),
        param(DECAY, "decay", (0.0, 0.98, 0.6), "", Curve::Linear),
        param(TONE, "tone", (500.0, 16_000.0, 6_000.0), "Hz", Curve::Log),
        param(
            PREDELAY,
            "predelay",
            (0.0, MAX_PREDELAY, 0.02),
            "s",
            Curve::Linear,
        ),
        param(MIX, "mix", (0.0, 1.0, 0.3), "", Curve::Linear),
    ];
}

/// A resonant filter, swept by its own LFO.
pub mod filter {
    use super::*;

    /// Low-pass, band-pass or high-pass.
    pub const TYPE: u32 = 0;
    pub const CUTOFF: u32 = 1;
    pub const RESONANCE: u32 = 2;
    /// How fast the LFO sweeps the cutoff.
    pub const RATE: u32 = 3;
    /// How far, in octaves either side of the cutoff.
    pub const SWEEP: u32 = 4;
    pub const MIX: u32 = 5;

    pub const TYPES: &[&str] = &["low", "band", "high"];

    pub(super) const PARAMS: [ParamInfo; 6] = [
        param(TYPE, "type", (0.0, 2.0, 0.0), "", Curve::Choice(TYPES)),
        param(
            CUTOFF,
            "cutoff",
            (20.0, 20_000.0, 2_000.0),
            "Hz",
            Curve::Log,
        ),
        param(RESONANCE, "resonance", (0.0, 0.95, 0.2), "", Curve::Linear),
        param(RATE, "rate", (0.02, 20.0, 0.25), "Hz", Curve::Log),
        param(SWEEP, "sweep", (0.0, 4.0, 0.0), "oct", Curve::Linear),
        param(MIX, "mix", (0.0, 1.0, 1.0), "", Curve::Linear),
    ];
}

/// A pitch shifter: the sound read back from a delay line faster or slower, by two heads half a
/// window apart, each faded in and out as it sweeps across the window.
pub mod pitch {
    use super::*;

    /// How far, in semitones: 12 is an octave up.
    pub const PITCH: u32 = 0;
    /// And in cents, to detune it a little.
    pub const FINE: u32 = 1;
    /// How long each head's sweep is: shorter is tighter but buzzier, longer smoother but
    /// smeared.
    pub const WINDOW: u32 = 2;
    pub const MIX: u32 = 3;

    /// The longest window, in seconds.
    pub const MAX_WINDOW: f32 = 0.2;

    pub(super) const PARAMS: [ParamInfo; 4] = [
        param(PITCH, "pitch", (-24.0, 24.0, 12.0), "st", Curve::Whole),
        param(FINE, "fine", (-50.0, 50.0, 0.0), "ct", Curve::Linear),
        param(WINDOW, "window", (0.02, MAX_WINDOW, 0.08), "s", Curve::Log),
        param(MIX, "mix", (0.0, 1.0, 1.0), "", Curve::Linear),
    ];
}

/// An effect in a chain, and its settings.
pub type Fx = Device<FxKind>;

/// The send or master bus: a level and a chain of effects.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SparseBus", into = "SparseBus")]
pub struct Bus {
    /// In dB; at [`crate::MIN_DB`] or below, silent.
    pub level: f32,
    pub effects: FxChain,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct SparseBus {
    #[serde(skip_serializing_if = "is_zero")]
    level: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    effects: Vec<Fx>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    lanes: Vec<FxLane>,
}

impl From<Bus> for SparseBus {
    fn from(bus: Bus) -> Self {
        let (effects, lanes) = bus.effects.into_parts();
        Self {
            level: bus.level,
            effects,
            lanes,
        }
    }
}

impl TryFrom<SparseBus> for Bus {
    type Error = String;

    fn try_from(sparse: SparseBus) -> Result<Self, String> {
        Ok(Self {
            level: sparse.level,
            effects: FxChain::from_parts(sparse.effects, sparse.lanes)?,
        })
    }
}

fn is_zero(x: &f32) -> bool {
    *x == 0.0
}

impl Bus {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub(crate) fn check(&self, name: &str) -> Result<(), String> {
        if !(self.level.is_finite() && self.level <= MAX_DB) {
            return Err(format!("the {name} bus's level goes up to {MAX_DB} dB"));
        }
        self.effects
            .check()
            .map_err(|e| format!("the {name} bus's effects: {e}"))
    }
}

/// The song's buses: every track sends to the send bus, and everything ends in the master.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Buses {
    #[serde(skip_serializing_if = "Bus::is_default")]
    pub send: Bus,
    #[serde(skip_serializing_if = "Bus::is_default")]
    pub master: Bus,
}

impl Buses {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}
