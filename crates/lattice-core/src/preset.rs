//! Presets: an instrument made into one of its own, with a name and a face. A preset is the
//! instrument and its settings, the chain of effects after it, and the macros that turn them,
//! which `:preset` puts on a track all at once, in place of its own.
//!
//! The face is what the instrument page shows of it first: its title, art drawn from that, and
//! the controls it puts forward, its macros and whichever of the instrument's settings it chose.
//! A track keeps its face, so the song does, until its instrument changes for another kind.

use serde::{Deserialize, Serialize};

use crate::chain::{FxChain, FxLane};
use crate::fx::{Fx, FxKind};
use crate::instrument::{InstKind, Instrument};
use crate::macros::{MAX_MACROS, MAX_TARGETS, Macro, Target};

/// The most of the instrument's settings a face puts forward, beside its macros.
pub const MAX_CONTROLS: usize = 6;

/// How a preset shows itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub title: String,
    /// The instrument's settings it puts forward, by id, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<u8>,
}

impl Face {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.into(),
            controls: vec![],
        }
    }

    /// Checks it fits an instrument of `kind`.
    pub fn check(&self, kind: InstKind) -> Result<(), String> {
        if self.controls.len() > MAX_CONTROLS {
            return Err(format!("a face has at most {MAX_CONTROLS} controls"));
        }
        let params = kind.params().len();
        match self.controls.iter().find(|&&c| usize::from(c) >= params) {
            Some(c) => Err(format!("{} has no setting {c}", crate::Kind::name(kind))),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub face: Face,
    pub instrument: Instrument,
    pub effects: FxChain,
    pub macros: Vec<Macro>,
}

impl Preset {
    /// Checks a track can take it.
    pub fn check(&self) -> Result<(), String> {
        self.face.check(self.instrument.kind)?;
        self.effects.check()?;
        if self.macros.len() > MAX_MACROS {
            return Err(format!("at most {MAX_MACROS} macros"));
        }
        let params = self.instrument.params().len();
        for m in &self.macros {
            if m.targets.len() > MAX_TARGETS {
                return Err(format!("macro {} turns more than {MAX_TARGETS}", m.name));
            }
            for t in &m.targets {
                let there = match t.fx {
                    None => usize::from(t.param) < params,
                    Some(fx) => (self.effects.get(usize::from(fx)))
                        .is_some_and(|fx| usize::from(t.param) < fx.params().len()),
                };
                if !there {
                    return Err(format!("macro {} turns a setting that isn't there", m.name));
                }
            }
        }
        Ok(())
    }
}

/// A preset as a file holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedPreset {
    pub face: Face,
    pub instrument: Instrument,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Fx>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lanes: Vec<FxLane>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros: Vec<Macro>,
}

impl From<Preset> for SavedPreset {
    fn from(preset: Preset) -> Self {
        let (effects, lanes) = preset.effects.into_parts();
        Self {
            face: preset.face,
            instrument: preset.instrument,
            effects,
            lanes,
            macros: preset.macros,
        }
    }
}

impl TryFrom<SavedPreset> for Preset {
    type Error = String;

    fn try_from(saved: SavedPreset) -> Result<Self, String> {
        let preset = Preset {
            face: saved.face,
            instrument: saved.instrument,
            effects: FxChain::from_parts(saved.effects, saved.lanes)?,
            macros: saved.macros,
        };
        preset.check()?;
        Ok(preset)
    }
}

/// The presets Lattice comes with.
pub const PRESETS: &[&str] = &["glasshouse", "night-bus", "rubber", "foundry", "monsoon"];

