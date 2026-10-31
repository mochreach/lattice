use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fx::{Buses, Fx};
use crate::group::Group;
use crate::instrument::Instrument;
use crate::transform::Rule;

/// Longest a track can be. The engine preallocates for this so edits never allocate on the audio
/// thread.
pub const MAX_STEPS: usize = 256;

/// Most tracks a grid can have. The engine preallocates for this too, so tracks can be added
/// while playing.
pub const MAX_TRACKS: usize = 16;

const DEFAULT_VELOCITY: u8 = 100;

/// A note to play on a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// MIDI note number, 0–127.
    pub pitch: u8,
    /// MIDI velocity, 0–127.
    #[serde(
        default = "default_velocity",
        skip_serializing_if = "is_default_velocity"
    )]
    pub velocity: u8,
    /// Length in steps.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub length: u16,
}

fn default_velocity() -> u8 {
    DEFAULT_VELOCITY
}

fn is_default_velocity(velocity: &u8) -> bool {
    *velocity == DEFAULT_VELOCITY
}

fn one() -> u16 {
    1
}

fn is_one(n: &u16) -> bool {
    *n == 1
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<Note>,
}

impl Step {
    pub fn note(pitch: u8) -> Self {
        Self {
            note: Some(Note {
                pitch,
                velocity: DEFAULT_VELOCITY,
                length: 1,
            }),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One instrument's line of steps. Tracks in a grid may differ in length (polymeter).
///
/// Saved sparsely: a length, and only the steps that aren't empty, by index.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SparseTrack", into = "SparseTrack")]
pub struct Track {
    pub name: String,
    pub steps: Vec<Step>,
    /// Transforms applied, in order, to realise the track for each pass.
    pub rules: Vec<Rule>,
    /// What plays the notes.
    pub instrument: Instrument,
    /// Effects after the instrument, before the mixer strip.
    pub effects: Vec<Fx>,
    pub mix: Mix,
    /// Its choke group within its track group, 1 to [`MAX_CHOKE`], or 0 for none: a hit on any
    /// track in the same group with the same choke cuts this one off, as a closed hat stops an
    /// open one.
    pub choke: u8,
}

/// The highest choke group.
pub const MAX_CHOKE: u8 = 8;

/// Quietest level in decibels; at or below it, a level or send is silent.
pub const MIN_DB: f32 = -60.0;
/// Loudest level in decibels.
pub const MAX_DB: f32 = 6.0;

/// How a track sits in the mix: its channel strip.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mix {
    /// Level in decibels, [`MIN_DB`] (silent) to [`MAX_DB`].
    #[serde(skip_serializing_if = "is_zero")]
    pub level: f32,
    /// -1 is hard left, 1 hard right.
    #[serde(skip_serializing_if = "is_zero")]
    pub pan: f32,
    /// Level sent to the send bus after the track's level and pan, in decibels; [`MIN_DB`]
    /// sends nothing.
    #[serde(skip_serializing_if = "is_min_db")]
    pub send: f32,
    #[serde(skip_serializing_if = "is_false")]
    pub mute: bool,
    /// While any track is soloed, only soloed tracks are heard.
    #[serde(skip_serializing_if = "is_false")]
    pub solo: bool,
}

impl Default for Mix {
    fn default() -> Self {
        Self {
            level: 0.0,
            pan: 0.0,
            send: MIN_DB,
            mute: false,
            solo: false,
        }
    }
}

impl Mix {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Decibels to a linear gain, silent at or below [`MIN_DB`].
pub fn db_to_gain(db: f32) -> f32 {
    if db <= MIN_DB {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

fn is_zero(x: &f32) -> bool {
    *x == 0.0
}

fn is_min_db(x: &f32) -> bool {
    *x <= MIN_DB
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// How a [`Track`] is written to a file.
#[derive(Serialize, Deserialize)]
#[serde(rename = "Track")]
struct SparseTrack {
    name: String,
    length: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    steps: BTreeMap<usize, Step>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rules: Vec<Rule>,
    #[serde(default)]
    instrument: Instrument,
    #[serde(default, skip_serializing_if = "Mix::is_default")]
    mix: Mix,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    effects: Vec<Fx>,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    choke: u8,
}

fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

impl From<Track> for SparseTrack {
    fn from(track: Track) -> Self {
        Self {
            name: track.name,
            length: track.steps.len(),
            steps: track
                .steps
                .into_iter()
                .enumerate()
                .filter(|(_, step)| !step.is_empty())
                .collect(),
            rules: track.rules,
            instrument: track.instrument,
            mix: track.mix,
            effects: track.effects,
            choke: track.choke,
        }
    }
}

impl TryFrom<SparseTrack> for Track {
    type Error = String;

    fn try_from(sparse: SparseTrack) -> Result<Self, String> {
        if !(1..=MAX_STEPS).contains(&sparse.length) {
            return Err(format!(
                "track {:?} has {} steps; tracks have 1 to {MAX_STEPS}",
                sparse.name, sparse.length
            ));
        }
        let mut steps = vec![Step::default(); sparse.length];
        for (index, step) in sparse.steps {
            let slot = steps.get_mut(index).ok_or_else(|| {
                format!(
                    "track {:?} has a step at {index}, past its length of {}",
                    sparse.name, sparse.length
                )
            })?;
            *slot = step;
        }
        Ok(Self {
            name: sparse.name,
            steps,
            rules: sparse.rules,
            instrument: sparse.instrument,
            mix: sparse.mix,
            effects: sparse.effects,
            choke: sparse.choke,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    pub name: String,
    pub steps_per_beat: u32,
    pub tracks: Vec<Track>,
    /// In track order, never overlapping. Change tracks through [`Grid::insert_track`] and its
    /// siblings, which keep these in line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Song {
    pub name: String,
    pub bpm: f64,
    pub grids: Vec<Grid>,
    /// Arrangement: indices into `grids`, played in order.
    pub order: Vec<usize>,
    #[serde(default, skip_serializing_if = "Buses::is_default")]
    pub buses: Buses,
}

impl Song {
    /// Where in `grids` the arrangement starts.
    pub fn first_grid(&self) -> usize {
        self.order.first().copied().unwrap_or(0)
    }
}
