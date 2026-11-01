//! Bouncing to a WAV file. `:export` renders the song as arranged, or this pattern for a number
//! of bars, and `:export-sel` renders only what's selected, once, for a sample. The editor works
//! out what to render; the frontend renders it, offline and faster than real time, through the
//! song's mixer.

use std::path::{Path, PathBuf};

use lattice_core::{BEATS_PER_BAR, Grid, Lane, MAX_BARS, Step};

use crate::{Editor, Effect};

/// What to render to a WAV file: `grid`'s tracks playing each part in turn, from the top,
/// through the song's buses at its tempo, then left to ring out.
#[derive(Debug, Clone, PartialEq)]
pub struct Export {
    pub path: PathBuf,
    pub grid: Grid,
    pub parts: Vec<Part>,
}

/// A pattern to play: a lane for each track, for so many steps.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub lanes: Vec<Lane>,
    pub steps: u64,
}

impl Export {
    /// Steps in all.
    pub fn steps(&self) -> u64 {
        self.parts.iter().map(|p| p.steps).sum()
    }
}

impl Editor {
    /// `:export [PATH] [BARS]`: the song as arranged, or this pattern for `BARS` bars, to `PATH`
    /// or a `.wav` beside the song's file.
    pub(crate) fn export_song(&self, args: &str) -> Result<Vec<Effect>, String> {
        let mut words: Vec<&str> = args.split_whitespace().collect();
        let a = &self.arrangement;
        let part = |pattern| Part {
            lanes: a.lanes(&self.grid, pattern),
            steps: a.steps(&self.grid, pattern),
        };
        let parts = match words.last().map(|w| w.parse::<u32>()) {
            Some(Ok(bars)) => {
                words.pop();
                if !(1..=MAX_BARS).contains(&bars) {
                    return Err(format!("bars must be 1 to {MAX_BARS}"));
                }
                let bar = u64::from(BEATS_PER_BAR * self.grid.steps_per_beat.max(1));
                vec![Part {
                    steps: u64::from(bars) * bar,
                    ..part(a.current)
                }]
            }
            // With nothing arranged, this pattern once round.
            _ if a.entries.is_empty() => vec![part(a.current)],
            _ => a
                .entries
                .iter()
                .flat_map(|e| std::iter::repeat_n(e.pattern, e.repeats as usize))
                .map(part)
                .collect(),
        };
        Ok(vec![Effect::Export(Box::new(Export {
            path: self.wav_path(&words.join(" "), "")?,
            grid: self.grid.clone(),
            parts,
        }))])
    }

    /// `:export-sel [PATH]`: the primary selection's steps on its tracks, once, alone.
    pub(crate) fn export_selection(&self, path: &str) -> Result<Vec<Effect>, String> {
        let region = self.selection();
        let mut grid = self.grid.clone();
        let mut lanes = vec![];
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
            lanes.push(Lane {
                steps,
                rules: track.rules.clone(),
            });
        }
        Ok(vec![Effect::Export(Box::new(Export {
            path: self.wav_path(path, "-selection")?,
            grid,
            parts: vec![Part {
                lanes,
                steps: region.rows() as u64,
            }],
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
    fn export_renders_the_song_or_the_bars_asked_for() {
        let mut e = editor(vec![track(16, vec![]), track(12, vec![])]);
        assert!(run(&mut e, "export").is_empty(), "no file name yet");
        assert!(e.message.as_deref().unwrap().contains("no file name"));

        let x = export(run(&mut e, "export out"));
        assert_eq!((x.path.clone(), x.steps()), ("out.wav".into(), 48));
        let x = export(run(&mut e, "export my song.wav 8"));
        assert_eq!((x.path.clone(), x.steps()), ("my song.wav".into(), 8 * 16));

        // As arranged: A, then B twice, two bars each.
        e.handle(Key::Esc);
        run(&mut e, "pattern-new");
        run(&mut e, "bars 2");
        e.arrangement.entries = vec![
            lattice_core::Entry {
                pattern: 0,
                repeats: 1,
            },
            lattice_core::Entry {
                pattern: 1,
                repeats: 2,
            },
        ];
        let x = export(run(&mut e, "export out"));
        assert_eq!(
            x.parts.iter().map(|p| p.steps).collect::<Vec<_>>(),
            [48, 32, 32]
        );
        assert_eq!(x.parts[1].lanes[1].steps.len(), 12);

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
        assert_eq!(x.steps(), 4);
        let tracks = &x.grid.tracks;
        assert!(tracks[0].mix.mute && !tracks[0].mix.solo);
        assert!(!tracks[1].mix.mute, "selected, so heard");
        let lanes = &x.parts[0].lanes;
        assert_eq!(lanes[1].steps.len(), 4);
        assert_eq!(lanes[1].steps[1].note.unwrap().pitch, 64);
        // Past the end of the short track, it's silent.
        assert_eq!(lanes[2].steps, vec![Step::default(); 4]);
    }
}
