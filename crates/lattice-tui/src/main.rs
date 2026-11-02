// Tests may unwrap and panic: that's how they fail.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::indexing_slicing
    )
)]
mod audio;
mod bars;
mod chains;
mod config;
mod ctl;
mod demo;
mod devices;
mod export;
mod face;
mod file;
mod fx_page;
mod help_page;
mod history;
mod inst_page;
mod key_log;
mod keys;
mod log;
mod looper;
mod mixer;
mod mixer_page;
mod presets;
mod samples;
mod sequencer;
mod song_page;
mod splash;
mod theme;
mod tracks;
mod ui;
mod wave;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use lattice_editor::{Editor, Effect, Key, Mode, PromptKind, Transport};
use lattice_engine::{Command, Handle, Status};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, Event, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::{execute, terminal};

use crate::devices::Machine;
use crate::export::Exports;
use crate::log::{Watch, log};
use crate::mixer::Mixer;
use crate::sequencer::Sequencer;
use crate::splash::Splash;
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
    // `lattice ctl …` talks to a Lattice that's running.
    if args.first().is_some_and(|a| a == "ctl") {
        return ctl::client(args.get(1..).unwrap_or_default());
    }
    log::start();
    log!("arguments: {args:?}");
    let demo = demo::song();
    let mut editor = Editor::new(demo.grid, demo.bpm)
        .with_buses(demo.buses)
        .with_arrangement(demo.arrangement);
    // `--config PATH` reads that config in place of the usual one.
    let config = args.iter().position(|a| a == "--config");
    let named = config.and_then(|i| args.get(i + 1)).map(PathBuf::from);
    // `lattice song.lattice` opens the song, or starts a new one there, like Helix.
    if let Some(path) = (args.iter().enumerate())
        .find(|&(i, a)| !a.starts_with("--") && config.is_none_or(|c| i != c + 1))
        .map(|(_, a)| PathBuf::from(a))
    {
        let (song, new) = file::open(&path, None)?;
        log!("opened {}", path.display());
        // The engine starts from the editor's song, so nothing needs sending yet.
        editor.load(song, path.clone());
        if new {
            editor.message = Some(format!("{} (new file)", path.display()));
        }
    }

    // `--tutor` starts the tutor, on a song of its own.
    if args.iter().any(|a| a == "--tutor") {
        // The engine starts from the editor's song, so nothing needs sending yet.
        let _ = editor.start_tutor();
    }

    let (mut machine, mut handle) = Machine::start(&editor.grid.clone(), editor.bpm, &mut editor)?;
    let sample_rate = machine.output.sample_rate;
    let mut tracks = Tracks::new(&editor, sample_rate);
    if let Some(problem) = tracks.take_problem() {
        editor.message = Some(problem);
    }
    let mut mixer = Mixer::new(sample_rate);
    let mut exports = Exports::new(sample_rate);
    mixer.sync(&editor, &mut handle);
    if args.iter().any(|a| a == "--no-click") {
        editor.click = false;
        handle.send(Command::SetClick(false));
    }
    editor.show_keys = args.iter().any(|a| a == "--show-keys");
    editor.history = history::load();
    editor.chains = chains::names();
    editor.presets = presets::names();

    let mut theme = Theme::detect();
    if let Some(path) = named.clone().or_else(config::default_path)
        && let Some(problem) = config::load(&path, named.is_some(), &mut editor.keymap, &mut theme)
    {
        editor.message = Some(problem);
    }
    // Without a terminal, run from a script say, there's nothing to draw in: say so, rather
    // than panicking.
    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(e) => {
            ratatui::restore();
            return Err(anyhow!("Lattice needs a terminal to draw in: {e}"));
        }
    };
    // Ask for every key as a code (the kitty protocol), as only then are the numpad's digits told
    // apart from the number row's, so they can be pads; with the shifted character, so `%` and
    // `C` still arrive as typed; and with releases, so recorded notes last as long as they're
    // held. Terminals that don't speak it carry on as before.
    let enhanced = terminal::supports_keyboard_enhancement().unwrap_or(false)
        && execute!(
            std::io::stdout(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )
        .is_ok();
    editor.releases = enhanced;
    let splash = (!args.iter().any(|a| a == "--no-splash")).then(|| Splash::new(Instant::now()));
    let server = ctl::Server::start(args.iter().any(|a| a == "--allow-agent-save"));
    // Lattice shouldn't panic, but if it does, the song's rescued once the terminal's back.
    let result = catch_unwind(AssertUnwindSafe(|| {
        run(
            &mut terminal,
            &mut editor,
            &mut tracks,
            &mut mixer,
            &mut exports,
            &mut machine,
            &mut handle,
            &theme,
            splash,
            server.as_ref(),
        )
    }));
    if enhanced {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    }
    ratatui::restore();
    let result = result.unwrap_or_else(|_| Err(rescue(&editor)));
    if let Err(e) = history::save(&editor.history) {
        log!("couldn't keep the command history: {e:#}");
    }
    match &result {
        Ok(()) => log!("quit"),
        Err(e) => log!("stopped by an error: {e:#}"),
    }
    result
}

