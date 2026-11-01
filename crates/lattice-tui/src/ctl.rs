//! Driving Lattice from outside, for scripts and agents: each Lattice listens on a socket of its
//! own, `$XDG_RUNTIME_DIR/lattice/<pid>.sock`, which only its user can open, and `lattice ctl`
//! talks to it.
//!
//! - `lattice ctl screen` prints the screen as it was last drawn.
//! - `lattice ctl state` prints where things are, as JSON.
//! - `lattice ctl keys 'gp:bpm 140<ret>'` types keys, as the tutor writes them.
//! - `lattice ctl cmd 'bpm 140'` runs a command, as `:` would.
//! - `lattice ctl log [LINES]` prints the end of the log.
//!
//! Keys and commands go the same way as typed ones, so they can be undone and show on the
//! status line. They can't save, open or quit, or throw away the song, unless Lattice was started
//! with `--allow-agent-save`.
//!
//! A request is one line, and the answer is the rest of what's sent back; a refusal starts
//! `error: `.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use lattice_editor::{COMMANDS, Editor, Effect, Key, Mode, Page, PromptKind};
use lattice_engine::Status;
use ratatui::buffer::Buffer;

use crate::log::log;

/// How long a request has to arrive once connected, and an answer to come back.
const PATIENCE: Duration = Duration::from_secs(2);

/// Commands that would throw away the song, refused unless saving is allowed.
const DISCARDING: &[&str] = &["new!", "open!", "quit!", "tutor!"];

/// Where the sockets are: `$XDG_RUNTIME_DIR/lattice`, or beside the log without one.
fn dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| !d.is_empty())
        .map(|d| PathBuf::from(d).join("lattice"))
        .or_else(|| Some(crate::log::path()?.with_file_name("sockets")))
}

/// The listening side, in Lattice itself.
pub struct Server {
    listener: UnixListener,
    path: PathBuf,
    /// Whether requests may save, open, quit or throw the song away.
    pub allow_save: bool,
}

impl Server {
    /// Listens, if it can; Lattice carries on without it if not.
    pub fn start(allow_save: bool) -> Option<Self> {
        let dir = dir()?;
        let made = std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir);
        let path = dir.join(format!("{}.sock", std::process::id()));
        // One left by a process that had this pid before.
        let _ = std::fs::remove_file(&path);
        let listener = match made.and_then(|()| UnixListener::bind(&path)) {
            Ok(listener) => listener,
            Err(e) => {
                log!("no control socket at {}: {e}", path.display());
                return None;
            }
        };
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        listener.set_nonblocking(true).ok()?;
        log!("control socket at {}", path.display());
        Some(Self {
            listener,
            path,
            allow_save,
        })
    }

    /// The requests waiting.
    pub fn requests(&self) -> Vec<Request> {
        let mut requests = vec![];
        while let Ok((stream, _)) = self.listener.accept() {
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(PATIENCE));
            let _ = stream.set_write_timeout(Some(PATIENCE));
            let mut line = String::new();
            let Ok(mut reader) = stream.try_clone().map(BufReader::new) else {
                continue;
            };
            // A client looking for the Lattice running hangs up without asking anything.
            if reader.read_line(&mut line).is_ok_and(|n| n > 0) {
                let line = line.trim_end_matches(['\r', '\n']).to_string();
                log!("ctl: {line}");
                requests.push(Request { stream, line });
            }
        }
        requests
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// One request, to answer.
pub struct Request {
    stream: UnixStream,
    pub line: String,
}

impl Request {
    pub fn answer(mut self, text: &str) {
        let _ = self.stream.write_all(text.as_bytes());
    }
}

/// What a request asks for.
pub enum Ask {
    Screen,
    State,
    Keys(Vec<Key>),
    Command(String),
}

impl Ask {
    pub fn parse(line: &str) -> Result<Ask, String> {
        let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
        match verb {
            "screen" => Ok(Ask::Screen),
            "state" => Ok(Ask::State),
            "keys" => Key::sequence(rest).map(Ask::Keys),
            "cmd" => Ok(Ask::Command(rest.trim_start_matches(':').to_string())),
            _ => Err(format!(
                "no request called {verb}: there's screen, state, keys and cmd"
            )),
        }
    }
}

/// Why a command line can't be run from outside, if it can't.
pub fn refuse_command(line: &str, allow_save: bool) -> Option<String> {
    let word = line.split_whitespace().next()?;
    let spec = COMMANDS
        .iter()
        .find(|c| c.name == word || c.alias == Some(word))?;
    (!allow_save && DISCARDING.contains(&spec.name)).then(|| {
        format!(
            ":{word} would throw away the song; start Lattice with --allow-agent-save to allow it"
        )
    })
}

/// Why `key` can't be typed from outside, if it can't: it would run a command that can't be.
pub fn refuse_key(editor: &Editor, key: Key, allow_save: bool) -> Option<String> {
    match (&editor.mode, key) {
        (Mode::Command(prompt), Key::Enter) if prompt.kind == PromptKind::Command => {
            refuse_command(&prompt.line, allow_save)
        }
        _ => None,
    }
}

/// Why an effect can't come from outside, if it can't.
pub fn refuse_effect(effect: &Effect, allow_save: bool) -> Option<&'static str> {
    let writes = matches!(
        effect,
        Effect::Save(_) | Effect::Open(_) | Effect::Quit | Effect::SaveChain(..)
    );
    (writes && !allow_save)
        .then_some("saving, opening and quitting need Lattice started with --allow-agent-save")
}

/// The screen as text, a line a row, without trailing spaces.
pub fn screen(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut text = String::new();
    for y in area.top()..area.bottom() {
        let mut line = String::new();
        for x in area.left()..area.right() {
            line.push_str(buffer[(x, y)].symbol());
        }
        text.push_str(line.trim_end());
        text.push('\n');
    }
    text
}

