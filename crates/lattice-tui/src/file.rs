//! Reading and writing `.lattice` song files.
//!
//! A sound file in the song's folder, or one inside it, is saved relative to the song, so the
//! two can be moved together; the editor only sees absolute paths. Samples made before the song
//! was saved are copied into its samples folder as it is.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lattice_core::{Song, from_ron, to_ron};
use lattice_editor::SAMPLES;

use crate::demo;

/// The song at `path`, or a new one if there's no such file; true if it's new.
pub fn open(path: &Path) -> Result<(Song, bool)> {
    let (song, new) = match read(path)? {
        Some(song) => (song, false),
        None => (new_song(), true),
    };
    Ok((song, new))
}

/// The song at `path`, or `None` if there's no such file.
fn read(path: &Path) -> Result<Option<Song>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("can't read {}", path.display())),
    };
    let mut song: Song =
        from_ron(&text).with_context(|| format!("can't open {}", path.display()))?;
    let folder = folder(path);
    for file in files(&mut song) {
        *file = folder.join(&*file);
    }
    Ok(Some(song))
}

pub fn write(path: &Path, song: &Song) -> Result<()> {
    let mut song = song.clone();
    let folder = folder(path);
    for file in files(&mut song) {
        if let Ok(inside) = file.strip_prefix(&folder) {
            *file = inside.to_path_buf();
        }
    }
    let text = to_ron(&song).with_context(|| format!("can't save {}", path.display()))?;
    std::fs::write(path, text).with_context(|| format!("can't write {}", path.display()))
}

/// Copies the samples `song` plays from `scratch`, made before it was saved, into the samples
/// folder beside `path`, and has the song play the copies. Returns each file moved, and where to.
pub fn collect(path: &Path, song: &mut Song, scratch: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
    let samples = folder(path).join(SAMPLES);
    let mut moved: Vec<(PathBuf, PathBuf)> = vec![];
    for file in files(song).filter(|f| f.starts_with(scratch)) {
        if let Some((_, to)) = moved.iter().find(|(from, _)| from == file) {
            *file = to.clone();
            continue;
        }
        let bytes =
            std::fs::read(&*file).with_context(|| format!("can't read {}", file.display()))?;
        std::fs::create_dir_all(&samples)
            .with_context(|| format!("can't make {}", samples.display()))?;
        let stem = file.file_stem().unwrap_or_default().to_string_lossy();
        // The same name if it's free, or already this sample; numbered if not.
        let to = std::iter::once(samples.join(format!("{stem}.wav")))
            .chain((2..).map(|n| samples.join(format!("{stem}-{n}.wav"))))
            .find(|to| std::fs::read(to).map_or(true, |there| there == bytes))
            .unwrap_or_else(|| samples.join(format!("{stem}.wav")));
        if !to.exists() {
            std::fs::write(&to, &bytes).with_context(|| format!("can't write {}", to.display()))?;
        }
        moved.push((file.clone(), to.clone()));
        *file = to;
    }
    Ok(moved)
}

