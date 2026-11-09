//! `lattice --help` and `--version`, and the options Lattice takes. The help is written for
//! someone stuck, or an agent helping them: what to type, where the files are, and what to try
//! when something's wrong, with the paths as they are on this machine.

use std::path::PathBuf;

use anyhow::{Result, bail};

/// The version, and the commit it was built from when that's known.
pub fn version() -> String {
    match option_env!("LATTICE_COMMIT") {
        Some(commit) if !commit.is_empty() => {
            format!("lattice {} ({commit})", env!("CARGO_PKG_VERSION"))
        }
        _ => format!("lattice {}", env!("CARGO_PKG_VERSION")),
    }
}

/// The options that take no value.
const FLAGS: &[&str] = &[
    "--tutor",
    "--no-click",
    "--show-keys",
    "--no-splash",
    "--allow-agent-save",
];

/// What Lattice was asked to do.
#[derive(Debug, Default, PartialEq)]
pub struct Options {
    pub song: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub tutor: bool,
    pub no_click: bool,
    pub show_keys: bool,
    pub no_splash: bool,
    pub allow_agent_save: bool,
}

/// What to do: start, or print something and stop.
#[derive(Debug, PartialEq)]
pub enum Run {
    Start(Options),
    Print(String),
}

/// Reads the options, refusing ones it doesn't know rather than leaving a mistyped one to do
/// nothing quietly.
pub fn parse(args: &[String]) -> Result<Run> {
    let mut options = Options::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Run::Print(help())),
            "-V" | "--version" => return Ok(Run::Print(version())),
            "--config" => match args.next() {
                Some(path) => options.config = Some(path.into()),
                None => bail!("--config needs a path: lattice --config PATH"),
            },
            "--tutor" => options.tutor = true,
            "--no-click" => options.no_click = true,
            "--show-keys" => options.show_keys = true,
            "--no-splash" => options.no_splash = true,
            "--allow-agent-save" => options.allow_agent_save = true,
            option if option.starts_with('-') && option.len() > 1 => {
                let near = (FLAGS.iter().chain(&["--config", "--help", "--version"]))
                    .find(|f| close(f, option));
                match near {
                    Some(near) => bail!(
                        "no option {option}; did you mean {near}? (lattice --help lists them)"
                    ),
                    None => bail!("no option {option}: lattice --help lists them"),
                }
            }
            song => match &options.song {
                None => options.song = Some(song.into()),
                Some(first) => bail!(
                    "one song at a time: {} and {song} were both given",
                    first.display()
                ),
            },
        }
    }
    Ok(Run::Start(options))
}

/// Whether `typed` is probably `option` mistyped: the same but for a letter or two.
fn close(option: &str, typed: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (option.chars().collect(), typed.chars().collect());
    // Edit distance, a row at a time.
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut next = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let (Some(&del), Some(&sub), Some(&ins)) = (row.get(j + 1), row.get(j), next.last())
            else {
                return false;
            };
            next.push((del + 1).min(ins + 1).min(sub + usize::from(ca != cb)));
        }
        row = next;
    }
    row.last().is_some_and(|&d| d <= 2)
}

fn shown(path: Option<PathBuf>) -> String {
    path.map_or_else(|| "(no home folder)".into(), |p| p.display().to_string())
}

