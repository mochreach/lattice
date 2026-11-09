//! The config file, `~/.config/lattice/config.toml`: keys and colours, as Helix sets its own,
//! and whether to open with the splash.
//!
//! ```toml
//! splash = false               # no opening, nor the thanks after it
//!
//! [keys.normal]
//! C-s = ":w"
//! x = "delete"
//! X = "none"
//!
//! [keys.normal.space]          # a menu, opened by space
//! m = "goto_mixer"
//!
//! [keys.insert]
//! C-c = "escape"
//!
//! [theme]
//! accent = "magenta"
//! beat-row = "none"
//! tracks = ["#ff8800", "cyan", "12"]
//! ```
//!
//! Anything wrong is reported when Lattice starts, and the rest still applies.

use std::path::{Path, PathBuf};

use lattice_editor::{Key, Keymap};
use ratatui::style::Color;
use toml::{Table, Value};

use crate::theme::Theme;

/// Where the config is, unless `--config` says otherwise: `$XDG_CONFIG_HOME/lattice`, or
/// `~/.config/lattice`.
pub fn default_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => lattice_editor::home()?.join(".config"),
    };
    Some(base.join("lattice").join("config.toml"))
}

/// What the config sets beyond keys and colours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// Whether to open with the splash, and the thanks after it.
    pub splash: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { splash: true }
    }
}

/// Reads the config at `path` into the keymap, theme and settings, and says what was wrong with
/// it, if anything. A missing file is only wrong if it was asked for by name.
pub fn load(
    path: &Path,
    named: bool,
    keymap: &mut Keymap,
    theme: &mut Theme,
    settings: &mut Settings,
) -> Option<String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !named => return None,
        Err(e) => return Some(format!("{}: {e}", path.display())),
    };
    let problems = apply(&text, keymap, theme, settings);
    let first = problems.first()?;
    Some(match problems.len() {
        1 => format!("{}: {first}", path.display()),
        n => format!("{}: {first} (and {} more)", path.display(), n - 1),
    })
}

/// Applies a config's text, returning what was wrong with it.
fn apply(
    text: &str,
    keymap: &mut Keymap,
    theme: &mut Theme,
    settings: &mut Settings,
) -> Vec<String> {
    let table: Table = match text.parse() {
        Ok(table) => table,
        Err(e) => return vec![e.message().to_string()],
    };
    let mut problems = vec![];
    for (name, value) in &table {
        match (name.as_str(), value) {
            ("keys", Value::Table(keys)) => apply_keys(keys, keymap, &mut problems),
            ("theme", Value::Table(colours)) => apply_theme(colours, theme, &mut problems),
            ("splash", Value::Boolean(on)) => settings.splash = *on,
            ("splash", _) => problems.push("splash should be true or false".into()),
            _ => problems.push(format!("unknown section [{name}]")),
        }
    }
    problems
}

fn apply_keys(keys: &Table, keymap: &mut Keymap, problems: &mut Vec<String>) {
    for (mode, value) in keys {
        let Value::Table(bindings) = value else {
            problems.push(format!("keys.{mode} should be a table"));
            continue;
        };
        for (name, target) in bindings {
            let Some(key) = Key::parse(name) else {
                problems.push(format!("keys.{mode}: no key called {name:?}"));
                continue;
            };
            let result = match (mode.as_str(), target) {
                ("normal", Value::String(target)) => keymap.bind_normal(None, key, target),
                ("normal", Value::Table(menu)) => {
                    for (item, target) in menu {
                        let result = match (Key::parse(item), target) {
                            (Some(item), Value::String(target)) => {
                                keymap.bind_normal(Some(key), item, target)
                            }
                            (None, _) => Err(format!("no key called {item:?}")),
                            (_, _) => Err("menus can't hold menus".into()),
                        };
                        if let Err(problem) = result {
                            problems.push(format!("keys.normal.{name}.{item}: {problem}"));
                        }
                    }
                    Ok(())
                }
                ("insert", Value::String(target)) => keymap.bind_insert(key, target),
                ("normal" | "insert", _) => Err("should be a name, or a table of keys".into()),
                _ => Err(format!(
                    "no mode called {mode:?}; there's normal and insert"
                )),
            };
            if let Err(problem) = result {
                problems.push(format!("keys.{mode}.{name}: {problem}"));
            }
        }
    }
}

fn apply_theme(colours: &Table, theme: &mut Theme, problems: &mut Vec<String>) {
    for (name, value) in colours {
        let result = match (name.as_str(), value) {
            ("tracks", Value::Array(values)) => {
                match values.iter().map(colour).collect::<Result<Vec<_>, _>>() {
                    Ok(tracks) if tracks.is_empty() => Err("needs at least one colour".into()),
                    Ok(tracks) => {
                        theme.tracks = tracks;
                        Ok(())
                    }
                    Err(problem) => Err(problem),
                }
            }
            ("beat-row", Value::String(none)) if none == "none" => {
                theme.beat_row = None;
                Ok(())
            }
            (name, value) => colour(value).and_then(|c| {
                match name {
                    "text" => theme.text = theme.text.fg(c),
                    "dim" => theme.dim = theme.dim.fg(c),
                    "faint" => theme.faint = theme.faint.fg(c),
                    "accent" => theme.accent = c,
                    "playing" => theme.playing = c,
                    "visual" => theme.visual = c,
                    "error" => theme.error = c,
                    "selection" => theme.selection = c,
                    "target" => theme.target = c,
                    "beat-row" => theme.beat_row = Some(c),
                    _ => return Err("not a colour Lattice has".into()),
                }
                Ok(())
            }),
        };
        if let Err(problem) = result {
            problems.push(format!("theme.{name}: {problem}"));
        }
    }
}

