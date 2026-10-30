//! Effects as a song stores them: which effect, and its settings. The engine builds the
//! processors from these; the descriptions of each effect's parameters live here so the editor
//! and the engine agree on them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::song::MAX_DB;

/// Effects in one chain. The engine preallocates for this.
pub const MAX_EFFECTS: usize = 8;

/// How a parameter's range is laid out, for bars and for nudging it up and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    Linear,
    /// Evenly spread by ratio rather than difference, as for frequencies.
    Log,
}

/// One control on an instrument or effect, as in CLAP: values are in the parameter's own units,
/// not normalised.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamInfo {
    /// Stable within the processor, and its index in [`Fx::values`].
    pub id: u32,
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    /// Shown after the value: `s`, `Hz`, or empty.
    pub unit: &'static str,
    pub curve: Curve,
}

/// Nudges in a parameter's whole range.
const NUDGES: f32 = 100.0;

impl ParamInfo {
    pub fn clamp(&self, value: f32) -> f32 {
        value.clamp(self.min, self.max)
    }

    /// Where `value` lies in the range, from 0 to 1.
    pub fn fraction(&self, value: f32) -> f32 {
        let f = match self.curve {
            Curve::Linear => (value - self.min) / (self.max - self.min),
            Curve::Log => (value / self.min).ln() / (self.max / self.min).ln(),
        };
        f.clamp(0.0, 1.0)
    }

    /// The value `fraction` of the way through the range.
    pub fn at(&self, fraction: f32) -> f32 {
        let f = fraction.clamp(0.0, 1.0);
        match self.curve {
            Curve::Linear => self.min + f * (self.max - self.min),
            Curve::Log => self.min * (self.max / self.min).powf(f),
        }
    }

    /// `value` moved `by` nudges, each a hundredth of the range, within the range.
    pub fn nudge(&self, value: f32, by: i32) -> f32 {
        self.at(self.fraction(value) + by as f32 / NUDGES)
    }

    /// The value with its unit, to a sensible precision: `0.375s`, `4000Hz`, `0.45`.
    pub fn format(&self, value: f32) -> String {
        let digits = if value.abs() >= 100.0 {
            0
        } else if value.abs() >= 10.0 {
            1
        } else {
            3
        };
        let text = format!("{value:.digits$}");
        let text = if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.')
        } else {
            &text
        };
        format!("{text}{}", self.unit)
    }
}

/// The effects Lattice has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FxKind {
    Delay,
}

impl FxKind {
    pub const ALL: [FxKind; 1] = [FxKind::Delay];

    /// What it's called on the command line and in the mixer.
    pub fn name(self) -> &'static str {
        match self {
            FxKind::Delay => "delay",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    pub const fn params(self) -> &'static [ParamInfo] {
        match self {
            FxKind::Delay => &delay::PARAMS,
        }
    }
}

/// The delay's parameters.
pub mod delay {
    use super::{Curve, ParamInfo};

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
///
/// Saved with only the settings that differ from their defaults, by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SparseFx", into = "SparseFx")]
pub struct Fx {
    pub kind: FxKind,
    /// One per parameter, indexed by [`ParamInfo::id`].
    pub values: Vec<f32>,
}

impl Fx {
    /// The effect with every parameter at its default.
    pub fn new(kind: FxKind) -> Self {
        Self {
            kind,
            values: kind.params().iter().map(|p| p.default).collect(),
        }
    }

    pub fn params(&self) -> &'static [ParamInfo] {
        self.kind.params()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename = "Fx")]
struct SparseFx {
    kind: FxKind,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    params: BTreeMap<String, f32>,
}

impl From<Fx> for SparseFx {
    fn from(fx: Fx) -> Self {
        Self {
            kind: fx.kind,
            params: fx
                .params()
                .iter()
                .zip(&fx.values)
                .filter(|(p, v)| p.default != **v)
                .map(|(p, &v)| (p.name.to_string(), v))
                .collect(),
        }
    }
}

impl TryFrom<SparseFx> for Fx {
    type Error = String;

    fn try_from(sparse: SparseFx) -> Result<Self, String> {
        let mut fx = Fx::new(sparse.kind);
        let kind = sparse.kind.name();
        for (name, value) in sparse.params {
            let param = fx
                .params()
                .iter()
                .find(|p| p.name == name)
                .ok_or_else(|| format!("the {kind} has no {name:?} setting"))?;
            if !(param.min..=param.max).contains(&value) {
                return Err(format!(
                    "the {kind}'s {name} goes from {} to {}, not {value}",
                    param.min, param.max
                ));
            }
            fx.values[param.id as usize] = value;
        }
        Ok(fx)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_params_spread_by_ratio() {
        let tone = FxKind::Delay.params()[delay::TONE as usize];
        assert!((tone.at(0.5) - (200.0f32 * 16_000.0).sqrt()).abs() < 0.5);
        assert!((tone.fraction(tone.at(0.3)) - 0.3).abs() < 1e-5);
        assert_eq!(tone.nudge(tone.max, 5), tone.max, "clamped");
        let feedback = FxKind::Delay.params()[delay::FEEDBACK as usize];
        assert!((feedback.nudge(0.45, 1) - 0.4595).abs() < 1e-5);
        assert_eq!(feedback.nudge(0.0, -3), 0.0);
    }

    #[test]
    fn values_format_with_units() {
        let [time, feedback, tone, _] = delay::PARAMS;
        assert_eq!(time.format(0.375), "0.375s");
        assert_eq!(time.format(1.5), "1.5s");
        assert_eq!(feedback.format(0.45), "0.45");
        assert_eq!(tone.format(4_000.0), "4000Hz");
        assert_eq!(tone.format(12.5), "12.5Hz");
    }
}
