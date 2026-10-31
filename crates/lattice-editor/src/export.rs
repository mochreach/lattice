//! Bouncing to a WAV file. `:export` renders the song, by default for as long as it takes every
//! track's pattern and rules to come round together, and `:export-sel` renders only what's
//! selected, once, for a sample. The editor works out what to render; the frontend renders it,
//! offline and faster than real time, through the song's mixer.

use std::path::{Path, PathBuf};

use lattice_core::{Condition, Grid, Step, Transform};

use crate::{BEATS_PER_BAR, Editor, Effect};

/// The longest an export runs without being told how long, and the longest it can be told.
pub const MAX_BARS: u64 = 256;
/// How long an export runs by default, at most.
const DEFAULT_MAX_BARS: u64 = 64;

/// What to render to a WAV file: `grid` played from the top for `steps` steps, through the
/// song's buses at its tempo, then left to ring out.
#[derive(Debug, Clone, PartialEq)]
pub struct Export {
    pub path: PathBuf,
    pub grid: Grid,
    pub steps: u64,
}

impl Editor {
    /// `:export [PATH] [BARS]`: the whole song, to `PATH` or a `.wav` beside the song's file.
    pub(crate) fn export_song(&self, args: &str) -> Result<Vec<Effect>, String> {
        let mut words: Vec<&str> = args.split_whitespace().collect();
        let bar = (BEATS_PER_BAR as u64) * u64::from(self.grid.steps_per_beat.max(1));
        let steps = match words.last().map(|w| w.parse::<u64>()) {
            Some(Ok(bars)) => {
                words.pop();
                if !(1..=MAX_BARS).contains(&bars) {
                    return Err(format!("bars must be 1 to {MAX_BARS}"));
                }
                bars * bar
            }
            _ => cycle(&self.grid).min(DEFAULT_MAX_BARS * bar),
        };
        Ok(vec![Effect::Export(Box::new(Export {
            path: self.wav_path(&words.join(" "), "")?,
            grid: self.grid.clone(),
            steps,
        }))])
    }

    /// `:export-sel [PATH]`: the primary selection's steps on its tracks, once, alone.
    pub(crate) fn export_selection(&self, path: &str) -> Result<Vec<Effect>, String> {
        let region = self.selection();
        let mut grid = self.grid.clone();
        for (t, track) in grid.tracks.iter_mut().enumerate() {
            let selected = (region.first_track..=region.last_track).contains(&t);
            // What's selected is heard, whatever the mixer's mutes and solos say.
            track.mix.mute = !selected;
            track.mix.solo = false;
            let mut steps: Vec<Step> = track
                .steps
                .iter()
                .skip(region.first_step)
                .take(region.rows())
                .cloned()
                .collect();
            steps.resize(region.rows(), Step::default());
            track.steps = steps;
        }
        Ok(vec![Effect::Export(Box::new(Export {
            path: self.wav_path(path, "-selection")?,
            steps: region.rows() as u64,
            grid,
        }))])
    }

    /// The path typed, with `.wav` added if it has no extension, or else the song's file with
    /// `suffix` and `.wav`.
    fn wav_path(&self, typed: &str, suffix: &str) -> Result<PathBuf, String> {
        let path = if typed.is_empty() {
            let song = self
                .path
                .as_deref()
                .ok_or("no file name yet: give one, or :w the song first")?;
            let stem = song.file_stem().unwrap_or_default().to_string_lossy();
            song.with_file_name(format!("{stem}{suffix}"))
        } else {
            PathBuf::from(typed)
        };
        Ok(match path.extension() {
            Some(_) => path,
            None => path.with_extension("wav"),
        })
    }
}