/// The whole of `lattice --help`.
pub fn help() -> String {
    let config = crate::config::default_path();
    let audio = config.as_ref().map(|c| c.with_file_name("audio.toml"));
    let log = crate::log::path();
    let autosave = log.as_ref().map(|l| l.with_file_name("autosave"));
    let version = version();
    let [config, audio, log, autosave] = [config, audio, log, autosave].map(shown);
    format!(
        "\
{version}: a music tracker for the terminal, played from the keyboard.

USAGE
  lattice [OPTIONS] [SONG]   start, opening SONG (a .lattice file), or a new song at that path;
                             with no SONG, a demo song to play with
  lattice ctl [--pid PID] VERB [ARGS]
                             drive a Lattice that's already running (below)
  lattice --help | --version

OPTIONS
  --tutor              start the tutor, which teaches Lattice a lesson at a time: start here
  --config PATH        read this config file in place of the usual one
  --no-click           start with the metronome off
  --show-keys          show keys as they're pressed, for screencasts
  --no-splash          leave out the opening animation and the thanks after it
  --allow-agent-save   let `lattice ctl` save, open and quit, which it otherwise can't
  -h, --help           print this
  -V, --version        print the version

FIRST STEPS, ONCE IT'S RUNNING
  Its keys work like the Helix editor's, in modes. The status line says which you're in.
  space       play and stop
  i           insert mode: the letter keys play and enter notes (z to m is an octave)
  esc         back to normal mode, where keys move, select and edit
  :tutor      the tutor, from inside Lattice
  :help       every key and command;  :manual  everything explained in full
  :w PATH     save the song to PATH (.lattice is added); :w after that saves it there again
  :q          quit, unless there are unsaved changes; :q! quits anyway; so does C-c twice
  :recover    bring back unsaved changes kept when Lattice last stopped without saving:
              they're kept every 30 seconds, and when the terminal closes

DRIVING IT FROM SCRIPTS OR AN AGENT (macOS and Linux)
  lattice ctl screen         the screen as last drawn, as text
  lattice ctl state          where things are (page, mode, cursor, song), as JSON
  lattice ctl keys 'KEYS'    type keys, as the tutor writes them: 'gp:bpm 140<ret>'
  lattice ctl cmd 'COMMAND'  run a command, as after `:`: 'bpm 140'
  lattice ctl log [LINES]    the end of the log
  With more than one running, --pid PID says which. Keys and commands can be undone and
  can't save, open, quit or throw the song away unless it was started with
  --allow-agent-save.

FILES
  {config}
      keys and colours: a [keys] table binds keys, a [theme] table sets colours
      and splash = false leaves out the opening
  {audio}
      the audio devices chosen with :output and :input
  {log}
      what happened this run, errors included; the last run's is lattice.log.1
  {autosave}
      unsaved changes kept for :recover, a folder for each song
  Songs are text (RON). Their samples go in a `samples` folder beside them.

IF SOMETHING'S WRONG
  Look in the log first: `lattice ctl log` while it runs, or the file above after.
  Lattice won't start, saying no audio output would open: it needs one to start. Check
    sound plays from other programs, and on Linux that PulseAudio or PipeWire is running.
  No sound: :output lists the audio devices and :output N picks one, kept for next
    time. On Linux Lattice plays through PulseAudio (or PipeWire's), else ALSA.
  The sound crackles: the computer can't keep up. Stop other busy programs, or try
    a release build (cargo build --release) if this one isn't.
  Keys do nothing, or the wrong thing: run it in a terminal that speaks the kitty
    keyboard protocol, as kitty, WezTerm (with enable_kitty_keyboard = true), Ghostty,
    foot and Alacritty do. Without it Lattice still works, but notes recorded from the
    keyboard can't last as long as they're held, and the numpad isn't told apart.
    On Windows any terminal will do; there's no numpad there yet.
  Alt keys (A-…) do nothing on a Mac: set Option to act as Alt in the terminal, as
    macos_option_as_alt in kitty or macos-option-as-alt in Ghostty.
  Note keys are in odd places: insert mode lays notes out by the characters a QWERTY
    keyboard types, so on AZERTY or QWERTZ some move. :layout tries other layouts.
  Lattice crashed: an unsaved song is saved beside its file as NAME.rescue.lattice,
    or in the log's folder if it had no file, and the log says what went wrong.
  A song won't open: the error gives the line and column. Lattice is in beta and
    the file format still changes, so a song from an older version may need mending
    by hand.
  The full manual is :manual inside Lattice, or docs/manual in the source,
  https://github.com/mochreach/lattice, where issues are welcome.
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Run> {
        parse(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn reads_options_in_any_order() {
        let Run::Start(options) =
            run(&["--no-splash", "song.lattice", "--config", "c.toml"]).unwrap()
        else {
            panic!("should start");
        };
        assert_eq!(options.song, Some("song.lattice".into()));
        assert_eq!(options.config, Some("c.toml".into()));
        assert!(options.no_splash && !options.tutor);
    }

    #[test]
    fn help_and_version_print_rather_than_start() {
        assert!(matches!(run(&["--help"]).unwrap(), Run::Print(h) if h.contains("USAGE")));
        assert!(matches!(run(&["-V"]).unwrap(), Run::Print(v) if v.starts_with("lattice ")));
    }

    #[test]
    fn refuses_an_option_it_doesnt_know_and_suggests_the_nearest() {
        let e = run(&["--no-spalsh"]).unwrap_err().to_string();
        assert!(e.contains("did you mean --no-splash?"), "{e}");
        let e = run(&["--frobnicate"]).unwrap_err().to_string();
        assert!(e.contains("lattice --help"), "{e}");
        assert!(run(&["--config"]).is_err());
        assert!(run(&["a.lattice", "b.lattice"]).is_err());
    }
}
