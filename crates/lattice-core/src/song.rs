use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::arrangement::Arrangement;
use crate::chain::{FxChain, FxLane};
use crate::fx::{Buses, Fx};
use crate::group::Group;
use crate::instrument::Instrument;
use crate::macros::Macro;
use crate::preset::Face;
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Step {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<Note>,
    /// Settings of the track's instrument or effects that change on this step.
    #[serde(default, skip_serializing_if = "Locks::is_empty")]
    pub locks: Locks,
}

impl Step {
    pub fn note(pitch: u8) -> Self {
        Self {
            note: Some(Note {
                pitch,
                velocity: DEFAULT_VELOCITY,
                length: 1,
            }),
            locks: Locks::default(),
        }
    }

    pub fn with_note(note: Note) -> Self {
        Self {
            note: Some(note),
            locks: Locks::default(),
        }
    }

    /// Whether the step changes what's playing: a note, or a lock. Locks hold until the next.
    pub fn triggers(&self) -> bool {
        self.note.is_some() || !self.locks.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Most settings a step can lock.
pub const MAX_LOCKS: usize = 4;

/// Most parameters an instrument or effect has, so players can keep each one's setting without
/// allocating.
pub const MAX_PARAMS: usize = 16;

/// A setting changed from one step: a parameter of the track's instrument, or of one of its
/// effects, at a value in the parameter's own units. It holds until the setting's next lock, as a
/// tracker's columns do, or until playing stops.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Lock {
    /// The effect, by its place in the track's chain, or `None` for the instrument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fx: Option<u8>,
    pub param: u8,
    pub value: f32,
    #[serde(default, skip_serializing_if = "Change::is_set")]
    pub change: Change,
}

/// How a [`Lock`] changes its setting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    /// To the lock's value, on its step.
    #[default]
    Set,
    /// To the lock's value gradually, from the setting's lock before, arriving on its step.
    Slide,
    /// Back to the setting's own value, from the mixer; the value is unused. Written `--`.
    Reset,
}

impl Change {
    fn is_set(&self) -> bool {
        *self == Self::Set
    }
}

impl Lock {
    /// Whether it's of the same setting as `other`, whatever the value.
    pub fn same_param(&self, other: &Lock) -> bool {
        self.fx == other.fx && self.param == other.param
    }

    /// The setting it's of.
    pub fn column(&self) -> LockColumn {
        LockColumn {
            fx: self.fx,
            param: self.param,
        }
    }
}

/// A setting a track shows as a column of locks down its steps: a parameter of its instrument
/// (`fx` is `None`) or of one of its effects, or a macro (`fx` is [`crate::MACRO`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockColumn {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fx: Option<u8>,
    pub param: u8,
}

impl LockColumn {
    /// A lock of this setting.
    pub fn lock(&self, value: f32, change: Change) -> Lock {
        Lock {
            fx: self.fx,
            param: self.param,
            value,
            change,
        }
    }
}

/// Up to [`MAX_LOCKS`] locks, kept in place so steps copy without allocating. Saved as a list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Vec<Lock>", into = "Vec<Lock>")]
pub struct Locks([Option<Lock>; MAX_LOCKS]);

