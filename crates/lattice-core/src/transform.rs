//! Non-destructive pattern transforms, applied each time a track loops.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::song::{Step, Track};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transform {
    /// Shift steps later by `n` (earlier if negative), wrapping around.
    Rotate(i32),
    /// Rotate by `n` more on every pass where the rule holds, building up over time.
    Drift(i32),
    Reverse,
    /// Shift every note by `n` semitones, clamped to the MIDI range.
    Transpose(i8),
}

/// Which passes a rule applies on. Passes count from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Condition {
    #[default]
    Always,
    /// Passes where `pass % n == offset`.
    Every { n: u32, offset: u32 },
    /// Only the given pass.
    Pass(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub transform: Transform,
    #[serde(default, skip_serializing_if = "Condition::is_always")]
    pub when: Condition,
}

impl Condition {
    fn is_always(&self) -> bool {
        *self == Condition::Always
    }

    pub fn holds(self, pass: u32) -> bool {
        match self {
            Condition::Always => true,
            Condition::Every { n, offset } => n != 0 && pass % n == offset % n,
            Condition::Pass(k) => pass == k,
        }
    }

    /// How many passes in `0..=pass` this condition held on.
    fn count_through(self, pass: u32) -> u32 {
        match self {
            Condition::Always => pass + 1,
            Condition::Every { n: 0, .. } => 0,
            Condition::Every { n, offset } => {
                let offset = offset % n;
                if pass < offset {
                    0
                } else {
                    (pass - offset) / n + 1
                }
            }
            Condition::Pass(k) => u32::from(pass >= k),
        }
    }
}

impl Transform {
    fn apply(self, steps: &mut [Step], times: u32) {
        match self {
            Transform::Rotate(n) => rotate(steps, i64::from(n)),
            Transform::Drift(n) => rotate(steps, i64::from(n) * i64::from(times)),
            Transform::Reverse => steps.reverse(),
            Transform::Transpose(n) => {
                for note in steps.iter_mut().filter_map(|s| s.note.as_mut()) {
                    note.pitch = (i16::from(note.pitch) + i16::from(n)).clamp(0, 127) as u8;
                }
            }
        }
    }
}

fn rotate(steps: &mut [Step], by: i64) {
    if steps.is_empty() {
        return;
    }
    let k = by.rem_euclid(steps.len() as i64) as usize;
    steps.rotate_right(k);
}

/// The steps that actually play on the given pass through `track`.
///
/// The written pattern is never modified, so the same track and pass always give the same result.
pub fn realise(track: &Track, pass: u32) -> Vec<Step> {
    let mut steps = Vec::with_capacity(track.steps.len());
    realise_into(track, pass, &mut steps);
    steps
}

/// Like [`realise`], but writes into `out`, reusing its allocation. Doesn't allocate if `out`
/// already has capacity for the track, so it's safe to call from the audio thread.
pub fn realise_into(track: &Track, pass: u32, out: &mut Vec<Step>) {
    out.clear();
    out.extend_from_slice(&track.steps);
    for rule in &track.rules {
        if rule.when.holds(pass) {
            rule.transform.apply(out, rule.when.count_through(pass));
        }
    }
}

/// Compact notation, as shown in the tracker: `rev/2+1`, `tr+5@3`, `drift+1`.
impl fmt::Display for Transform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Transform::Rotate(n) => write!(f, "rot{n:+}"),
            Transform::Drift(n) => write!(f, "drift{n:+}"),
            Transform::Reverse => write!(f, "rev"),
            Transform::Transpose(n) => write!(f, "tr{n:+}"),
        }
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Condition::Always => Ok(()),
            Condition::Every { n, offset: 0 } => write!(f, "/{n}"),
            Condition::Every { n, offset } => write!(f, "/{n}+{offset}"),
            Condition::Pass(k) => write!(f, "@{k}"),
        }
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.transform, self.when)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pitches(steps: &[Step]) -> Vec<Option<u8>> {
        steps.iter().map(|s| s.note.map(|n| n.pitch)).collect()
    }

    fn track(rules: Vec<Rule>) -> Track {
        Track {
            name: "test".into(),
            steps: vec![
                Step::note(60),
                Step::default(),
                Step::note(64),
                Step::note(67),
            ],
            mix: Default::default(),
            rules,
        }
    }

    fn rule(transform: Transform, when: Condition) -> Rule {
        Rule { transform, when }
    }

    #[test]
    fn no_rules_plays_as_written() {
        let t = track(vec![]);
        assert_eq!(realise(&t, 0), t.steps);
        assert_eq!(realise(&t, 7), t.steps);
    }

    #[test]
    fn rotate_wraps_both_ways() {
        let t = track(vec![rule(Transform::Rotate(1), Condition::Always)]);
        assert_eq!(
            pitches(&realise(&t, 0)),
            [Some(67), Some(60), None, Some(64)]
        );
        let t = track(vec![rule(Transform::Rotate(-5), Condition::Always)]);
        assert_eq!(
            pitches(&realise(&t, 0)),
            [None, Some(64), Some(67), Some(60)]
        );
    }

    #[test]
    fn drift_accumulates_per_pass() {
        let t = track(vec![rule(Transform::Drift(1), Condition::Always)]);
        assert_eq!(
            pitches(&realise(&t, 0)),
            [Some(67), Some(60), None, Some(64)]
        );
        assert_eq!(
            pitches(&realise(&t, 1)),
            [Some(64), Some(67), Some(60), None]
        );
        // A full cycle comes back to the written pattern.
        assert_eq!(realise(&t, 3), t.steps);
    }

    #[test]
    fn reverse_every_second_pass() {
        let t = track(vec![rule(
            Transform::Reverse,
            Condition::Every { n: 2, offset: 1 },
        )]);
        assert_eq!(realise(&t, 0), t.steps);
        assert_eq!(
            pitches(&realise(&t, 1)),
            [Some(67), Some(64), None, Some(60)]
        );
        assert_eq!(realise(&t, 2), t.steps);
    }

    #[test]
    fn transpose_on_one_pass_clamps() {
        let t = track(vec![rule(Transform::Transpose(64), Condition::Pass(2))]);
        assert_eq!(realise(&t, 1), t.steps);
        assert_eq!(
            pitches(&realise(&t, 2)),
            [Some(124), None, Some(127), Some(127)]
        );
    }

    #[test]
    fn rules_apply_in_order() {
        let t = track(vec![
            rule(Transform::Reverse, Condition::Always),
            rule(Transform::Rotate(1), Condition::Always),
        ]);
        assert_eq!(
            pitches(&realise(&t, 0)),
            [Some(60), Some(67), Some(64), None]
        );
    }

    #[test]
    fn rules_display_compactly() {
        let shown: Vec<String> = [
            rule(Transform::Reverse, Condition::Every { n: 2, offset: 1 }),
            rule(Transform::Transpose(5), Condition::Pass(3)),
            rule(Transform::Drift(-1), Condition::Always),
            rule(Transform::Rotate(2), Condition::Every { n: 4, offset: 0 }),
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(shown, ["rev/2+1", "tr+5@3", "drift-1", "rot+2/4"]);
    }

    #[test]
    fn source_is_untouched() {
        let t = track(vec![rule(Transform::Reverse, Condition::Always)]);
        let before = t.clone();
        realise(&t, 0);
        assert_eq!(t, before);
    }
}
