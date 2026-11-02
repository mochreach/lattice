//! A track's gestures: things that happen once on a step, written in the step's two gesture
//! fields. Note gestures act on the step's note, or the one still sounding: retrigger it, delay
//! it, cut it, arpeggiate it or accent it, and on some instruments reverse it, start it part way
//! in or glide to it. Playhead gestures act on the track's playback: jump,
//! wait, turn backwards, or change speed. Times are in 24ths of a step.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::instrument::InstKind;
use crate::song::MAX_STEPS;

/// The 24ths of a step a delay or cut is timed in.
pub const TICKS: u8 = 24;

/// The widest a gesture is written, as its field shows it.
pub const WIDTH: usize = 4;

/// Most hits a retrigger plays in its step.
pub const MAX_HITS: u8 = 9;

/// Fastest and slowest a track plays, as many times or as a share of the song's speed.
pub const MAX_SPEED: u8 = 4;

/// The longest a step waits, in steps' time.
pub const MAX_WAIT: u8 = 99;

/// The sixteenths of a sound file, or a chop's piece, an offset starts a note in.
pub const OFFSETS: u8 = 16;

/// Something a track does once, on a step. Written as it's typed: `R3`, `r3`, `D8`, `X6`, `A37`,
/// `!`, `<`, `O8`, `G6`, `J12`, `J?`, `W2`, `B`, `*2`, `/2`. Steps are numbered from 0, as they're shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// `R3`: plays the note 3 times, evenly, within the step.
    Retrigger(u8),
    /// `r3`: as `R3`, each hit quieter than the last.
    Fade(u8),
    /// `D8`: starts the note 8 24ths of a step late.
    Delay(u8),
    /// `X6`: ends the note 6 24ths into the step.
    Cut(u8),
    /// `A37`: plays the note, then 3 and 7 semitones up, in turn, a sixth of a step each.
    Arpeggio(u8, u8),
    /// `!`: plays the note at full velocity.
    Accent,
    /// `<`: plays a sampler's or chop's note the other way round: backwards, or forwards if
    /// its direction is reverse.
    Reverse,
    /// `O8`: starts a sampler's or chop's note 8 sixteenths into its sound, or its piece.
    Offset(u8),
    /// `G6`: slides a synth's or string's note from the last one's pitch over 6 24ths of a step.
    Glide(u8),
    /// `J12`: after this step, carries on from step 12.
    Jump(u8),
    /// `J?`: after this step, carries on from any step, the same one each time the song plays.
    Wander,
    /// `W2`: stays on this step for 2 more steps' time, playing it again each time.
    Wait(u8),
    /// `B`: plays the track backwards from here, or forwards again if it was going backwards.
    Backwards,
    /// `*2`: plays the track twice as fast from here, until the next speed or the top of its
    /// loop. `*1` is the song's speed.
    Faster(u8),
    /// `/2`: plays the track at half speed, likewise.
    Slower(u8),
}

impl Gesture {
    /// Whether it moves the track's playhead, rather than acting on a note.
    pub fn is_playhead(self) -> bool {
        matches!(
            self,
            Gesture::Jump(_)
                | Gesture::Wander
                | Gesture::Wait(_)
                | Gesture::Backwards
                | Gesture::Faster(_)
                | Gesture::Slower(_)
        )
    }

    /// Whether it does anything on an instrument of `kind`: `<` on a synth doesn't.
    pub fn works_on(self, kind: InstKind) -> bool {
        match self {
            Gesture::Reverse | Gesture::Offset(_) => {
                matches!(kind, InstKind::Sampler | InstKind::Chop)
            }
            Gesture::Glide(_) => matches!(kind, InstKind::Synth | InstKind::String),
            _ => true,
        }
    }

    /// Whether more could be typed after `text` to make another gesture: a `D2` might become
    /// `D23`, and `J1` `J12`, but `R3`, `D3`, `!` and `J?` are done.
    pub fn could_go_on(text: &str) -> bool {
        text.len() < WIDTH
            && ('0'..='9').chain(['?']).any(|c| {
                let longer = format!("{text}{c}");
                longer.parse::<Gesture>().is_ok() || Gesture::could_go_on(&longer)
            })
    }

