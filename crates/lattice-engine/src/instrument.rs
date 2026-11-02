//! Building instruments from the song's description of them.

use std::sync::Arc;

use lattice_core::{InstKind, Instrument};

use crate::chop::Chop;
use crate::monochord::Monochord;
use crate::processor::Processor;
use crate::rhythmicon;
use crate::sampler::{Sample, Sampler};
use crate::synth::Synth;

/// The processor for `instrument`, starting at its settings, with `sample` loaded from its file if
/// it plays one. Allocates, so call it off the audio thread.
pub fn build(
    instrument: &Instrument,
    sample_rate: f32,
    sample: Option<Arc<Sample>>,
) -> Box<dyn Processor> {
    match instrument.kind {
        InstKind::Sampler => Box::new(Sampler::new(sample_rate, &instrument.values, sample)),
        InstKind::Chop => Box::new(Chop::new(sample_rate, &instrument.values, sample)),
        InstKind::Synth => Box::new(Synth::with_values(sample_rate, &instrument.values)),
        InstKind::String => Box::new(Monochord::with_values(sample_rate, &instrument.values)),
        // Every other instrument is a drum; were one not, it'd play as a synth.
        kind => rhythmicon::build(kind, sample_rate, &instrument.values)
            .unwrap_or_else(|| Box::new(Synth::with_values(sample_rate, &instrument.values))),
    }
}
