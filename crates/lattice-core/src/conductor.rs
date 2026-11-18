//! The conductor: a lane of each pattern for what belongs to the whole song rather than one
//! track. Each step has two gesture fields, things that happen once on it: set the tempo or slide
//! to one, stop, cue a pattern, mute or solo a track, change key or scale. A tempo holds until
//! the next, as a lock column's value does, but it's the song's only such value, so it's a
//! gesture; a key or scale holds likewise.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::arrangement::MAX_PATTERNS;
use crate::scale::{SCALES, key_name, parse_root};
use crate::song::{MAX_STEPS, MAX_TRACKS};

/// The slowest and fastest tempos, in BPM, as `:bpm` allows.
pub const MIN_BPM: u16 = 20;
pub const MAX_BPM: u16 = 400;

/// Gesture fields on each step, the conductor's and each track's.
pub const GESTURES: usize = 2;

/// Each of the conductor's gestures as it's typed, an example of it, and what it does, as the
/// infobox lists them while one's typed.
pub const FORMS: &[(&str, &str)] = &[
    ("T140", "tempo: 20 to 400 bpm from here"),
    ("~140", "slide to the tempo, arriving here"),
    ("T--", "back to the song's tempo"),
    ("X", "stop at the end of this step"),
    ("P3", "cue pattern 3 for the next bar"),
    ("M2", "mute track 2; M-2 unmutes"),
    ("S2", "solo track 2; S-2 unsolos"),
    ("KG", "key: G from here, the scale as it was"),
    ("K+2", "key: 1 to 11 semitones up, or down"),
    ("Dor", "scale: dorian from here, the key as it was"),
];

/// Something the conductor does once, on a step. Written as it's typed: `T140`, `~140`, `T--`,
/// `X`, `P3`, `M2` `M-2`, `S2` `S-2`, `KG` `K+2`, `Dor`. Patterns and tracks are numbered from 1,
/// as they're shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongGesture {
    /// `T140`: this tempo from this step, until the next.
    Tempo(u16),
    /// `~140`: slides from the last tempo to this one, arriving on this step.
    Slide(u16),
    /// `T--`: back to the song's own tempo, which `:bpm` sets.
    SongTempo,
    /// `X`: stops the song at the end of this step.
    Stop,
    /// `P3`: cues pattern 3 (from 1) to take over at the next bar.
    Pattern(u8),
    /// `M2`: mutes track 2 (from 1); `M-2` unmutes it. Only while the song plays.
    Mute { track: u8, on: bool },
    /// `S2`: solos track 2 (from 1); `S-2` unsolos it. Only while the song plays.
    Solo { track: u8, on: bool },
    /// `KG`, `K+2`: the key from here, until the next, keeping the scale.
    Key(KeyChange),
    /// `Dor`: the scale from here, by its place in [`SCALES`], keeping the key.
    Scale(u8),
}

/// What a `K` gesture does to the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyChange {
    /// `KG`: this root, in semitones above C.
    Root(u8),
    /// `K+2`, `K-3`: the root so many semitones up or down from where it was.
    Shift(i8),
}

impl KeyChange {
    /// The root it leaves `root` at.
    pub fn from(self, root: u8) -> u8 {
        match self {
            KeyChange::Root(r) => r % 12,
            KeyChange::Shift(by) => (i16::from(root) + i16::from(by)).rem_euclid(12) as u8,
        }
    }
}

impl SongGesture {
    /// The tempo it sets or slides to, if it's a tempo.
    pub fn tempo(self) -> Option<u16> {
        match self {
            SongGesture::Tempo(bpm) | SongGesture::Slide(bpm) => Some(bpm),
            _ => None,
        }
    }

    /// Whether it changes the tempo.
    pub fn is_tempo(self) -> bool {
        matches!(
            self,
            SongGesture::Tempo(_) | SongGesture::Slide(_) | SongGesture::SongTempo
        )
    }

