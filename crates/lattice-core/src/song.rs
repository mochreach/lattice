use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::transform::Rule;

/// Longest a track can be. The engine preallocates for this so edits never allocate on the audio
/// thread.
pub const MAX_STEPS: usize = 256;

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
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    pub name: String,
    pub steps_per_beat: u32,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Song {
    pub name: String,
    pub bpm: f64,
    pub grids: Vec<Grid>,
    /// Arrangement: indices into `grids`, played in order.
    pub order: Vec<usize>,
}

impl Song {
    /// Where in `grids` the arrangement starts.
    pub fn first_grid(&self) -> usize {
        self.order.first().copied().unwrap_or(0)
    }
}
