//! Song data and pattern transforms.
//!
//! Everything here is plain data and pure functions: no UI types, no audio types and no
//! playback state. Playheads and pass counters belong to the sequencer.

pub mod arrangement;
pub mod chain;
pub mod device;
pub mod file;
pub mod fx;
pub mod group;
pub mod instrument;
pub mod macros;
pub mod preset;
pub mod song;
pub mod transform;

pub use arrangement::{
    Arrangement, BEATS_PER_BAR, Entry, Lane, MAX_BARS, MAX_ENTRIES, MAX_PATTERNS, MAX_REPEATS,
    Pattern, pattern_name,
};
pub use chain::{
    CHAINS, FxChain, FxLane, FxRow, MAX_LANES, MAX_SENDS, RowAt, SavedChain, built_in_chain,
    is_feedback, max_send,
};
pub use device::{Curve, Device, Kind, ParamInfo};
pub use file::{FileError, from_ron, to_ron};
pub use fx::{Bus, Buses, Fx, FxKind, MAX_EFFECTS};
pub use group::{Group, MAX_GROUPS};
pub use instrument::{DRUM_ROOT, InstKind, Instrument};
pub use macros::{MACRO, MAX_MACROS, MAX_TARGETS, Macro, Target as MacroTarget};
pub use preset::{Face, MAX_CONTROLS, PRESETS, Preset, SavedPreset, preset};
pub use song::{
    Grid, Lock, Locks, MAX_CHOKE, MAX_DB, MAX_LOCKS, MAX_PARAMS, MAX_STEPS, MAX_TRACKS, MIN_DB,
    Mix, Note, Song, Step, Track, db_to_gain,
};
pub use transform::{Condition, Rule, Transform, realise, realise_into};
