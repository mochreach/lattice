//! Keeps a copy of unsaved changes, so closing the terminal by mistake, a power cut or a crash
//! loses half a minute at most. Every [`EVERY`] while the song has changes it hasn't been saved
//! with, and as Lattice stops for any reason but quitting, the song is written to
//! `~/.local/state/lattice/autosave/`, a folder for each song, with the samples it recorded
//! that are only in the scratch folder, which a reboot may clear. The song's own file is left
//! alone: `:recover` opens the copy, and saving or quitting throws it away.
//!
//! When the terminal closes, crossterm can be left reading it for ever, the UI thread with it, so
//! a thread of its own keeps the song then, from the copy it's given each frame, and stops
//! Lattice, if the UI thread hasn't done both itself within [`GRACE`].

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use lattice_core::Song;
use lattice_editor::Editor;

use crate::file;
use crate::log::log;

/// How often unsaved changes are kept.
pub const EVERY: Duration = Duration::from_secs(30);

/// How long the UI thread has to stop by itself once it's been told to.
const GRACE: Duration = Duration::from_secs(1);

/// The song with unsaved changes as it is now, and its file, for the watching thread.
type Latest = Arc<Mutex<Option<(Option<PathBuf>, Song)>>>;

/// Where the copy of the song at `path` goes, or of the song not yet saved anywhere.
pub fn path(song: Option<&Path>) -> Option<PathBuf> {
    let root = crate::log::path()?.with_file_name("autosave");
    // The whole path, so two songs called the same in different folders keep apart.
    let name: String = match song {
        Some(song) => {
            let song = std::path::absolute(song).unwrap_or_else(|_| song.to_path_buf());
            (song.to_string_lossy().chars())
                .map(|c| {
                    if c.is_alphanumeric() || c == '.' || c == '-' {
                        c
                    } else {
                        '%'
                    }
                })
                .collect()
        }
        None => "untitled".into(),
    };
    Some(root.join(name).join("song.lattice"))
}

/// When the copy at `copy` was made, if it's newer than the song at `song`, or the song was
/// never saved there: something to say as Lattice starts.
pub fn waiting(song: Option<&Path>) -> Option<String> {
    let copy = path(song)?;
    let made = std::fs::metadata(&copy).and_then(|m| m.modified()).ok()?;
    let saved = song.and_then(|s| std::fs::metadata(s).and_then(|m| m.modified()).ok());
    if saved.is_some_and(|saved| saved >= made) {
        return None;
    }
    let ago = SystemTime::now().duration_since(made).unwrap_or_default();
    Some(format!(
        "unsaved changes from {} ago were kept: :recover to bring them back",
        how_long(ago)
    ))
}

fn how_long(ago: Duration) -> String {
    let minutes = ago.as_secs() / 60;
    match minutes {
        0 => "under a minute".into(),
        1 => "a minute".into(),
        2..60 => format!("{minutes} minutes"),
        60..120 => "an hour".into(),
        120..2880 => format!("{} hours", minutes / 60),
        _ => format!("{} days", minutes / 1440),
    }
}

/// Keeps the editor's changes now and then.
pub struct Keeper {
    /// When the song was last looked at, and what was kept then.
    looked: Instant,
    kept: Option<Song>,
    /// The copy that's this run's to throw away: that of the song with changes now. One kept
    /// from before isn't, until the song it's for is changed, as it may be wanted back.
    mine: Option<PathBuf>,
    latest: Latest,
    /// Set once the UI thread has finished with the song, so the watching thread needn't.
    done: Arc<AtomicBool>,
}

impl Keeper {
    /// Starts keeping, with a thread to keep the song itself if `stop` is set and the UI
    /// thread doesn't finish.
    pub fn new(stop: Arc<AtomicBool>) -> Self {
        let latest = Latest::default();
        let done = Arc::new(AtomicBool::new(false));
        let (watched, finished) = (Arc::clone(&latest), Arc::clone(&done));
        std::thread::spawn(move || watch(&stop, &finished, &watched));
        Self {
            looked: Instant::now(),
            kept: None,
            mine: None,
            latest,
            done,
        }
    }

    /// The UI thread's finished with the song, and kept it if it needed to.
    pub fn finish(&self) {
        self.done.store(true, Ordering::Relaxed);
    }

