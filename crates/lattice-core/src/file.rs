//! Song files: RON text, with the `.lattice` extension.
//!
//! Only the conversion to and from text lives here; reading and writing files is up to the
//! frontend. Tracks are written sparsely (see [`Track`](crate::Track)), and fields left at their
//! defaults (velocity 100, length 1, a rule that always applies) are left out.

use std::fmt;

use ron::extensions::Extensions;
use ron::ser::PrettyConfig;

use crate::fx::MAX_EFFECTS;
use crate::song::{Grid, MAX_DB, MAX_TRACKS, Song, Track};

/// Nesting below which values go on one line: each step, note and rule fits on its own line.
const DEPTH_LIMIT: usize = 6;

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

pub fn to_ron(song: &Song) -> String {
    let config = PrettyConfig::new()
        .depth_limit(DEPTH_LIMIT)
        .extensions(Extensions::IMPLICIT_SOME);
    let mut text = ron::ser::to_string_pretty(song, config).expect("songs always serialise");
    text.push('\n');
    text
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
    if song.grids.is_empty() {
        return Err("the song has no grids".into());
    }
    if let Some(i) = song.order.iter().find(|&&i| i >= song.grids.len()) {
        return Err(format!("order refers to grid {i}, which doesn't exist"));
    }
    song.buses.send.check("send")?;
    song.buses.master.check("master")?;
    song.grids.iter().try_for_each(check_grid)
}

fn check_grid(grid: &Grid) -> Result<(), String> {
    if grid.steps_per_beat == 0 {
        return Err(format!("grid {:?} has 0 steps per beat", grid.name));
    }
    if !(1..=MAX_TRACKS).contains(&grid.tracks.len()) {
        return Err(format!(
            "grid {:?} has {} tracks; grids have 1 to {MAX_TRACKS}",
            grid.name,
            grid.tracks.len()
        ));
    }
    grid.tracks.iter().try_for_each(check_track)
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
    if track.effects.len() > MAX_EFFECTS {
        return Err(format!(
            "track {:?} has more than {MAX_EFFECTS} effects",
            track.name
        ));
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
    use crate::fx::{self, Bus, Buses, Fx, FxKind};
    use crate::{Condition, Mix, Note, Rule, Step, Transform};

    fn song() -> Song {
        let mut bass = Track {
            name: "bass".into(),
            steps: vec![Step::default(); 16],
            mix: Default::default(),
            effects: vec![],
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
        bass.steps[3].note = Some(Note {
            pitch: 43,
            velocity: 80,
            length: 2,
        });
        let mut delay = Fx::new(FxKind::Delay);
        delay.values[fx::delay::FEEDBACK as usize] = 0.7;
        bass.effects.push(Fx::new(FxKind::Delay));
        let empty = Track {
            name: "empty".into(),
            steps: vec![Step::default(); 6],
            mix: Default::default(),
            effects: vec![],
            rules: vec![],
        };
        Song {
            name: "test".into(),
            bpm: 117.5,
            grids: vec![Grid {
                name: "a".into(),
                steps_per_beat: 4,
                tracks: vec![bass, empty],
            }],
            order: vec![0],
            buses: Buses {
                send: Bus {
                    level: -2.0,
                    effects: vec![delay],
                },
                master: Bus::default(),
            },
        }
    }

    #[test]
    fn round_trips() {
        let song = song();
        let text = to_ron(&song);
        assert_eq!(from_ron(&text).unwrap(), song, "{text}");
    }

    #[test]
    fn writes_sparsely_and_leaves_out_defaults() {
        let text = to_ron(&song());
        assert!(text.contains("length: 16"), "{text}");
        assert!(text.contains("0: (note: (pitch: 36)),"), "{text}");
        assert!(
            text.contains("3: (note: (pitch: 43, velocity: 80, length: 2)),"),
            "{text}"
        );
        assert!(text.contains("(transform: Reverse),"), "{text}");
        assert!(!text.contains("None"), "{text}");
        assert!(text.contains("level: -3.5,\n"), "{text}");
        assert!(text.contains("mute: true,\n"), "{text}");
        assert!(text.contains("(kind: Delay),"), "{text}");
        assert!(text.contains("\"feedback\": 0.7,"), "{text}");
        assert_eq!(text.matches("mix:").count(), 1, "{text}");
        assert!(text.lines().count() < 70, "{text}");
    }

    #[test]
    fn reads_hand_written_files() {
        // No extension header, so `Some` is implied by default.
        let text = r#"(
            name: "hand",
            bpm: 90,
            grids: [(name: "a", steps_per_beat: 4, tracks: [
                (name: "t", length: 4, steps: {2: (note: (pitch: 60))}),
            ])],
            order: [0],
        )"#;
        let song = from_ron(text).unwrap();
        let steps = &song.grids[0].tracks[0].steps;
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[2], Step::note(60));
        assert!(steps[0].is_empty());
    }

    #[test]
    fn rejects_songs_that_cannot_play() {
        let mut bad = song();
        bad.grids[0].tracks[0].steps.clear();
        let err = from_ron(&to_ron(&bad)).unwrap_err().to_string();
        assert!(err.contains("has 0 steps"), "{err}");

        let mut bad = song();
        bad.grids.clear();
        assert!(matches!(
            from_ron(&to_ron(&bad)),
            Err(FileError::Invalid(_))
        ));

        let mut bad = song();
        let track = bad.grids[0].tracks[0].clone();
        bad.grids[0].tracks.resize(MAX_TRACKS + 1, track);
        let err = from_ron(&to_ron(&bad)).unwrap_err().to_string();
        assert!(err.contains("17 tracks"), "{err}");

        let text = to_ron(&song()).replace("length: 16", "length: 2");
        let err = from_ron(&text).unwrap_err().to_string();
        assert!(err.contains("past its length"), "{err}");

        assert!(matches!(from_ron("(name: 1)"), Err(FileError::Syntax(_))));

        let text = to_ron(&song()).replace("\"feedback\": 0.7", "\"feedback\": 3");
        let err = from_ron(&text).unwrap_err().to_string();
        assert!(err.contains("feedback goes from 0 to 0.95"), "{err}");
        let text = to_ron(&song()).replace("\"feedback\"", "\"wobble\"");
        let err = from_ron(&text).unwrap_err().to_string();
        assert!(err.contains("no \"wobble\" setting"), "{err}");
    }
}
