//! A short demo song for hearing the engine and the pattern transforms.

use lattice_core::{Condition, Grid, Rule, Step, Track, Transform};
use lattice_engine::{AdsrParams, Patch, Wave};

pub const BPM: f64 = 120.0;

/// Parses a pattern like `"C2 . . C2"`: one note name or `.` per step.
fn pattern(s: &str) -> Vec<Step> {
    s.split_whitespace()
        .map(|tok| match tok {
            "." => Step::default(),
            name => Step::note(note_number(name).unwrap_or_else(|| panic!("bad note {name}"))),
        })
        .collect()
}

/// `"C4"` → 60, `"Eb3"` → 51.
fn note_number(name: &str) -> Option<u8> {
    let mut chars = name.chars();
    let base = match chars.next()? {
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
    let (shift, octave) = match rest.strip_prefix('#') {
        Some(o) => (1, o),
        None => match rest.strip_prefix('b') {
            Some(o) => (-1, o),
            None => (0, rest),
        },
    };
    let octave: i32 = octave.parse().ok()?;
    u8::try_from((octave + 1) * 12 + base + shift).ok()
}

fn rule(transform: Transform, when: Condition) -> Rule {
    Rule { transform, when }
}

/// i–i–iv–v over four passes: every track follows the same chord changes.
fn chord_changes() -> Vec<Rule> {
    vec![
        rule(
            Transform::Transpose(5),
            Condition::Every { n: 4, offset: 2 },
        ),
        rule(
            Transform::Transpose(7),
            Condition::Every { n: 4, offset: 3 },
        ),
    ]
}

/// A new, empty song for the demo's instruments: one bar on each track, named after them.
pub fn blank() -> Grid {
    let (mut grid, _) = song();
    for track in &mut grid.tracks {
        track.steps = vec![Step::default(); 16];
        track.rules.clear();
    }
    grid.name = "new".into();
    grid
}

pub fn song() -> (Grid, Vec<Patch>) {
    let bass = Track {
        name: "bass".into(),
        steps: pattern("C2 . . C2 . . C3 . C2 . G2 C2 . . Bb2 ."),
        mix: Default::default(),
        rules: chord_changes(),
    };

    let mut arp_rules = vec![rule(
        Transform::Reverse,
        Condition::Every { n: 2, offset: 1 },
    )];
    arp_rules.extend(chord_changes());
    let arp = Track {
        name: "arp".into(),
        steps: pattern("C4 Eb4 G4 C5 C4 Eb4 G4 C5 C4 Eb4 G4 C5 Bb4 G4 Eb4 G4"),
        mix: Default::default(),
        rules: arp_rules,
    };

    // Six steps against sixteen, drifting one step further each pass.
    let bleep = Track {
        name: "bleep".into(),
        steps: pattern("G5 . . Eb5 . ."),
        mix: Default::default(),
        rules: vec![rule(Transform::Drift(1), Condition::Always)],
    };

    let grid = Grid {
        name: "demo".into(),
        steps_per_beat: 4,
        tracks: vec![bass, arp, bleep],
    };

    let patches = vec![
        Patch {
            wave: Wave::Square,
            amp: AdsrParams {
                attack: 0.003,
                decay: 0.15,
                sustain: 0.4,
                release: 0.06,
            },
            cutoff: 250.0,
            env_amount: 1_800.0,
            resonance: 0.45,
            gain: 0.45,
        },
        Patch {
            wave: Wave::Saw,
            amp: AdsrParams {
                attack: 0.002,
                decay: 0.1,
                sustain: 0.15,
                release: 0.12,
            },
            cutoff: 900.0,
            env_amount: 4_500.0,
            resonance: 0.25,
            gain: 0.16,
        },
        Patch {
            wave: Wave::Square,
            amp: AdsrParams {
                attack: 0.001,
                decay: 0.05,
                sustain: 0.0,
                release: 0.2,
            },
            cutoff: 3_000.0,
            env_amount: 2_000.0,
            resonance: 0.1,
            gain: 0.1,
        },
    ];

    (grid, patches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_names() {
        assert_eq!(note_number("C4"), Some(60));
        assert_eq!(note_number("Eb3"), Some(51));
        assert_eq!(note_number("C#-1"), Some(1));
        assert_eq!(note_number("H2"), None);
    }

    #[test]
    fn demo_builds() {
        let (grid, patches) = song();
        assert_eq!(grid.tracks.len(), patches.len());
    }
}
