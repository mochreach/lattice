//! Building instruments from the song's description of them.

use lattice_core::{InstKind, Instrument};

use crate::processor::Processor;
use crate::rhythmicon;
use crate::synth::Synth;

/// The processor for `instrument`, starting at its settings. Allocates, so call it off the audio
/// thread.
pub fn build(instrument: &Instrument, sample_rate: f32) -> Box<dyn Processor> {
    match instrument.kind {
        InstKind::Synth => Box::new(Synth::with_values(sample_rate, &instrument.values)),
        kind => rhythmicon::build(kind, sample_rate, &instrument.values)
            .expect("every other instrument is a drum"),
    }
}
