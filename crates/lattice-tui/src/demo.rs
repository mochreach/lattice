//! A short demo song for hearing the engine and the pattern transforms.

use lattice_core::fx::{comp, delay, drive};
use lattice_core::instrument::{hat, kick, snare, synth};
use lattice_core::{
    Bus, Buses, Condition, DRUM_ROOT, Fx, FxKind, Grid, Group, InstKind, Instrument, Note, Rule,
    Step, Track, Transform,
};

/// The Amen break's own tempo.
pub const BPM: f64 = 136.0;

/// Parses a drum line like `"X..g|x..."`: a step per character, `X` an accent, `x` a hit, `m`
/// a medium hit, `g` a ghost note and `.` a rest. `|` and spaces are left out, to mark bars.
fn hits(s: &str) -> Vec<Step> {
    s.chars()
        .filter(|c| !matches!(c, '|' | ' '))
        .map(|c| {
            let velocity = match c {
                'X' => 120,
                'x' => 100,
                'm' => 80,
                'g' => 55,
                '.' => return Step::default(),
                _ => panic!("bad hit {c:?}"),
            };
            Step {
                note: Some(Note {
                    pitch: DRUM_ROOT,
                    velocity,
                    length: 1,
                }),
            }
        })
        .collect()
}

/// A drum track playing `line` (see [`hits`]) on `instrument` with these settings.
fn drum(name: &str, line: &str, kind: InstKind, settings: &[(u32, f32)]) -> Track {
    let mut instrument = Instrument::new(kind);
    for &(id, value) in settings {
        instrument.values[id as usize] = value;
    }
    Track {
        name: name.into(),
        steps: hits(line),
        instrument,
        ..Track::default()
    }
}

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

/// What a song starts with: a delay on the send bus, for tracks to send to.
pub fn buses() -> Buses {
    // A send bus carries only what's sent to it, so its delay is all echo.
    let mut delay = Fx::new(FxKind::Delay);
    delay.values[delay::MIX as usize] = 1.0;
    Buses {
        send: Bus {
            effects: vec![delay],
            ..Bus::default()
        },
        master: Bus::default(),
    }
}

/// A new, empty song for the demo's instruments: one bar on each track, named after them.
pub fn blank() -> Grid {
    let mut grid = song();
    for track in &mut grid.tracks {
        track.steps = vec![Step::default(); 16];
        track.rules.clear();
    }
    grid.name = "new".into();
    grid
}

/// The test synth with these settings.
#[allow(clippy::too_many_arguments)]
fn synth(
    wave: f32,
    (attack, decay, sustain, release): (f32, f32, f32, f32),
    cutoff: f32,
    env: f32,
    resonance: f32,
    gain: f32,
) -> Instrument {
    let mut instrument = Instrument::new(InstKind::Synth);
    for (id, value) in [
        (synth::WAVE, wave),
        (synth::ATTACK, attack),
        (synth::DECAY, decay),
        (synth::SUSTAIN, sustain),
        (synth::RELEASE, release),
        (synth::CUTOFF, cutoff),
        (synth::ENV, env),
        (synth::RESONANCE, resonance),
        (synth::GAIN, gain),
    ] {
        instrument.values[id as usize] = value;
    }
    instrument
}

fn track(name: &str, steps: &str, instrument: Instrument, rules: Vec<Rule>) -> Track {
    Track {
        name: name.into(),
        steps: pattern(steps),
        rules,
        instrument,
        ..Track::default()
    }
}

const SAW: f32 = 0.0;
const SQUARE: f32 = 1.0;

