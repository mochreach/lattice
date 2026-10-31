//! Song data and pattern transforms.
//!
//! Everything here is plain data and pure functions: no UI types, no audio types and no
//! playback state. Playheads and pass counters belong to the sequencer.

pub mod device;
pub mod file;
pub mod fx;
pub mod group;
pub mod instrument;
pub mod song;
pub mod transform;

pub use device::{Curve, Device, Kind, ParamInfo};
pub use file::{FileError, from_ron, to_ron};
pub use fx::{Bus, Buses, Fx, FxKind, MAX_EFFECTS};
pub use group::{Group, MAX_GROUPS};
pub use instrument::{DRUM_ROOT, InstKind, Instrument};
pub use song::{
    Grid, MAX_DB, MAX_STEPS, MAX_TRACKS, MIN_DB, Mix, Note, Song, Step, Track, db_to_gain,
};
pub use transform::{Condition, Rule, Transform, realise, realise_into};