    /// What it does, for the status line: `R3: retrigger, 3 hits`.
    pub fn describe(self) -> String {
        let what = match self {
            Gesture::Retrigger(n) => format!("retrigger, {n} hits"),
            Gesture::Fade(n) => format!("fading retrigger, {n} hits"),
            Gesture::Delay(n) => format!("delay, {n}/24 of a step late"),
            Gesture::Cut(n) => format!("cut, {n}/24 into the step"),
            Gesture::Arpeggio(a, b) => format!("arpeggio, the note, +{a} and +{b}"),
            Gesture::Accent => "accent, full velocity".into(),
            Gesture::Reverse => "reverse, played the other way round".into(),
            Gesture::Offset(n) => format!("offset, from {n}/16 of the way in"),
            Gesture::Glide(n) => format!("glide, from the last pitch over {n}/24 of a step"),
            Gesture::Jump(n) => format!("jump, on from step {n}"),
            Gesture::Wander => "jump to any step".into(),
            Gesture::Wait(n) => format!("wait, {n} more steps' time here"),
            Gesture::Backwards => "backwards, or forwards again".into(),
            Gesture::Faster(1) | Gesture::Slower(1) => "the song's speed".into(),
            Gesture::Faster(n) => format!("{n} times as fast"),
            Gesture::Slower(n) => format!("1/{n} as fast"),
        };
        format!("{self}: {what}")
    }

    /// Checks it's one Lattice can play.
    pub fn check(self) -> Result<(), String> {
        let within = |n: u8, low: u8, high: u8, what: &str| match (low..=high).contains(&n) {
            true => Ok(()),
            false => Err(format!("{what} go from {low} to {high}")),
        };
        match self {
            Gesture::Retrigger(n) | Gesture::Fade(n) => within(n, 2, MAX_HITS, "retriggers"),
            Gesture::Delay(n) | Gesture::Cut(n) | Gesture::Glide(n) => {
                within(n, 1, TICKS - 1, "24ths of a step")
            }
            Gesture::Offset(n) => within(n, 0, OFFSETS - 1, "sixteenths"),
            Gesture::Arpeggio(a, b) => {
                within(a, 0, 9, "arpeggio steps")?;
                within(b, 0, 9, "arpeggio steps")
            }
            Gesture::Wait(n) => within(n, 1, MAX_WAIT, "waits"),
            Gesture::Faster(n) | Gesture::Slower(n) => within(n, 1, MAX_SPEED, "speeds"),
            Gesture::Jump(n) if usize::from(n) >= MAX_STEPS => {
                Err(format!("steps go from 0 to {}", MAX_STEPS - 1))
            }
            _ => Ok(()),
        }
    }
}

impl fmt::Display for Gesture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Gesture::Retrigger(n) => write!(f, "R{n}"),
            Gesture::Fade(n) => write!(f, "r{n}"),
            Gesture::Delay(n) => write!(f, "D{n}"),
            Gesture::Cut(n) => write!(f, "X{n}"),
            Gesture::Arpeggio(a, b) => write!(f, "A{a}{b}"),
            Gesture::Accent => write!(f, "!"),
            Gesture::Reverse => write!(f, "<"),
            Gesture::Offset(n) => write!(f, "O{n}"),
            Gesture::Glide(n) => write!(f, "G{n}"),
            Gesture::Jump(n) => write!(f, "J{n}"),
            Gesture::Wander => write!(f, "J?"),
            Gesture::Wait(n) => write!(f, "W{n}"),
            Gesture::Backwards => write!(f, "B"),
            Gesture::Faster(n) => write!(f, "*{n}"),
            Gesture::Slower(n) => write!(f, "/{n}"),
        }
    }
}

impl std::str::FromStr for Gesture {
    type Err = String;

