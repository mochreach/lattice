//! Effects as a song stores them: which effect, and its settings. The engine builds the
//! processors from these; the descriptions of each effect's parameters live here so the editor
//! and the engine agree on them.

use serde::{Deserialize, Serialize};

use crate::device::{Device, Kind, ParamInfo};
use crate::song::MAX_DB;

/// Effects in one chain. The engine preallocates for this.
pub const MAX_EFFECTS: usize = 8;

/// The effects Lattice has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FxKind {
    Delay,
}

impl FxKind {
    pub const ALL: [FxKind; 1] = [FxKind::Delay];

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    pub const fn params(self) -> &'static [ParamInfo] {
        match self {
            FxKind::Delay => &delay::PARAMS,
        }
    }
}

impl Kind for FxKind {
    fn name(self) -> &'static str {
        match self {
            FxKind::Delay => "delay",
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

/// An effect in a chain, and its settings.
pub type Fx = Device<FxKind>;

/// The send or master bus: a level and a chain of effects.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bus {
    /// In dB; at [`crate::MIN_DB`] or below, silent.
    #[serde(skip_serializing_if = "is_zero")]
    pub level: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Fx>,
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
        if self.effects.len() > MAX_EFFECTS {
            return Err(format!(
                "the {name} bus has more than {MAX_EFFECTS} effects"
            ));
        }
        Ok(())
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
