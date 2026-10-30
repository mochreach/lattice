//! Song data and pattern transforms.
//!
//! Everything here is plain data and pure functions: no UI types, no audio types and no
//! playback state. Playheads and pass counters belong to the sequencer.

pub mod file;
pub mod song;
pub mod transform;

pub use file::{FileError, from_ron, to_ron};
pub use song::{Grid, MAX_STEPS, Note, Song, Step, Track};
pub use transform::{Condition, Rule, Transform, realise, realise_into};
