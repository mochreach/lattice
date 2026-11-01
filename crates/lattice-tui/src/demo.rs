//! The demo song: a techno track in F minor, arranged in six sections, that uses a bit of
//! everything: every instrument, every effect, groups, a choke, sends, locks, a pattern mute,
//! tracks of different lengths drifting against each other, and rules that change the parts as
//! they repeat.

use lattice_core::fx::{comp, delay, drive, filter, reverb};
use lattice_core::instrument::{clap, hat, kick, snare, string, synth, tom};
use lattice_core::{
    Arrangement, Bus, Buses, Condition, DRUM_ROOT, Entry, Fx, FxKind, Grid, Group, InstKind,
    Instrument, Lane, Lock, Note, Pattern, Rule, Song, Step, Track, Transform,
};

pub const BPM: f64 = 130.0;

/// A dotted eighth at [`BPM`], for the echoes.
const DOTTED_EIGHTH: f32 = (60.0 / BPM as f32) * 0.75;

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
            Step::with_note(Note {
                pitch: DRUM_ROOT,
                velocity,
                length: 1,
            })
        })
        .collect()
}

/// Parses notes like `"F1 . Ab2! C3_4 Eb3?"`: one per step, `.` a rest. `_N` holds a note for N
/// steps, `!` accents it and `?` plays it softly. `|` marks bars and is left out.
fn notes(s: &str) -> Vec<Step> {
    s.split_whitespace()
        .filter(|tok| *tok != "|")
        .map(|tok| {
            if tok == "." {
                return Step::default();
            }
            let (tok, velocity) = match tok.strip_suffix('!') {
                Some(t) => (t, 120),
                None => match tok.strip_suffix('?') {
                    Some(t) => (t, 65),
                    None => (tok, 95),
                },
            };
            let (name, length) = match tok.split_once('_') {
                Some((name, n)) => (name, n.parse().unwrap_or_else(|_| panic!("bad length {n}"))),
                None => (tok, 1),
            };
            let pitch = note_number(name).unwrap_or_else(|| panic!("bad note {name}"));
            Step::with_note(Note {
                pitch,
                velocity,
                length,
            })
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

fn every(n: u32, offset: u32) -> Condition {
    Condition::Every { n, offset }
}

/// i–i–VI–VII over four passes, which the bass, stabs and pad all follow.
fn changes() -> Vec<Rule> {
    vec![
        rule(Transform::Transpose(-4), every(4, 2)),
        rule(Transform::Transpose(-2), every(4, 3)),
    ]
}

fn instrument(kind: InstKind, settings: &[(u32, f32)]) -> Instrument {
    let mut instrument = Instrument::new(kind);
    for &(id, value) in settings {
        instrument.values[id as usize] = value;
    }
    instrument
}

fn fx(kind: FxKind, settings: &[(u32, f32)]) -> Fx {
    let mut fx = Fx::new(kind);
    for &(id, value) in settings {
        fx.values[id as usize] = value;
    }
    fx
}

fn track(name: &str, instrument: Instrument) -> Track {
    Track {
        name: name.into(),
        instrument,
        ..Track::default()
    }
}

// The tracks, in order.
const KICK: usize = 0;
const CLAP: usize = 1;
const SNARE: usize = 2;
const HAT: usize = 3;
const OPEN: usize = 4;
const PERC: usize = 5;
const RIDE: usize = 6;
const BASS: usize = 7;
const ACID: usize = 8;
const STAB: usize = 9;
const PAD: usize = 10;
const PLUCK: usize = 11;
const TRACKS: usize = 12;

const SAW: f32 = 0.0;
const SQUARE: f32 = 1.0;

/// The tracks' sounds, mix and effects, with no steps yet.
fn tracks() -> Vec<Track> {
    use InstKind::*;
    let mut kick = track(
        "kick",
        instrument(
            Kick,
            &[
                (kick::MODEL, 2.0),
                (kick::TUNE, 48.0),
                (kick::PUNCH, 0.7),
                (kick::DECAY, 0.5),
                (kick::DRIVE, 0.5),
            ],
        ),
    );
    kick.mix.level = -1.0;

    let mut clap = track(
        "clap",
        instrument(
            Clap,
            &[(clap::MODEL, 1.0), (clap::DECAY, 0.35), (clap::ROOM, 0.6)],
        ),
    );
    clap.mix.level = -5.0;
    clap.mix.send = -18.0;

    // For rolls.
    let mut snare = track(
        "snare",
        instrument(
            Snare,
            &[
                (snare::MODEL, 0.0),
                (snare::TUNE, 220.0),
                (snare::DECAY, 0.15),
                (snare::SNAP, 0.7),
            ],
        ),
    );
    snare.mix.level = -8.0;
    snare.mix.send = -20.0;

    // The open hat rings until the closed one cuts it off: they share a choke.
    let mut hat = track(
        "hat",
        instrument(
            Hat,
            &[(hat::MODEL, 0.0), (hat::DECAY, 0.04), (hat::TONE, 9_000.0)],
        ),
    );
    hat.choke = 1;
    hat.mix.level = -9.0;
    hat.mix.pan = 0.2;
    let mut open = track(
        "open",
        instrument(
            Hat,
            &[
                (hat::MODEL, 2.0),
                (hat::DECAY, 0.4),
                (hat::TONE, 8_000.0),
                (hat::NOISE, 0.5),
            ],
        ),
    );
    open.choke = 1;
    open.mix.level = -10.0;
    open.mix.pan = 0.2;

    let mut perc = track(
        "perc",
        instrument(
            Tom,
            &[
                (tom::MODEL, 0.0),
                (tom::TUNE, 260.0),
                (tom::DECAY, 0.12),
                (tom::BEND, 0.7),
            ],
        ),
    );
    perc.mix.level = -11.0;
    perc.mix.pan = -0.35;
    perc.mix.send = -12.0;

    let mut ride = track(
        "ride",
        instrument(
            Hat,
            &[
                (hat::MODEL, 1.0),
                (hat::TUNE, 420.0),
                (hat::DECAY, 0.9),
                (hat::NOISE, 0.2),
            ],
        ),
    );
    ride.mix.level = -15.0;
    ride.mix.pan = -0.2;

    let mut bass = track(
        "bass",
        synth(SAW, (0.002, 0.14, 0.2, 0.05), 220.0, 900.0, 0.35, 0.5),
    );
    bass.mix.level = -3.0;

    // A squelchy acid line, its filter opening and closing over four bars, through a drive and
    // echoing off the send.
    let mut acid = track(
        "acid",
        synth(SAW, (0.002, 0.18, 0.0, 0.06), 350.0, 3_800.0, 0.86, 0.32),
    );
    acid.effects = vec![
        fx(
            FxKind::Filter,
            &[
                (filter::CUTOFF, 1_400.0),
                (filter::RESONANCE, 0.5),
                (filter::RATE, BPM as f32 / 60.0 / 16.0),
                (filter::SWEEP, 1.5),
            ],
        ),
        fx(
            FxKind::Drive,
            &[
                (drive::DRIVE, 0.55),
                (drive::TONE, 5_000.0),
                (drive::MIX, 0.8),
            ],
        ),
    ]
    .into();
    acid.mix.level = -7.0;
    acid.mix.pan = 0.15;
    acid.mix.send = -14.0;

    // Dub chords: a short stab, most of it in the echoes.
    let mut stab = track(
        "stab",
        synth(
            SQUARE,
            (0.002, 0.22, 0.0, 0.25),
            1_300.0,
            1_400.0,
            0.4,
            0.18,
        ),
    );
    stab.mix.level = -9.0;
    stab.mix.pan = -0.25;
    stab.mix.send = -3.0;

    // A bowed steel string, held for a bar at a time, with its own echo.
    let mut pad = track(
        "pad",
        instrument(
            String,
            &[
                (string::MODEL, 1.0),
                (string::PLUCK, 0.2),
                (string::DECAY, 8.0),
                (string::TONE, 0.5),
                (string::BOW, 0.45),
            ],
        ),
    );
    pad.effects = vec![fx(
        FxKind::Delay,
        &[
            (delay::TIME, DOTTED_EIGHTH * 2.0),
            (delay::FEEDBACK, 0.4),
            (delay::TONE, 2_500.0),
            (delay::MIX, 0.25),
        ],
    )]
    .into();
    pad.mix.level = -12.0;
    pad.mix.pan = -0.4;
    pad.mix.send = -20.0;

    let mut pluck = track(
        "pluck",
        instrument(
            String,
            &[
                (string::MODEL, 2.0),
                (string::PLUCK, 0.8),
                (string::DECAY, 1.5),
            ],
        ),
    );
    pluck.mix.level = -13.0;
    pluck.mix.pan = 0.45;
    pluck.mix.send = -9.0;

    vec![
        kick, clap, snare, hat, open, perc, ride, bass, acid, stab, pad, pluck,
    ]
}

/// The test synth with these settings.
fn synth(
    wave: f32,
    (attack, decay, sustain, release): (f32, f32, f32, f32),
    cutoff: f32,
    env: f32,
    resonance: f32,
    gain: f32,
) -> Instrument {
    instrument(
        InstKind::Synth,
        &[
            (synth::WAVE, wave),
            (synth::ATTACK, attack),
            (synth::DECAY, decay),
            (synth::SUSTAIN, sustain),
            (synth::RELEASE, release),
            (synth::CUTOFF, cutoff),
            (synth::ENV, env),
            (synth::RESONANCE, resonance),
            (synth::GAIN, gain),
        ],
    )
}

/// A section's lanes: these tracks' steps and rules, and a silent bar on the rest.
fn section(parts: Vec<(usize, Vec<Step>, Vec<Rule>)>) -> Vec<Lane> {
    let mut lanes = vec![
        Lane {
            mute: false,
            steps: vec![Step::default(); 16],
            rules: vec![],
        };
        TRACKS
    ];
    for (track, steps, rules) in parts {
        lanes[track] = Lane {
            steps,
            rules,
            mute: false,
        };
    }
    lanes
}

// The parts, shared between sections.
fn four_to_the_floor() -> Vec<Step> {
    hits("X...x...X...x...")
}
fn clap_on_two_and_four() -> Vec<Step> {
    hits("....x.......x...")
}
fn hats() -> Vec<Step> {
    hits("x..mx..mx..mx.gm")
}
fn open_hats() -> Vec<Step> {
    hits("..x...x...x...x.")
}
/// Five steps against sixteen, a step later each pass and backwards every third.
fn perc() -> (Vec<Step>, Vec<Rule>) {
    (
        hits("x..mg"),
        vec![
            rule(Transform::Drift(1), Condition::Always),
            rule(Transform::Reverse, every(3, 2)),
        ],
    )
}
fn bass() -> Vec<Step> {
    notes(". F1 F1 F2? . F1 F1 F2? . F1 F1 F2? . F1 Ab1 C2")
}
/// Twelve steps against sixteen, so it lands differently on the beat each bar.
fn acid() -> Vec<Step> {
    notes("F2! F2 F3 . Eb3 F2! . C3 F2 Ab2! F3? C3")
}
/// The second, answering stab brighter and ringing, from settings locked on its step. On the
/// fifth, which the changes keep in key, as every chord in them has a perfect fifth.
fn stabs() -> Vec<Step> {
    let mut steps = notes(". . . C4_2 . . . . . . C4 . . . . .");
    for (param, value) in [(synth::CUTOFF, 3_200.0), (synth::RESONANCE, 0.7)] {
        steps[10].locks.set(Lock {
            fx: None,
            param: param as u8,
            value,
        });
    }
    steps
}
/// The fifth, then the root, which the changes keep in key.
fn pad() -> Vec<Step> {
    notes("C4_16 . . . . . . . . . . . . . . . | F3_16 . . . . . . . . . . . . . . .")
}
/// Seven steps, turned a step back every other pass.
fn pluck() -> (Vec<Step>, Vec<Rule>) {
    (
        notes("C5 . Eb5 . G5? F5 ."),
        vec![rule(Transform::Drift(-1), every(2, 0))],
    )
}

/// The song's sections, in the order they're numbered.
fn sections() -> Vec<(&'static str, u32, Vec<Lane>)> {
    let (perc_steps, perc_rules) = perc();
    let (pluck_steps, pluck_rules) = pluck();
    let perc = || (PERC, perc_steps.clone(), perc_rules.clone());
    let pluck = || (PLUCK, pluck_steps.clone(), pluck_rules.clone());
    vec![
        (
            "intro",
            4,
            section(vec![
                (KICK, four_to_the_floor(), vec![]),
                (HAT, hats(), vec![]),
                perc(),
                (PAD, pad(), vec![]),
            ]),
        ),
        (
            "build",
            4,
            section(vec![
                (KICK, four_to_the_floor(), vec![]),
                (CLAP, clap_on_two_and_four(), vec![]),
                (HAT, hats(), vec![]),
                (OPEN, open_hats(), vec![]),
                perc(),
                (BASS, bass(), vec![]),
                pluck(),
            ]),
        ),
        (
            "acid",
            8,
            section(vec![
                (KICK, four_to_the_floor(), vec![]),
                (CLAP, clap_on_two_and_four(), vec![]),
                (HAT, hats(), vec![]),
                (OPEN, open_hats(), vec![]),
                perc(),
                (BASS, bass(), changes()),
                // Turned round every other pass, and up an octave on the eighth.
                (
                    ACID,
                    acid(),
                    vec![
                        rule(Transform::Rotate(3), every(2, 1)),
                        rule(Transform::Transpose(12), Condition::Pass(9)),
                    ],
                ),
                (STAB, stabs(), changes()),
            ]),
        ),
        (
            // No kick until a roll into the drop, in a lane four bars long.
            "break",
            4,
            section(vec![
                (
                    KICK,
                    hits("................|................|................|x...x...x.x.xxXX"),
                    vec![],
                ),
                (CLAP, hits("............x..."), vec![]),
                // Eighths, then sixteenths, louder as they go.
                (
                    SNARE,
                    hits("................|................|g...g...m...m...|x.x.x.x.xxxxXXXX"),
                    vec![],
                ),
                (ACID, acid(), vec![rule(Transform::Reverse, every(2, 1))]),
                (STAB, stabs(), changes()),
                (PAD, pad(), changes()),
                pluck(),
            ]),
        ),
        (
            "peak",
            8,
            section(vec![
                (KICK, hits("X...x...X...x..g"), vec![]),
                (CLAP, clap_on_two_and_four(), vec![]),
                (HAT, hats(), vec![]),
                (OPEN, open_hats(), vec![]),
                perc(),
                (RIDE, hits("x.m.x.m.x.m.x.mm"), vec![]),
                (BASS, bass(), changes()),
                (
                    ACID,
                    acid(),
                    vec![
                        rule(Transform::Drift(2), every(2, 1)),
                        rule(Transform::Transpose(12), every(4, 3)),
                    ],
                ),
                (STAB, stabs(), changes()),
                (PAD, pad(), changes()),
                pluck(),
            ]),
        ),
        (
            "outro",
            4,
            // The bass is still written out, but muted in this pattern: `tM` brings it back.
            muted(
                section(vec![
                    (KICK, four_to_the_floor(), vec![]),
                    (HAT, hats(), vec![]),
                    (BASS, bass(), vec![]),
                    (PAD, pad(), vec![]),
                    pluck(),
                ]),
                BASS,
            ),
        ),
    ]
}

/// The lanes with `track`'s muted.
fn muted(mut lanes: Vec<Lane>, track: usize) -> Vec<Lane> {
    lanes[track].mute = true;
    lanes
}

/// Which section is open to begin with: the one with everything in it.
const OPENS_ON: usize = 4;

/// The send bus's echo into a plate, and the master's glue.
pub fn buses() -> Buses {
    // A send bus carries only what's sent to it, so its delay is all echo.
    let echo = fx(
        FxKind::Delay,
        &[
            (delay::TIME, DOTTED_EIGHTH),
            (delay::FEEDBACK, 0.55),
            (delay::TONE, 2_200.0),
            (delay::MIX, 1.0),
        ],
    );
    let plate = fx(
        FxKind::Reverb,
        &[
            (reverb::SIZE, 0.7),
            (reverb::DECAY, 0.7),
            (reverb::TONE, 4_500.0),
            (reverb::PREDELAY, 0.03),
            (reverb::MIX, 0.35),
        ],
    );
    let glue = fx(
        FxKind::Comp,
        &[
            (comp::THRESHOLD, -10.0),
            (comp::RATIO, 2.0),
            (comp::ATTACK, 0.02),
            (comp::RELEASE, 0.2),
            (comp::MAKEUP, 1.5),
        ],
    );
    Buses {
        send: Bus {
            effects: vec![echo, plate].into(),
            ..Bus::default()
        },
        master: Bus {
            effects: vec![glue].into(),
            ..Bus::default()
        },
    }
}

/// The drums, folded into pads, on a bus that drives them a little and squeezes them together;
/// and the bass and acid line, compressed together.
fn groups() -> Vec<Group> {
    let mut drums = Group::new("drums", KICK..RIDE + 1);
    drums.folded = true;
    drums.bus.effects = vec![
        fx(
            FxKind::Drive,
            &[
                (drive::DRIVE, 0.3),
                (drive::TONE, 10_000.0),
                (drive::MIX, 0.5),
            ],
        ),
        fx(
            FxKind::Comp,
            &[
                (comp::THRESHOLD, -18.0),
                (comp::RATIO, 4.0),
                (comp::ATTACK, 0.01),
                (comp::RELEASE, 0.12),
                (comp::MAKEUP, 3.0),
                (comp::MIX, 0.6),
            ],
        ),
    ]
    .into();
    let mut low = Group::new("low", BASS..ACID + 1);
    low.bus.effects = vec![fx(
        FxKind::Comp,
        &[
            (comp::THRESHOLD, -14.0),
            (comp::RATIO, 3.0),
            (comp::MAKEUP, 2.0),
        ],
    )]
    .into();
    vec![drums, low]
}

pub fn song() -> Song {
    let mut tracks = tracks();
    let mut patterns = vec![];
    for (i, (name, bars, lanes)) in sections().into_iter().enumerate() {
        let lanes = if i == OPENS_ON {
            for (track, lane) in tracks.iter_mut().zip(lanes) {
                track.steps = lane.steps;
                track.rules = lane.rules;
                track.pattern_mute = lane.mute;
            }
            vec![]
        } else {
            lanes
        };
        patterns.push(Pattern {
            name: name.into(),
            bars: Some(bars),
            lanes,
        });
    }
    let entry = |pattern, repeats| Entry { pattern, repeats };
    Song {
        name: "demo".into(),
        bpm: BPM,
        grid: Grid {
            name: "demo".into(),
            steps_per_beat: 4,
            tracks,
            groups: groups(),
        },
        arrangement: Arrangement {
            patterns,
            current: OPENS_ON,
            entries: vec![
                entry(0, 1),
                entry(1, 1),
                entry(2, 2),
                entry(3, 1),
                entry(4, 2),
                entry(5, 1),
            ],
        },
        buses: buses(),
    }
}

/// A new, empty song for the demo's instruments: one bar on each track, named after them.
pub fn blank() -> Grid {
    let mut grid = song().grid;
    for track in &mut grid.tracks {
        track.steps = vec![Step::default(); 16];
        track.rules.clear();
    }
    grid.name = "new".into();
    grid
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
    fn note_lines() {
        let steps = notes("F1! . C3_4 | Eb3?");
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].note.unwrap().velocity, 120);
        assert_eq!(steps[2].note.unwrap().length, 4);
        assert_eq!(steps[3].note.unwrap().pitch, 51);
    }

    #[test]
    fn demo_is_a_valid_song() {
        let song = song();
        assert_eq!(song.grid.tracks.len(), TRACKS);
        assert_eq!(song.grid.tracks[KICK].instrument.kind, InstKind::Kick);
        let read = lattice_core::from_ron(&lattice_core::to_ron(&song)).unwrap();
        assert_eq!(read.arrangement, song.arrangement);
        let tracks = &song.grid.tracks;
        // All but those that play a file, which the demo hasn't got.
        for kind in InstKind::ALL
            .into_iter()
            .filter(|&k| !matches!(k, InstKind::Sampler | InstKind::Chop))
        {
            assert!(tracks.iter().any(|t| t.instrument.kind == kind), "{kind:?}");
        }
    }
}