/// Steps until every track is back at its first step on a pass where its rules do the same as
/// on the first: the length of the song's loop.
pub fn cycle(grid: &Grid) -> u64 {
    grid.tracks
        .iter()
        .map(|track| {
            let len = track.steps.len().max(1) as u64;
            let passes = track.rules.iter().fold(1, |passes, rule| {
                let every = match rule.when {
                    Condition::Every { n, .. } => u64::from(n.max(1)),
                    Condition::Always | Condition::Pass(_) => 1,
                };
                // Drifting comes round when it's rotated by a whole number of tracks.
                let drift = match rule.transform {
                    Transform::Drift(n) => len / gcd(len, u64::from(n.unsigned_abs())),
                    _ => 1,
                };
                lcm(passes, every * drift)
            });
            len * passes
        })
        .fold(1, lcm)
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn lcm(a: u64, b: u64) -> u64 {
    a / gcd(a, b) * b
}

/// The name of a file, for messages.
pub fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;
    use lattice_core::{Note, Rule, Track};

    fn track(len: usize, rules: Vec<Rule>) -> Track {
        Track {
            name: "t".into(),
            steps: vec![Step::default(); len],
            mix: Default::default(),
            instrument: Default::default(),
            effects: vec![],
            choke: 0,
            rules,
        }
    }

    fn editor(tracks: Vec<Track>) -> Editor {
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks,
            groups: vec![],
        };
        Editor::new(grid, 120.0)
    }

    fn run(e: &mut Editor, line: &str) -> Vec<Effect> {
        e.handle(Key::Char(':'));
        for c in line.chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter)
    }

    fn export(effects: Vec<Effect>) -> Export {
        match effects.as_slice() {
            [Effect::Export(export)] => (**export).clone(),
            other => panic!("expected an export, got {other:?}"),
        }
    }

    #[test]
    fn the_song_loops_when_every_track_and_rule_comes_round() {
        let rule = |transform, when| Rule { transform, when };
        let grid = |tracks| editor(tracks).grid;
        assert_eq!(cycle(&grid(vec![track(16, vec![]), track(12, vec![])])), 48);
        let every_3 = Condition::Every { n: 3, offset: 1 };
        let tr = rule(Transform::Transpose(12), every_3);
        assert_eq!(cycle(&grid(vec![track(16, vec![tr])])), 48);
        // Drifting by 4 of 16 comes round in 4 passes.
        let drift = rule(Transform::Drift(-4), Condition::Always);
        assert_eq!(cycle(&grid(vec![track(16, vec![drift])])), 64);
    }

    #[test]
    fn export_renders_the_loop_or_the_bars_asked_for() {
        let mut e = editor(vec![track(16, vec![]), track(12, vec![])]);
        assert!(run(&mut e, "export").is_empty(), "no file name yet");
        assert!(e.message.as_deref().unwrap().contains("no file name"));

        let x = export(run(&mut e, "export out"));
        assert_eq!((x.path, x.steps), ("out.wav".into(), 48));
        let x = export(run(&mut e, "export my song.wav 8"));
        assert_eq!((x.path, x.steps), ("my song.wav".into(), 8 * 16));

        e.path = Some("songs/tune.lattice".into());
        assert_eq!(
            export(run(&mut e, "export")).path,
            PathBuf::from("songs/tune.wav")
        );
        assert_eq!(
            export(run(&mut e, "export-sel")).path,
            PathBuf::from("songs/tune-selection.wav")
        );
    }

    #[test]
    fn export_sel_renders_only_the_selection() {
        let mut tracks = vec![track(16, vec![]), track(16, vec![]), track(4, vec![])];
        tracks[0].mix.solo = true;
        tracks[1].mix.mute = true;
        tracks[1].steps[5].note = Some(Note {
            pitch: 64,
            velocity: 100,
            length: 1,
        });
        let mut e = editor(tracks);
        // Tracks 1 and 2, steps 4 to 7.
        e.selections = vec![crate::Selection {
            anchor: crate::Cursor { track: 1, step: 4 },
            cursor: crate::Cursor { track: 2, step: 7 },
        }];
        let x = export(run(&mut e, "export-sel hit"));
        assert_eq!(x.steps, 4);
        let tracks = &x.grid.tracks;
        assert!(tracks[0].mix.mute && !tracks[0].mix.solo);
        assert!(!tracks[1].mix.mute, "selected, so heard");
        assert_eq!(tracks[1].steps.len(), 4);
        assert_eq!(tracks[1].steps[1].note.unwrap().pitch, 64);
        // Past the end of the short track, it's silent.
        assert_eq!(tracks[2].steps, vec![Step::default(); 4]);
    }
}