/// After a panic: saves the song, if it had changes, beside its file as `.rescue.lattice`, or in
/// the log's folder if it's never been saved, and says where. The original's left alone, as the
/// song may be half changed.
fn rescue(editor: &Editor) -> anyhow::Error {
    let log = log::path().map_or_else(|| "the log".into(), |p| p.display().to_string());
    if !editor.modified() {
        return anyhow!(
            "Lattice crashed, sorry, with nothing unsaved. What went wrong is in {log}."
        );
    }
    let path = match &editor.path {
        Some(path) => Some(path.with_extension("rescue.lattice")),
        None => log::path().map(|p| p.with_file_name("rescue.lattice")),
    };
    // The editor may be what panicked, so making the song might too.
    let saved = path.and_then(|path| {
        catch_unwind(AssertUnwindSafe(|| file::write(&path, &editor.song())))
            .ok()?
            .ok()
            .map(|()| path)
    });
    match saved {
        Some(path) => {
            log!("rescued the song to {}", path.display());
            anyhow!(
                "Lattice crashed, sorry. Your song, as it was, is saved in {}; what went wrong is in {log}.",
                path.display()
            )
        }
        None => anyhow!(
            "Lattice crashed, sorry, and couldn't save the song. What went wrong is in {log}."
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    terminal: &mut DefaultTerminal,
    editor: &mut Editor,
    tracks: &mut Tracks,
    mixer: &mut Mixer,
    exports: &mut Exports,
    machine: &mut Machine,
    handle: &mut Handle,
    theme: &Theme,
    mut splash: Option<Splash>,
    server: Option<&ctl::Server>,
) -> Result<()> {
    let mut view = ui::ViewState::default();
    let mut playhead = Playhead::default();
    let mut sequencer = Sequencer::default();
    let mut watch = Watch::new();
    let mut said = None;
    let mut page = editor.page;
    let mut playing = false;
    loop {
        handle.collect_garbage();
        if let Some(problem) = watch.look(handle.status().leaked()) {
            editor.message = Some(problem);
        }
        // What the status line says, as it changes, so the log shows what the player saw.
        if editor.message != said {
            if let Some(message) = &editor.message {
                log!("says: {message}");
            }
            said.clone_from(&editor.message);
        }
        if editor.page != page {
            page = editor.page;
            log!("page: {}", whereabouts(editor));
        }
        if handle.status().playing() != playing {
            playing = !playing;
            log!(
                "{} at {} BPM",
                if playing { "playing" } else { "stopped" },
                editor.bpm
            );
        }
        let written = exports.poll(editor);
        if !written.is_empty() {
            written.iter().for_each(|path| tracks.reload(path));
            apply(
                Effect::TracksChanged,
                editor,
                tracks,
                mixer,
                exports,
                &sequencer,
                machine,
                handle,
            );
        }
        for effect in machine.tick(editor, handle) {
            apply(
                effect, editor, tracks, mixer, exports, &sequencer, machine, handle,
            );
        }
        sequencer.sync(editor, handle);
        // Watched every frame, so it sees each step start within a frame of it.
        editor.set_transport(playhead.transport(handle.status(), editor, Instant::now()));
        // The tutor checks its lesson as playback starts and stops, and after each key.
        for effect in editor.tutor_tick() {
            if let Flow::Quit = apply(
                effect, editor, tracks, mixer, exports, &sequencer, machine, handle,
            ) {
                return Ok(());
            }
        }
        let now = Instant::now();
        if splash.as_ref().is_some_and(|s| s.done(now)) {
            splash = None;
        }
        let requests = server.map(ctl::Server::requests).unwrap_or_default();
        let drawn = terminal.draw(|frame| {
            ui::draw(frame, editor, handle.status(), theme, &mut view);
            if let Some(splash) = &splash {
                let area = frame.area();
                splash.draw(frame.buffer_mut(), area, theme, now);
            }
        })?;
        let screen =
            (requests.iter().any(|r| r.line.trim() == "screen")).then(|| ctl::screen(drawn.buffer));
        // Answered after drawing, so `screen` shows what's just been drawn.
        for request in requests {
            let allow_save = server.is_some_and(|s| s.allow_save);
            let (answer, flow) = match ctl::Ask::parse(&request.line) {
                Err(e) => (Err(e), Flow::Next),
                Ok(ctl::Ask::Screen) => (Ok(screen.clone().unwrap_or_default()), Flow::Next),
                Ok(ctl::Ask::State) => (Ok(ctl::state(editor, handle.status())), Flow::Next),
                Ok(ask) => {
                    let said = editor.message.clone();
                    let (refused, flow) = serve(
                        ask, allow_save, editor, tracks, mixer, exports, &sequencer, machine,
                        handle,
                    );
                    let answer = match refused {
                        Some(why) => Err(why),
                        // What the status line says now, if the request changed it.
                        None if editor.message != said => {
                            Ok(editor.message.clone().unwrap_or_default())
                        }
                        None => Ok(String::new()),
                    };
                    (answer, flow)
                }
            };
            match answer {
                Ok(text) => request.answer(&text),
                Err(why) => request.answer(&format!("error: {why}\n")),
            }
            if let Flow::Quit = flow {
                return Ok(());
            }
        }

        if !event::poll(FRAME)? {
            continue;
        }
        // Handle everything that's waiting before drawing again.
        loop {
            if let Event::Key(event) = event::read()?
                && let Some(key) = keys::convert(event)
            {
                // A key skips the opening, and does nothing else. Only a press: the release of
                // the Enter that started Lattice comes through too.
                if event.kind == KeyEventKind::Press && splash.take().is_some() {
                    break;
                }
                let now = Instant::now();
                editor.set_transport(playhead.transport(handle.status(), editor, now));
                let effects = match event.kind {
                    KeyEventKind::Release => editor.release(key),
                    // A held note plays once, as on a keyboard; held motions still repeat.
                    KeyEventKind::Repeat if editor.plays(key) => vec![],
                    _ => {
                        if let (Key::Enter, Mode::Command(prompt)) = (key, &editor.mode)
                            && prompt.kind == PromptKind::Command
                        {
                            log!(":{}  ({})", prompt.line, whereabouts(editor));
                        }
                        let effects = editor.handle(key);
                        let typing = matches!(editor.mode, Mode::Command(_));
                        view.keys.record(key, typing, now);
                        effects
                    }
                };
                for effect in effects {
                    match apply(
                        effect, editor, tracks, mixer, exports, &sequencer, machine, handle,
                    ) {
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

/// The first track's position, if it's started, and the count-in still to come.
type Mark = (Option<(usize, u32)>, u32);

/// Where playback is, for recording. The engine says which step each track is on; how long
/// ago that changed says how far into the step it is.
#[derive(Default)]
struct Playhead {
    /// The first track's position and the count-in, and when they were first seen.
    seen: Option<(Mark, Instant)>,
}

impl Playhead {
    fn transport(&mut self, status: &Status, editor: &Editor, now: Instant) -> Option<Transport> {
        let positions: Vec<_> = (0..editor.grid.tracks.len())
            .map(|t| status.position(t))
            .collect();
        let count_in = status.count_in();
        let first = positions.first().copied().flatten();
        if !status.playing() || (first.is_none() && count_in == 0) {
            self.seen = None;
            return None;
        }
        let since = match self.seen {
            Some((seen, since)) if seen == (first, count_in) => since,
            _ => {
                self.seen = Some(((first, count_in), now));
                now
            }
        };
        let step = 60.0 / (editor.bpm * f64::from(editor.grid.steps_per_beat.max(1)));
        Some(Transport {
            positions,
            into_step: (now.duration_since(since).as_secs_f64() / step).min(0.99),
            count_in: count_in as usize,
        })
    }
}

/// Where the player is, for the log: `pattern page, on SPARKLE`.
fn whereabouts(editor: &Editor) -> String {
    let page = format!("{:?}", editor.page).to_lowercase();
    let track = (editor.grid.tracks.get(editor.cursor().track)).map_or("", |t| t.name.as_str());
    format!("{page} page, on {}", track.to_uppercase())
}

/// Types keys or runs a command for [`ctl`], returning why it was refused, if it was.
#[allow(clippy::too_many_arguments)]
fn serve(
    ask: ctl::Ask,
    allow_save: bool,
    editor: &mut Editor,
    tracks: &mut Tracks,
    mixer: &mut Mixer,
    exports: &mut Exports,
    sequencer: &Sequencer,
    machine: &mut Machine,
    handle: &mut Handle,
) -> (Option<String>, Flow) {
    let keys = match ask {
        ctl::Ask::Keys(keys) => keys,
        ctl::Ask::Command(line) => {
            if let Some(why) = ctl::refuse_command(&line, allow_save) {
                return (Some(why), Flow::Next);
            }
            let effects = editor.run_command(&line);
            return run_outside(
                effects, allow_save, editor, tracks, mixer, exports, sequencer, machine, handle,
            );
        }
        ctl::Ask::Screen | ctl::Ask::State => return (None, Flow::Next),
    };
    for key in keys {
        if let Some(why) = ctl::refuse_key(editor, key, allow_save) {
            editor.mode = Mode::Normal;
            return (Some(why), Flow::Next);
        }
        let effects = editor.handle(key);
        match run_outside(
            effects, allow_save, editor, tracks, mixer, exports, sequencer, machine, handle,
        ) {
            (None, Flow::Next) => {}
            done => return done,
        }
    }
    (None, Flow::Next)
}

/// Carries out effects asked for from outside, stopping at one that isn't allowed.
#[allow(clippy::too_many_arguments)]
fn run_outside(
    effects: Vec<Effect>,
    allow_save: bool,
    editor: &mut Editor,
    tracks: &mut Tracks,
    mixer: &mut Mixer,
    exports: &mut Exports,
    sequencer: &Sequencer,
    machine: &mut Machine,
    handle: &mut Handle,
) -> (Option<String>, Flow) {
    for effect in effects {
        if let Some(why) = ctl::refuse_effect(&effect, allow_save) {
            return (Some(why.into()), Flow::Next);
        }
        match apply(
            effect, editor, tracks, mixer, exports, sequencer, machine, handle,
        ) {
            Flow::Next => {}
            Flow::Stop => break,
            Flow::Quit => return (None, Flow::Quit),
        }
    }
    (None, Flow::Next)
}

/// Carries out an editor effect.
#[allow(clippy::too_many_arguments)]
fn apply(
    effect: Effect,
    editor: &mut Editor,
    tracks: &mut Tracks,
    mixer: &mut Mixer,
    exports: &mut Exports,
    sequencer: &Sequencer,
    machine: &mut Machine,
    handle: &mut Handle,
) -> Flow {
    let command = match effect {
        // Effects follow their tracks, so the mixer syncs after the tracks do.
        Effect::TracksChanged | Effect::MixerChanged => {
            if !(tracks.sync(editor, handle) && mixer.sync(editor, handle)) {
                editor.message = Some("audio engine is busy; tracks out of step".into());
            }
            if let Some(problem) = tracks.take_problem() {
                editor.message = Some(problem);
            }
            return Flow::Next;
        }
        // A track gone since is the tracks' sync to catch up on, not this.
        Effect::TrackChanged(index) => match editor.grid.tracks.get(index) {
            Some(track) => Command::SetTrack {
                index,
                track: Box::new(track.clone()),
                swaps: sequencer.swaps(),
            },
            None => return Flow::Next,
        },
        Effect::TogglePlay if handle.status().playing() => Command::Stop,
        // A count-in only when recording, to play along from the first beat.
        Effect::TogglePlay => {
            // The song's mutes from the first step.
            let (now, next) = editor.song_mutes();
            handle.send(Command::SetMutes { now, next });
            Command::Play {
                count_in: if editor.record { editor.count_in } else { 0 },
            }
        }
        Effect::Bpm(bpm) => Command::SetBpm(bpm),
        Effect::Click(on) => Command::SetClick(on),
        Effect::Preview { track, pitch } => Command::Preview {
            track,
            pitch,
            velocity: editor.velocity,
            held: false,
        },
        Effect::Lock { track, lock } => Command::Lock { track, lock },
        Effect::Hold { track, pitch } => Command::Preview {
            track,
            pitch,
            velocity: editor.velocity,
            held: true,
        },
        Effect::Repeat {
            track,
            pitch,
            every,
            now,
        } => Command::Repeat {
            track,
            pitch,
            velocity: editor.velocity,
            every: every as u32,
            now,
        },
        Effect::Release { track, pitch } => Command::Release { track, pitch },
        Effect::Save(path) => {
            let mut song = editor.song();
            let saved = file::collect(&path, &mut song, &lattice_editor::scratch())
                .and_then(|moved| file::write(&path, &song).map(|()| moved));
            return match saved {
                Ok(moved) => {
                    // The tracks play the copies in the song's folder from now on, and the
                    // engine's already playing the same audio.
                    for (from, to) in &moved {
                        samples::copied(from, to);
                        tracks.copied(from, to);
                    }
                    for (track, collected) in editor.grid.tracks.iter_mut().zip(&song.grid.tracks) {
                        track.instrument.file.clone_from(&collected.instrument.file);
                    }
                    editor.saved(path);
                    Flow::Next
                }
                Err(e) => {
                    editor.message = Some(format!("{e:#}"));
                    Flow::Stop
                }
            };
        }
        Effect::Open(path) => {
            return open(
                path, editor, tracks, mixer, exports, sequencer, machine, handle,
            );
        }
        Effect::Export(export) => {
            exports.start(*export, editor);
            return Flow::Next;
        }
        Effect::Latency(ms) => {
            machine.set_latency(ms, editor, handle);
            return Flow::Next;
        }
        Effect::Device(direction, id) => {
            machine.choose(direction, id, editor, handle);
            return Flow::Next;
        }
        Effect::SaveChain(name, chain) => {
            editor.message = Some(match chains::save(&name, *chain) {
                Ok(saved) => saved,
                Err(e) => format!("{e:#}"),
            });
            editor.chains = chains::names();
            return Flow::Next;
        }
        Effect::LoadChain(name) => {
            match chains::load(&name) {
                Ok(chain) => {
                    for effect in editor.put_chain(&name, chain) {
                        apply(
                            effect, editor, tracks, mixer, exports, sequencer, machine, handle,
                        );
                    }
                }
                Err(e) => editor.message = Some(format!("{e:#}")),
            }
            return Flow::Next;
        }
        Effect::SavePreset(name, preset) => {
            editor.message = Some(match presets::save(&name, *preset) {
                Ok(saved) => saved,
                Err(e) => format!("{e:#}"),
            });
            editor.presets = presets::names();
            return Flow::Next;
        }
        Effect::LoadPreset(name) => {
            match (presets::load(&name), editor.preset_track()) {
                (Ok(preset), Ok(t)) => {
                    for effect in editor.put_preset(t, &name, preset) {
                        apply(
                            effect, editor, tracks, mixer, exports, sequencer, machine, handle,
                        );
                    }
                }
                (Err(e), _) => editor.message = Some(format!("{e:#}")),
                (_, Err(e)) => editor.message = Some(e),
            }
            return Flow::Next;
        }
        Effect::Quit => return Flow::Quit,
    };
    if !handle.send(command) {
        editor.message = Some("audio engine is busy; change dropped".into());
    }
    Flow::Next
}

/// Replaces the song with the one at `path`, or a new one if there's no such file.
#[allow(clippy::too_many_arguments)]
fn open(
    path: PathBuf,
    editor: &mut Editor,
    tracks: &mut Tracks,
    mixer: &mut Mixer,
    exports: &mut Exports,
    sequencer: &Sequencer,
    machine: &mut Machine,
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
        apply(
            effect, editor, tracks, mixer, exports, sequencer, machine, handle,
        );
    }
    if message.is_some() {
        editor.message = message;
    }
    Flow::Next
}