    /// Whether more could be typed after `text` to make another gesture: a `T14` might become
    /// `T140`, and `M-` `M-1`, but `T140`, `X`, `T--` and `M-16` are done.
    pub fn could_go_on(text: &str) -> bool {
        let mut chars = text.chars();
        let first = chars.next();
        let rest = chars.as_str();
        let digits =
            |rest: &str, most: usize| rest.chars().all(|c| c.is_ascii_digit()) && rest.len() < most;
        match first {
            Some('T' | 't') if rest.starts_with('-') => rest.len() < 2,
            Some('T' | 't' | '~') => digits(rest, 3),
            Some('P' | 'p') if digits(rest, 2) => true,
            Some('K' | 'k') if rest.starts_with(['+', '-']) => {
                digits(rest.get(1..).unwrap_or(""), 2)
            }
            // A root, then perhaps a sharp or flat.
            Some('K' | 'k') => rest.is_empty() || (rest.len() == 1 && parse_root(rest).is_some()),
            Some('M' | 'm' | 'S' | 's') if digits(rest.strip_prefix('-').unwrap_or(rest), 2) => {
                true
            }
            _ => SCALES.iter().any(|s| {
                s.short.len() > text.len()
                    && s.short
                        .get(..text.len())
                        .is_some_and(|p| p.eq_ignore_ascii_case(text))
            }),
        }
    }

    /// Checks it's one Lattice can play.
    pub fn check(self) -> Result<(), String> {
        let bpm = MIN_BPM..=MAX_BPM;
        match self {
            SongGesture::Tempo(t) | SongGesture::Slide(t) if !bpm.contains(&t) => {
                Err(format!("tempos go from {MIN_BPM} to {MAX_BPM}"))
            }
            SongGesture::Pattern(p) if !(1..=MAX_PATTERNS).contains(&usize::from(p)) => {
                Err(format!("patterns go from 1 to {MAX_PATTERNS}"))
            }
            SongGesture::Mute { track: t, .. } | SongGesture::Solo { track: t, .. }
                if !(1..=MAX_TRACKS).contains(&usize::from(t)) =>
            {
                Err(format!("tracks go from 1 to {MAX_TRACKS}"))
            }
            SongGesture::Key(KeyChange::Shift(by)) if by == 0 || by.unsigned_abs() > 11 => {
                Err("keys move from 1 to 11 semitones up or down".into())
            }
            SongGesture::Scale(s) if usize::from(s) >= SCALES.len() => {
                Err(format!("there are {} scales", SCALES.len()))
            }
            _ => Ok(()),
        }
    }
}

impl fmt::Display for SongGesture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SongGesture::Tempo(bpm) => write!(f, "T{bpm}"),
            SongGesture::Slide(bpm) => write!(f, "~{bpm}"),
            SongGesture::SongTempo => write!(f, "T--"),
            SongGesture::Stop => write!(f, "X"),
            SongGesture::Pattern(p) => write!(f, "P{p}"),
            SongGesture::Mute { track, on } => write!(f, "M{}{track}", if *on { "" } else { "-" }),
            SongGesture::Solo { track, on } => write!(f, "S{}{track}", if *on { "" } else { "-" }),
            SongGesture::Key(KeyChange::Root(root)) => write!(f, "K{}", key_name(*root)),
            SongGesture::Key(KeyChange::Shift(by)) => write!(f, "K{by:+}"),
            SongGesture::Scale(s) => {
                let short = SCALES.get(usize::from(*s)).map_or("?", |s| s.short);
                write!(f, "{short}")
            }
        }
    }
}

impl std::str::FromStr for SongGesture {
    type Err = String;

