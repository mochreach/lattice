//! Bouncing to a WAV file. `:export` renders the song as arranged, or this pattern for a number
//! of bars, and `:export-sel` renders only what's selected, once, for a sample; `:export-loop`
//! makes that exactly the selection's length, to loop. The editor works
//! out what to render; the frontend renders it, offline and faster than real time, through the
//! song's mixer.
//!
//! Samples go in the song's [`SAMPLES`] folder, or until it's saved the [`scratch`] folder, from
//! which saving copies the ones its tracks play.

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
    /// Exactly as long as the parts, with the tail mixed back into the start, so it loops
    /// without a gap or a click.
    pub looped: bool,
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
            path: self.wav_path(&words.join(" "))?,
            grid: self.grid.clone(),
            parts,
            looped: false,
        }))])
    }

    /// `:export-sel [PATH]`: the primary selection's steps on its tracks, once, alone; or with
    /// `:export-loop`, `looped`.
    pub(crate) fn export_selection(
        &mut self,
        path: &str,
        looped: bool,
    ) -> Result<Vec<Effect>, String> {
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
                mute: false,
            });
        }
        let path = self.sample_file(path, if looped { "loop" } else { "selection" });
        // For `:sample` and `:chop` to pick up.
        self.last_export = Some(path.clone());
        Ok(vec![Effect::Export(Box::new(Export {
            path,
            grid,
            parts: vec![Part {
                lanes,
                steps: region.rows() as u64,
            }],
            looped,
        }))])
    }

    /// The path typed, with `.wav` added if it has no extension, or else the song's file with
    /// `.wav` in place of `.lattice`.
    fn wav_path(&self, typed: &str) -> Result<PathBuf, String> {
        if typed.is_empty() {
            let song = self
                .path
                .as_deref()
                .ok_or("no file name yet: give one, or :w the song first")?;
            return Ok(song.with_extension("wav"));
        }
        Ok(with_wav(PathBuf::from(typed)))
    }

    /// Where a new sample goes: a name typed alone in the samples folder, a path as typed, or
    /// with nothing typed, the song's name and `kind`, numbered so as not to replace a sample
    /// already made.
    pub(crate) fn sample_file(&self, typed: &str, kind: &str) -> PathBuf {
        let folder = self.samples_folder();
        if !typed.is_empty() {
            let path = with_wav(PathBuf::from(typed));
            return match path.parent() {
                Some(parent) if parent.as_os_str().is_empty() => folder.join(path),
                _ => path,
            };
        }
        let name = match self.path.as_deref().and_then(Path::file_stem) {
            Some(stem) => format!("{}-{kind}", stem.to_string_lossy()),
            None => kind.to_string(),
        };
        (1..)
            .map(|n| folder.join(format!("{name}-{n}.wav")))
            // The last may still be rendering.
            .find(|path| !path.exists() && self.last_export.as_ref() != Some(path))
            .unwrap()
    }

    /// The song's [`SAMPLES`] folder, beside it, or the [`scratch`] folder until it's saved.
    fn samples_folder(&self) -> PathBuf {
        match &self.path {
            Some(song) => song.with_file_name(SAMPLES),
            None => scratch(),
        }
    }
}

/// The folder beside a song that its samples are made in.
pub const SAMPLES: &str = "samples";

/// Where samples go before the song's saved, to be copied into its [`SAMPLES`] folder when it is:
/// one for each time Lattice runs.
pub fn scratch() -> PathBuf {
    std::env::temp_dir().join(format!("lattice-{}", std::process::id()))
}

fn with_wav(path: PathBuf) -> PathBuf {
    match path.extension() {
        Some(_) => path,
        None => path.with_extension("wav"),
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
            pattern_mute: false,
            name: "t".into(),
            steps: vec![Step::default(); len],
            mix: Default::default(),
            instrument: Default::default(),
            effects: Default::default(),
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
        assert_eq!((x.path.clone(), x.steps()), ("out.wav".into(), 16));
        let x = export(run(&mut e, "export my song.wav 8"));
        assert_eq!((x.path.clone(), x.steps()), ("my song.wav".into(), 8 * 16));

        // As arranged: A for a bar, then B twice, two bars each.
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
            [16, 32, 32]
        );
        assert_eq!(x.parts[1].lanes[1].steps.len(), 12);

        e.path = Some("songs/tune.lattice".into());
        assert_eq!(
            export(run(&mut e, "export")).path,
            PathBuf::from("songs/tune.wav")
        );
        assert_eq!(
            export(run(&mut e, "export-sel")).path,
            PathBuf::from("songs/samples/tune-selection-1.wav")
        );
        // Numbered, so as not to replace the last, which may not be written yet.
        assert_eq!(
            export(run(&mut e, "export-loop")).path,
            PathBuf::from("songs/samples/tune-loop-1.wav")
        );
        assert_eq!(
            export(run(&mut e, "export-loop")).path,
            PathBuf::from("songs/samples/tune-loop-2.wav")
        );
        // A name alone goes in the samples folder; a path, where it says.
        assert_eq!(
            export(run(&mut e, "export-loop hit")).path,
            PathBuf::from("songs/samples/hit.wav")
        );
        assert_eq!(
            export(run(&mut e, "export-loop ./hit")).path,
            PathBuf::from("./hit.wav")
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
        assert!(!x.looped);
        let x = export(run(&mut e, "export-loop"));
        assert!(x.looped && x.steps() == 4);
        // Not saved yet, so in the scratch folder.
        assert_eq!(x.path, scratch().join("loop-1.wav"));
        assert_eq!(e.last_export, Some(x.path.clone()), "for :chop to pick up");
    }
}
