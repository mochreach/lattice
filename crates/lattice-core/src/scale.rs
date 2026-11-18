//! Scales and keys: what `:scale` sets for playing and entering notes, and the conductor's `K`
//! and scale gestures change as the song goes.

/// A scale, as semitones above its root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scale {
    pub name: &'static str,
    /// Its name shortened to fit a conductor's field: `Dor`, `Hmi`.
    pub short: &'static str,
    pub steps: &'static [u8],
}

/// How far a numeral track's notes move to play in the key on `root`: they're kept in the key
/// on C, and go to the root the shortest way, up to six semitones up or five down, so its
/// chords stay about where they were written.
pub fn key_shift(root: u8) -> i8 {
    // Under 12, so it fits.
    let up = (root % 12) as i8;
    if up <= 6 { up } else { up - 12 }
}

/// `pitch` moved `by` semitones, kept within MIDI's range.
pub fn shifted(pitch: u8, by: i8) -> u8 {
    (i16::from(pitch) + i16::from(by)).clamp(0, 127) as u8
}

pub const MAJOR: Scale = Scale {
    name: "major",
    short: "Maj",
    steps: &[0, 2, 4, 5, 7, 9, 11],
};

pub const SCALES: [Scale; 22] = [
    MAJOR,
    // The church modes, brightest first, after major.
    Scale {
        name: "minor",
        short: "Min",
        steps: &[0, 2, 3, 5, 7, 8, 10],
    },
    Scale {
        name: "dorian",
        short: "Dor",
        steps: &[0, 2, 3, 5, 7, 9, 10],
    },
    Scale {
        name: "phrygian",
        short: "Phr",
        steps: &[0, 1, 3, 5, 7, 8, 10],
    },
    Scale {
        name: "lydian",
        short: "Lyd",
        steps: &[0, 2, 4, 6, 7, 9, 11],
    },
    Scale {
        name: "mixolydian",
        short: "Mix",
        steps: &[0, 2, 4, 5, 7, 9, 10],
    },
    Scale {
        name: "locrian",
        short: "Loc",
        steps: &[0, 1, 3, 5, 6, 8, 10],
    },
    // Minor and major with a raised or lowered note: the harmonic ones each hold an augmented
    // triad and a diminished seventh.
    Scale {
        name: "harmonic-minor",
        short: "Hmi",
        steps: &[0, 2, 3, 5, 7, 8, 11],
    },
    Scale {
        name: "melodic-minor",
        short: "Mmi",
        steps: &[0, 2, 3, 5, 7, 9, 11],
    },
    Scale {
        name: "harmonic-major",
        short: "Hma",
        steps: &[0, 2, 4, 5, 7, 8, 11],
    },
    // The augmented second's scales, of flamenco, klezmer, the Balkans and the Middle East.
    Scale {
        name: "phrygian-dominant",
        short: "Pdo",
        steps: &[0, 1, 4, 5, 7, 8, 10],
    },
    Scale {
        name: "hungarian-minor",
        short: "Hun",
        steps: &[0, 2, 3, 6, 7, 8, 11],
    },
    Scale {
        name: "double-harmonic",
        short: "Dbl",
        steps: &[0, 1, 4, 5, 7, 8, 11],
    },
    // Five notes, and so few to clash.
    Scale {
        name: "pentatonic",
        short: "Pen",
        steps: &[0, 2, 4, 7, 9],
    },
    Scale {
        name: "minor-pentatonic",
        short: "Mpe",
        steps: &[0, 3, 5, 7, 10],
    },
    Scale {
        name: "blues",
        short: "Blu",
        steps: &[0, 3, 5, 6, 7, 10],
    },
    Scale {
        name: "hirajoshi",
        short: "Hir",
        steps: &[0, 2, 3, 7, 8],
    },
    Scale {
        name: "in",
        short: "In",
        steps: &[0, 1, 5, 7, 8],
    },
    // The symmetrical scales: whole tones, all augmented; alternate whole and half steps, all
    // diminished; and minor thirds and semitones, two augmented triads a semitone apart.
    Scale {
        name: "whole-tone",
        short: "Who",
        steps: &[0, 2, 4, 6, 8, 10],
    },
    Scale {
        name: "octatonic",
        short: "Oct",
        steps: &[0, 2, 3, 5, 6, 8, 9, 11],
    },
    Scale {
        name: "augmented",
        short: "Aug",
        steps: &[0, 3, 4, 7, 8, 11],
    },
    Scale {
        name: "chromatic",
        short: "Chr",
        steps: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
    },
];

/// A root note's semitone above C: `C`, `F#`, `Bb`, any case.
pub fn parse_root(text: &str) -> Option<u8> {
    let mut chars = text.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    let letter = b"CDEFGAB".iter().position(|&c| c == letter as u8)?;
    let natural = *MAJOR.steps.get(letter)?;
    let semitone = match chars.as_str() {
        "" => natural + 12,
        "#" => natural + 13,
        "b" => natural + 11,
        _ => return None,
    };
    Some(semitone % 12)
}

/// Each key's name, by its root's semitone above C, as key signatures name them: flats for the
/// keys written with flats, `Bb` not `A#`.
const KEY_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];

/// A key's name, from its root's semitone above C.
pub fn key_name(root: u8) -> &'static str {
    // Always there: a root is a semitone, below 12.
    KEY_NAMES
        .get(usize::from(root % 12))
        .copied()
        .unwrap_or("C")
}

/// The scale called `name`, whole (`dorian`) or short (`Dor`), any case, by its place in
/// [`SCALES`].
pub fn find(name: &str) -> Option<usize> {
    SCALES
        .iter()
        .position(|s| s.name.eq_ignore_ascii_case(name) || s.short.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scale_has_its_own_short_name() {
        for (i, scale) in SCALES.iter().enumerate() {
            assert!(scale.short.len() <= 3, "{}", scale.name);
            assert_eq!(find(scale.short), Some(i), "{}", scale.name);
            assert_eq!(find(scale.name), Some(i));
        }
        assert_eq!(find("dor"), Some(2));
        assert_eq!(find("Dorian"), Some(2));
        assert_eq!(parse_root("bb"), Some(10));
        assert_eq!(key_name(6), "F#");
        assert_eq!(key_name(10), "Bb");
    }
}