    /// Reads a gesture as it's written, upper or lower case, and checks it.
    fn from_str(text: &str) -> Result<Self, String> {
        let unknown = || {
            format!(
                "{text:?} isn't a conductor's gesture: T140, ~140, T--, X, P3, M2, M-2, S2, S-2, KG, \
                 K+2 or a scale, as Dor"
            )
        };
        // A scale by its short name, `Dor`; whole names don't fit.
        if let Some(found) = (SCALES.iter()).position(|s| s.short.eq_ignore_ascii_case(text)) {
            return Ok(SongGesture::Scale(found as u8));
        }
        let mut chars = text.chars();
        let first = chars.next().ok_or_else(unknown)?.to_ascii_uppercase();
        let rest = chars.as_str();
        // `M-2` and `S-2` turn off what `M2` and `S2` turn on.
        let (on, rest) = match rest.strip_prefix('-') {
            Some(after) if matches!(first, 'M' | 'S') => (false, after),
            _ => (true, rest),
        };
        let number = |what: &str| -> Result<u16, String> {
            rest.parse::<u16>().map_err(|_| {
                format!(
                    "{text:?}: {what} after {first}, as in {first}{}",
                    match first {
                        'T' | '~' => "140",
                        _ => "2",
                    }
                )
            })
        };
        let small = |n: u16| u8::try_from(n).map_err(|_| unknown());
        let gesture = match first {
            'T' if rest == "--" => SongGesture::SongTempo,
            'T' => SongGesture::Tempo(number("a tempo")?),
            '~' => SongGesture::Slide(number("a tempo")?),
            'X' if rest.is_empty() => SongGesture::Stop,
            'K' if rest.starts_with(['+', '-']) => match rest.parse::<i8>() {
                Ok(by) => SongGesture::Key(KeyChange::Shift(by)),
                Err(_) => return Err(format!("{text:?}: a number after K+ or K-, as in K+2")),
            },
            // Typed in capitals, so `KBB` is B flat.
            'K' => match parse_root(&rest.get(..1).zip(rest.get(1..)).map_or_else(
                || rest.to_string(),
                |(letter, accidental)| format!("{letter}{}", accidental.to_ascii_lowercase()),
            )) {
                Some(root) => SongGesture::Key(KeyChange::Root(root)),
                None => return Err(format!("{text:?}: a key after K, as in KG, KF# or KBb")),
            },
            'P' => SongGesture::Pattern(small(number("a pattern")?)?),
            'M' => SongGesture::Mute {
                track: small(number("a track")?)?,
                on,
            },
            'S' => SongGesture::Solo {
                track: small(number("a track")?)?,
                on,
            },
            _ => return Err(unknown()),
        };
        gesture.check()?;
        Ok(gesture)
    }
}

impl Serialize for SongGesture {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for SongGesture {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// A step of the conductor: its two gesture fields.
pub type ConductorStep = [Option<SongGesture>; GESTURES];

/// A pattern's conductor lane. It has its own length, so a tempo change can come round every
/// bar or once a song; with no steps, it's as long as the pattern and does nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SparseConductor", into = "SparseConductor")]
pub struct Conductor {
    pub steps: Vec<ConductorStep>,
}

impl Conductor {
    /// An empty lane `len` steps long.
    pub fn new(len: usize) -> Self {
        Self {
            steps: vec![[None; GESTURES]; len],
        }
    }

    /// Whether it has no gestures at all.
    pub fn is_empty(&self) -> bool {
        self.steps.iter().flatten().all(Option::is_none)
    }

    /// The gesture in field `field` of step `at`, if there's one.
    pub fn get(&self, at: usize, field: usize) -> Option<SongGesture> {
        self.steps.get(at)?.get(field).copied().flatten()
    }

    /// Every gesture, with its step, in order.
    pub fn gestures(&self) -> impl Iterator<Item = (usize, SongGesture)> + '_ {
        (self.steps.iter().enumerate())
            .flat_map(|(at, step)| step.iter().flatten().map(move |&g| (at, g)))
    }
}

/// How a [`Conductor`] is written to a file: its length and the steps with gestures, each a list
/// of them. Left out when there are none.
#[derive(Serialize, Deserialize)]
#[serde(rename = "Conductor")]
struct SparseConductor {
    length: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    steps: BTreeMap<usize, Vec<SongGesture>>,
}

impl From<Conductor> for SparseConductor {
    fn from(conductor: Conductor) -> Self {
        Self {
            length: conductor.steps.len(),
            steps: (conductor.steps.iter().enumerate())
                .map(|(at, step)| (at, step.iter().flatten().copied().collect::<Vec<_>>()))
                .filter(|(_, gestures)| !gestures.is_empty())
                .collect(),
        }
    }
}

impl TryFrom<SparseConductor> for Conductor {
    type Error = String;

