//! Presets: an instrument made into one of its own, with a name and a face. A preset is the
//! instrument and its settings, the chain of effects after it, and the macros that turn them,
//! which `:preset` puts on a track all at once, in place of its own.
//!
//! The face is what the instrument page shows of it first: its title, its art, drawn by hand or
//! else from the title, and the controls it puts forward, its macros and whichever of the
//! instrument's settings it chose.
//! A track keeps its face, so the song does, until its instrument changes for another kind.

use serde::{Deserialize, Serialize};

use crate::chain::{FxChain, FxLane};
use crate::fx::{Fx, FxKind};
use crate::instrument::{InstKind, Instrument};
use crate::macros::{Macro, Target, check_macros};

/// The most of the instrument's settings a face puts forward, beside its macros.
pub const MAX_CONTROLS: usize = 6;
/// The most rows and characters a row a frame of a face's hand-drawn art has, as much as the
/// instrument page has room for at most, and the most frames it has.
pub const ART_ROWS: usize = 8;
pub const ART_WIDTH: usize = 31;
pub const ART_FRAMES: usize = 8;

/// How a preset shows itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub title: String,
    /// The instrument's settings it puts forward, by id, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<u8>,
    /// Art drawn by hand, a row of text a line, in place of the art the title picks: one frame,
    /// or several, each after an empty row, that the track steps through as it plays.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub art: Vec<String>,
}

