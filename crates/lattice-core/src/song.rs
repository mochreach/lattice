use crate::transform::Rule;

/// Longest a track can be. The engine preallocates for this so edits never allocate on the audio
/// thread.
pub const MAX_STEPS: usize = 256;

/// Most tracks a grid can have. The engine preallocates for this too, so tracks can be added
/// while playing.
pub const MAX_TRACKS: usize = 16;

/// A note to play on a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Note {
    /// MIDI note number, 0–127.
    pub pitch: u8,
    /// MIDI velocity, 0–127.
    pub velocity: u8,
    /// Length in steps.
    pub length: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Step {
    pub note: Option<Note>,
}

impl Step {
    pub fn note(pitch: u8) -> Self {
        Self {
            note: Some(Note {
                pitch,
                velocity: 100,
                length: 1,
            }),
        }
    }
}

/// One instrument's line of steps. Tracks in a grid may differ in length (polymeter).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub name: String,
    pub steps: Vec<Step>,
    /// Transforms applied, in order, to realise the track for each pass.
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub name: String,
    pub steps_per_beat: u32,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Song {
    pub name: String,
    pub bpm: f64,
    pub grids: Vec<Grid>,
    /// Arrangement: indices into `grids`, played in order.
    pub order: Vec<usize>,
}