/// The folder the song at `path` is in, as an absolute path.
fn folder(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    path.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// The sound files the song's instruments play.
fn files(song: &mut Song) -> impl Iterator<Item = &mut PathBuf> {
    (song.grid.tracks.iter_mut()).filter_map(|t| t.instrument.file.as_mut())
}

/// What to edit when a file doesn't exist yet.
fn new_song() -> Song {
    Song {
        name: "new".into(),
        bpm: demo::BPM,
        grid: demo::blank(),
        arrangement: Default::default(),
        buses: demo::buses(),
        click: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{Gesture, InstKind, Instrument};

    #[test]
    fn the_amen_example_opens_and_plays_its_sample() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/amen/amen.lattice");
        let (song, new) = open(&path).unwrap();
        assert!(!new);
        let amen = (song.grid.tracks.iter())
            .find(|t| t.name == "amen")
            .unwrap();
        assert_eq!(amen.instrument.kind, InstKind::Chop);
        let sample = crate::samples::load(amen.instrument.file.as_ref().unwrap()).unwrap();
        // Four bars at the song's tempo.
        let seconds = sample.len() as f64 / f64::from(sample.rate);
        assert!(
            (seconds - 4.0 * 4.0 * 60.0 / song.bpm).abs() < 0.001,
            "{seconds}"
        );
    }

    #[test]
    fn the_mountain_king_example_opens_and_chops_its_own_render() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/mountain-king/mountain-king.lattice");
        let (song, _) = open(&path).unwrap();
        let tracks = &song.grid.tracks;
        let hall = tracks.iter().find(|t| t.name == "hall").unwrap();
        assert_eq!(hall.instrument.kind, InstKind::Chop);
        let sample = crate::samples::load(hall.instrument.file.as_ref().unwrap()).unwrap();
        let seconds = sample.len() as f64 / f64::from(sample.rate);
        assert!(
            (seconds - 4.0 * 4.0 * 60.0 / song.bpm).abs() < 0.001,
            "{seconds}"
        );
        let troll = &tracks[0];
        assert_eq!(troll.lock_columns().len(), 2);
        assert_eq!(troll.macros.len(), 1);
        assert_eq!(troll.effects.lanes().len(), 2);
        assert_eq!(song.grid.groups.len(), 2);
        // The conductor speeds the frenzy up through its three times round.
        let frenzy = (song.arrangement.patterns.iter())
            .find(|p| p.name == "frenzy")
            .unwrap();
        assert_eq!(frenzy.conductor.steps.len(), 192);
        assert_eq!(frenzy.conductor.gestures().count(), 4);
    }

    #[test]
    fn the_scarlatti_example_opens_with_both_halves_repeated() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/scarlatti/scarlatti.lattice");
        let (song, _) = open(&path).unwrap();
        // 2/2: two half-note beats of four eighths.
        assert_eq!((song.grid.beats_per_bar, song.grid.steps_per_beat), (2, 4));
        assert_eq!(song.grid.steps_per_bar(), 8);
        let names: Vec<_> = (song.arrangement.entries.iter())
            .map(|e| song.arrangement.patterns[e.pattern].name.as_str())
            .collect();
        assert_eq!(names.len(), 15);
        assert_eq!(names.first(), Some(&"launch"));
        assert_eq!(names.last(), Some(&"fine"));
        // The fills: a canon on the echo, held back until one plays.
        let echo = song
            .grid
            .tracks
            .iter()
            .position(|t| t.name == "echo")
            .unwrap();
        let a1 = song
            .arrangement
            .patterns
            .iter()
            .find(|p| p.name == "A1")
            .unwrap();
        assert!(
            a1.lanes[echo]
                .steps
                .iter()
                .any(|s| s.gestures().any(|g| g == Gesture::Fill))
        );
        // The lead's shimmer, an octave-up echo on a lane of its own.
        let lead = song.grid.tracks.iter().find(|t| t.name == "lead").unwrap();
        assert_eq!(lead.effects.lanes().len(), 2);
    }

    #[test]
    fn sound_files_beside_the_song_are_saved_relative_to_it() {
        let folder = std::env::temp_dir().join(format!("lattice-file-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("song.lattice");
        let mut song = new_song();
        song.grid.tracks[0].instrument = Instrument::new(InstKind::Chop);
        song.grid.tracks[0].instrument.file = Some(folder.join("loops/amen.wav"));
        song.grid.tracks[1].instrument = Instrument::new(InstKind::Sampler);
        song.grid.tracks[1].instrument.file = Some("/elsewhere/kick.wav".into());
        write(&path, &song).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("file: \"loops/amen.wav\""), "{text}");
        assert!(text.contains("file: \"/elsewhere/kick.wav\""), "{text}");
        let (read, _) = open(&path).unwrap();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(read.grid.tracks, song.grid.tracks);
    }

    #[test]
    fn samples_made_before_saving_are_copied_beside_the_song() {
        let root = std::env::temp_dir().join(format!("lattice-collect-{}", std::process::id()));
        let (scratch, folder) = (root.join("scratch"), root.join("song"));
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::create_dir_all(folder.join(SAMPLES)).unwrap();
        std::fs::write(scratch.join("loop-1.wav"), b"new").unwrap();
        // Something else already has the name.
        std::fs::write(folder.join(SAMPLES).join("loop-1.wav"), b"old").unwrap();
        let path = folder.join("song.lattice");
        let mut song = new_song();
        for t in 0..2 {
            song.grid.tracks[t].instrument = Instrument::new(InstKind::Chop);
            song.grid.tracks[t].instrument.file = Some(scratch.join("loop-1.wav"));
        }
        song.grid.tracks[2].instrument = Instrument::new(InstKind::Sampler);
        song.grid.tracks[2].instrument.file = Some("/elsewhere/kick.wav".into());

        let moved = collect(&path, &mut song, &scratch).unwrap();
        let to = folder.join(SAMPLES).join("loop-1-2.wav");
        assert_eq!(moved, [(scratch.join("loop-1.wav"), to.clone())]);
        assert_eq!(std::fs::read(&to).unwrap(), b"new");
        let files: Vec<_> = (song.grid.tracks.iter())
            .filter_map(|t| t.instrument.file.clone())
            .collect();
        assert_eq!(files, [to.clone(), to, "/elsewhere/kick.wav".into()]);

        // Once collected, saving again copies nothing.
        assert!(collect(&path, &mut song, &scratch).unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
