//! The real-time half of Lattice: sequencer clock, instruments, DSP and mixer.
//!
//! Everything reachable from the audio callback must be real-time safe: no locks, no
//! allocation, no I/O. The host (cpal) layer lives outside this crate, so the engine can be
//! driven by tests, offline rendering or a plugin wrapper just as well.

mod click;
pub mod clock;
pub mod delay;
pub mod denormal;
pub mod dsp;
pub mod engine;
pub mod fx;
pub mod param;
pub mod processor;
pub mod synth;

pub use clock::Clock;
pub use delay::Delay;
pub use dsp::{AdsrParams, Wave};
pub use engine::{
    Chain, Command, EffectSlot, Engine, Handle, MAX_EFFECTS, MAX_TRACKS, Status, Target,
    TrackPlayer,
};
pub use param::{ParamInfo, Smoothed};
pub use processor::{Event, EventKind, Processor};
pub use synth::{Patch, Synth};