impl Face {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.into(),
            controls: vec![],
            art: vec![],
        }
    }

    /// The frames of its art: its rows, split at each empty one.
    pub fn frames(&self) -> impl Iterator<Item = &[String]> {
        (self.art.split(String::is_empty)).filter(|frame| !frame.is_empty())
    }

    /// Checks it fits an instrument of `kind`.
    pub fn check(&self, kind: InstKind) -> Result<(), String> {
        if self.controls.len() > MAX_CONTROLS {
            return Err(format!("a face has at most {MAX_CONTROLS} controls"));
        }
        if self.frames().count() > ART_FRAMES {
            return Err(format!("a face's art has at most {ART_FRAMES} frames"));
        }
        if self.frames().any(|frame| frame.len() > ART_ROWS) {
            return Err(format!(
                "a frame of a face's art has at most {ART_ROWS} rows"
            ));
        }
        for row in &self.art {
            if row.chars().count() > ART_WIDTH {
                return Err(format!(
                    "a face's art is at most {ART_WIDTH} characters wide"
                ));
            }
            if row.chars().any(char::is_control) {
                return Err("a face's art can't have tabs or other control characters".into());
            }
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
        check_macros(&self.macros, &self.instrument, &self.effects)
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

/// The presets Lattice comes with, each in the folder of its instrument's engine: `tuss/flim`.
pub const PRESETS: &[&str] = &[
    "synth/night-bus",
    "synth/paper-lanterns",
    "synth/cathedral",
    "synth/sodium",
    "synth/wasp",
    "synth/acid-rain",
    "synth/rubber",
    "synth/sub-zero",
    "monochord/glasshouse",
    "monochord/dust-harp",
    "monochord/cello-ghost",
    "monochord/tidewater",
    "monochord/monsoon",
    "kick/foundry",
    "kick/velvet-boom",
    "snare/tin-roof",
    "snare/brushfire",
    "hat/cicada",
    "hat/brass-bell",
    "clap/crowd-pleaser",
    "tom/thunderhead",
    "tom/hollow-log",
    "krell/great-machine",
    "tuss/tregaskin",
    "tuss/flim",
    "tuss/suitcase",
    "tuss/music-box",
    "tuss/hollow-tine",
    "tuss/monologue",
    "tuss/pentagon",
];

/// A preset Lattice comes with, by name, folder and all: `tuss/flim`.
pub fn preset(name: &str) -> Option<Preset> {
    use crate::fx::{comp, delay, drive, ensemble, filter, phaser, pitch, reverb};
    use crate::instrument::{clap, hat, kick, krell, monochord, snare, synth, tom, tuss};

    let inst = |kind, values: &[(u32, f32)]| {
        let mut instrument = Instrument::new(kind);
        // The ids are the kind's own, so each is there to set.
        for &(id, value) in values {
            instrument.set(id, value);
        }
        instrument
    };
    let fx = |kind, values: &[(u32, f32)]| {
        let mut fx = Fx::new(kind);
        for &(id, value) in values {
            fx.set(id, value);
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
        art: vec![],
    };
    // The Tuss presets' faces: its tine.
    let tine = || crate::art::engine_face(InstKind::Tuss).art;
    let preset = match name {
        // A steel string, plucked bright near the bridge, in a large glassy room.
        "monochord/glasshouse" => Preset {
            face: face("Glasshouse", &[monochord::PLUCK, monochord::DECAY]),
            instrument: inst(
                InstKind::Monochord,
                &[
                    (monochord::MODEL, 1.0),
                    (monochord::PLUCK, 0.75),
                    (monochord::POSITION, 0.08),
                    (monochord::DECAY, 5.0),
                    (monochord::TONE, 0.8),
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
                    &[(None, monochord::BOW, 0.8), (None, monochord::PLUCK, -0.4)],
                ),
            ],
        },
        // A slow saw pad, dark until it's opened, in Lattice's shimmer.
        "synth/night-bus" => {
            let mut effects = crate::chain::built_in_chain("shimmer")?;
            effects.first_mut()?.set(reverb::MIX, 0.45);
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
        "synth/rubber" => Preset {
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
        "kick/foundry" => Preset {
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
        "monochord/monsoon" => Preset {
            face: face("Monsoon", &[monochord::POSITION]),
            instrument: inst(
                InstKind::Monochord,
                &[
                    (monochord::MODEL, 2.0),
                    (monochord::DECAY, 4.0),
                    (monochord::TONE, 0.7),
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
        // A 303's squelch: a saw through a filter that snaps open on each note, dripping echoes.
        "synth/acid-rain" => Preset {
            face: face("Acid Rain", &[synth::CUTOFF, synth::DECAY]),
            instrument: inst(
                InstKind::Synth,
                &[
                    (synth::DECAY, 0.3),
                    (synth::SUSTAIN, 0.0),
                    (synth::RELEASE, 0.05),
                    (synth::CUTOFF, 300.0),
                    (synth::ENV, 4_000.0),
                    (synth::RESONANCE, 0.85),
                    (synth::GAIN, 0.25),
                ],
            ),
            effects: vec![
                fx(FxKind::Drive, &[(drive::DRIVE, 0.4), (drive::MIX, 0.5)]),
                fx(
                    FxKind::Delay,
                    &[
                        (delay::TIME, 0.3),
                        (delay::FEEDBACK, 0.4),
                        (delay::MIX, 0.2),
                    ],
                ),
            ]
            .into(),
            macros: vec![
                mac(
                    "acid",
                    0.2,
                    &[(None, synth::ENV, 0.4), (None, synth::RESONANCE, 0.1)],
                ),
                mac(
                    "drip",
                    0.0,
                    &[(Some(1), delay::MIX, 0.4), (Some(1), delay::FEEDBACK, 0.3)],
                ),
            ],
        },
        // A square organ held in a stone nave, its reverb as long as the building.
        "synth/cathedral" => Preset {
            face: face("Cathedral", &[synth::ATTACK, synth::RELEASE]),
            instrument: inst(
                InstKind::Synth,
                &[
                    (synth::WAVE, 1.0),
                    (synth::ATTACK, 0.02),
                    (synth::SUSTAIN, 1.0),
                    (synth::RELEASE, 1.5),
                    (synth::CUTOFF, 3_000.0),
                    (synth::ENV, 0.0),
                    (synth::RESONANCE, 0.1),
                ],
            ),
            effects: vec![fx(
                FxKind::Reverb,
                &[
                    (reverb::SIZE, 0.95),
                    (reverb::DECAY, 0.9),
                    (reverb::PREDELAY, 0.05),
                    (reverb::MIX, 0.45),
                ],
            )]
            .into(),
            macros: vec![
                mac(
                    "nave",
                    0.3,
                    &[(Some(0), reverb::MIX, 0.4), (Some(0), reverb::TONE, -0.2)],
                ),
                mac("light", 0.0, &[(None, synth::CUTOFF, 0.3)]),
            ],
        },
        // A bright plucked lead, orange as a streetlight, trailing echoes.
        "synth/sodium" => Preset {
            face: face("Sodium Lamp", &[synth::CUTOFF, synth::DECAY]),
            instrument: inst(
                InstKind::Synth,
                &[
                    (synth::DECAY, 0.15),
                    (synth::SUSTAIN, 0.2),
                    (synth::RELEASE, 0.2),
                    (synth::CUTOFF, 1_200.0),
                    (synth::ENV, 5_000.0),
                    (synth::RESONANCE, 0.5),
                ],
            ),
            effects: vec![fx(
                FxKind::Delay,
                &[
                    (delay::TIME, 0.25),
                    (delay::FEEDBACK, 0.5),
                    (delay::MIX, 0.35),
                ],
            )]
            .into(),
            macros: vec![
                mac(
                    "glow",
                    0.0,
                    &[(None, synth::CUTOFF, 0.3), (None, synth::ENV, 0.2)],
                ),
                mac(
                    "trail",
                    0.2,
                    &[(Some(0), delay::FEEDBACK, 0.4), (Some(0), delay::MIX, 0.3)],
                ),
            ],
        },
        // A low, round bass under everything, squeezed even.
        "synth/sub-zero" => Preset {
            face: face("Sub Zero", &[synth::RELEASE]),
            instrument: inst(
                InstKind::Synth,
                &[
                    (synth::DECAY, 0.5),
                    (synth::SUSTAIN, 1.0),
                    (synth::RELEASE, 0.08),
                    (synth::CUTOFF, 120.0),
                    (synth::ENV, 200.0),
                    (synth::RESONANCE, 0.0),
                    (synth::GAIN, 0.35),
                ],
            ),
            effects: vec![fx(FxKind::Comp, &[(comp::RATIO, 6.0), (comp::MAKEUP, 4.0)])].into(),
            macros: vec![
                mac(
                    "growl",
                    0.0,
                    &[(None, synth::CUTOFF, 0.35), (None, synth::ENV, 0.2)],
                ),
                mac("thaw", 0.0, &[(None, synth::RESONANCE, 0.5)]),
            ],
        },
        // A slow pad drifting through a swaying band-pass, as if lit from inside.
        "synth/paper-lanterns" => Preset {
            face: face("Paper Lanterns", &[synth::ATTACK, synth::RELEASE]),
            instrument: inst(
                InstKind::Synth,
                &[
                    (synth::ATTACK, 1.2),
                    (synth::DECAY, 2.0),
                    (synth::SUSTAIN, 0.7),
                    (synth::RELEASE, 3.0),
                    (synth::CUTOFF, 900.0),
                    (synth::ENV, 600.0),
                    (synth::GAIN, 0.4),
                ],
            ),
            effects: vec![
                fx(
                    FxKind::Filter,
                    &[
                        (filter::TYPE, 1.0),
                        (filter::CUTOFF, 1_200.0),
                        (filter::RATE, 0.08),
                        (filter::SWEEP, 2.0),
                        (filter::MIX, 0.5),
                    ],
                ),
                fx(FxKind::Reverb, &[(reverb::SIZE, 0.8), (reverb::MIX, 0.4)]),
            ]
            .into(),
            macros: vec![
                mac(
                    "drift",
                    0.2,
                    &[(Some(0), filter::SWEEP, 0.4), (Some(0), filter::RATE, 0.2)],
                ),
                mac(
                    "air",
                    0.0,
                    &[(Some(1), reverb::MIX, 0.4), (Some(1), reverb::DECAY, 0.2)],
                ),
            ],
        },
        // A square lead, driven till it buzzes.
        "synth/wasp" => Preset {
            face: face("Wasp", &[synth::CUTOFF, synth::RESONANCE]),
            instrument: inst(
                InstKind::Synth,
                &[
                    (synth::WAVE, 1.0),
                    (synth::DECAY, 0.4),
                    (synth::SUSTAIN, 0.6),
                    (synth::CUTOFF, 2_500.0),
                    (synth::ENV, 3_000.0),
                    (synth::RESONANCE, 0.4),
                ],
            ),
            effects: vec![fx(
                FxKind::Drive,
                &[
                    (drive::DRIVE, 0.7),
                    (drive::TONE, 6_000.0),
                    (drive::LEVEL, -6.0),
                ],
            )]
            .into(),
            macros: vec![
                mac(
                    "sting",
                    0.0,
                    &[(Some(0), drive::DRIVE, 0.3), (None, synth::RESONANCE, 0.3)],
                ),
                mac(
                    "swarm",
                    0.0,
                    &[(None, synth::CUTOFF, 0.3), (Some(0), drive::TONE, 0.2)],
                ),
            ],
        },
        // A soft nylon string, long and dark, in a far-off room.
        "monochord/dust-harp" => Preset {
            face: face("Dust Harp", &[monochord::PLUCK, monochord::TONE]),
            instrument: inst(
                InstKind::Monochord,
                &[
                    (monochord::PLUCK, 0.3),
                    (monochord::POSITION, 0.25),
                    (monochord::DECAY, 8.0),
                    (monochord::TONE, 0.4),
                ],
            ),
            effects: vec![
                fx(
                    FxKind::Delay,
                    &[
                        (delay::TIME, 0.5),
                        (delay::FEEDBACK, 0.3),
                        (delay::TONE, 1_500.0),
                        (delay::MIX, 0.15),
                    ],
                ),
                fx(FxKind::Reverb, &[(reverb::SIZE, 0.85), (reverb::MIX, 0.35)]),
            ]
            .into(),
            macros: vec![
                mac(
                    "settle",
                    0.0,
                    &[(None, monochord::TONE, -0.3), (None, monochord::DECAY, 0.2)],
                ),
                mac(
                    "distance",
                    0.2,
                    &[(Some(1), reverb::MIX, 0.4), (Some(0), delay::MIX, 0.2)],
                ),
            ],
        },
        // A bowed string, breathy and long, haunting the room an octave up.
        "monochord/cello-ghost" => Preset {
            face: face("Cello Ghost", &[monochord::POSITION, monochord::BOW]),
            instrument: inst(
                InstKind::Monochord,
                &[
                    (monochord::PLUCK, 0.2),
                    (monochord::POSITION, 0.12),
                    (monochord::DECAY, 12.0),
                    (monochord::TONE, 0.45),
                    (monochord::BOW, 0.8),
                ],
            ),
            effects: vec![
                fx(
                    FxKind::Pitch,
                    &[
                        (pitch::PITCH, 12.0),
                        (pitch::WINDOW, 0.12),
                        (pitch::MIX, 0.0),
                    ],
                ),
                fx(FxKind::Reverb, &[(reverb::SIZE, 0.7), (reverb::MIX, 0.35)]),
            ]
            .into(),
            macros: vec![
                mac(
                    "breath",
                    0.0,
                    &[(None, monochord::BOW, 0.2), (None, monochord::TONE, 0.2)],
                ),
                mac(
                    "haunt",
                    0.0,
                    &[(Some(0), pitch::MIX, 0.4), (Some(1), reverb::MIX, 0.3)],
                ),
            ],
        },
        // A steel string plucked hard at the bridge, twanging off a slapback echo.
        "monochord/tidewater" => Preset {
            face: face("Tidewater", &[monochord::POSITION, monochord::DECAY]),
            instrument: inst(
                InstKind::Monochord,
                &[
                    (monochord::MODEL, 1.0),
                    (monochord::PLUCK, 0.9),
                    (monochord::POSITION, 0.04),
                    (monochord::DECAY, 2.0),
                    (monochord::TONE, 0.9),
                ],
            ),
            effects: vec![fx(
                FxKind::Delay,
                &[
                    (delay::TIME, 0.12),
                    (delay::FEEDBACK, 0.2),
                    (delay::MIX, 0.25),
                ],
            )]
            .into(),
            macros: vec![
                mac(
                    "twang",
                    0.0,
                    &[
                        (None, monochord::POSITION, -0.1),
                        (None, monochord::PLUCK, 0.1),
                    ],
                ),
                mac("slap", 0.3, &[(Some(0), delay::MIX, 0.4)]),
            ],
        },
        // An 808's long, soft boom, glued with a compressor.
        "kick/velvet-boom" => Preset {
            face: face("Velvet Boom", &[kick::TUNE, kick::DECAY]),
            instrument: inst(
                InstKind::Kick,
                &[
                    (kick::TUNE, 45.0),
                    (kick::PUNCH, 0.4),
                    (kick::DECAY, 1.4),
                    (kick::DRIVE, 0.2),
                ],
            ),
            effects: vec![fx(FxKind::Comp, &[(comp::RATIO, 3.0), (comp::MAKEUP, 3.0)])].into(),
            macros: vec![
                mac("length", 0.0, &[(None, kick::DECAY, 0.2)]),
                mac("growl", 0.0, &[(None, kick::DRIVE, 0.6)]),
            ],
        },
        // A 909's crack off a tin roof: bright, short, in a small room.
        "snare/tin-roof" => Preset {
            face: face("Tin Roof", &[snare::TUNE, snare::SNAP]),
            instrument: inst(
                InstKind::Snare,
                &[
                    (snare::MODEL, 2.0),
                    (snare::TUNE, 240.0),
                    (snare::DECAY, 0.18),
                    (snare::SNAP, 0.8),
                    (snare::TONE, 8_000.0),
                ],
            ),
            effects: vec![fx(
                FxKind::Reverb,
                &[(reverb::SIZE, 0.3), (reverb::MIX, 0.25)],
            )]
            .into(),
            macros: vec![
                mac(
                    "ring",
                    0.0,
                    &[(None, snare::DECAY, 0.4), (None, snare::TONE, 0.2)],
                ),
                mac(
                    "room",
                    0.2,
                    &[(Some(0), reverb::MIX, 0.4), (Some(0), reverb::SIZE, 0.3)],
                ),
            ],
        },
        // A snare all wires and no shell, dulled like brushes.
        "snare/brushfire" => Preset {
            face: face("Brushfire", &[snare::DECAY, snare::TONE]),
            instrument: inst(
                InstKind::Snare,
                &[
                    (snare::MODEL, 0.0),
                    (snare::DECAY, 0.4),
                    (snare::SNAP, 1.0),
                    (snare::TONE, 3_000.0),
                ],
            ),
            effects: vec![fx(FxKind::Filter, &[(filter::CUTOFF, 4_000.0)])].into(),
            macros: vec![
                mac("sweep", 0.0, &[(None, snare::DECAY, 0.3)]),
                mac(
                    "flare",
                    0.0,
                    &[(Some(0), filter::CUTOFF, 0.3), (None, snare::TONE, 0.3)],
                ),
            ],
        },
        // A hat of almost all noise, ticking, with a sweep through it like a swarm.
        "hat/cicada" => Preset {
            face: face("Cicada", &[hat::DECAY, hat::TONE]),
            instrument: inst(
                InstKind::Hat,
                &[
                    (hat::MODEL, 2.0),
                    (hat::TUNE, 400.0),
                    (hat::DECAY, 0.03),
                    (hat::TONE, 11_000.0),
                    (hat::NOISE, 0.7),
                ],
            ),
            effects: vec![fx(
                FxKind::Filter,
                &[
                    (filter::TYPE, 1.0),
                    (filter::CUTOFF, 8_000.0),
                    (filter::RESONANCE, 0.5),
                    (filter::RATE, 4.0),
                    (filter::MIX, 0.6),
                ],
            )]
            .into(),
            macros: vec![
                mac("chirp", 0.0, &[(None, hat::DECAY, 0.3)]),
                mac(
                    "swarm",
                    0.0,
                    &[(Some(0), filter::SWEEP, 0.4), (Some(0), filter::RATE, 0.2)],
                ),
            ],
        },
        // The hat's metal, tuned low and left to ring like a bell, echoing.
        "hat/brass-bell" => Preset {
            face: face("Brass Bell", &[hat::TUNE, hat::DECAY]),
            instrument: inst(
                InstKind::Hat,
                &[
                    (hat::MODEL, 0.0),
                    (hat::TUNE, 320.0),
                    (hat::DECAY, 0.8),
                    (hat::TONE, 4_000.0),
                    (hat::NOISE, 0.1),
                ],
            ),
            effects: vec![fx(
                FxKind::Delay,
                &[
                    (delay::TIME, 0.45),
                    (delay::FEEDBACK, 0.35),
                    (delay::MIX, 0.2),
                ],
            )]
            .into(),
            macros: vec![
                mac(
                    "toll",
                    0.0,
                    &[(None, hat::DECAY, 0.3), (Some(0), delay::FEEDBACK, 0.2)],
                ),
                mac(
                    "tarnish",
                    0.0,
                    &[(None, hat::NOISE, 0.4), (None, hat::TONE, -0.2)],
                ),
            ],
        },
        // A clap from a crowd of hands, loose and roomy.
        "clap/crowd-pleaser" => Preset {
            face: face("Crowd Pleaser", &[clap::DECAY, clap::TUNE]),
            instrument: inst(
                InstKind::Clap,
                &[
                    (clap::MODEL, 2.0),
                    (clap::DECAY, 0.6),
                    (clap::SPREAD, 0.02),
                    (clap::ROOM, 0.8),
                ],
            ),
            effects: vec![fx(
                FxKind::Reverb,
                &[(reverb::SIZE, 0.6), (reverb::MIX, 0.25)],
            )]
            .into(),
            macros: vec![
                mac(
                    "crowd",
                    0.3,
                    &[(None, clap::SPREAD, 0.3), (None, clap::ROOM, 0.2)],
                ),
                mac(
                    "hall",
                    0.0,
                    &[(Some(0), reverb::MIX, 0.4), (Some(0), reverb::SIZE, 0.3)],
                ),
            ],
        },
        // A low tom that drops and rolls on, in a storm of a reverb.
        "tom/thunderhead" => Preset {
            face: face("Thunderhead", &[tom::TUNE, tom::BEND]),
            instrument: inst(
                InstKind::Tom,
                &[
                    (tom::TUNE, 70.0),
                    (tom::DECAY, 1.2),
                    (tom::BEND, 0.7),
                    (tom::NOISE, 0.3),
                ],
            ),
            effects: vec![
                fx(FxKind::Drive, &[(drive::DRIVE, 0.4), (drive::MIX, 0.5)]),
                fx(
                    FxKind::Reverb,
                    &[
                        (reverb::SIZE, 0.9),
                        (reverb::DECAY, 0.8),
                        (reverb::MIX, 0.3),
                    ],
                ),
            ]
            .into(),
            macros: vec![
                mac(
                    "roll",
                    0.0,
                    &[(None, tom::BEND, 0.3), (None, tom::DECAY, 0.2)],
                ),
                mac(
                    "storm",
                    0.0,
                    &[(Some(1), reverb::MIX, 0.4), (Some(0), drive::DRIVE, 0.3)],
                ),
            ],
        },
        // A short, high, hard tom: a knock on a hollow log.
        "tom/hollow-log" => Preset {
            face: face("Hollow Log", &[tom::TUNE, tom::DECAY]),
            instrument: inst(
                InstKind::Tom,
                &[
                    (tom::MODEL, 2.0),
                    (tom::TUNE, 300.0),
                    (tom::DECAY, 0.12),
                    (tom::BEND, 0.1),
                    (tom::NOISE, 0.0),
                ],
            ),
            effects: vec![fx(
                FxKind::Reverb,
                &[(reverb::SIZE, 0.2), (reverb::MIX, 0.15)],
            )]
            .into(),
            macros: vec![
                mac(
                    "knock",
                    0.0,
                    &[(None, tom::TUNE, 0.2), (None, tom::DECAY, -0.1)],
                ),
                mac(
                    "hollow",
                    0.0,
                    &[(None, tom::DECAY, 0.2), (Some(0), reverb::MIX, 0.3)],
                ),
            ],
        },
        // Krell, the harpsichord of Forbidden Planet's lost race: all five choirs drifting apart
        // over a glass body, echoing down the Great Machine's halls. Its face is the machine.
        "krell/great-machine" => Preset {
            face: Face {
                art: crate::art::engine_face(InstKind::Krell).art,
                ..face(
                    "The Great Machine",
                    &[krell::STOPS, krell::DRIFT, krell::GLASS],
                )
            },
            instrument: inst(
                InstKind::Krell,
                &[
                    (krell::STOPS, 4.0),
                    (krell::DRIFT, 0.3),
                    (krell::QUILL, 0.85),
                    (krell::DECAY, 4.0),
                    (krell::DAMPER, 0.8),
                    (krell::BODY, 0.5),
                    (krell::HULL, 196.0),
                    (krell::GLASS, 0.75),
                    (krell::GAIN, 0.8),
                ],
            ),
            effects: vec![
                fx(
                    FxKind::Delay,
                    &[
                        (delay::TIME, 0.375),
                        (delay::FEEDBACK, 0.35),
                        (delay::TONE, 4000.0),
                        (delay::MIX, 0.2),
                    ],
                ),
                fx(
                    FxKind::Reverb,
                    &[
                        (reverb::SIZE, 0.95),
                        (reverb::DECAY, 0.85),
                        (reverb::MIX, 0.3),
                    ],
                ),
            ]
            .into(),
            macros: vec![
                mac(
                    "alien",
                    0.0,
                    &[(None, krell::DRIFT, 0.6), (None, krell::GLASS, 0.25)],
                ),
                mac(
                    "halls",
                    0.0,
                    &[(Some(1), reverb::MIX, 0.4), (Some(0), delay::FEEDBACK, 0.3)],
                ),
            ],
        },
        // Tuss, named for the Tuss's made-up Cornish siblings: a warm electric piano on a worn
        // tape, wavering through a chorus into a soft room. Its face is the tine.
        "tuss/tregaskin" => Preset {
            face: Face {
                art: tine(),
                ..face("Tregaskin", &[tuss::STRIKE, tuss::BARK, tuss::DRIFT])
            },
            instrument: inst(
                InstKind::Tuss,
                &[
                    (tuss::STRIKE, 0.6),
                    (tuss::BELL, 0.5),
                    (tuss::PICKUP, 0.75),
                    (tuss::BARK, 0.35),
                    (tuss::DECAY, 6.0),
                    (tuss::DAMPER, 0.6),
                    (tuss::DRIFT, 0.25),
                    (tuss::GAIN, 0.7),
                ],
            ),
            effects: vec![
                fx(
                    FxKind::Ensemble,
                    &[
                        (ensemble::RATE, 0.4),
                        (ensemble::DEPTH, 0.35),
                        (ensemble::MIX, 0.4),
                    ],
                ),
                fx(
                    FxKind::Reverb,
                    &[
                        (reverb::SIZE, 0.5),
                        (reverb::DECAY, 0.5),
                        (reverb::MIX, 0.2),
                    ],
                ),
            ]
            .into(),
            macros: vec![
                mac(
                    "worn",
                    0.0,
                    &[(None, tuss::DRIFT, 0.5), (Some(0), ensemble::DEPTH, 0.4)],
                ),
                mac(
                    "growl",
                    0.0,
                    &[(None, tuss::BARK, 0.5), (None, tuss::STRIKE, 0.3)],
                ),
            ],
        },
        // Soft and glassy, close up: a felt hammer, the bell only just there, a short room,
        // for a quiet phrase that rings on under a busy beat.
        "tuss/flim" => Preset {
            face: Face {
                art: tine(),
                ..face("Flim", &[tuss::STRIKE, tuss::BELL, tuss::DECAY])
            },
            instrument: inst(
                InstKind::Tuss,
                &[
                    (tuss::STRIKE, 0.25),
                    (tuss::BELL, 0.45),
                    (tuss::PICKUP, 0.8),
                    (tuss::BARK, 0.1),
                    (tuss::DECAY, 8.0),
                    (tuss::DAMPER, 0.5),
                    (tuss::DRIFT, 0.08),
                    (tuss::GAIN, 0.7),
                ],
            ),
            effects: vec![fx(
                FxKind::Reverb,
                &[
                    (reverb::SIZE, 0.3),
                    (reverb::DECAY, 0.4),
                    (reverb::MIX, 0.15),
                ],
            )]
            .into(),
            macros: vec![
                mac("glass", 0.0, &[(None, tuss::BELL, 0.5)]),
                mac(
                    "hush",
                    0.0,
                    &[(None, tuss::STRIKE, -0.25), (Some(0), reverb::MIX, 0.2)],
                ),
            ],
        },
        // The seventies stage piano: a hard hammer into a pickup driven till it barks, through
        // a phaser and a little drive.
        "tuss/suitcase" => Preset {
            face: Face {
                art: tine(),
                ..face("Suitcase", &[tuss::STRIKE, tuss::BARK, tuss::PICKUP])
            },
            instrument: inst(
                InstKind::Tuss,
                &[
                    (tuss::STRIKE, 0.65),
                    (tuss::BELL, 0.35),
                    (tuss::PICKUP, 0.55),
                    (tuss::BARK, 0.6),
                    (tuss::DECAY, 5.0),
                    (tuss::DAMPER, 0.75),
                    (tuss::DRIFT, 0.1),
                    (tuss::GAIN, 0.6),
                ],
            ),
            effects: vec![
                fx(
                    FxKind::Phaser,
                    &[
                        (phaser::RATE, 0.4),
                        (phaser::DEPTH, 0.6),
                        (phaser::FEEDBACK, 0.3),
                        (phaser::MIX, 0.5),
                    ],
                ),
                fx(FxKind::Drive, &[(drive::DRIVE, 0.25), (drive::MIX, 0.5)]),
            ]
            .into(),
            macros: vec![
                mac("bark", 0.0, &[(None, tuss::BARK, 0.4)]),
                mac(
                    "swirl",
                    0.0,
                    &[(Some(0), phaser::RATE, 0.3), (Some(0), phaser::MIX, 0.4)],
                ),
            ],
        },
        // A hard little hammer on short tines, the bell all the way up, dying quickly into a
        // long echo: a music box played through a tape delay.
        "tuss/music-box" => Preset {
            face: Face {
                art: tine(),
                ..face("Music Box", &[tuss::BELL, tuss::DECAY])
            },
            instrument: inst(
                InstKind::Tuss,
                &[
                    (tuss::STRIKE, 0.9),
                    (tuss::BELL, 1.0),
                    (tuss::PICKUP, 0.9),
                    (tuss::BARK, 0.1),
                    (tuss::DECAY, 1.2),
                    (tuss::DAMPER, 0.3),
                    (tuss::DRIFT, 0.05),
                    (tuss::GAIN, 0.6),
                ],
            ),
            effects: vec![
                fx(
                    FxKind::Delay,
                    &[
                        (delay::TIME, 0.375),
                        (delay::FEEDBACK, 0.55),
                        (delay::TONE, 3_000.0),
                        (delay::MIX, 0.35),
                    ],
                ),
                fx(FxKind::Reverb, &[(reverb::SIZE, 0.6), (reverb::MIX, 0.25)]),
            ]
            .into(),
            macros: vec![
                mac(
                    "chime",
                    0.0,
                    &[(None, tuss::STRIKE, 0.1), (None, tuss::DECAY, 0.3)],
                ),
                mac(
                    "echo",
                    0.0,
                    &[(Some(0), delay::FEEDBACK, 0.3), (Some(0), delay::MIX, 0.3)],
                ),
            ],
        },
        // The pickup in line with the tine, so each note sounds an octave up and hollow, far off
        // in a large room.
        "tuss/hollow-tine" => Preset {
            face: Face {
                art: tine(),
                ..face("Hollow Tine", &[tuss::PICKUP, tuss::STRIKE])
            },
            instrument: inst(
                InstKind::Tuss,
                &[
                    (tuss::STRIKE, 0.45),
                    (tuss::BELL, 0.3),
                    (tuss::PICKUP, 0.05),
                    (tuss::BARK, 0.25),
                    (tuss::DECAY, 10.0),
                    (tuss::DAMPER, 0.4),
                    (tuss::DRIFT, 0.12),
                    (tuss::GAIN, 0.75),
                ],
            ),
            effects: vec![fx(
                FxKind::Reverb,
                &[
                    (reverb::SIZE, 0.85),
                    (reverb::DECAY, 0.8),
                    (reverb::PREDELAY, 0.03),
                    (reverb::MIX, 0.4),
                ],
            )]
            .into(),
            macros: vec![
                mac("hollow", 0.0, &[(None, tuss::PICKUP, 0.6)]),
                mac(
                    "space",
                    0.0,
                    &[(Some(0), reverb::MIX, 0.4), (Some(0), reverb::DECAY, 0.15)],
                ),
            ],
        },
        // After the microtunings Aphex Twin made for Korg's monologue: tuned to the harmonic
        // series, its sevenths flat, each note drifting its own way, into an echo.
        "tuss/monologue" => Preset {
            face: Face {
                art: tine(),
                ..face("Monologue", &[tuss::DRIFT, tuss::TUNING, tuss::KEY])
            },
            instrument: inst(
                InstKind::Tuss,
                &[
                    (tuss::STRIKE, 0.5),
                    (tuss::BELL, 0.5),
                    (tuss::PICKUP, 0.7),
                    (tuss::BARK, 0.3),
                    (tuss::DECAY, 5.0),
                    (tuss::DAMPER, 0.6),
                    (tuss::DRIFT, 0.45),
                    (tuss::TUNING, 5.0),
                    (tuss::GAIN, 0.7),
                ],
            ),
            effects: vec![
                fx(
                    FxKind::Delay,
                    &[
                        (delay::TIME, 0.5),
                        (delay::FEEDBACK, 0.4),
                        (delay::TONE, 2_500.0),
                        (delay::MIX, 0.25),
                    ],
                ),
                fx(
                    FxKind::Ensemble,
                    &[
                        (ensemble::RATE, 0.3),
                        (ensemble::DEPTH, 0.3),
                        (ensemble::MIX, 0.3),
                    ],
                ),
            ]
            .into(),
            macros: vec![
                mac("wonk", 0.0, &[(None, tuss::DRIFT, 0.5)]),
                mac(
                    "tape",
                    0.0,
                    &[
                        (Some(1), ensemble::DEPTH, 0.5),
                        (Some(0), delay::TONE, -0.3),
                    ],
                ),
            ],
        },
        // A Rhodes down in the bass, barking hard, held level by a compressor.
        "tuss/pentagon" => Preset {
            face: Face {
                art: tine(),
                ..face("Pentagon", &[tuss::BARK, tuss::STRIKE, tuss::DAMPER])
            },
            instrument: inst(
                InstKind::Tuss,
                &[
                    (tuss::STRIKE, 0.6),
                    (tuss::BELL, 0.2),
                    (tuss::PICKUP, 0.6),
                    (tuss::BARK, 0.8),
                    (tuss::DECAY, 4.0),
                    (tuss::DAMPER, 0.85),
                    (tuss::DRIFT, 0.05),
                    (tuss::GAIN, 0.6),
                ],
            ),
            effects: vec![
                fx(
                    FxKind::Comp,
                    &[
                        (comp::THRESHOLD, -20.0),
                        (comp::RATIO, 4.0),
                        (comp::MAKEUP, 4.0),
                    ],
                ),
                fx(FxKind::Drive, &[(drive::DRIVE, 0.2), (drive::MIX, 0.4)]),
            ]
            .into(),
            macros: vec![
                mac(
                    "snarl",
                    0.0,
                    &[(None, tuss::BARK, 0.2), (Some(1), drive::DRIVE, 0.4)],
                ),
                mac(
                    "squash",
                    0.0,
                    &[
                        (Some(0), comp::RATIO, 0.4),
                        (Some(0), comp::THRESHOLD, -0.3),
                    ],
                ),
            ],
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
            let devices = std::iter::once((preset.instrument.params(), &preset.instrument.values))
                .chain(preset.effects.iter().map(|fx| (fx.params(), &fx.values)));
            for (params, values) in devices {
                for (p, &v) in params.iter().zip(values) {
                    assert_eq!(p.clamp(v), v, "{name}'s {} is out of range", p.name);
                }
            }
            let saved = SavedPreset::from(preset.clone());
            assert_eq!(Preset::try_from(saved), Ok(preset));
        }
        assert!(preset("nothing").is_none());
        assert!(preset("flim").is_none(), "only by its folder");
        for name in PRESETS {
            use crate::device::Kind;
            let kind = preset(name).unwrap().instrument.kind;
            assert_eq!(
                name.split_once('/').unwrap().0,
                kind.name(),
                "{name}'s folder"
            );
        }
        let titles: std::collections::HashSet<String> = (PRESETS.iter())
            .map(|n| preset(n).unwrap().face.title)
            .collect();
        assert_eq!(titles.len(), PRESETS.len(), "each has its own title");
    }

    #[test]
    fn a_face_fits_its_instrument() {
        let face = Face {
            title: "x".into(),
            controls: vec![8],
            art: vec![],
        };
        assert!(face.check(InstKind::Synth).is_ok());
        assert!(face.check(InstKind::Kick).is_err());
    }

    #[test]
    fn a_faces_art_fits_the_page() {
        let art = |rows: &[&str]| Face {
            art: rows.iter().map(|r| r.to_string()).collect(),
            ..Face::new("x")
        };
        assert!(art(&["⣿⣿", "⠿⠿"]).check(InstKind::Synth).is_ok());
        assert!(
            art(&[&"⣿".repeat(ART_WIDTH + 1)])
                .check(InstKind::Synth)
                .is_err()
        );
        assert!(art(&["⣿"; ART_ROWS + 1]).check(InstKind::Synth).is_err());
        // Frames, each after an empty row, and each as big as one.
        let frames = art(&["", "⣿", "⣿", "", "", "⠿", ""]);
        assert_eq!(frames.frames().count(), 2);
        let mut rows = vec!["⣿"; ART_ROWS];
        rows.push("");
        rows.extend(["⠿"; ART_ROWS]);
        assert!(art(&rows).check(InstKind::Synth).is_ok());
        assert!(
            art(&["⣿", ""].repeat(ART_FRAMES + 1))
                .check(InstKind::Synth)
                .is_err()
        );
        assert!(art(&["a\tb"]).check(InstKind::Synth).is_err());
        // The Great Machine wears Krell's.
        let machine = preset("krell/great-machine").unwrap().face;
        assert_eq!(machine.art, crate::art::engine_face(InstKind::Krell).art);
    }
}
