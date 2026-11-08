//! Tunings: where each of the twelve notes of the octave sits above the key note, in cents, so
//! an instrument can play out of equal temperament. A tuning is built on a key: in just
//! intonation on A, A's fifth is a pure 3/2 above it and its third a pure 5/4, while the notes
//! far from A come out further from equal temperament. The key note itself stays where equal
//! temperament puts it, so A-4 is still 440 Hz.

/// The tunings, by name, in the order their tables are in.
pub const TUNINGS: &[&str] = &[
    "equal",
    "just",
    "pythagorean",
    "meantone",
    "werckmeister",
    "harmonic",
];

/// The keys a tuning can be built on, from C.
pub const KEYS: &[&str] = &[
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Each tuning's notes, in cents above the key note.
const CENTS: [[f32; 12]; 6] = [
    // Equal temperament: every semitone the same, 2^(1/12).
    [
        0.0, 100.0, 200.0, 300.0, 400.0, 500.0, 600.0, 700.0, 800.0, 900.0, 1000.0, 1100.0,
    ],
    // Five-limit just intonation: 1/1 16/15 9/8 6/5 5/4 4/3 45/32 3/2 8/5 5/3 9/5 15/8.
    [
        0.0, 111.73, 203.91, 315.64, 386.31, 498.04, 590.22, 701.96, 813.69, 884.36, 1017.6,
        1088.27,
    ],
    // Pythagorean: pure 3/2 fifths stacked, from E♭ to G♯.
    [
        0.0, 90.22, 203.91, 294.13, 407.82, 498.04, 611.73, 701.96, 792.18, 905.87, 996.09, 1109.78,
    ],
    // Quarter-comma meantone: fifths narrowed so four of them make a pure 5/4 third, from E♭
    // to G♯, the wolf between them.
    [
        0.0, 76.05, 193.16, 310.26, 386.31, 503.42, 579.47, 696.58, 772.63, 889.74, 1006.84,
        1082.89,
    ],
    // Werckmeister III (1691): a well temperament, every key playable, each its own colour.
    [
        0.0, 90.22, 192.18, 294.13, 390.22, 498.04, 588.27, 696.09, 792.18, 888.27, 996.09, 1092.18,
    ],
    // The harmonic series, folded into an octave: 1/1 17/16 9/8 19/16 5/4 21/16 11/8 3/2 13/8
    // 27/16 7/4 15/8, its seventh and eleventh far from anything equal temperament has.
    [
        0.0, 104.96, 203.91, 297.51, 386.31, 470.78, 551.32, 701.96, 840.53, 905.87, 968.83,
        1088.27,
    ],
];

/// How far `note` (a MIDI note) sits from equal temperament, in cents, in tuning `tuning` built
/// on key `key` (0 is C). A tuning or key past the end is equal temperament: nothing moves.
pub fn offset(note: u8, tuning: usize, key: usize) -> f32 {
    let degree = (usize::from(note) + 12 - key % 12) % 12;
    CENTS
        .get(tuning)
        .and_then(|cents| cents.get(degree))
        .map_or(0.0, |cents| cents - 100.0 * degree as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tuning_has_a_name_and_keeps_its_key_note() {
        assert_eq!(TUNINGS.len(), CENTS.len());
        assert_eq!(KEYS.len(), 12);
        for tuning in 0..TUNINGS.len() {
            for key in 0..12 {
                // A key note, in any octave.
                assert_eq!(offset(key as u8 + 48, tuning, key), 0.0);
                assert_eq!(offset(key as u8 + 60, tuning, key), 0.0);
            }
        }
    }

    #[test]
    fn just_intonation_on_a_makes_its_thirds_and_fifths_pure() {
        let (just, a) = (1, 9);
        // E above A is a pure fifth, 3/2, two cents wider than equal temperament's.
        assert!((offset(76, just, a) - 1.955).abs() < 0.01);
        // C♯ a pure major third, 5/4, fourteen cents narrower.
        assert!((offset(73, just, a) + 13.69).abs() < 0.01);
        // In equal temperament nothing moves, and nothing does for a tuning that isn't there.
        assert_eq!(offset(73, 0, a), 0.0);
        assert_eq!(offset(73, 99, a), 0.0);
    }
}
