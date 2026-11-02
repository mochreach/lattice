//! The real-time half of Lattice: sequencer clock, instruments, DSP and mixer.
//!
//! Everything reachable from the audio callback must be real-time safe: no locks, no
//! allocation, no I/O. The host (cpal) layer lives outside this crate, so the engine can be
//! driven by tests, offline rendering or a plugin wrapper just as well.

// Indexing panics when it's out of range; see AGENTS.md. The DSP keeps a few, each with its
// reason, where a wrapped index into a delay line is clearest.
#![deny(clippy::indexing_slicing)]
// Tests may unwrap and panic: that's how they fail.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::indexing_slicing
    )
)]

pub mod chop;
mod click;
pub mod clock;
pub mod comp;
pub mod delay;
pub mod denormal;
pub mod drive;
pub mod dsp;
pub mod engine;
pub mod filter;
pub mod fx;
pub mod instrument;
pub mod looper;
pub mod monochord;
pub mod param;
pub mod pitch;
pub mod processor;
pub mod reverb;
pub mod rhythmicon;
pub mod routing;
pub mod sampler;
pub mod synth;

pub use chop::Chop;
pub use clock::Clock;
pub use comp::Comp;
pub use delay::Delay;
pub use drive::Drive;
pub use dsp::{AdsrParams, Wave};
pub use engine::{
    Chain, Command, Cue, EffectSlot, Engine, Handle, MAX_EFFECTS, MAX_TRACKS, Section, Status,
    Target, TrackPlayer,
};
pub use looper::Take;
pub use param::{ParamInfo, Smoothed};
pub use processor::{Event, EventKind, Processor};
pub use routing::Routing;
pub use sampler::{Sample, Sampler};
pub use synth::{Patch, Synth};
