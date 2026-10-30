//! Note patterns, which `s`, `K` and `A-K` match against the notes in the selections, much as
//! Helix matches a regex against the text.

/// One or more notes separated by spaces, any of which may match: `C`, `C#`, `Eb4`, `C-4`, or
/// `*` for any note (`*4` for any note in octave 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern(Vec<NoteName>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NoteName {
    /// Semitones above C, or `None` for any.
    class: Option<u8>,
    /// As the grid shows it, so `C-4` is MIDI note 60; `None` for any.
    octave: Option<u8>,
}

impl Pattern {
    pub fn parse(text: &str) -> Result<Self, String> {
        let names = text
            .split_whitespace()
            .map(NoteName::parse)
            .collect::<Result<Vec<_>, _>>()?;
        if names.is_empty() {
            return Err("which notes?".into());
        }
        Ok(Self(names))
    }

    pub fn matches(&self, pitch: u8) -> bool {
        self.0.iter().any(|n| {
            n.class.is_none_or(|c| pitch % 12 == c) && n.octave.is_none_or(|o| pitch / 12 == o + 1)
        })
    }
}

impl NoteName {
    fn parse(word: &str) -> Result<Self, String> {
        let invalid = || format!("not a note: {word}");
        let mut chars = word.chars().peekable();
        let class = match chars.next().ok_or_else(invalid)?.to_ascii_uppercase() {
            '*' => None,
            letter => {
                let natural = match letter {
                    'C' => 0,
                    'D' => 2,
                    'E' => 4,
                    'F' => 5,
                    'G' => 7,
                    'A' => 9,
                    'B' => 11,
                    _ => return Err(invalid()),
                };
                let accidental = match chars.peek() {
                    Some('#') => 1,
                    Some('b') => 11,
                    _ => 0,
                };
                if accidental != 0 {
                    chars.next();
                }
                Some((natural + accidental) % 12)
            }
        };
        // Tracker spelling puts a `-` where there's no sharp: `C-4`.
        chars.next_if_eq(&'-');
        let rest: String = chars.collect();
        let octave = match rest.as_str() {
            "" => None,
            digits => Some(
                digits
                    .parse::<u8>()
                    .ok()
                    .filter(|&o| o <= 9)
                    .ok_or_else(invalid)?,
            ),
        };
        Ok(Self { class, octave })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matching(pattern: &str) -> Vec<u8> {
        let pattern = Pattern::parse(pattern).unwrap();
        (0..=127).filter(|&p| pattern.matches(p)).collect()
    }

    #[test]
    fn note_patterns() {
        assert_eq!(matching("C-4"), [60]);
        assert_eq!(matching("c4"), [60]);
        assert_eq!(matching("C#4"), [61]);
        assert_eq!(matching("Db4"), [61]);
        assert_eq!(matching("bb4"), [70], "lower-case b is still a note");
        assert_eq!(matching("C").len(), 11);
        assert!(matching("C").iter().all(|p| p % 12 == 0));
        assert_eq!(matching("*").len(), 128);
        assert_eq!(matching("*4"), (60..72).collect::<Vec<_>>());
        assert_eq!(matching("C-4 E-4 G-4"), [60, 64, 67]);
        assert!(Pattern::parse("H").is_err());
        assert!(Pattern::parse("C12").is_err());
        assert!(Pattern::parse(" ").is_err());
    }
}
