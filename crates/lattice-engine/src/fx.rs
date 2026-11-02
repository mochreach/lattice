//! Building effect processors from the song's description of them.

use lattice_core::{Fx, FxKind};

use crate::comp::Comp;
use crate::delay::Delay;
use crate::drive::Drive;
use crate::ensemble::Ensemble;
use crate::filter::Filter;
use crate::phaser::Phaser;
use crate::pitch::Pitch;
use crate::processor::Processor;
use crate::reverb::Reverb;

/// The processor for `fx`, starting at its settings. Allocates, so call it off the audio thread.
pub fn build(fx: &Fx, sample_rate: f32) -> Box<dyn Processor> {
    match fx.kind {
        FxKind::Delay => Box::new(Delay::with_values(sample_rate, &fx.values)),
        FxKind::Drive => Box::new(Drive::with_values(sample_rate, &fx.values)),
        FxKind::Comp => Box::new(Comp::with_values(sample_rate, &fx.values)),
        FxKind::Reverb => Box::new(Reverb::with_values(sample_rate, &fx.values)),
        FxKind::Filter => Box::new(Filter::with_values(sample_rate, &fx.values)),
        FxKind::Pitch => Box::new(Pitch::with_values(sample_rate, &fx.values)),
        FxKind::Ensemble => Box::new(Ensemble::with_values(sample_rate, &fx.values)),
        FxKind::Phaser => Box::new(Phaser::with_values(sample_rate, &fx.values)),
    }
}