pub fn song() -> Grid {
    // The Amen break, as the Winstons played it in 1969: four bars of sixteenths, the last two
    // varied, with ghost notes on the snare and a crash in place of the ride near the end.
    let mut kick = drum(
        "kick",
        "x.x.......xx....|x.x.......xx....|x.x.......x.....|..xx......x.....",
        InstKind::Kick,
        &[(kick::TUNE, 55.0), (kick::PUNCH, 0.6), (kick::DECAY, 0.4)],
    );
    kick.mix.level = -2.0;
    let mut snare = drum(
        "snare",
        "....X..g.g..X..m|....X..g.g..X..m|....X..g.g....X.|.m..X..g.g....X.",
        InstKind::Snare,
        &[
            (snare::TUNE, 200.0),
            (snare::SNAP, 0.65),
            (snare::TONE, 6_000.0),
        ],
    );
    snare.mix.level = -3.0;
    snare.mix.send = -16.0;
    let mut ride = drum(
        "ride",
        "x.m.x.m.x.m.x.m.|x.m.x.m.x.m.x.m.|x.m.x.m.x.m.x.m.|x.m.x.m.x...x.m.",
        InstKind::Hat,
        &[
            (hat::TUNE, 320.0),
            (hat::DECAY, 0.35),
            (hat::TONE, 5_000.0),
            (hat::NOISE, 0.15),
        ],
    );
    ride.mix.level = -6.0;
    ride.mix.pan = 0.25;
    let mut crash = drum(
        "crash",
        "................|................|................|..........X.....",
        InstKind::Hat,
        &[
            (hat::TUNE, 250.0),
            (hat::DECAY, 1.6),
            (hat::TONE, 4_000.0),
            (hat::NOISE, 0.6),
        ],
    );
    crash.mix.level = -6.0;
    crash.mix.pan = -0.3;

    let bass = track(
        "bass",
        "C2 . . C2 . . C3 . C2 . G2 C2 . . Bb2 .",
        synth(SQUARE, (0.003, 0.15, 0.4, 0.06), 250.0, 1_800.0, 0.45, 0.45),
        chord_changes(),
    );

    let mut arp_rules = vec![rule(
        Transform::Reverse,
        Condition::Every { n: 2, offset: 1 },
    )];
    arp_rules.extend(chord_changes());
    let arp = track(
        "arp",
        "C4 Eb4 G4 C5 C4 Eb4 G4 C5 C4 Eb4 G4 C5 Bb4 G4 Eb4 G4",
        synth(SAW, (0.002, 0.1, 0.15, 0.12), 900.0, 4_500.0, 0.25, 0.16),
        arp_rules,
    );

    // Six steps against sixteen, drifting one step further each pass.
    let bleep = track(
        "bleep",
        "G5 . . Eb5 . .",
        synth(SQUARE, (0.001, 0.05, 0.0, 0.2), 3_000.0, 2_000.0, 0.1, 0.1),
        vec![rule(Transform::Drift(1), Condition::Always)],
    );

    Grid {
        name: "demo".into(),
        steps_per_beat: 4,
        tracks: vec![kick, snare, ride, crash, bass, arp, bleep],
        groups: vec![drums()],
    }
}

/// The drums as a folded group, on a bus that warms them up and glues them together: a little
/// drive, then a compressor slow enough to let the hits' fronts through, mixed in parallel.
fn drums() -> Group {
    let mut warm = Fx::new(FxKind::Drive);
    warm.values[drive::DRIVE as usize] = 0.35;
    warm.values[drive::TONE as usize] = 9_000.0;
    warm.values[drive::MIX as usize] = 0.6;
    let mut glue = Fx::new(FxKind::Comp);
    glue.values[comp::THRESHOLD as usize] = -20.0;
    glue.values[comp::ATTACK as usize] = 0.01;
    glue.values[comp::RELEASE as usize] = 0.1;
    glue.values[comp::MAKEUP as usize] = 3.0;
    glue.values[comp::MIX as usize] = 0.7;
    let mut group = Group::new("amen", 0..4);
    group.folded = true;
    group.bus.effects = vec![warm, glue];
    group
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
    fn drum_lines() {
        let steps = hits("Xg.|m");
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[1].note.unwrap().velocity, 55);
        assert!(steps[2].is_empty());
    }

    #[test]
    fn demo_builds() {
        let grid = song();
        assert_eq!(grid.tracks[0].instrument.kind, InstKind::Kick);
        // Four bars of drums against one-bar synth parts, which change chord each bar.
        assert!(grid.tracks[..4].iter().all(|t| t.steps.len() == 64));
    }
}