impl Locks {
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(Option::is_none)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Lock> {
        self.0.iter().flatten()
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// The lock of the same setting as `lock`, if there is one.
    pub fn find(&self, lock: &Lock) -> Option<&Lock> {
        self.iter().find(|l| l.same_param(lock))
    }

    /// Sets `lock`, replacing one of the same setting. False if there's no room for it.
    pub fn set(&mut self, lock: Lock) -> bool {
        let slot = match self
            .0
            .iter()
            .position(|l| l.is_some_and(|l| l.same_param(&lock)))
        {
            Some(i) => i,
            None => match self.0.iter().position(Option::is_none) {
                Some(i) => i,
                None => return false,
            },
        };
        match self.0.get_mut(slot) {
            Some(slot) => {
                *slot = Some(lock);
                true
            }
            None => false,
        }
    }

    /// Keeps only the locks `keep` says to, letting it change them as it goes.
    pub fn retain(&mut self, mut keep: impl FnMut(&mut Lock) -> bool) {
        for slot in &mut self.0 {
            if slot.as_mut().is_some_and(|l| !keep(l)) {
                *slot = None;
            }
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

impl TryFrom<Vec<Lock>> for Locks {
    type Error = String;

    fn try_from(list: Vec<Lock>) -> Result<Self, String> {
        let mut locks = Locks::default();
        for lock in list {
            if !locks.set(lock) {
                return Err(format!("a step has at most {MAX_LOCKS} locks"));
            }
        }
        Ok(locks)
    }
}

impl From<Locks> for Vec<Lock> {
    fn from(locks: Locks) -> Self {
        locks.iter().copied().collect()
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
    /// Muted in this pattern only: its steps stay, but none of them play. Kept with the steps
    /// and rules, in the pattern's lane.
    pub pattern_mute: bool,
    /// What plays the notes.
    pub instrument: Instrument,
    /// Effects after the instrument, before the mixer strip.
    pub effects: FxChain,
    pub mix: Mix,
    /// Its choke group within its track group, 1 to [`MAX_CHOKE`], or 0 for none: a hit on any
    /// track in the same group with the same choke cuts this one off, as a closed hat stops an
    /// open one.
    pub choke: u8,
    /// Knobs that each turn several of its settings.
    pub macros: Vec<Macro>,
    /// The face of the preset it was given, until its instrument changes for another kind.
    pub face: Option<Face>,
    /// The settings its steps lock, in the order they're shown, at most [`MAX_LOCKS`]. Shared
    /// by every pattern. See [`Track::lock_columns`].
    pub columns: Vec<LockColumn>,
}

impl Track {
    /// The settings its steps lock, as columns: [`Track::columns`], then any locked on its steps
    /// that aren't among them, such as a file's written without, in the order they come.
    pub fn lock_columns(&self) -> Vec<LockColumn> {
        let mut columns = self.columns.clone();
        for lock in self.steps.iter().flat_map(|s| s.locks.iter()) {
            if !columns.contains(&lock.column()) {
                columns.push(lock.column());
            }
        }
        columns
    }

    /// Adds `column` to the track's columns, if it isn't there. False if there's no room.
    pub fn add_column(&mut self, column: LockColumn) -> bool {
        let mut columns = self.lock_columns();
        if !columns.contains(&column) {
            if columns.len() >= MAX_LOCKS {
                return false;
            }
            columns.push(column);
        }
        self.columns = columns;
        true
    }
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
    #[serde(default, skip_serializing_if = "is_false")]
    pattern_mute: bool,
    #[serde(default)]
    instrument: Instrument,
    #[serde(default, skip_serializing_if = "Mix::is_default")]
    mix: Mix,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    effects: Vec<Fx>,
    /// How the effects branch, if they do.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lanes: Vec<FxLane>,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    choke: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    macros: Vec<Macro>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    face: Option<Face>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    columns: Vec<LockColumn>,
}

fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

impl From<Track> for SparseTrack {
    fn from(track: Track) -> Self {
        let (effects, lanes) = track.effects.into_parts();
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
            pattern_mute: track.pattern_mute,
            instrument: track.instrument,
            mix: track.mix,
            effects,
            lanes,
            choke: track.choke,
            macros: track.macros,
            face: track.face,
            columns: track.columns,
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
        let effects = FxChain::from_parts(sparse.effects, sparse.lanes)
            .map_err(|e| format!("track {:?}'s effects: {e}", sparse.name))?;
        Ok(Self {
            name: sparse.name,
            steps,
            rules: sparse.rules,
            pattern_mute: sparse.pattern_mute,
            instrument: sparse.instrument,
            mix: sparse.mix,
            effects,
            choke: sparse.choke,
            macros: sparse.macros,
            face: sparse.face,
            columns: sparse.columns,
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
    /// The tracks, with the lanes of the arrangement's current pattern.
    pub grid: Grid,
    /// The other patterns' lanes, and the order the patterns play in.
    #[serde(default)]
    pub arrangement: Arrangement,
    #[serde(default, skip_serializing_if = "Buses::is_default")]
    pub buses: Buses,
}
