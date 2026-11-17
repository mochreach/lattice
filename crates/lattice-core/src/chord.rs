//! Chords: a note can carry more notes with it, played together on its step, so a chord is one
//! step on one track rather than a note each on several. The note's pitch is the chord's root as
//! it was played, and the chord holds the other notes as semitones from it, so it keeps the
//! voicing exactly. Its name, `Am7` or `G/B`, isn't stored but worked out from the notes, as a
//! musician reading them would.

use serde::{Deserialize, Serialize};

/// Most notes a chord holds besides its root: enough for a seventh with its bass and
/// extensions, or a whole hand of fingers on a keyboard.
pub const MAX_CHORD: usize = 7;

/// The notes a [`Note`](crate::Note) plays besides its own pitch, as semitones from it. Fixed
/// in size, so a step stays `Copy` and the audio thread needn't allocate to play one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<i8>", into = "Vec<i8>")]
pub struct Chord {
    notes: [i8; MAX_CHORD],
    len: u8,
}

impl Chord {
    /// The chord of these notes besides the root, as semitones from it; `None` if there are
    /// more than [`MAX_CHORD`].
    pub fn new(offsets: &[i8]) -> Option<Chord> {
        let mut chord = Chord::default();
        for (slot, &offset) in chord.notes.iter_mut().zip(offsets) {
            *slot = offset;
        }
        chord.len = u8::try_from(offsets.len())
            .ok()
            .filter(|&n| usize::from(n) <= MAX_CHORD)?;
        Some(chord)
    }

    /// The notes besides the root, as semitones from it.
    pub fn offsets(&self) -> &[i8] {
        // `len` is never more than MAX_CHORD, as only `new` sets it.
        self.notes.get(..usize::from(self.len)).unwrap_or(&[])
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The pitches it plays over `root`, the root first, leaving out any off the MIDI range.
    pub fn pitches(self, root: u8) -> impl Iterator<Item = u8> {
        let offsets = self.notes.into_iter().take(usize::from(self.len));
        std::iter::once(root).chain(offsets.filter_map(move |offset| {
            u8::try_from(i16::from(root) + i16::from(offset))
                .ok()
                .filter(|&p| p <= 127)
        }))
    }
}

impl TryFrom<Vec<i8>> for Chord {
    type Error = String;

