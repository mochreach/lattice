//! The real-time half of Lattice: sequencer clock, instruments, DSP and mixer.
//!
//! Everything reachable from the audio callback must be real-time safe: no locks, no
//! allocation, no I/O. The host (cpal) layer lives outside this crate, so the engine can be
//! driven by tests, offline rendering or a plugin wrapper just as well.

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

pub mod bessel;
pub mod chop;
mod click;
pub mod clock;
pub mod comp;
pub mod delay;
pub mod denormal;
pub mod drive;
pub mod dsp;
pub mod engine;
pub mod ensemble;
pub mod filter;
pub mod fx;
pub mod instrument;
pub mod krell;
pub mod looper;
pub mod midi_out;
pub mod monochord;
pub mod mosaic;
pub mod param;
pub mod phaser;
pub mod pitch;
pub mod processor;
pub mod reverb;
pub mod rhythmicon;
pub mod routing;
pub mod sampler;
pub mod switched;
pub mod synth;
pub mod tuss;
mod waveguide;

pub use chop::Chop;
pub use clock::Clock;
pub use comp::Comp;
pub use delay::Delay;
pub use drive::Drive;
pub use dsp::{AdsrParams, Wave};
pub use engine::{
    Chain, Command, Conducted, Cue, EffectSlot, Engine, Handle, MAX_EFFECTS, MAX_TRACKS, Section,
    Status, Target, TrackPlayer,
};
pub use looper::Take;
pub use param::{ParamInfo, Smoothed};
pub use processor::{Event, EventKind, Processor};
pub use routing::Routing;
pub use sampler::{Sample, Sampler};
pub use synth::{Patch, Synth};
