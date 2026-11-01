//! Instruments and effects as a song stores them: a kind, and a value for each of its
//! parameters. The descriptions of each kind's parameters live here, so the editor and the engine
//! agree on them.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// How a parameter's range is laid out, for bars and for nudging it up and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    Linear,
    /// Evenly spread by ratio rather than difference, as for frequencies.
    Log,
    /// A whole number, nudged by one, as for a count.
    Whole,
    /// One of these, by index from 0, as for a waveform. The range is 0 to one less than the
    /// number of choices.
    Choice(&'static [&'static str]),
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

/// A [`ParamInfo`] in one line, for the tables of them.
pub(crate) const fn param(
    id: u32,
    name: &'static str,
    (min, max, default): (f32, f32, f32),
    unit: &'static str,
    curve: Curve,
) -> ParamInfo {
    ParamInfo {
        id,
        name,
        min,
        max,
        default,
        unit,
        curve,
    }
}

/// Nudges in a parameter's whole range.
const NUDGES: f32 = 100.0;

impl ParamInfo {
    pub fn clamp(&self, value: f32) -> f32 {
        let value = value.clamp(self.min, self.max);
        match self.curve {
            Curve::Choice(_) | Curve::Whole => value.round(),
            _ => value,
        }
    }

    /// Where `value` lies in the range, from 0 to 1.
    pub fn fraction(&self, value: f32) -> f32 {
        let f = match self.curve {
            Curve::Linear | Curve::Choice(_) | Curve::Whole => {
                (value - self.min) / (self.max - self.min)
            }
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
            Curve::Choice(_) | Curve::Whole => (self.min + f * (self.max - self.min)).round(),
        }
    }

    /// `value` moved `by` nudges, each a hundredth of the range or one choice or count, within
    /// the range.
    pub fn nudge(&self, value: f32, by: i32) -> f32 {
        match self.curve {
            Curve::Choice(_) | Curve::Whole => self.clamp(value + by as f32),
            _ => self.at(self.fraction(value) + by as f32 / NUDGES),
        }
    }

    /// The value with its unit, to a sensible precision: `0.375s`, `4000Hz`, `0.45`.
    pub fn format(&self, value: f32) -> String {
        if let Curve::Choice(names) = self.curve {
            let i = (self.clamp(value) - self.min) as usize;
            return names.get(i).map_or_else(String::new, |n| n.to_string());
        }
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

/// A kind of instrument or effect.
pub trait Kind: Copy + PartialEq + std::fmt::Debug + Serialize + DeserializeOwned {
    /// What it's called on the command line and in the mixer.
    fn name(self) -> &'static str;

    fn params(self) -> &'static [ParamInfo];
}

/// An instrument or effect, and its settings.
///
/// Saved with only the settings that differ from their defaults, by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "Sparse<K>",
    into = "Sparse<K>",
    bound(serialize = "K: Kind", deserialize = "K: Kind")
)]
pub struct Device<K: Kind> {
    pub kind: K,
    /// One per parameter, indexed by [`ParamInfo::id`].
    pub values: Vec<f32>,
    /// The file it plays, for a sampler.
    pub file: Option<PathBuf>,
}

impl<K: Kind> Device<K> {
    /// The device with every parameter at its default.
    pub fn new(kind: K) -> Self {
        Self {
            kind,
            values: kind.params().iter().map(|p| p.default).collect(),
            file: None,
        }
    }

    pub fn params(&self) -> &'static [ParamInfo] {
        self.kind.params()
    }

    /// The value of the parameter with this id.
    pub fn get(&self, id: u32) -> f32 {
        self.values[id as usize]
    }
}

#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "K: Kind", deserialize = "K: Kind"))]
struct Sparse<K: Kind> {
    kind: K,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    params: BTreeMap<String, f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file: Option<PathBuf>,
}

impl<K: Kind> From<Device<K>> for Sparse<K> {
    fn from(device: Device<K>) -> Self {
        Self {
            kind: device.kind,
            params: device
                .params()
                .iter()
                .zip(&device.values)
                .filter(|(p, v)| p.default != **v)
                .map(|(p, &v)| (p.name.to_string(), v))
                .collect(),
            file: device.file,
        }
    }
}

impl<K: Kind> TryFrom<Sparse<K>> for Device<K> {
    type Error = String;

    fn try_from(sparse: Sparse<K>) -> Result<Self, String> {
        let mut device = Device::new(sparse.kind);
        device.file = sparse.file;
        let kind = sparse.kind.name();
        for (name, value) in sparse.params {
            let param = device
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
            device.values[param.id as usize] = value;
        }
        Ok(device)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::{FxKind, delay};

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
        let [time, feedback, tone, _] = *FxKind::Delay.params() else {
            unreachable!()
        };
        assert_eq!(time.format(0.375), "0.375s");
        assert_eq!(time.format(1.5), "1.5s");
        assert_eq!(feedback.format(0.45), "0.45");
        assert_eq!(tone.format(4_000.0), "4000Hz");
        assert_eq!(tone.format(12.5), "12.5Hz");
    }

    #[test]
    fn choices_step_one_at_a_time() {
        let wave = ParamInfo {
            id: 0,
            name: "wave",
            min: 0.0,
            max: 2.0,
            default: 0.0,
            unit: "",
            curve: Curve::Choice(&["saw", "square", "sine"]),
        };
        assert_eq!(wave.nudge(0.0, 1), 1.0);
        assert_eq!(wave.nudge(1.0, 5), 2.0);
        assert_eq!(wave.format(1.0), "square");
        assert_eq!(wave.clamp(1.4), 1.0);
    }
}