/// A colour as ratatui reads one: a name (`light-blue`), a palette index (`"12"`, or `12`), or
/// `#rrggbb`. `default` is the terminal's own.
fn colour(value: &Value) -> Result<Color, String> {
    match value {
        Value::String(s) if s == "default" => Ok(Color::Reset),
        Value::String(s) => s.parse().map_err(|_| format!("no colour called {s:?}")),
        Value::Integer(i) => (u8::try_from(*i).map(Color::Indexed))
            .map_err(|_| format!("palette colours go from 0 to 255, not {i}")),
        _ => Err("should be a colour".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_editor::{Action, Binding};

    fn applied(text: &str) -> (Keymap, Theme, Vec<String>) {
        let (mut keymap, mut theme) = (Keymap::default(), Theme::terminal());
        let problems = apply(text, &mut keymap, &mut theme, &mut Settings::default());
        (keymap, theme, problems)
    }

    #[test]
    fn can_leave_out_the_splash() {
        let mut settings = Settings::default();
        let (mut keymap, mut theme) = (Keymap::default(), Theme::terminal());
        let problems = apply("splash = false", &mut keymap, &mut theme, &mut settings);
        assert!(problems.is_empty() && !settings.splash, "{problems:?}");
        let problems = apply("splash = \"no\"", &mut keymap, &mut theme, &mut settings);
        assert_eq!(problems, ["splash should be true or false"]);
    }

    #[test]
    fn binds_actions_commands_and_menus() {
        let (keymap, _, problems) = applied(
            r#"
            [keys.normal]
            C-s = ":w"
            X = "none"
            Q = "goto_mixer"

            [keys.normal.space]
            m = "goto_mixer"

            [keys.normal.g]
            m = "goto_song"

            [keys.insert]
            C-c = "escape"
            "#,
        );
        assert_eq!(problems, Vec::<String>::new());
        let normal = &keymap.normal;
        assert_eq!(normal[&Key::Ctrl('s')], Binding::Command("w".into()));
        assert!(!normal.contains_key(&Key::Char('X')));
        assert_eq!(normal[&Key::Char('Q')], Binding::Action(Action::GotoMixer));
        let menu = |key| match &normal[&key] {
            Binding::Menu(menu) => menu.clone(),
            other => panic!("{other:?}"),
        };
        let space = menu(Key::Char(' '));
        assert_eq!(
            space.items,
            [(Key::Char('m'), Binding::Action(Action::GotoMixer))]
        );
        // An existing menu keeps its other keys.
        let goto = menu(Key::Char('g'));
        assert_eq!(goto.title, "goto");
        assert!(
            goto.items
                .contains(&(Key::Char('m'), Binding::Action(Action::GotoSong)))
        );
        assert!(goto.items.iter().any(|(k, _)| *k == Key::Char('e')));
        assert_eq!(keymap.insert[&Key::Ctrl('c')], Action::Escape);
    }

    #[test]
    fn sets_colours() {
        let (_, theme, problems) = applied(
            r##"
            [theme]
            accent = "magenta"
            text = "#102030"
            beat-row = "none"
            selection = 236
            target = 240
            tracks = ["red", "12"]
            "##,
        );
        assert_eq!(problems, Vec::<String>::new());
        assert_eq!(theme.accent, Color::Magenta);
        assert_eq!(theme.text.fg, Some(Color::Rgb(0x10, 0x20, 0x30)));
        assert_eq!(theme.beat_row, None);
        assert_eq!(theme.selection, Color::Indexed(236));
        assert_eq!(theme.target, Color::Indexed(240));
        assert_eq!(theme.tracks, [Color::Red, Color::Indexed(12)]);
    }

    #[test]
    fn reports_mistakes_and_applies_the_rest() {
        let (keymap, theme, problems) = applied(
            r#"
            [keys.normal]
            Q = "no_such_thing"
            W = ":no-such-command"
            hyper-x = "quit"
            Z = "quit"

            [keys.insert]
            C-w = ":w"

            [theme]
            accent = "mauve"
            "#,
        );
        assert_eq!(problems.len(), 5, "{problems:?}");
        assert!(
            problems.iter().any(|p| p.contains("no_such_thing")),
            "{problems:?}"
        );
        assert_eq!(
            keymap.normal[&Key::Char('Z')],
            Binding::Action(Action::Quit)
        );
        assert_eq!(theme.accent, Color::Yellow);
        assert!(!applied("[keys").2.is_empty());
    }
}
