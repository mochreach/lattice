//! Reading and writing `.lattice` song files.
//!
//! A sound file in the song's folder, or one inside it, is saved relative to the song, so the
//! two can be moved together; the editor only sees absolute paths.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lattice_core::{Song, from_ron, to_ron};

use crate::demo;

/// The song at `path` if the engine can play it, or a new one if there's no such file; true if
/// it's new. `steps_per_beat` is the engine's, once it's running.
pub fn open(path: &Path, steps_per_beat: Option<u32>) -> Result<(Song, bool)> {
    let (song, new) = match read(path)? {
        Some(song) => (song, false),
        None => (new_song(), true),
    };
    check(&song, steps_per_beat).with_context(|| format!("can't open {}", path.display()))?;
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
    std::fs::write(path, to_ron(&song)).with_context(|| format!("can't write {}", path.display()))
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
    }
}

/// Checks the running engine can play `song`: it can't change steps per beat yet. Track counts
/// are checked as the song is read.
fn check(song: &Song, steps_per_beat: Option<u32>) -> Result<()> {
    let grid = &song.grid;
    if let Some(spb) = steps_per_beat
        && spb != grid.steps_per_beat
    {
        bail!(
            "the song has {} steps per beat; restart Lattice with it to change from {spb}",
            grid.steps_per_beat
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{InstKind, Instrument};

    #[test]
    fn the_amen_example_opens_and_plays_its_sample() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/amen/amen.lattice");
        let (song, new) = open(&path, None).unwrap();
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
        let (read, _) = open(&path, None).unwrap();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(read.grid.tracks, song.grid.tracks);
    }
}