    /// Keeps the song if it's been [`EVERY`] since it was last looked at, and it's changed;
    /// throws its copy away once it's saved, or changed back as it was.
    pub fn tick(&mut self, editor: &Editor) {
        if !editor.modified() {
            self.forget();
            return;
        }
        self.mine = path(editor.path.as_deref());
        let song = editor.song();
        // The UI thread is the only other taker, so this never waits long.
        let mut latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        if latest.as_ref().is_none_or(|(_, kept)| *kept != song) {
            *latest = Some((editor.path.clone(), song));
        }
        drop(latest);
        if self.looked.elapsed() >= EVERY {
            self.looked = Instant::now();
            self.keep(editor);
        }
    }

    /// Keeps the song now, if it has changes not saved or kept already.
    pub fn keep(&mut self, editor: &Editor) {
        if !editor.modified() {
            return;
        }
        let song = editor.song();
        if self.kept.as_ref() == Some(&song) {
            return;
        }
        match write(editor.path.as_deref(), song.clone()) {
            Ok(copy) => log!("kept unsaved changes in {}", copy.display()),
            // Not worth interrupting the player for; the next try may work.
            Err(e) => log!("couldn't keep unsaved changes: {e:#}"),
        }
        self.kept = Some(song);
    }

    /// The song's been saved, or deliberately left: its copy's no longer wanted.
    pub fn forget(&mut self) {
        self.kept = None;
        *self.latest.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(copy) = self.mine.take()
            && let Some(folder) = copy.parent()
            && folder.exists()
            && let Err(e) = std::fs::remove_dir_all(folder)
        {
            log!("couldn't throw away {}: {e}", folder.display());
        }
    }
}

/// Waits for `stop`, then gives the UI thread [`GRACE`] to finish; if it doesn't, it's stuck,
/// so keeps the song from `latest` and stops Lattice.
fn watch(stop: &AtomicBool, done: &AtomicBool, latest: &Latest) {
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let asked = Instant::now();
    while asked.elapsed() < GRACE {
        if done.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if let Some((path, song)) = latest.lock().unwrap_or_else(|e| e.into_inner()).take() {
        match write(path.as_deref(), song) {
            Ok(copy) => log!("kept unsaved changes in {}", copy.display()),
            Err(e) => log!("couldn't keep unsaved changes: {e:#}"),
        }
    }
    log!("stopped, the terminal gone");
    std::process::exit(1);
}

fn write(song_path: Option<&Path>, mut song: Song) -> Result<PathBuf> {
    let copy = path(song_path).context("there's no home folder to keep it in")?;
    if let Some(folder) = copy.parent() {
        std::fs::create_dir_all(folder)
            .with_context(|| format!("can't make {}", folder.display()))?;
    }
    file::collect(&copy, &mut song, &lattice_editor::scratch())?;
    file::write(&copy, &song)?;
    Ok(copy)
}

/// The kept copy of the song at `path`, its samples put back in the scratch folder, as if just
/// recorded, so saving copies them beside the song.
pub fn read(path: Option<&Path>) -> Result<Song> {
    let copy = self::path(path).context("there's no home folder to look in")?;
    if !copy.exists() {
        anyhow::bail!("no unsaved changes were kept for this song");
    }
    let (mut song, _) = file::open(&copy)?;
    let folder = copy.parent().unwrap_or(Path::new("")).to_path_buf();
    let scratch = lattice_editor::scratch();
    for track in &mut song.grid.tracks {
        let Some(sample) = track.instrument.file.as_mut() else {
            continue;
        };
        if !sample.starts_with(&folder) {
            continue;
        }
        let Some(name) = sample.file_name() else {
            continue;
        };
        std::fs::create_dir_all(&scratch)
            .with_context(|| format!("can't make {}", scratch.display()))?;
        let to = scratch.join(name);
        std::fs::copy(&*sample, &to).with_context(|| format!("can't copy {}", sample.display()))?;
        *sample = to;
    }
    Ok(song)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_long_ago_is_said_roughly() {
        let ago = |s| how_long(Duration::from_secs(s));
        assert_eq!(ago(20), "under a minute");
        assert_eq!(ago(65), "a minute");
        assert_eq!(ago(600), "10 minutes");
        assert_eq!(ago(3 * 3600), "3 hours");
        assert_eq!(ago(3 * 86_400), "3 days");
    }

    #[test]
    fn each_song_keeps_its_copy_in_a_folder_of_its_own() {
        let a = path(Some(Path::new("/songs/a/tune.lattice"))).unwrap();
        let b = path(Some(Path::new("/songs/b/tune.lattice"))).unwrap();
        assert_ne!(a.parent(), b.parent());
        assert_eq!(a.file_name().unwrap(), "song.lattice");
        assert!(
            path(None)
                .unwrap()
                .ends_with("autosave/untitled/song.lattice")
        );
    }
}