    /// Reads a gesture as it's written, and checks it. Letters may be either case but `r`,
    /// which fades where `R` doesn't.
    fn from_str(text: &str) -> Result<Self, String> {
        let unknown = || {
            format!(
                "{text:?} isn't a gesture: R3, r3, D8, X6, A37, !, <, O8, G6, J12, J?, W2, B, *2 or /2"
            )
        };
        let mut chars = text.chars();
        let first = chars.next().ok_or_else(unknown)?;
        let rest = chars.as_str();
        let first = match first {
            'r' => 'r',
            c => c.to_ascii_uppercase(),
        };
        let number = |example: &str| -> Result<u8, String> {
            // One way to write each: `J0`, not `J00`.
            if rest.len() > 1 && rest.starts_with('0') {
                return Err(unknown());
            }
            match rest.chars().all(|c| c.is_ascii_digit()) {
                true => rest.parse::<u8>().map_err(|_| match rest.is_empty() {
                    true => format!("{text:?}: a number after {first}, as in {example}"),
                    false => unknown(),
                }),
                false => Err(unknown()),
            }
        };
        let gesture = match first {
            'R' => Gesture::Retrigger(number("R3")?),
            'r' => Gesture::Fade(number("r3")?),
            'D' => Gesture::Delay(number("D8")?),
            'X' => Gesture::Cut(number("X6")?),
            'A' => {
                let digits: Vec<u8> = rest.bytes().map(|b| b.wrapping_sub(b'0')).collect();
                match digits.as_slice() {
                    &[a, b] if a < 10 && b < 10 => Gesture::Arpeggio(a, b),
                    _ => return Err(format!("{text:?}: two digits after A, as in A37")),
                }
            }
            '!' if rest.is_empty() => Gesture::Accent,
            '<' if rest.is_empty() => Gesture::Reverse,
            'O' => Gesture::Offset(number("O8")?),
            'G' => Gesture::Glide(number("G6")?),
            'J' if rest == "?" => Gesture::Wander,
            'J' => Gesture::Jump(number("J12")?),
            'W' => Gesture::Wait(number("W2")?),
            'B' if rest.is_empty() => Gesture::Backwards,
            '*' => Gesture::Faster(number("*2")?),
            '/' => Gesture::Slower(number("/2")?),
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

/// A step's gesture fields as a file has them: a list, without the empty fields at its end, and
/// `""` for an empty one before a full one: `["R3"]`, `["", "J0"]`.
pub(crate) mod fields {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::Gesture;
    use crate::conductor::GESTURES;

    pub fn serialize<S: Serializer>(
        fields: &[Option<Gesture>; GESTURES],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let used = fields
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |i| i + 1);
        let written: Vec<String> = (fields.iter().take(used))
            .map(|g| g.map(|g| g.to_string()).unwrap_or_default())
            .collect();
        written.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[Option<Gesture>; GESTURES], D::Error> {
        let written = Vec::<String>::deserialize(deserializer)?;
        if written.len() > GESTURES {
            return Err(serde::de::Error::custom(format!(
                "a step has {GESTURES} gesture fields"
            )));
        }
        let mut fields = [None; GESTURES];
        for (field, text) in fields.iter_mut().zip(&written) {
            if !text.is_empty() {
                *field = Some(text.parse().map_err(serde::de::Error::custom)?);
            }
        }
        Ok(fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gestures_read_as_they_are_written() {
        let all = [
            "R3", "r9", "D8", "X23", "A37", "A05", "!", "<", "O0", "O15", "G6", "J12", "J?", "W2",
            "B", "*2", "/4", "*1",
        ];
        for text in all {
            let gesture: Gesture = text.parse().unwrap();
            assert_eq!(gesture.to_string(), text);
        }
        assert_eq!("d8".parse(), Ok(Gesture::Delay(8)));
        assert_eq!("r3".parse(), Ok(Gesture::Fade(3)));
        assert_eq!("j255".parse(), Ok(Gesture::Jump(255)));
        assert!("R1".parse::<Gesture>().unwrap_err().contains("2 to 9"));
        assert!("D24".parse::<Gesture>().unwrap_err().contains("1 to 23"));
        assert!("*5".parse::<Gesture>().is_err());
        assert!("A3".parse::<Gesture>().unwrap_err().contains("two digits"));
        assert!("J256".parse::<Gesture>().is_err());
        assert!("D".parse::<Gesture>().unwrap_err().contains("as in D8"));
        assert!("Q1".parse::<Gesture>().is_err());
        assert!("D-1".parse::<Gesture>().is_err());
        assert!("J00".parse::<Gesture>().is_err());
        assert!("O16".parse::<Gesture>().unwrap_err().contains("0 to 15"));
        assert!("G0".parse::<Gesture>().is_err());
        assert_eq!("o8".parse(), Ok(Gesture::Offset(8)));
        assert!(!Gesture::Reverse.works_on(InstKind::Synth));
        assert!(Gesture::Reverse.works_on(InstKind::Chop));
        assert!(Gesture::Glide(3).works_on(InstKind::String));
        assert!(!Gesture::Glide(3).works_on(InstKind::Kick));
    }

    #[test]
    fn typing_goes_on_until_a_gesture_is_whole() {
        for text in [
            "R", "D", "D2", "X1", "A", "A3", "J", "J1", "J25", "W", "W9", "*", "O", "O1", "G2",
        ] {
            assert!(Gesture::could_go_on(text), "{text}");
        }
        for text in [
            "R3", "D3", "D23", "A37", "!", "J0", "J?", "J26", "J255", "W99", "B", "*2", "<", "O2",
            "O15", "G3",
        ] {
            assert!(!Gesture::could_go_on(text), "{text}");
        }
    }
}
