//! A short demo song for hearing the engine and the pattern transforms.

use lattice_core::fx::delay;
use lattice_core::instrument::synth;
use lattice_core::{
    Bus, Buses, Condition, Fx, FxKind, Grid, InstKind, Instrument, Rule, Step, Track, Transform,
};

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
    // Rhythmicon: four on the floor, a backbeat with a little echo, and off-beat hats, with one
    // tuned an octave up at the end of the bar.
    let mut kick = track(
        "kick",
        "C4 . . . C4 . . . C4 . . . C4 . C4 .",
        Instrument::new(InstKind::Kick),
        vec![],
    );
    kick.mix.level = -2.0;
    let mut snare = track(
        "snare",
        ". . . . C4 . . . . . . . C4 . . .",
        Instrument::new(InstKind::Snare),
        vec![],
    );
    snare.mix.level = -4.0;
    snare.mix.send = -14.0;
    let mut hats = track(
        "hats",
        ". . C4 . . . C4 . . . C4 . . . C5 C4",
        Instrument::new(InstKind::Hat),
        vec![],
    );
    hats.mix.level = -8.0;
    hats.mix.pan = 0.2;

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
        tracks: vec![kick, snare, hats, bass, arp, bleep],
    }
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
        let grid = song();
        assert_eq!(grid.tracks[0].instrument.kind, InstKind::Kick);
        assert!(grid.tracks.iter().all(|t| t.steps.len() <= 16));
    }
}
