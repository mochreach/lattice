//! Macros: knobs on a track that each turn several of its settings at once, of its instrument
//! or its effects. A macro goes from 0 to 1, and each setting it turns has a depth, the share of
//! the setting's range it moves by at 1, up or down. So a setting's value is its own, moved by
//! every macro that turns it, along the setting's curve: turning the setting by hand still
//! works, and the macros move it from there.
//!
//! A step can lock a macro as it can a setting: a [`Lock`] whose `fx` is [`MACRO`], whose
//! `param` is the macro's index.

use serde::{Deserialize, Serialize};

use crate::device::{Curve, ParamInfo};
use crate::song::Lock;

/// Most macros a track has.
pub const MAX_MACROS: usize = 8;
/// Most settings one macro turns.
pub const MAX_TARGETS: usize = 8;
/// What a lock's `fx` is when it locks a macro rather than a setting.
pub const MACRO: u8 = 0xFE;

/// A macro's own value, as a setting, for bars and nudging.
pub const MACRO_INFO: ParamInfo = ParamInfo {
    id: 0,
    name: "macro",
    min: 0.0,
    max: 1.0,
    default: 0.0,
    unit: "",
    curve: Curve::Linear,
};

/// A target's depth, as a setting.
pub const DEPTH_INFO: ParamInfo = ParamInfo {
    id: 0,
    name: "depth",
    min: -1.0,
    max: 1.0,
    default: 0.5,
    unit: "",
    curve: Curve::Linear,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    /// From 0 to 1.
    #[serde(default)]
    pub value: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<Target>,
}

impl Macro {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            value: 0.0,
            targets: vec![],
        }
    }
}

/// A setting a macro turns.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Target {
    /// The effect, by its place in the track's chain, or `None` for the instrument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fx: Option<u8>,
    pub param: u8,
    /// How far the macro at 1 moves the setting, as a share of its range, from -1 to 1.
    pub depth: f32,
}

impl Target {
    pub fn is(&self, fx: Option<u8>, param: u8) -> bool {
        self.fx == fx && self.param == param
    }
}

impl Lock {
    /// A lock of macro `index` at `value`.
    pub fn of_macro(index: usize, value: f32) -> Self {
        Self {
            fx: Some(MACRO),
            param: index as u8,
            value,
        }
    }

    pub fn is_macro(&self) -> bool {
        self.fx == Some(MACRO)
    }
}

/// How far the macros move the setting `param` of `fx`, as a share of its range, with each
/// macro at `value(index)`.
pub fn offset(macros: &[Macro], value: impl Fn(usize) -> f32, fx: Option<u8>, param: u8) -> f32 {
    let mut offset = 0.0;
    for (i, m) in macros.iter().enumerate() {
        for target in m.targets.iter().filter(|t| t.is(fx, param)) {
            offset += value(i) * target.depth;
        }
    }
    offset
}

/// Whether any macro turns the setting `param` of `fx`.
pub fn turned(macros: &[Macro], fx: Option<u8>, param: u8) -> bool {
    (macros.iter()).any(|m| m.targets.iter().any(|t| t.is(fx, param)))
}

/// `base` moved by `offset`, a share of the range, along the setting's curve.
pub fn moved(info: &ParamInfo, base: f32, offset: f32) -> f32 {
    if offset == 0.0 {
        base
    } else {
        info.at(info.fraction(base) + offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FxKind;

    #[test]
    fn macros_move_settings_along_their_curves() {
        let mut bloom = Macro::new("bloom");
        bloom.targets.push(Target {
            fx: Some(0),
            param: 1,
            depth: 0.5,
        });
        bloom.targets.push(Target {
            fx: None,
            param: 1,
            depth: -1.0,
        });
        let macros = [bloom];
        assert_eq!(offset(&macros, |_| 0.5, Some(0), 1), 0.25);
        assert_eq!(offset(&macros, |_| 1.0, None, 1), -1.0);
        assert_eq!(offset(&macros, |_| 1.0, Some(1), 1), 0.0);

        // A cutoff, on a log curve: a quarter of the range up from its bottom is a quarter of the
        // way by ratio, and moves stop at the ends.
        let cutoff = *FxKind::Filter
            .params()
            .iter()
            .find(|p| p.name == "cutoff")
            .unwrap();
        let quarter = moved(&cutoff, cutoff.min, 0.25);
        assert!((cutoff.fraction(quarter) - 0.25).abs() < 1e-4);
        assert_eq!(moved(&cutoff, cutoff.max, 0.5), cutoff.max);
        assert_eq!(moved(&cutoff, 1000.0, 0.0), 1000.0);
    }
}