/// A preset Lattice comes with, by name.
pub fn preset(name: &str) -> Option<Preset> {
    use crate::fx::{delay, drive, filter, reverb};
    use crate::instrument::{kick, string, synth};

    let inst = |kind, values: &[(u32, f32)]| {
        let mut instrument = Instrument::new(kind);
        for &(id, value) in values {
            instrument.values[id as usize] = value;
        }
        instrument
    };
    let fx = |kind, values: &[(u32, f32)]| {
        let mut fx = Fx::new(kind);
        for &(id, value) in values {
            fx.values[id as usize] = value;
        }
        fx
    };
    let mac = |name: &str, value, targets: &[(Option<u8>, u32, f32)]| Macro {
        name: name.into(),
        value,
        targets: (targets.iter())
            .map(|&(fx, param, depth)| Target {
                fx,
                param: param as u8,
                depth,
            })
            .collect(),
    };
    let face = |title: &str, controls: &[u32]| Face {
        title: title.into(),
        controls: controls.iter().map(|&c| c as u8).collect(),
    };
    let preset = match name {
        // A steel string, plucked bright near the bridge, in a large glassy room.
        "glasshouse" => Preset {
            face: face("Glasshouse", &[string::PLUCK, string::DECAY]),
            instrument: inst(
                InstKind::String,
                &[
                    (string::MODEL, 1.0),
                    (string::PLUCK, 0.75),
                    (string::POSITION, 0.08),
                    (string::DECAY, 5.0),
                    (string::TONE, 0.8),
                ],
            ),
            effects: vec![fx(
                FxKind::Reverb,
                &[
                    (reverb::SIZE, 0.75),
                    (reverb::DECAY, 0.7),
                    (reverb::MIX, 0.3),
                ],
            )]
            .into(),
            macros: vec![
                mac(
                    "bloom",
                    0.2,
                    &[(Some(0), reverb::MIX, 0.5), (Some(0), reverb::DECAY, 0.25)],
                ),
                mac(
                    "bow",
                    0.0,
                    &[(None, string::BOW, 0.8), (None, string::PLUCK, -0.4)],
                ),
            ],
        },
        // A slow saw pad, dark until it's opened, in Lattice's shimmer.
        "night-bus" => {
            let mut effects = crate::chain::built_in_chain("shimmer").expect("there's a shimmer");
            effects[0].values[reverb::MIX as usize] = 0.45;
            Preset {
                face: face("Night Bus", &[synth::ATTACK, synth::RELEASE]),
                instrument: inst(
                    InstKind::Synth,
                    &[
                        (synth::ATTACK, 0.6),
                        (synth::DECAY, 1.5),
                        (synth::SUSTAIN, 0.8),
                        (synth::RELEASE, 2.0),
                        (synth::CUTOFF, 500.0),
                        (synth::ENV, 800.0),
                        (synth::RESONANCE, 0.2),
                    ],
                ),
                effects,
                macros: vec![
                    mac(
                        "dawn",
                        0.0,
                        &[(None, synth::CUTOFF, 0.45), (None, synth::ENV, 0.2)],
                    ),
                    mac("haze", 0.3, &[(Some(0), reverb::MIX, 0.5)]),
                ],
            }
        }
        // A square bass that squelches as it's opened, a little driven.
        "rubber" => Preset {
            face: face("Rubber", &[synth::DECAY]),
            instrument: inst(
                InstKind::Synth,
                &[
                    (synth::WAVE, 1.0),
                    (synth::DECAY, 0.25),
                    (synth::SUSTAIN, 0.3),
                    (synth::RELEASE, 0.05),
                    (synth::CUTOFF, 250.0),
                    (synth::ENV, 1_500.0),
                    (synth::RESONANCE, 0.6),
                    (synth::GAIN, 0.3),
                ],
            ),
            effects: vec![fx(FxKind::Drive, &[(drive::DRIVE, 0.3), (drive::MIX, 0.6)])].into(),
            macros: vec![
                mac(
                    "squelch",
                    0.3,
                    &[(None, synth::ENV, 0.5), (None, synth::RESONANCE, 0.3)],
                ),
                mac("grit", 0.0, &[(Some(0), drive::DRIVE, 0.6)]),
            ],
        },
        // A hard kick, driven, with a low filter to darken it.
        "foundry" => Preset {
            face: face("Foundry", &[kick::TUNE, kick::DECAY]),
            instrument: inst(
                InstKind::Kick,
                &[
                    (kick::MODEL, 2.0),
                    (kick::PUNCH, 0.7),
                    (kick::DECAY, 0.6),
                    (kick::DRIVE, 0.5),
                ],
            ),
            effects: vec![fx(
                FxKind::Filter,
                &[(filter::CUTOFF, 6_000.0), (filter::RESONANCE, 0.2)],
            )]
            .into(),
            macros: vec![
                mac(
                    "weight",
                    0.0,
                    &[(None, kick::DECAY, 0.4), (None, kick::TUNE, -0.2)],
                ),
                mac(
                    "heat",
                    0.2,
                    &[(None, kick::DRIVE, 0.5), (Some(0), filter::CUTOFF, -0.3)],
                ),
            ],
        },
        // A sitar, in a dotted echo that thickens as the rain comes on.
        "monsoon" => Preset {
            face: face("Monsoon", &[string::POSITION]),
            instrument: inst(
                InstKind::String,
                &[
                    (string::MODEL, 2.0),
                    (string::DECAY, 4.0),
                    (string::TONE, 0.7),
                ],
            ),
            effects: vec![
                fx(
                    FxKind::Delay,
                    &[(delay::FEEDBACK, 0.35), (delay::MIX, 0.25)],
                ),
                fx(FxKind::Reverb, &[(reverb::MIX, 0.2)]),
            ]
            .into(),
            macros: vec![mac(
                "rain",
                0.0,
                &[
                    (Some(0), delay::FEEDBACK, 0.4),
                    (Some(0), delay::MIX, 0.3),
                    (Some(1), reverb::MIX, 0.3),
                ],
            )],
        },
        _ => return None,
    };
    Some(preset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_presets_are_whole() {
        for name in PRESETS {
            let preset = preset(name).unwrap();
            assert_eq!(preset.check(), Ok(()), "{name}");
            let saved = SavedPreset::from(preset.clone());
            assert_eq!(Preset::try_from(saved), Ok(preset));
        }
        assert!(preset("nothing").is_none());
    }

    #[test]
    fn a_face_fits_its_instrument() {
        let face = Face {
            title: "x".into(),
            controls: vec![8],
        };
        assert!(face.check(InstKind::Synth).is_ok());
        assert!(face.check(InstKind::Kick).is_err());
    }
}
