//! Reading and writing `.lattice` song files.
//!
//! Instrument patches aren't part of the song yet: track `i` always plays the demo's
//! instrument `i`, so a song can't have more tracks than there are instruments.

use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, Result, bail};
use lattice_core::{Grid, Song, Track, from_ron, to_ron};

use crate::demo;

/// The song at `path` if the engine can play it, or a new one if there's no such file; true if
/// it's new. `steps_per_beat` is the engine's, once it's running.
pub fn open(path: &Path, instruments: usize, steps_per_beat: Option<u32>) -> Result<(Song, bool)> {
    let (song, new) = match read(path)? {
        Some(song) => (song, false),
        None => (new_song(), true),
    };
    check(&song, instruments, steps_per_beat)
        .with_context(|| format!("can't open {}", path.display()))?;
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
    }
}

/// Checks the engine can play `song`: no more tracks than instruments and, once the engine is
/// running, the same steps per beat, which it can't change yet.
fn check(song: &Song, instruments: usize, steps_per_beat: Option<u32>) -> Result<()> {
    let grid = &song.grids[song.first_grid()];
    if grid.tracks.len() > instruments {
        bail!(
            "the song has {} tracks, but there are only {instruments} instruments so far",
            grid.tracks.len()
        );
    }
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

/// The grid for the engine: the song's tracks, then silent ones for any instruments left over.
pub fn engine_grid(grid: &Grid, instruments: usize) -> Grid {
    let mut grid = grid.clone();
    grid.tracks.resize_with(instruments, Track::default);
    grid
}