    fn try_from(offsets: Vec<i8>) -> Result<Self, Self::Error> {
        Chord::new(&offsets).ok_or_else(|| {
            format!(
                "a chord has up to {MAX_CHORD} notes besides its root, not {}",
                offsets.len()
            )
        })
    }
}

impl From<Chord> for Vec<i8> {
    fn from(chord: Chord) -> Self {
        chord.offsets().to_vec()
    }
}

/// The chords Lattice names, each by what follows the root's name and its notes as semitones
/// up from the root, close together. Simpler ones first, so a set of notes that's more than one
/// is named the plainer way.
pub const CHORDS: [(&str, &[i8]); 22] = [
    ("", &[0, 4, 7]),
    ("m", &[0, 3, 7]),
    ("dim", &[0, 3, 6]),
    ("+", &[0, 4, 8]),
    ("sus4", &[0, 5, 7]),
    ("sus2", &[0, 2, 7]),
    ("5", &[0, 7]),
    ("7", &[0, 4, 7, 10]),
    ("maj7", &[0, 4, 7, 11]),
    ("m7", &[0, 3, 7, 10]),
    ("6", &[0, 4, 7, 9]),
    ("m6", &[0, 3, 7, 9]),
    ("m7b5", &[0, 3, 6, 10]),
    ("dim7", &[0, 3, 6, 9]),
    ("mmaj7", &[0, 3, 7, 11]),
    ("+7", &[0, 4, 8, 10]),
    ("7sus4", &[0, 5, 7, 10]),
    ("add9", &[0, 4, 7, 14]),
    ("madd9", &[0, 3, 7, 14]),
    ("9", &[0, 4, 7, 10, 14]),
    ("maj9", &[0, 4, 7, 11, 14]),
    ("m9", &[0, 3, 7, 10, 14]),
];

/// Other ways of writing a chord's kind, as lead sheets do, and the one Lattice writes.
const ALIASES: [(&str, &str); 14] = [
    ("maj", ""),
    ("M", ""),
    ("min", "m"),
    ("-", "m"),
    ("aug", "+"),
    ("°", "dim"),
    ("o", "dim"),
    ("ø", "m7b5"),
    ("ø7", "m7b5"),
    ("M7", "maj7"),
    ("Δ", "maj7"),
    ("°7", "dim7"),
    ("sus", "sus4"),
    ("mM7", "mmaj7"),
];

/// Each pitch class's name in a chord, as lead sheets most often spell them.
const NAMES: [&str; 12] = [
    "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];

/// Pitch class `pc`'s name.
fn pc_name(pc: u8) -> &'static str {
    NAMES.get(usize::from(pc % 12)).copied().unwrap_or("?")
}

/// The pitch classes in `pitches`, as bits from C.
fn classes(pitches: impl Iterator<Item = u8>) -> u16 {
    pitches.fold(0, |mask, p| mask | 1 << (p % 12))
}

/// `intervals`' pitch classes over a root of C, as bits.
fn shape(intervals: &[i8]) -> u16 {
    (intervals.iter()).fold(0, |mask, &i| mask | 1 << i.rem_euclid(12))
}

/// The chord `pitches` make, by name, `Am7` or `G/B`, the lowest note after a slash when it
/// isn't the root; `None` if they aren't a chord Lattice knows. A seventh or ninth with its
/// fifth left out, as players often leave it, is still named.
pub fn name(pitches: &[u8]) -> Option<String> {
    let bass = *pitches.iter().min()?;
    let notes = classes(pitches.iter().copied());
    // Each root the notes have, the bass first; each chord, the simplest first. An exact match
    // beats one missing its fifth, and over the bass beats over another root.
    let mut best: Option<((bool, bool, usize), u8, &str)> = None;
    let roots = std::iter::once(bass % 12).chain((0..12).filter(|&r| r != bass % 12));
    for root in roots.filter(|&r| notes & 1 << r != 0) {
        let over = (notes >> root | notes << (12 - root)) & 0xfff;
        for (i, (suffix, intervals)) in CHORDS.iter().enumerate() {
            let want = shape(intervals);
            let fifthless = intervals.len() >= 4 && intervals.contains(&7);
            let missing = match () {
                _ if over == want => false,
                _ if fifthless && over == want & !(1 << 7) => true,
                _ => continue,
            };
            let rank = (missing, root != bass % 12, i);
            if best.is_none_or(|(b, _, _)| rank < b) {
                best = Some((rank, root, suffix));
            }
        }
    }
    let (_, root, suffix) = best?;
    let slash = match bass % 12 == root {
        true => String::new(),
        false => format!("/{}", pc_name(bass)),
    };
    Some(format!("{}{suffix}{slash}", pc_name(root)))
}

/// `note`'s chord as the grid shows it: by name if Lattice knows it, else its notes listed;
/// `None` if it's a note on its own.
pub fn label(note: &crate::Note) -> Option<String> {
    if note.chord.is_empty() {
        return None;
    }
    let pitches: Vec<u8> = note.chord.pitches(note.pitch).collect();
    Some(name(&pitches).unwrap_or_else(|| listed(&pitches)))
}

/// The notes as a lead sheet can't name them, lowest first, each once: `C,E,F#`.
pub fn listed(pitches: &[u8]) -> String {
    let mut sorted = pitches.to_vec();
    sorted.sort_unstable();
    let mut seen = 0u16;
    let mut names = vec![];
    for p in sorted {
        if seen & 1 << (p % 12) == 0 {
            seen |= 1 << (p % 12);
            names.push(pc_name(p));
        }
    }
    names.join(",")
}

/// A chord read from its name, `Am7`, `F#dim`, `C/E`: its root's pitch class, its notes as
/// semitones up from the root, close together, and the bass's pitch class after a slash.
pub fn parse(name: &str) -> Result<(u8, &'static [i8], Option<u8>), String> {
    let (chord, bass) = match name.split_once('/') {
        Some((chord, bass)) => (chord, Some(bass)),
        None => (name, None),
    };
    let (root, rest) = parse_root(chord).ok_or_else(|| format!("{name} has no root: try Am7"))?;
    let suffix = (ALIASES.iter())
        .find(|(alias, _)| *alias == rest)
        .map_or(rest, |&(_, own)| own);
    let intervals = (CHORDS.iter())
        .find(|(s, _)| *s == suffix)
        .map(|&(_, intervals)| intervals)
        .ok_or_else(|| {
            let kinds: Vec<_> = (CHORDS.iter()).map(|(s, _)| format!("C{s}")).collect();
            format!("no chord {name}: try {}", kinds.join(", "))
        })?;
    let bass = match bass {
        Some(bass) => match parse_root(bass) {
            Some((pc, "")) => Some(pc),
            _ => return Err(format!("{bass} isn't a note: try C/E")),
        },
        None => None,
    };
    Ok((root, intervals, bass))
}

/// A note's name at the start of `text`, `C`, `F#` or `Bb`, as a pitch class, and what follows.
fn parse_root(text: &str) -> Option<(u8, &str)> {
    let mut chars = text.chars();
    let pc: u8 = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest = chars.as_str();
    Some(match rest.chars().next() {
        Some('#') => ((pc + 1) % 12, rest.get(1..)?),
        Some('b') => ((pc + 11) % 12, rest.get(1..)?),
        _ => (pc, rest),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(pitches: &[u8]) -> Option<String> {
        name(pitches)
    }

    #[test]
    fn names_chords_as_a_lead_sheet_would() {
        assert_eq!(named(&[60, 64, 67]).as_deref(), Some("C"));
        assert_eq!(named(&[57, 60, 64]).as_deref(), Some("Am"));
        assert_eq!(named(&[57, 60, 64, 67]).as_deref(), Some("Am7"));
        // C6 and Am7 are the same notes: the bass decides.
        assert_eq!(named(&[60, 64, 67, 69]).as_deref(), Some("C6"));
        assert_eq!(named(&[64, 67, 72]).as_deref(), Some("C/E"));
        assert_eq!(named(&[59, 62, 65, 69]).as_deref(), Some("Bm7b5"));
        assert_eq!(named(&[67, 71, 74, 77]).as_deref(), Some("G7"));
        // A seventh without its fifth, as hands often play it.
        assert_eq!(named(&[67, 71, 77]).as_deref(), Some("G7"));
        assert_eq!(named(&[48, 55]).as_deref(), Some("C5"));
        assert_eq!(named(&[60, 62, 67]).as_deref(), Some("Csus2"));
        // Doubled notes don't matter.
        assert_eq!(named(&[48, 60, 64, 67, 72]).as_deref(), Some("C"));
        assert_eq!(named(&[60, 61, 66]), None);
        assert_eq!(listed(&[66, 60, 61, 72]), "C,C#,F#");
    }

    #[test]
    fn reads_chords_by_name_in_any_common_spelling() {
        assert_eq!(parse("Am7"), Ok((9, &[0, 3, 7, 10][..], None)));
        assert_eq!(parse("F#dim"), Ok((6, &[0, 3, 6][..], None)));
        assert_eq!(parse("BbM7"), Ok((10, &[0, 4, 7, 11][..], None)));
        assert_eq!(parse("C/E"), Ok((0, &[0, 4, 7][..], Some(4))));
        assert_eq!(parse("Gsus"), Ok((7, &[0, 5, 7][..], None)));
        assert!(parse("H7").is_err());
        assert!(parse("Cwobble").is_err());
        assert!(parse("C/X").is_err());
        // Every chord named reads back as itself.
        for (suffix, intervals) in CHORDS {
            let pitches: Vec<u8> = intervals.iter().map(|&i| (62 + i) as u8).collect();
            assert_eq!(named(&pitches), Some(format!("D{suffix}")), "{suffix}");
        }
    }

    #[test]
    fn a_chord_plays_its_root_then_its_notes_and_writes_as_a_list() {
        let chord = Chord::new(&[4, 7, -12]).unwrap();
        assert_eq!(chord.pitches(60).collect::<Vec<_>>(), [60, 64, 67, 48]);
        assert_eq!(chord.pitches(125).collect::<Vec<_>>(), [125, 113]);
        assert!(Chord::new(&[1; MAX_CHORD + 1]).is_none());
        let text = ron::to_string(&chord).unwrap();
        assert_eq!(text, "[4,7,-12]");
        assert_eq!(ron::from_str::<Chord>(&text).unwrap(), chord);
        assert!(ron::from_str::<Chord>("[1,2,3,4,5,6,7,8]").is_err());
    }
}