/// Where things are, as JSON.
pub fn state(editor: &Editor, status: &Status) -> String {
    let cursor = editor.cursor();
    let (mode, prompt) = match &editor.mode {
        Mode::Normal => ("normal", None),
        Mode::Select => ("select", None),
        Mode::Insert => ("insert", None),
        Mode::Command(prompt) => ("prompt", Some(prompt.line.as_str())),
    };
    let page = match editor.page {
        Page::Pattern => "pattern",
        Page::Song => "song",
        Page::Mixer => "mixer",
        Page::Fx => "fx",
        Page::Instrument => "instrument",
        Page::Help => "help",
    };
    let tracks: Vec<String> = (editor.grid.tracks.iter()).map(|t| json(&t.name)).collect();
    let positions: Vec<String> = (0..editor.grid.tracks.len())
        .map(|t| {
            status
                .position(t)
                .map_or("null".into(), |(step, _)| step.to_string())
        })
        .collect();
    let path = editor.path.as_ref().map(|p| p.display().to_string());
    let fields = [
        ("pid", std::process::id().to_string()),
        ("song", json(&editor.grid.name)),
        ("path", optional(path.as_deref())),
        ("modified", editor.modified().to_string()),
        ("page", json(page)),
        ("mode", json(mode)),
        ("prompt", optional(prompt)),
        ("track", cursor.track.to_string()),
        ("step", cursor.step.to_string()),
        ("tracks", format!("[{}]", tracks.join(", "))),
        ("playing", status.playing().to_string()),
        ("positions", format!("[{}]", positions.join(", "))),
        ("bpm", editor.bpm.to_string()),
        ("record", editor.record.to_string()),
        ("message", optional(editor.message.as_deref())),
    ];
    let fields: Vec<String> = (fields.iter())
        .map(|(name, value)| format!("  \"{name}\": {value}"))
        .collect();
    format!("{{\n{}\n}}\n", fields.join(",\n"))
}

fn optional(text: Option<&str>) -> String {
    text.map_or("null".into(), json)
}

/// `text` as a JSON string.
fn json(text: &str) -> String {
    let mut out = String::from('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `lattice ctl …`: asks a running Lattice, prints the answer, and fails if it was refused.
pub fn client(args: &[String]) -> Result<()> {
    let mut args = args.to_vec();
    let pid = match args.iter().position(|a| a == "--pid") {
        Some(i) => {
            let pid = args
                .get(i + 1)
                .context("--pid needs the process's id")?
                .clone();
            args.drain(i..i + 2);
            Some(pid)
        }
        None => None,
    };
    let Some(verb) = args.first() else {
        bail!(
            "usage: lattice ctl [--pid PID] screen | state | keys KEYS | cmd COMMAND | log [LINES]"
        );
    };
    if verb == "log" {
        let lines: usize = args.get(1).map_or(Ok(40), |n| n.parse())?;
        let path = crate::log::path().context("no home folder, so no log")?;
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let all: Vec<&str> = text.lines().collect();
        for line in &all[all.len().saturating_sub(lines)..] {
            println!("{line}");
        }
        return Ok(());
    }
    let mut stream = connect(pid.as_deref())?;
    stream.set_read_timeout(Some(PATIENCE * 2))?;
    writeln!(stream, "{}", args.join(" "))?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer)?;
    if let Some(error) = answer.strip_prefix("error: ") {
        bail!("{}", error.trim_end());
    }
    print!("{answer}");
    if !answer.is_empty() && !answer.ends_with('\n') {
        println!();
    }
    Ok(())
}

/// The Lattice with `pid`, or the only one running.
fn connect(pid: Option<&str>) -> Result<UnixStream> {
    let dir = dir().context("nowhere to look for Lattice's socket")?;
    if let Some(pid) = pid {
        let path = dir.join(format!("{pid}.sock"));
        return UnixStream::connect(&path).with_context(|| format!("no Lattice with pid {pid}"));
    }
    let mut running = vec![];
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "sock") {
            continue;
        }
        match UnixStream::connect(&path) {
            Ok(stream) => running.push((path, stream)),
            // Left by a Lattice that didn't get to tidy up.
            Err(_) => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    match running.len() {
        0 => bail!("no Lattice is running"),
        1 => Ok(running.remove(0).1),
        _ => {
            let pids: Vec<String> = (running.iter())
                .filter_map(|(p, _)| Some(p.file_stem()?.to_string_lossy().into_owned()))
                .collect();
            bail!(
                "{} are running ({}): say which with --pid",
                running.len(),
                pids.join(", ")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_parse() {
        assert!(matches!(Ask::parse("screen"), Ok(Ask::Screen)));
        assert!(matches!(Ask::parse("keys gp<ret>"), Ok(Ask::Keys(k)) if k.len() == 3));
        assert!(matches!(Ask::parse("cmd :bpm 140"), Ok(Ask::Command(c)) if c == "bpm 140"));
        assert!(Ask::parse("dance").is_err());
    }

    #[test]
    fn the_song_is_kept_unless_saving_is_allowed() {
        assert!(refuse_command("q!", false).is_some());
        assert!(refuse_command("new! song.lattice", false).is_some());
        assert!(refuse_command("new! song.lattice", true).is_none());
        assert!(refuse_command("bpm 140", false).is_none());
        assert!(refuse_effect(&Effect::Quit, false).is_some());
        assert!(refuse_effect(&Effect::Quit, true).is_none());
        assert!(refuse_effect(&Effect::TogglePlay, false).is_none());
    }

    #[test]
    fn json_strings_are_escaped() {
        assert_eq!(json("a \"b\"\\\n\u{1}"), r#""a \"b\"\\\n\u0001""#);
    }
}
