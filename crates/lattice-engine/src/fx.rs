//! Building effect processors from the song's description of them.

use lattice_core::{Fx, FxKind};

use crate::comp::Comp;
use crate::delay::Delay;
use crate::drive::Drive;
use crate::processor::Processor;

/// The processor for `fx`, starting at its settings. Allocates, so call it off the audio thread.
pub fn build(fx: &Fx, sample_rate: f32) -> Box<dyn Processor> {
    match fx.kind {
        FxKind::Delay => Box::new(Delay::with_values(sample_rate, &fx.values)),
        FxKind::Drive => Box::new(Drive::with_values(sample_rate, &fx.values)),
        FxKind::Comp => Box::new(Comp::with_values(sample_rate, &fx.values)),
    }
}
