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
pub const PRESETS: &[&str] = &[
    // Synths.
    "night-bus",
    "paper-lanterns",
    "cathedral",
    "sodium",
    "wasp",
    "acid-rain",
    "rubber",
    "sub-zero",
    // Strings.
    "glasshouse",
    "dust-harp",
    "cello-ghost",
    "tidewater",
    "monsoon",
    // Drums.
    "foundry",
    "velvet-boom",
    "tin-roof",
    "brushfire",
    "cicada",
    "brass-bell",
    "crowd-pleaser",
    "thunderhead",
    "hollow-log",
];

/// A preset Lattice comes with, by name.
pub fn preset(name: &str) -> Option<Preset> {
    use crate::fx::{comp, delay, drive, filter, pitch, reverb};
    use crate::instrument::{clap, hat, kick, snare, string, synth, tom};

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
            let mut effects = crate::chain::built_in_chain("shimmer")?;
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
        // A 303's squelch: a saw through a filter that snaps open on each note, dripping echoes.
        "acid-rain" => Preset {
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
        "cathedral" => Preset {
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
        "sodium" => Preset {
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
        "sub-zero" => Preset {
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
        "paper-lanterns" => Preset {
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
        "wasp" => Preset {
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
        "dust-harp" => Preset {
            face: face("Dust Harp", &[string::PLUCK, string::TONE]),
            instrument: inst(
                InstKind::String,
                &[
                    (string::PLUCK, 0.3),
                    (string::POSITION, 0.25),
                    (string::DECAY, 8.0),
                    (string::TONE, 0.4),
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
                    &[(None, string::TONE, -0.3), (None, string::DECAY, 0.2)],
                ),
                mac(
                    "distance",
                    0.2,
                    &[(Some(1), reverb::MIX, 0.4), (Some(0), delay::MIX, 0.2)],
                ),
            ],
        },
        // A bowed string, breathy and long, haunting the room an octave up.
        "cello-ghost" => Preset {
            face: face("Cello Ghost", &[string::POSITION, string::BOW]),
            instrument: inst(
                InstKind::String,
                &[
                    (string::PLUCK, 0.2),
                    (string::POSITION, 0.12),
                    (string::DECAY, 12.0),
                    (string::TONE, 0.45),
                    (string::BOW, 0.8),
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
                    &[(None, string::BOW, 0.2), (None, string::TONE, 0.2)],
                ),
                mac(
                    "haunt",
                    0.0,
                    &[(Some(0), pitch::MIX, 0.4), (Some(1), reverb::MIX, 0.3)],
                ),
            ],
        },
        // A steel string plucked hard at the bridge, twanging off a slapback echo.
        "tidewater" => Preset {
            face: face("Tidewater", &[string::POSITION, string::DECAY]),
            instrument: inst(
                InstKind::String,
                &[
                    (string::MODEL, 1.0),
                    (string::PLUCK, 0.9),
                    (string::POSITION, 0.04),
                    (string::DECAY, 2.0),
                    (string::TONE, 0.9),
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
                    &[(None, string::POSITION, -0.1), (None, string::PLUCK, 0.1)],
                ),
                mac("slap", 0.3, &[(Some(0), delay::MIX, 0.4)]),
            ],
        },
        // An 808's long, soft boom, glued with a compressor.
        "velvet-boom" => Preset {
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
        "tin-roof" => Preset {
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
        "brushfire" => Preset {
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
        "cicada" => Preset {
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
        "brass-bell" => Preset {
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
        "crowd-pleaser" => Preset {
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
        "thunderhead" => Preset {
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
        "hollow-log" => Preset {
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
        };
        assert!(face.check(InstKind::Synth).is_ok());
        assert!(face.check(InstKind::Kick).is_err());
    }
}
