//! The real-time half of Lattice: sequencer clock, voices, DSP and mixer.
//!
//! Everything reachable from the audio callback must be real-time safe: no locks, no
//! allocation, no I/O. The host (cpal) layer lives outside this crate, so the engine can be
//! driven by tests, offline rendering or a plugin wrapper just as well.

mod click;
pub mod clock;
pub mod dsp;
pub mod engine;
pub mod synth;

pub use clock::Clock;
pub use dsp::{AdsrParams, Wave};
pub use engine::Engine;
pub use synth::Patch;
