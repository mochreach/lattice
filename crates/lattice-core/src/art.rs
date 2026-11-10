//! Art drawn by hand for faces: each engine's, nodding to what it's named after, which a track
//! shows when it has no face of its own. Each is braille, a frame of 31 characters by 8 rows,
//! and the next frame after an empty line; they were drawn dot by dot rather than traced, as
//! pictures don't survive being made so small.

use crate::instrument::InstKind;
use crate::preset::Face;

/// The art of `kind`'s engine.
pub fn engine(kind: InstKind) -> &'static str {
    match kind {
        // An oscilloscope, a wave scrolling across it.
        InstKind::Synth => include_str!("art/synth.txt"),
        // The Rhythmicon's lamp shining through its spinning disc of holes onto its photocells.
        InstKind::Kick | InstKind::Snare | InstKind::Hat | InstKind::Clap | InstKind::Tom => {
            include_str!("art/rhythmicon.txt")
        }
        // Pythagoras's monochord, its string ringing over the bridge set at a fifth.
        InstKind::Monochord => include_str!("art/monochord.txt"),
        // A reel-to-reel tape machine, as at the Radiophonic Workshop, its reels turning.
        InstKind::Sampler => include_str!("art/sampler.txt"),
        // A break's waveform sliced up, a slice and its pad lit in turn.
        InstKind::Chop => include_str!("art/chop.txt"),
        // A Moog's wall of modules, its patch cables swaying.
        InstKind::Switched => include_str!("art/switched.txt"),
        // The Great Machine of the Krell, from *Forbidden Planet*, and its halls.
        InstKind::Krell => include_str!("art/krell.txt"),
        // An electric piano's tine and the tone bar over it, clamped in their block, struck by
        // the hammer and swinging in front of the pickup.
        InstKind::Tuss => include_str!("art/tuss.txt"),
        // A carrier bent by a modulator, squeezed more each frame, over its spectrum: the
        // carrier and its sidebands, as strong as Bessel's functions say, the carrier gone at an
        // index of 2.4.
        InstKind::Bessel => include_str!("art/bessel.txt"),
        // A bow drawn across a string over a body of tiles, which light up as it sounds.
        InstKind::Mosaic => include_str!("art/mosaic.txt"),
        // A MIDI socket, its five pins, and a note on going out down the wire, bit by bit.
        InstKind::Midi => include_str!("art/midi.txt"),
    }
}

/// A face for a track that has none: the engine's name and art.
pub fn engine_face(kind: InstKind) -> Face {
    Face {
        art: engine(kind).lines().map(String::from).collect(),
        ..Face::new(crate::Kind::name(kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::{ART_ROWS, ART_WIDTH};

    #[test]
    fn every_engine_has_art_that_fits_and_moves() {
        for kind in InstKind::ALL {
            let face = engine_face(kind);
            assert_eq!(face.check(kind), Ok(()), "{kind:?}");
            assert!(face.frames().count() > 1, "{kind:?} moves");
            for frame in face.frames() {
                assert_eq!(frame.len(), ART_ROWS, "{kind:?}");
                assert!(
                    frame.iter().all(|r| r.chars().count() == ART_WIDTH),
                    "{kind:?}"
                );
            }
        }
    }
}
