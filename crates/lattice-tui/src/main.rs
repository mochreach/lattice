mod audio;
mod demo;
mod file;
mod keys;
mod mixer;
mod theme;
mod tracks;
mod ui;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use lattice_editor::{Editor, Effect};
use lattice_engine::{Command, Handle};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::mixer::Mixer;
use crate::theme::Theme;
use crate::tracks::Tracks;

// In debug builds, any allocation inside `assert_no_alloc` (the audio callback) aborts.
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

/// Redraw at least this often, so the playhead moves smoothly.
const FRAME: Duration = Duration::from_millis(16);

/// What to do after carrying out an effect.
enum Flow {
    Next,
    /// Something failed; drop the effects that were to follow.
    Stop,
    Quit,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (grid, palette) = demo::song();
    let mut editor = Editor::new(grid, demo::BPM).with_buses(demo::buses());
    // `lattice song.lattice` opens the song, or starts a new one there, like Helix.
    if let Some(path) = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from)
    {
        let (song, new) = file::open(&path, None)?;
        // The engine starts from the editor's song, so nothing needs sending yet.
        editor.load(song, path.clone());
        if new {
            editor.message = Some(format!("{} (new file)", path.display()));
        }
    }

    let patches = Tracks::patches(&palette, editor.grid.tracks.len());
    let mut audio = audio::start(&editor.grid, patches, editor.bpm)?;
    let mut tracks = Tracks::new(&editor, palette, audio.sample_rate);
    let mut mixer = Mixer::new(audio.sample_rate);
    mixer.sync(&editor, &mut audio.handle);
    if args.iter().any(|a| a == "--no-click") {
        editor.click = false;
        audio.handle.send(Command::SetClick(false));
    }

    let theme = Theme::detect();
    let mut terminal = ratatui::init();
    let result = run(
        &mut terminal,
        &mut editor,
        &mut tracks,
        &mut mixer,
        &mut audio.handle,
        &theme,
    );
    ratatui::restore();
    result
}

fn run(
    terminal: &mut DefaultTerminal,
    editor: &mut Editor,
    tracks: &mut Tracks,
    mixer: &mut Mixer,
    handle: &mut Handle,
    theme: &Theme,
) -> Result<()> {
    let mut view = ui::ViewState::default();
    loop {
        handle.collect_garbage();
        terminal.draw(|frame| ui::draw(frame, editor, handle.status(), theme, &mut view))?;

        if !event::poll(FRAME)? {
            continue;
        }
        // Handle everything that's waiting before drawing again.
        loop {
            if let Event::Key(key) = event::read()?
                && key.kind != KeyEventKind::Release
                && let Some(key) = keys::convert(key)
            {
                for effect in editor.handle(key) {
                    match apply(effect, editor, tracks, mixer, handle) {
                        Flow::Next => {}
                        Flow::Stop => break,
                        Flow::Quit => return Ok(()),
                    }
                }
            }
            if !event::poll(Duration::ZERO)? {
                break;
            }
        }
    }
}

/// Carries out an editor effect.
fn apply(
    effect: Effect,
    editor: &mut Editor,
    tracks: &mut Tracks,
    mixer: &mut Mixer,
    handle: &mut Handle,
) -> Flow {
    let command = match effect {
        // Effects follow their tracks, so the mixer syncs after the tracks do.
        Effect::TracksChanged | Effect::MixerChanged => {
            if !(tracks.sync(editor, handle) && mixer.sync(editor, handle)) {
                editor.message = Some("audio engine is busy; tracks out of step".into());
            }
            return Flow::Next;
        }
        Effect::TrackChanged(index) => Command::SetTrack {
            index,
            track: Box::new(editor.grid.tracks[index].clone()),
        },
        Effect::TogglePlay if handle.status().playing() => Command::Stop,
        Effect::TogglePlay => Command::Play,
        Effect::Bpm(bpm) => Command::SetBpm(bpm),
        Effect::Click(on) => Command::SetClick(on),
        Effect::Preview { track, pitch } => Command::Preview {
            track,
            pitch,
            velocity: 100,
        },
        Effect::Save(path) => {
            return match file::write(&path, &editor.song()) {
                Ok(()) => {
                    editor.saved(path);
                    Flow::Next
                }
                Err(e) => {
                    editor.message = Some(format!("{e:#}"));
                    Flow::Stop
                }
            };
        }
        Effect::Open(path) => return open(path, editor, tracks, mixer, handle),
        Effect::Quit => return Flow::Quit,
    };
    if !handle.send(command) {
        editor.message = Some("audio engine is busy; change dropped".into());
    }
    Flow::Next
}

/// Replaces the song with the one at `path`, or a new one if there's no such file.
fn open(
    path: PathBuf,
    editor: &mut Editor,
    tracks: &mut Tracks,
    mixer: &mut Mixer,
    handle: &mut Handle,
) -> Flow {
    let (song, new) = match file::open(&path, Some(editor.grid.steps_per_beat)) {
        Ok(opened) => opened,
        Err(e) => {
            editor.message = Some(format!("{e:#}"));
            return Flow::Stop;
        }
    };
    let message = new.then(|| format!("{} (new file)", path.display()));
    tracks.restart_palette();
    for effect in editor.load(song, path) {
        apply(effect, editor, tracks, mixer, handle);
    }
    if message.is_some() {
        editor.message = message;
    }
    Flow::Next
}
