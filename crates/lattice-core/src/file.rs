//! Song files: RON text, with the `.lattice` extension.
//!
//! Only the conversion to and from text lives here; reading and writing files is up to the
//! frontend. Tracks are written sparsely (see [`Track`](crate::Track)), and fields left at their
//! defaults (velocity 100, length 1, a rule that always applies) are left out.

use std::fmt;

use ron::extensions::Extensions;
use ron::ser::PrettyConfig;

use crate::arrangement::{MAX_BEATS_PER_BAR, MAX_STEPS_PER_BEAT};
use crate::group::MAX_GROUPS;
use crate::song::{Grid, MAX_CHOKE, MAX_DB, MAX_TRACKS, Song, Track};

/// Nesting below which values go on one line: each step, note and rule of a track fits on its
/// own line, and each lane of another pattern on one.
const DEPTH_LIMIT: usize = 5;

#[derive(Debug)]
pub enum FileError {
    /// Not valid RON, or not shaped like a song.
    Syntax(ron::error::SpannedError),
    /// Well formed, but not a song Lattice can play.
    Invalid(String),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileError::Syntax(e) => write!(f, "{e}"),
            FileError::Invalid(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for FileError {}

pub fn to_ron(song: &Song) -> Result<String, FileError> {
    let config = PrettyConfig::new()
        .depth_limit(DEPTH_LIMIT)
        .extensions(Extensions::IMPLICIT_SOME);
    let mut text =
        ron::ser::to_string_pretty(song, config).map_err(|e| FileError::Invalid(e.to_string()))?;
    text.push('\n');
    Ok(text)
}

pub fn from_ron(text: &str) -> Result<Song, FileError> {
    let song: Song = ron::Options::default()
        .with_default_extension(Extensions::IMPLICIT_SOME)
        .from_str(text)
        .map_err(FileError::Syntax)?;
    check(&song).map_err(FileError::Invalid)?;
    Ok(song)
}

/// Catches what the types allow but the editor and engine don't. Track lengths are checked as
/// tracks are read.
fn check(song: &Song) -> Result<(), String> {
    if !(song.bpm.is_finite() && song.bpm > 0.0) {
        return Err(format!("bpm must be above 0, not {}", song.bpm));
    }
    song.buses.send.check("send")?;
    song.buses.master.check("master")?;
    check_grid(&song.grid)?;
    song.arrangement.check(song.grid.tracks.len())
}

fn check_grid(grid: &Grid) -> Result<(), String> {
    if !(1..=MAX_STEPS_PER_BEAT).contains(&grid.steps_per_beat) {
        return Err(format!(
            "grid {:?} has {} steps per beat; beats have 1 to {MAX_STEPS_PER_BEAT}",
            grid.name, grid.steps_per_beat
        ));
    }
    if !(1..=MAX_BEATS_PER_BAR).contains(&grid.beats_per_bar) {
        return Err(format!(
            "grid {:?} has {} beats per bar; bars have 1 to {MAX_BEATS_PER_BAR}",
            grid.name, grid.beats_per_bar
        ));
    }
    if !(1..=MAX_TRACKS).contains(&grid.tracks.len()) {
        return Err(format!(
            "grid {:?} has {} tracks; grids have 1 to {MAX_TRACKS}",
            grid.name,
            grid.tracks.len()
        ));
    }
    grid.tracks.iter().try_for_each(check_track)?;
    check_groups(grid)
}

fn check_groups(grid: &Grid) -> Result<(), String> {
    if grid.groups.len() > MAX_GROUPS {
        return Err(format!(
            "grid {:?} has more than {MAX_GROUPS} groups",
            grid.name
        ));
    }
    let mut next = 0;
    for group in &grid.groups {
        if group.len == 0 || group.first < next || group.first + group.len > grid.tracks.len() {
            return Err(format!(
                "group {:?} must cover tracks that exist, after the group before it",
                group.name
            ));
        }
        next = group.first + group.len;
        group.bus.check(&group.name)?;
    }
    Ok(())
}

fn check_track(track: &Track) -> Result<(), String> {
    let mix = track.mix;
    let level = |db: f32| db.is_finite() && db <= MAX_DB;
    if !(level(mix.level) && level(mix.send) && (-1.0..=1.0).contains(&mix.pan)) {
        return Err(format!(
            "track {:?}: levels go up to {MAX_DB} dB and pan from -1 to 1",
            track.name
        ));
    }
    if track.choke > MAX_CHOKE {
        return Err(format!(
            "track {:?}: choke groups go up to {MAX_CHOKE}",
            track.name
        ));
    }
    if let Some(face) = &track.face
        && let Err(e) = face.check(track.instrument.kind)
    {
        return Err(format!("track {:?}'s face: {e}", track.name));
    }
    if let Err(e) = track.effects.check() {
        return Err(format!("track {:?}'s effects: {e}", track.name));
    }
    for (i, step) in track.steps.iter().enumerate() {
        if let Some(note) = step.note
            && (note.pitch > 127 || note.velocity > 127)
        {
            return Err(format!(
                "track {:?}, step {i}: pitch and velocity go up to 127",
                track.name
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::{Arrangement, Entry, Lane, Pattern};
    use crate::conductor::{Conductor, SongGesture};
    use crate::fx::{self, Bus, Buses, Fx, FxKind};
    use crate::group::Group;
    use crate::instrument::{InstKind, Instrument, kick};
    use crate::{Condition, Mix, Note, Rule, Step, Transform};

    fn song() -> Song {
        let mut bass = Track {
            face: None,
            pattern_mute: false,
            columns: vec![],
            name: "bass".into(),
            steps: vec![Step::default(); 16],
            mix: Default::default(),
            instrument: Default::default(),
            effects: Default::default(),
            choke: 0,
            macros: vec![crate::Macro {
                name: "growl".into(),
                value: 0.25,
                targets: vec![crate::MacroTarget {
                    fx: None,
                    param: 1,
                    depth: -0.5,
                }],
            }],
            rules: vec![
                Rule {
                    transform: Transform::Transpose(5),
                    when: Condition::Every { n: 4, offset: 2 },
                },
                Rule {
                    transform: Transform::Reverse,
                    when: Condition::Always,
                },
                Rule {
                    transform: Transform::Drift(-1),
                    when: Condition::Pass(3),
                },
            ],
        };
        bass.mix = Mix {
            level: -3.5,
            pan: 0.25,
            mute: true,
            ..Mix::default()
        };
        bass.steps[0] = Step::note(36);
        bass.steps[0].gestures = [Some("R3".parse().unwrap()), None];
        bass.steps[5].gestures = [None, Some("J0".parse().unwrap())];
        let cutoff = crate::LockColumn {
            fx: Some(0),
            param: 1,
        };
        bass.columns = vec![cutoff];
        bass.steps[2]
            .locks
            .set(cutoff.lock(0.5, crate::Change::Slide));
        bass.steps[5]
            .locks
            .set(cutoff.lock(0.0, crate::Change::Reset));
        bass.steps[3].note = Some(Note {
            pitch: 43,
            velocity: 80,
            length: 2,
        });
        let mut delay = Fx::new(FxKind::Delay);
        delay.values[fx::delay::FEEDBACK as usize] = 0.7;
        bass.effects.push(Fx::new(FxKind::Delay));
        bass.instrument = Instrument::new(InstKind::Kick);
        bass.instrument.values[kick::PUNCH as usize] = 0.8;
        let empty = Track {
            face: None,
            pattern_mute: false,
            columns: vec![],
            name: "empty".into(),
            steps: vec![Step::default(); 6],
            mix: Default::default(),
            instrument: Default::default(),
            effects: Default::default(),
            choke: 0,
            macros: vec![],
            rules: vec![],
        };
        Song {
            name: "test".into(),
            bpm: 117.5,
            grid: Grid {
                name: "a".into(),
                steps_per_beat: 4,
                beats_per_bar: 4,
                tracks: vec![bass, empty],
                groups: vec![Group {
                    folded: true,
                    ..Group::new("pair", 0..2)
                }],
            },
            arrangement: Arrangement {
                patterns: vec![
                    Pattern {
                        name: "A".into(),
                        bars: None,
                        lanes: vec![],
                        conductor: Conductor {
                            steps: vec![
                                [Some(SongGesture::Tempo(120)), None],
                                [None; 2],
                                [
                                    Some(SongGesture::Slide(140)),
                                    Some(SongGesture::Mute { track: 2, on: true }),
                                ],
                                [Some(SongGesture::Stop), None],
                            ],
                        },
                    },
                    Pattern {
                        name: "B".into(),
                        bars: Some(2),
                        conductor: Conductor::default(),
                        lanes: vec![
                            Lane {
                                mute: false,
                                steps: vec![Step::note(40), Step::default()],
                                rules: vec![],
                            },
                            Lane {
                                mute: false,
                                steps: vec![Step::default(); 3],
                                rules: vec![],
                            },
                        ],
                    },
                ],
                current: 0,
                entries: vec![
                    Entry {
                        pattern: 0,
                        repeats: 2,
                        mutes: 0,
                    },
                    Entry {
                        pattern: 1,
                        repeats: 1,
                        mutes: 0,
                    },
                ],
            },
            buses: Buses {
                send: Bus {
                    level: -2.0,
                    effects: vec![delay].into(),
                },
                master: Bus::default(),
            },
            click: true,
        }
    }

    #[test]
    fn round_trips() {
        let song = song();
        let text = to_ron(&song).unwrap();
        assert_eq!(from_ron(&text).unwrap(), song, "{text}");
    }

    #[test]
    fn keeps_the_click_off() {
        let song = Song {
            click: false,
            ..song()
        };
        let text = to_ron(&song).unwrap();
        assert!(text.contains("click: false"), "{text}");
        assert_eq!(from_ron(&text).unwrap(), song, "{text}");
    }

    #[test]
    fn writes_sparsely_and_leaves_out_defaults() {
        let text = to_ron(&song()).unwrap();
        assert!(text.contains("length: 16"), "{text}");
        assert!(
            text.contains(r#"0: (note: (pitch: 36), gestures: ["R3"]),"#),
            "{text}"
        );
        assert!(text.contains(r#"gestures: ["", "J0"]),"#), "{text}");
        assert!(
            text.contains("3: (note: (pitch: 43, velocity: 80, length: 2)),"),
            "{text}"
        );
        assert!(text.contains("(transform: Reverse),"), "{text}");
        assert!(text.contains("change: slide"), "{text}");
        assert!(text.contains("change: reset"), "{text}");
        assert!(
            text.contains(r#"steps: {0: ["T120"], 2: ["~140", "M2"], 3: ["X"]},"#),
            "{text}"
        );
        assert!(!text.contains("None"), "{text}");
        assert!(!text.contains("click"), "{text}");
        assert!(text.contains("level: -3.5,\n"), "{text}");
        assert!(text.contains("mute: true,\n"), "{text}");
        assert!(text.contains("(kind: Delay),"), "{text}");
        assert!(text.contains("params: {\"feedback\": 0.7}"), "{text}");
        assert!(text.contains("kind: Kick,"), "{text}");
        assert!(text.contains("\"punch\": 0.8"), "{text}");
        assert_eq!(text.matches("mix:").count(), 1, "{text}");
        assert!(text.contains("bars: 2,"), "{text}");
        assert!(text.contains("pattern: 1,\n"), "{text}");
        assert!(text.lines().count() < 110, "{text}");
    }

    #[test]
    fn reads_hand_written_files() {
        // No extension header, so `Some` is implied by default.
        let text = r#"(
            name: "hand",
            bpm: 90,
            grid: (name: "a", steps_per_beat: 4, tracks: [
                (name: "t", length: 4, steps: {2: (note: (pitch: 60))}),
            ]),
        )"#;
        let song = from_ron(text).unwrap();
        let steps = &song.grid.tracks[0].steps;
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[2], Step::note(60));
        assert!(steps[0].is_empty());
    }

    #[test]
    fn rejects_songs_that_cannot_play() {
        let mut bad = song();
        bad.grid.groups[0].len = 3;
        let err = from_ron(&to_ron(&bad).unwrap()).unwrap_err().to_string();
        assert!(err.contains("group \"pair\""), "{err}");

        let mut bad = song();
        bad.grid.tracks[0].steps.clear();
        let err = from_ron(&to_ron(&bad).unwrap()).unwrap_err().to_string();
        assert!(err.contains("has 0 steps"), "{err}");

        let mut bad = song();
        bad.arrangement.entries[0].pattern = 3;
        assert!(matches!(
            from_ron(&to_ron(&bad).unwrap()),
            Err(FileError::Invalid(_))
        ));

        let mut bad = song();
        let track = bad.grid.tracks[0].clone();
        bad.grid.tracks.resize(MAX_TRACKS + 1, track);
        let err = from_ron(&to_ron(&bad).unwrap()).unwrap_err().to_string();
        assert!(err.contains("17 tracks"), "{err}");

        let text = to_ron(&song()).unwrap().replace("length: 16", "length: 2");
        let err = from_ron(&text).unwrap_err().to_string();
        assert!(err.contains("past its length"), "{err}");

        assert!(matches!(from_ron("(name: 1)"), Err(FileError::Syntax(_))));

        let text = to_ron(&song())
            .unwrap()
            .replace("\"feedback\": 0.7", "\"feedback\": 3");
        let err = from_ron(&text).unwrap_err().to_string();
        assert!(err.contains("feedback goes from 0 to 0.95"), "{err}");
        let text = to_ron(&song())
            .unwrap()
            .replace("\"feedback\"", "\"wobble\"");
        let err = from_ron(&text).unwrap_err().to_string();
        assert!(err.contains("no \"wobble\" setting"), "{err}");
    }
}
