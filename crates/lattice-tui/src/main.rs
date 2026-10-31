mod audio;
mod bars;
mod demo;
mod file;
mod key_log;
mod keys;
mod mixer;
mod mixer_page;
mod theme;
mod tracks;
mod ui;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use lattice_editor::{Editor, Effect, Mode, Transport};
use lattice_engine::{Command, Handle, Status};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, Event, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::{execute, terminal};

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
    let grid = demo::song();
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

    let mut audio = audio::start(&editor.grid, editor.bpm)?;
    let mut tracks = Tracks::new(&editor, audio.sample_rate);
    let mut mixer = Mixer::new(audio.sample_rate);
    mixer.sync(&editor, &mut audio.handle);
    if args.iter().any(|a| a == "--no-click") {
        editor.click = false;
        audio.handle.send(Command::SetClick(false));
    }
    editor.show_keys = args.iter().any(|a| a == "--show-keys");

    let theme = Theme::detect();
    let mut terminal = ratatui::init();
    // Ask for keys to be told apart (the kitty protocol), so the numpad can be pads, and for
    // releases, so recorded notes last as long as they're held. Terminals that don't speak it
    // carry on as before.
    let enhanced = terminal::supports_keyboard_enhancement().unwrap_or(false)
        && execute!(
            std::io::stdout(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )
        .is_ok();
    let result = run(
        &mut terminal,
        &mut editor,
        &mut tracks,
        &mut mixer,
        &mut audio.handle,
        &theme,
    );
    if enhanced {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    }
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
    let mut playhead = Playhead::default();
    loop {
        handle.collect_garbage();
        // Watched every frame, so it sees each step start within a frame of it.
        editor.set_transport(playhead.transport(handle.status(), editor, Instant::now()));
        terminal.draw(|frame| ui::draw(frame, editor, handle.status(), theme, &mut view))?;

        if !event::poll(FRAME)? {
            continue;
        }
        // Handle everything that's waiting before drawing again.
        loop {
            if let Event::Key(event) = event::read()?
                && let Some(key) = keys::convert(event)
            {
                let now = Instant::now();
                editor.set_transport(playhead.transport(handle.status(), editor, now));
                let effects = match event.kind {
                    KeyEventKind::Release => editor.release(key),
                    // A held key repeating would record the note over and over.
                    KeyEventKind::Repeat if editor.recording() => vec![],
                    _ => {
                        let effects = editor.handle(key);
                        let typing = matches!(editor.mode, Mode::Command(_));
                        view.keys.record(key, typing, now);
                        effects
                    }
                };
                for effect in effects {
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

/// Where playback is, for recording. The engine says which step each track is on; how long
/// ago that changed says how far into the step it is.
#[derive(Default)]
struct Playhead {
    /// The first track's position, and when it was first seen.
    seen: Option<((usize, u32), Instant)>,
}

impl Playhead {
    fn transport(&mut self, status: &Status, editor: &Editor, now: Instant) -> Option<Transport> {
        let positions: Vec<_> = (0..editor.grid.tracks.len())
            .map(|t| status.position(t))
            .collect();
        let Some(first) = positions
            .first()
            .copied()
            .flatten()
            .filter(|_| status.playing())
        else {
            self.seen = None;
            return None;
        };
        let since = match self.seen {
            Some((position, since)) if position == first => since,
            _ => {
                self.seen = Some((first, now));
                now
            }
        };
        let step = 60.0 / (editor.bpm * f64::from(editor.grid.steps_per_beat.max(1)));
        Some(Transport {
            positions,
            into_step: (now.duration_since(since).as_secs_f64() / step).min(0.99),
        })
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
    for effect in editor.load(song, path) {
        apply(effect, editor, tracks, mixer, handle);
    }
    if message.is_some() {
        editor.message = message;
    }
    Flow::Next
}
