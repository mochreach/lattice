//! Reading and writing `.lattice` song files.

use std::io::ErrorKind;
use std::path::Path;

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
    let song = from_ron(&text).with_context(|| format!("can't open {}", path.display()))?;
    Ok(Some(song))
}

pub fn write(path: &Path, song: &Song) -> Result<()> {
    std::fs::write(path, to_ron(song)).with_context(|| format!("can't write {}", path.display()))
}

/// What to edit when a file doesn't exist yet.
fn new_song() -> Song {
    Song {
        name: "new".into(),
        bpm: demo::BPM,
        grids: vec![demo::blank()],
        order: vec![0],
        buses: demo::buses(),
    }
}

/// Checks the running engine can play `song`: it can't change steps per beat yet. Track counts
/// are checked as the song is read.
fn check(song: &Song, steps_per_beat: Option<u32>) -> Result<()> {
    let grid = &song.grids[song.first_grid()];
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
