//! The conductor: a lane of each pattern for what belongs to the whole song rather than one
//! track. Each step has two gesture fields, things that happen once on it: set the tempo or slide
//! to one, stop, cue a pattern, mute or solo a track. A tempo holds until the next, as a lock
//! column's value does, but it's the song's only such value, so it's a gesture.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::arrangement::MAX_PATTERNS;
use crate::song::{MAX_STEPS, MAX_TRACKS};

/// The slowest and fastest tempos, in BPM, as `:bpm` allows.
pub const MIN_BPM: u16 = 20;
pub const MAX_BPM: u16 = 400;

/// Gesture fields on each of the conductor's steps.
pub const GESTURES: usize = 2;

/// Something the conductor does once, on a step. Written as it's typed: `T140`, `~140`, `T--`,
/// `X`, `P3`, `M2` `M-2`, `S2` `S-2`. Patterns and tracks are numbered from 1, as they're shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
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
}

impl Gesture {
    /// The tempo it sets or slides to, if it's a tempo.
    pub fn tempo(self) -> Option<u16> {
        match self {
            Gesture::Tempo(bpm) | Gesture::Slide(bpm) => Some(bpm),
            _ => None,
        }
    }

    /// Whether it changes the tempo.
    pub fn is_tempo(self) -> bool {
        matches!(
            self,
            Gesture::Tempo(_) | Gesture::Slide(_) | Gesture::SongTempo
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
            Some('P' | 'p') => digits(rest, 2),
            Some('M' | 'm' | 'S' | 's') => digits(rest.strip_prefix('-').unwrap_or(rest), 2),
            _ => false,
        }
    }

    /// Checks it's one Lattice can play.
    pub fn check(self) -> Result<(), String> {
        let bpm = MIN_BPM..=MAX_BPM;
        match self {
            Gesture::Tempo(t) | Gesture::Slide(t) if !bpm.contains(&t) => {
                Err(format!("tempos go from {MIN_BPM} to {MAX_BPM}"))
            }
            Gesture::Pattern(p) if !(1..=MAX_PATTERNS).contains(&usize::from(p)) => {
                Err(format!("patterns go from 1 to {MAX_PATTERNS}"))
            }
            Gesture::Mute { track: t, .. } | Gesture::Solo { track: t, .. }
                if !(1..=MAX_TRACKS).contains(&usize::from(t)) =>
            {
                Err(format!("tracks go from 1 to {MAX_TRACKS}"))
            }
            _ => Ok(()),
        }
    }
}

impl fmt::Display for Gesture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Gesture::Tempo(bpm) => write!(f, "T{bpm}"),
            Gesture::Slide(bpm) => write!(f, "~{bpm}"),
            Gesture::SongTempo => write!(f, "T--"),
            Gesture::Stop => write!(f, "X"),
            Gesture::Pattern(p) => write!(f, "P{p}"),
            Gesture::Mute { track, on } => write!(f, "M{}{track}", if *on { "" } else { "-" }),
            Gesture::Solo { track, on } => write!(f, "S{}{track}", if *on { "" } else { "-" }),
        }
    }
}

impl std::str::FromStr for Gesture {
    type Err = String;

    /// Reads a gesture as it's written, upper or lower case, and checks it.
    fn from_str(text: &str) -> Result<Self, String> {
        let unknown = || {
            format!(
                "{text:?} isn't a conductor's gesture: T140, ~140, T--, X, P3, M2, M-2, S2 or S-2"
            )
        };
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
            'T' if rest == "--" => Gesture::SongTempo,
            'T' => Gesture::Tempo(number("a tempo")?),
            '~' => Gesture::Slide(number("a tempo")?),
            'X' if rest.is_empty() => Gesture::Stop,
            'P' => Gesture::Pattern(small(number("a pattern")?)?),
            'M' => Gesture::Mute {
                track: small(number("a track")?)?,
                on,
            },
            'S' => Gesture::Solo {
                track: small(number("a track")?)?,
                on,
            },
            _ => return Err(unknown()),
        };
        gesture.check()?;
        Ok(gesture)
    }
}

impl Serialize for Gesture {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Gesture {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// A step of the conductor: its two gesture fields.
pub type Gestures = [Option<Gesture>; GESTURES];

/// A pattern's conductor lane. It has its own length, so a tempo change can come round every
/// bar or once a song; with no steps, it's as long as the pattern and does nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SparseConductor", into = "SparseConductor")]
pub struct Conductor {
    pub steps: Vec<Gestures>,
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
    pub fn get(&self, at: usize, field: usize) -> Option<Gesture> {
        self.steps.get(at)?.get(field).copied().flatten()
    }

    /// Every gesture, with its step, in order.
    pub fn gestures(&self) -> impl Iterator<Item = (usize, Gesture)> + '_ {
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
    steps: BTreeMap<usize, Vec<Gesture>>,
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
    fn gestures_read_as_they_are_written() {
        for text in ["T140", "~96", "T--", "X", "P3", "M2", "M-2", "S16", "S-16"] {
            let gesture: Gesture = text.parse().unwrap();
            assert_eq!(gesture.to_string(), text);
        }
        assert_eq!("t140".parse(), Ok(Gesture::Tempo(140)));
        assert_eq!("p12".parse(), Ok(Gesture::Pattern(12)));
        assert!("T10".parse::<Gesture>().unwrap_err().contains("20 to 400"));
        assert!("S17".parse::<Gesture>().is_err());
        assert!("P0".parse::<Gesture>().is_err());
        assert!("Q1".parse::<Gesture>().is_err());
        assert!("T".parse::<Gesture>().is_err());
    }

    #[test]
    fn typing_goes_on_until_a_gesture_is_whole() {
        assert!(Gesture::could_go_on("T"));
        assert!(Gesture::could_go_on("T14"));
        assert!(!Gesture::could_go_on("T140"));
        assert!(Gesture::could_go_on("T-"));
        assert!(!Gesture::could_go_on("T--"));
        assert!(!Gesture::could_go_on("X"));
        assert!(Gesture::could_go_on("P1"));
        assert!(!Gesture::could_go_on("P12"));
        assert!(Gesture::could_go_on("M-"));
        assert!(Gesture::could_go_on("M-1"));
        assert!(!Gesture::could_go_on("M-16"));
        assert_eq!(
            "m-3".parse(),
            Ok(Gesture::Mute {
                track: 3,
                on: false
            })
        );
    }
}
