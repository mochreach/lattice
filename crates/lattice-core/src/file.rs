//! Song files: RON text, with the `.lattice` extension.
//!
//! Only the conversion to and from text lives here; reading and writing files is up to the
//! frontend. Tracks are written sparsely (see [`Track`](crate::Track)), and fields left at their
//! defaults (velocity 100, length 1, a rule that always applies) are left out.

use std::fmt;

use ron::extensions::Extensions;
use ron::ser::PrettyConfig;

use crate::song::{Grid, Song, Track};

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
    song.grids.iter().try_for_each(check_grid)
}

fn check_grid(grid: &Grid) -> Result<(), String> {
    if grid.steps_per_beat == 0 {
        return Err(format!("grid {:?} has 0 steps per beat", grid.name));
    }
    if grid.tracks.is_empty() {
        return Err(format!("grid {:?} has no tracks", grid.name));
    }
    grid.tracks.iter().try_for_each(check_track)
}

fn check_track(track: &Track) -> Result<(), String> {
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
    use crate::{Condition, Note, Rule, Step, Transform};

    fn song() -> Song {
        let mut bass = Track {
            name: "bass".into(),
            steps: vec![Step::default(); 16],
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
        bass.steps[0] = Step::note(36);
        bass.steps[3].note = Some(Note {
            pitch: 43,
            velocity: 80,
            length: 2,
        });
        let empty = Track {
            name: "empty".into(),
            steps: vec![Step::default(); 6],
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
        assert!(text.lines().count() < 40, "{text}");
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

        let text = to_ron(&song()).replace("length: 16", "length: 2");
        let err = from_ron(&text).unwrap_err().to_string();
        assert!(err.contains("past its length"), "{err}");

        assert!(matches!(from_ron("(name: 1)"), Err(FileError::Syntax(_))));
    }
}
