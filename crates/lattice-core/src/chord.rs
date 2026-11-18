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

    /// The chord of `others` played over `root`: the first [`MAX_CHORD`] that are in MIDI's
    /// range of it and aren't the root again; any more are left out.
    pub fn of(root: u8, others: impl IntoIterator<Item = u8>) -> Chord {
        others
            .into_iter()
            .fold(Chord::default(), |chord, pitch| chord.with(root, pitch))
    }

    /// The chord with `pitch` added over `root`, as it is if it already plays it or is full.
    pub fn with(mut self, root: u8, pitch: u8) -> Chord {
        let offset = i8::try_from(i16::from(pitch) - i16::from(root)).ok();
        let Some(offset) = offset.filter(|&o| o != 0 && !self.offsets().contains(&o)) else {
            return self;
        };
        if let Some(slot) = self.notes.get_mut(usize::from(self.len)) {
            *slot = offset;
            self.len += 1;
        }
        self
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

impl Chord {
    /// The chord over `root` an inversion up, its lowest note an octave higher, or down, its
    /// highest an octave lower: further if a note's there already, so none is lost. The root
    /// stays the note's pitch wherever it goes, so this gives the root with the chord. As it is
    /// where that would leave MIDI's range. Doesn't allocate, so the engine can play one.
    pub fn inverted(self, root: u8, up: bool) -> (u8, Chord) {
        let mut pitches = [0; MAX_CHORD + 1];
        let mut len = 0;
        for (slot, pitch) in pitches.iter_mut().zip(self.pitches(root)) {
            *slot = pitch;
            len += 1;
        }
        let played = pitches.get(..len).unwrap_or(&[]);
        let outer = match up {
            true => (played.iter().enumerate()).min_by_key(|&(_, p)| p),
            false => (played.iter().enumerate()).max_by_key(|&(_, p)| p),
        };
        let Some((i, &from)) = outer else {
            return (root, self);
        };
        let octaves = (1..=10).map(|n| match up {
            true => i16::from(from) + 12 * n,
            false => i16::from(from) - 12 * n,
        });
        let Some(to) = octaves
            .filter_map(|p| u8::try_from(p).ok().filter(|&p| p <= 127))
            .find(|p| !played.contains(p))
        else {
            return (root, self);
        };
        if let Some(p) = pitches.get_mut(i) {
            *p = to;
        }
        let Some((&root, rest)) = pitches.get(..len).and_then(<[u8]>::split_first) else {
            return (root, self);
        };
        (root, Chord::of(root, rest.iter().copied()))
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
    let (root, suffix, bass) = identify(pitches)?;
    let slash = match bass == root {
        true => String::new(),
        false => format!("/{}", pc_name(bass)),
    };
    Some(format!("{}{suffix}{slash}", pc_name(root)))
}

/// The chord `pitches` make: its root's pitch class, what follows the root in its name, and
/// the bass's pitch class.
fn identify(pitches: &[u8]) -> Option<(u8, &'static str, u8)> {
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
    Some((root, suffix, bass % 12))
}

/// Each degree's numeral, in semitones above the key's root, counted from its major scale, as
/// lead sheets write them: a note between two of its degrees flattens the one above, but for
/// the sharpened fourth.
const NUMERALS: [&str; 12] = [
    "I", "bII", "II", "bIII", "III", "IV", "#IV", "V", "bVI", "VI", "bVII", "VII",
];

/// Each degree as a number, for notes a numeral can't name: in semitones above the key's root.
const DEGREES: [&str; 12] = [
    "1", "b2", "2", "b3", "3", "4", "#4", "5", "b6", "6", "b7", "7",
];

/// The degrees of the major scale, in semitones above its root.
const MAJOR_DEGREES: [i8; 7] = [0, 2, 4, 5, 7, 9, 11];

/// How a chord's kind is written after a numeral, its case saying whether it's minor: each of
/// [`CHORDS`]' suffixes, whether its numeral's lower case, and what follows it.
const QUALITIES: [(&str, bool, &str); 22] = [
    ("", false, ""),
    ("m", true, ""),
    ("dim", true, "°"),
    ("+", false, "+"),
    ("sus4", false, "sus4"),
    ("sus2", false, "sus2"),
    ("5", false, "5"),
    ("7", false, "7"),
    ("maj7", false, "maj7"),
    ("m7", true, "7"),
    ("6", false, "6"),
    ("m6", true, "6"),
    ("m7b5", true, "ø7"),
    ("dim7", true, "°7"),
    ("mmaj7", true, "maj7"),
    ("+7", false, "+7"),
    ("7sus4", false, "7sus4"),
    ("add9", false, "add9"),
    ("madd9", true, "add9"),
    ("9", false, "9"),
    ("maj9", false, "maj9"),
    ("m9", true, "9"),
];

/// Other ways of writing what follows a numeral, and the way Lattice writes it.
const NUMERAL_ALIASES: [(&str, &str); 5] = [
    ("o", "°"),
    ("o7", "°7"),
    ("ø", "ø7"),
    ("dim", "°"),
    ("dim7", "°7"),
];

/// A degree's number within a chord, for the bass after a numeral's slash: `3` for its third,
/// from the semitones it is above the chord's root.
fn interval_number(semitones: i8) -> &'static str {
    match semitones.rem_euclid(12) {
        1 | 2 => "9",
        3 | 4 => "3",
        5 => "4",
        6..=8 => "5",
        9 => "6",
        _ => "7",
    }
}

/// The chord `pitches` make as a numeral, `vi`, `V7` or `bVII`, its root's degree in the key
/// on C, and its bass's place in the chord after a slash when it isn't the root: `I/3` has its
/// third in the bass. `None` if they aren't a chord Lattice knows.
pub fn numeral(pitches: &[u8]) -> Option<String> {
    let (root, suffix, bass) = identify(pitches)?;
    let &(_, lower, after) = QUALITIES.iter().find(|(s, _, _)| *s == suffix)?;
    let numeral = NUMERALS.get(usize::from(root))?;
    let numeral = match lower {
        true => numeral.to_lowercase(),
        false => (*numeral).to_string(),
    };
    let slash = match bass == root {
        true => String::new(),
        // Both pitch classes, under 12.
        false => {
            let above = (bass + 12 - root) as i8;
            let number = match (above, suffix) {
                (9, "dim7") => "7",
                (2, s) if s.contains('2') => "2",
                (a, _) => interval_number(a),
            };
            format!("/{number}")
        }
    };
    Some(format!("{numeral}{after}{slash}"))
}

/// `note` as a numeral track shows it, in the key on C: a chord as a numeral, or its notes'
/// degrees listed if it's none Lattice knows; a note on its own as its degree.
pub fn numeral_label(note: &crate::Note) -> String {
    let pitches: Vec<u8> = note.chord.pitches(note.pitch).collect();
    let degree = |p: u8| DEGREES.get(usize::from(p % 12)).copied().unwrap_or("?");
    if note.chord.is_empty() {
        return degree(note.pitch).to_string();
    }
    numeral(&pitches).unwrap_or_else(|| {
        let mut sorted = pitches.clone();
        sorted.sort_unstable();
        let mut seen = 0u16;
        let mut names = vec![];
        for p in sorted {
            if seen & 1 << (p % 12) == 0 {
                seen |= 1 << (p % 12);
                names.push(degree(p));
            }
        }
        names.join(",")
    })
}

/// A chord read as a numeral, `vi`, `bVII7`, `vii°`, `I/3`: its root in semitones above the
/// key's, its notes as semitones up from the root, close together, and the bass's in semitones
/// above the key's root after a slash. `None` if it doesn't start as a numeral does, so it can
/// be read as a chord's name instead.
pub fn parse_numeral(text: &str) -> Option<Result<Parsed, String>> {
    let (chord, bass) = match text.split_once('/') {
        Some((chord, bass)) => (chord, Some(bass)),
        None => (text, None),
    };
    let (accidental, rest) = match chord.chars().next()? {
        'b' | '♭' => (-1, chord.get(chord.chars().next()?.len_utf8()..)?),
        '#' | '♯' => (1, chord.get(chord.chars().next()?.len_utf8()..)?),
        _ => (0, chord),
    };
    let letters = rest.len() - rest.trim_start_matches(['I', 'V', 'i', 'v']).len();
    let (roman, after) = rest.split_at_checked(letters)?;
    let lower = roman.chars().all(|c| c.is_ascii_lowercase());
    if !lower && !roman.chars().all(|c| c.is_ascii_uppercase()) {
        return None;
    }
    let after = (NUMERAL_ALIASES.iter())
        .find(|(alias, _)| *alias == after)
        .map_or(after, |&(_, own)| own);
    // Diminished chords are minor, however they're written.
    let lower = lower || after.starts_with(['°', 'ø']);
    let degree = ["I", "II", "III", "IV", "V", "VI", "VII"]
        .iter()
        .position(|n| n.eq_ignore_ascii_case(roman))?;
    Some((|| {
        // `degree` is a position in a list of seven.
        let root = (MAJOR_DEGREES.get(degree).copied().unwrap_or(0) + accidental).rem_euclid(12);
        let suffix = (QUALITIES.iter())
            .find(|&&(_, l, a)| l == lower && a == after)
            .map(|&(s, _, _)| s)
            .ok_or_else(|| format!("no chord {text}: try vi, V7, bVII or vii°"))?;
        let intervals = (CHORDS.iter())
            .find(|(s, _)| *s == suffix)
            .map(|&(_, intervals)| intervals)
            .ok_or_else(|| format!("no chord {text}"))?;
        let bass = match bass {
            Some(number) => {
                let found = intervals.iter().copied().find(|&i| {
                    let named = match (i, suffix) {
                        (9, "dim7") => "7",
                        (2 | 14, s) if s.contains('2') => "2",
                        (i, _) => interval_number(i),
                    };
                    i != 0 && named == number
                });
                let Some(i) = found else {
                    return Err(format!("{text}'s chord has no {number}: try I/3 or V7/7"));
                };
                Some(((root + i).rem_euclid(12)) as u8)
            }
            None => None,
        };
        Ok((root as u8, intervals, bass))
    })())
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

/// A chord as it's read: its root's pitch class, its notes as semitones up from the root, close
/// together, and the bass's pitch class after a slash, if it has one.
pub type Parsed = (u8, &'static [i8], Option<u8>);

/// A chord read from its name, `Am7`, `F#dim`, `C/E`: its root's pitch class, its notes as
/// semitones up from the root, close together, and the bass's pitch class after a slash.
pub fn parse(name: &str) -> Result<Parsed, String> {
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
    fn names_chords_as_numerals_in_the_key_on_c() {
        let numeral = |p: &[u8]| numeral(p).unwrap();
        assert_eq!(numeral(&[60, 64, 67]), "I");
        assert_eq!(numeral(&[57, 60, 64]), "vi");
        assert_eq!(numeral(&[59, 62, 65]), "vii°");
        assert_eq!(numeral(&[67, 71, 74, 77]), "V7");
        assert_eq!(numeral(&[62, 65, 69, 72]), "ii7");
        assert_eq!(numeral(&[58, 62, 65]), "bVII");
        assert_eq!(numeral(&[66, 69, 72]), "#iv°");
        assert_eq!(numeral(&[59, 62, 65, 69]), "viiø7");
        assert_eq!(numeral(&[64, 67, 72]), "I/3");
        assert_eq!(numeral(&[65, 67, 71, 74]), "V7/7");
        let note = |pitch, offsets: &[i8]| crate::Note {
            pitch,
            velocity: 100,
            length: 1,
            chord: Chord::new(offsets).unwrap(),
        };
        assert_eq!(numeral_label(&note(63, &[])), "b3");
        assert_eq!(numeral_label(&note(60, &[1, 6])), "1,b2,#4");
    }

    #[test]
    fn reads_numerals_and_every_numeral_written_reads_back() {
        let read = |t: &str| parse_numeral(t).unwrap();
        assert_eq!(read("vi"), Ok((9, &[0, 3, 7][..], None)));
        assert_eq!(read("bVII7"), Ok((10, &[0, 4, 7, 10][..], None)));
        assert_eq!(read("viio"), Ok((11, &[0, 3, 6][..], None)));
        assert_eq!(read("VII°"), Ok((11, &[0, 3, 6][..], None)));
        assert_eq!(read("I/3"), Ok((0, &[0, 4, 7][..], Some(4))));
        assert!(read("I/7").is_err());
        assert!(read("vsus4").is_err());
        // Not numerals, so read as names.
        assert_eq!(parse_numeral("Am"), None);
        assert_eq!(parse_numeral("Bb7"), None);
        assert_eq!(parse_numeral("bdim"), None);
        assert_eq!(parse_numeral("Iv"), None);
        for root in 0..12 {
            for (_, intervals) in CHORDS {
                let pitches: Vec<u8> = intervals.iter().map(|&i| (60 + root + i) as u8).collect();
                let written = numeral(&pitches).unwrap();
                let (r, i, _) = read(&written).unwrap();
                let back: Vec<u8> = i.iter().map(|&i| (60 + r as i8 + i) as u8).collect();
                assert_eq!(
                    classes(back.into_iter()),
                    classes(pitches.into_iter()),
                    "{written}"
                );
            }
        }
    }

    #[test]
    fn a_chord_plays_its_root_then_its_notes_and_writes_as_a_list() {
        let chord = Chord::new(&[4, 7, -12]).unwrap();
        assert_eq!(chord.pitches(60).collect::<Vec<_>>(), [60, 64, 67, 48]);
        assert_eq!(chord.pitches(125).collect::<Vec<_>>(), [125, 113]);
        assert!(Chord::new(&[1; MAX_CHORD + 1]).is_none());
        // Built from notes played: the root and repeats left out, and no more than fit.
        assert_eq!(Chord::of(60, [64, 60, 67, 64, 48]).offsets(), [4, 7, -12]);
        assert_eq!(Chord::of(60, 61..80).offsets().len(), MAX_CHORD);
        let text = ron::to_string(&chord).unwrap();
        assert_eq!(text, "[4,7,-12]");
        assert_eq!(ron::from_str::<Chord>(&text).unwrap(), chord);
        assert!(ron::from_str::<Chord>("[1,2,3,4,5,6,7,8]").is_err());
    }
}
