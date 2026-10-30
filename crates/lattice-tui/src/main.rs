mod audio;
mod demo;
mod keys;
mod theme;
mod tracks;
mod ui;

use std::time::Duration;

use anyhow::Result;
use lattice_editor::{Editor, Effect};
use lattice_engine::{Command, Handle};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::theme::Theme;
use crate::tracks::Tracks;

// In debug builds, any allocation inside `assert_no_alloc` (the audio callback) aborts.
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

/// Redraw at least this often, so the playhead moves smoothly.
const FRAME: Duration = Duration::from_millis(16);

fn main() -> Result<()> {
    let (grid, patches) = demo::song();
    let mut audio = audio::start(&grid, patches.clone(), demo::BPM)?;
    let mut editor = Editor::new(grid, demo::BPM);
    let mut tracks = Tracks::new(&editor, patches, audio.sample_rate);
    if std::env::args().any(|a| a == "--no-click") {
        editor.click = false;
        audio.handle.send(Command::SetClick(false));
    }

    let theme = Theme::detect();
    let mut terminal = ratatui::init();
    let result = run(
        &mut terminal,
        &mut editor,
        &mut tracks,
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
                    if !apply(effect, editor, tracks, handle) {
                        return Ok(());
                    }
                }
            }
            if !event::poll(Duration::ZERO)? {
                break;
            }
        }
    }
}

/// Carries out an editor effect. Returns false to quit.
fn apply(effect: Effect, editor: &mut Editor, tracks: &mut Tracks, handle: &mut Handle) -> bool {
    let command = match effect {
        Effect::TracksChanged => {
            if !tracks.sync(editor, handle) {
                editor.message = Some("audio engine is busy; tracks out of step".into());
            }
            return true;
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
        Effect::Quit => return false,
    };
    if !handle.send(command) {
        editor.message = Some("audio engine is busy; change dropped".into());
    }
    true
}