    fn try_from(sparse: SparseConductor) -> Result<Self, String> {
        if sparse.length > MAX_STEPS {
            return Err(format!("the conductor has 0 to {MAX_STEPS} steps"));
        }
        let mut conductor = Conductor::new(sparse.length);
        for (at, gestures) in sparse.steps {
            let step = conductor.steps.get_mut(at).ok_or_else(|| {
                format!(
                    "the conductor has a step at {at}, past its length of {}",
                    sparse.length
                )
            })?;
            if gestures.len() > GESTURES {
                return Err(format!(
                    "the conductor's step {at} has more than {GESTURES} gestures"
                ));
            }
            for (field, gesture) in step.iter_mut().zip(gestures) {
                *field = Some(gesture);
            }
        }
        Ok(conductor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_form_listed_is_a_gesture() {
        for (example, _) in FORMS {
            assert!(example.parse::<SongGesture>().is_ok(), "{example}");
        }
    }

    #[test]
    fn gestures_read_as_they_are_written() {
        for text in [
            "T140", "~96", "T--", "X", "P3", "M2", "M-2", "S16", "S-16", "KC", "KF#", "KBb", "K+2",
            "K-11", "Dor", "Maj", "Hmi", "In", "Chr",
        ] {
            let gesture: SongGesture = text.parse().unwrap();
            assert_eq!(gesture.to_string(), text);
        }
        assert_eq!("t140".parse(), Ok(SongGesture::Tempo(140)));
        assert_eq!("p12".parse(), Ok(SongGesture::Pattern(12)));
        assert!(
            "T10"
                .parse::<SongGesture>()
                .unwrap_err()
                .contains("20 to 400")
        );
        assert!("S17".parse::<SongGesture>().is_err());
        assert!("P0".parse::<SongGesture>().is_err());
        assert!("Q1".parse::<SongGesture>().is_err());
        assert!("T".parse::<SongGesture>().is_err());
        // As typed, in capitals.
        assert_eq!("KBB".parse(), Ok(SongGesture::Key(KeyChange::Root(10))));
        assert_eq!("DOR".parse(), Ok(SongGesture::Scale(2)));
        assert_eq!("kf#".parse(), Ok(SongGesture::Key(KeyChange::Root(6))));
        assert!(
            "K+12"
                .parse::<SongGesture>()
                .unwrap_err()
                .contains("1 to 11")
        );
        assert!("K+0".parse::<SongGesture>().is_err());
        assert!(
            "KH".parse::<SongGesture>()
                .unwrap_err()
                .contains("as in KG")
        );
        assert!("Dorian".parse::<SongGesture>().is_err());
        assert_eq!(KeyChange::Shift(-3).from(1), 10);
        assert_eq!(KeyChange::Root(7).from(1), 7);
    }

    #[test]
    fn typing_goes_on_until_a_gesture_is_whole() {
        assert!(SongGesture::could_go_on("T"));
        assert!(SongGesture::could_go_on("T14"));
        assert!(!SongGesture::could_go_on("T140"));
        assert!(SongGesture::could_go_on("T-"));
        assert!(!SongGesture::could_go_on("T--"));
        assert!(!SongGesture::could_go_on("X"));
        assert!(SongGesture::could_go_on("P1"));
        assert!(!SongGesture::could_go_on("P12"));
        assert!(SongGesture::could_go_on("M-"));
        assert!(SongGesture::could_go_on("M-1"));
        assert!(!SongGesture::could_go_on("M-16"));
        for text in ["K", "KF", "K+", "K-1", "D", "DO", "MI", "P", "PE", "H", "I"] {
            assert!(SongGesture::could_go_on(text), "{text}");
        }
        for text in ["KF#", "K+11", "DOR", "MIX", "PEN", "IN", "Q"] {
            assert!(!SongGesture::could_go_on(text), "{text}");
        }
        assert_eq!(
            "m-3".parse(),
            Ok(SongGesture::Mute {
                track: 3,
                on: false
            })
        );
    }
}
