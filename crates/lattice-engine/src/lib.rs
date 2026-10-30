//! The real-time half of Lattice: sequencer clock, instruments, DSP and mixer.
//!
//! Everything reachable from the audio callback must be real-time safe: no locks, no
//! allocation, no I/O. The host (cpal) layer lives outside this crate, so the engine can be
//! driven by tests, offline rendering or a plugin wrapper just as well.

mod click;
pub mod clock;
pub mod denormal;
pub mod dsp;
pub mod engine;
pub mod instrument;
pub mod synth;

pub use clock::Clock;
pub use dsp::{AdsrParams, Wave};
pub use engine::{Command, Engine, Handle, MAX_TRACKS, Status};
pub use instrument::{Event, EventKind, Instrument};
pub use synth::{Patch, Synth};
