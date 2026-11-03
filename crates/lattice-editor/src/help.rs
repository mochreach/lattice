//! The help page (`:help`): every key and command, drawn from the keymap and the command table
//! so it never goes out of date.

use crate::keymap::{Action, Binding, Key};
use crate::{COMMANDS, Editor, Effect, Mode, Page};

/// A line of the help page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpLine {
    Heading(String),
    /// Keys (or a command), what they take, and what they do.
    Item {
        keys: String,
        args: String,
        what: String,
    },
    Blank,
}

/// Where the help page is scrolled to, and the page to go back to.
#[derive(Debug, Clone, Copy, Default)]
pub struct Help {
    pub scroll: usize,
    back: Page,
}

/// How the other pages use the keys differently, which the keymap doesn't say.
const PAGES: &[(&str, &str)] = &[
    (
        "mixer (gm)",
        "h l strips · j k into the detail · ( ) strip · tab chain · m mute · s solo",
    ),
    (
        "",
        "C-a C-x adjust · c set · L … L lock changes on the selected steps · :fx-add · d remove",
    ),
    (
        "fx (gf)",
        "j k lines · h l lanes, left of A the macros · w b effects · ( ) strip · m mute · s solo",
    ),
    (
        "",
        "C-a C-x turn · c set, or :fx-add · o add an effect after · d remove · L lock",
    ),
    (
        "instrument (gi)",
        "h l tracks · j k lines · w b parts · C-a C-x turn · c set · ret face, or the fx page",
    ),
    (
        "",
        "i a play it on the keyboard · d remove a macro · L lock · m mute · s solo",
    ),
    (
        "song (gs)",
        "j k bars · w b entries · h l tracks · C-a C-x pattern · + - repeats · o O add",
    ),
    (
        "",
        "v select · x the entry, again the next · m mute · s solo, in the entries selected",
    ),
    (
        "",
        "d y p P delete, copy, paste · | loop · c rename · ret edit at this bar · space play",
    ),
];

fn heading(title: &str) -> HelpLine {
    HelpLine::Heading(title.into())
}

fn item(keys: impl Into<String>, what: impl Into<String>) -> HelpLine {
    HelpLine::Item {
        keys: keys.into(),
        args: String::new(),
        what: what.into(),
    }
}

/// The keys bound to each action in `bindings`, in the order the actions are declared, so
/// related ones sit together.
fn by_action(bindings: impl Iterator<Item = (Key, Action)>) -> Vec<HelpLine> {
    let mut actions: Vec<(Action, Vec<Key>)> = vec![];
    for (key, action) in bindings {
        match actions.iter_mut().find(|(a, _)| *a == action) {
            Some((_, keys)) => keys.push(key),
            None => actions.push((action, vec![key])),
        }
    }
    actions.sort_by_key(|(action, _)| *action as usize);
    actions
        .into_iter()
        .map(|(action, mut keys)| {
            // Letters before the arrows and the like.
            keys.sort_by_key(|k| (!matches!(k, Key::Char(_)), k.to_string()));
            let keys: Vec<String> = keys.iter().map(Key::to_string).collect();
            // Its name, for binding it in the config.
            HelpLine::Item {
                keys: keys.join(" "),
                args: action.name(),
                what: action.describe().into(),
            }
        })
        .collect()
}

impl Editor {
    /// Opens the help page, coming back to this one.
    pub(crate) fn open_help(&mut self) {
        if self.page != Page::Help {
            self.help = Help {
                scroll: 0,
                back: self.page,
            };
        }
        self.page = Page::Help;
        self.mode = Mode::Normal;
    }

    /// Every key, then every command.
    pub fn help_lines(&self) -> Vec<HelpLine> {
        let mut lines = vec![heading("normal and select mode")];
        let normal = self
            .keymap
            .normal
            .iter()
            .filter_map(|(key, binding)| match binding {
                Binding::Action(action) => Some((*key, *action)),
                _ => None,
            });
        lines.extend(by_action(normal));
        // Keys bound to commands in the config, in the order of their keys.
        let mut commands: Vec<_> = (self.keymap.normal.iter())
            .filter(|(_, binding)| matches!(binding, Binding::Command(_)))
            .map(|(key, binding)| item(key.to_string(), binding.describe()))
            .collect();
        commands.sort_by_key(|line| format!("{line:?}"));
        lines.extend(commands);

        // The menus, in the order of their keys.
        let mut menus: Vec<_> = (self.keymap.normal.iter())
            .filter_map(|(key, binding)| match binding {
                Binding::Menu(menu) => Some((key.to_string(), menu)),
                _ => None,
            })
            .collect();
        menus.sort_by(|a, b| a.0.cmp(&b.0));
        for (prefix, menu) in menus {
            lines.push(HelpLine::Blank);
            lines.push(HelpLine::Heading(menu.title.clone()));
            for (key, binding) in &menu.items {
                let keys = format!("{prefix}{key}");
                lines.push(match binding {
                    Binding::Action(action) => HelpLine::Item {
                        keys,
                        args: action.name(),
                        what: action.describe().into(),
                    },
                    _ => item(keys, binding.describe()),
                });
            }
        }

        lines.push(HelpLine::Blank);
        lines.push(heading("insert mode"));
        lines.push(item(
            "notes",
            "as laid out by :layout, in the keyboard octave",
        ));
        lines.extend(by_action(self.keymap.insert.iter().map(|(k, a)| (*k, *a))));

        lines.push(HelpLine::Blank);
        lines.push(heading("pages"));
        for (page, keys) in PAGES {
            lines.push(item(*page, *keys));
        }

        lines.push(HelpLine::Blank);
        lines.push(heading("commands"));
        for spec in COMMANDS {
            let keys = match spec.alias {
                Some(alias) => format!(":{} :{alias}", spec.name),
                None => format!(":{}", spec.name),
            };
            lines.push(HelpLine::Item {
                keys,
                args: spec.signature(),
                what: spec.description(),
            });
        }
        lines
    }

    /// The help page's keys: scrolling, and going back. Playing, the command line and the
    /// pages work as anywhere; anything else would edit the pattern out of sight, so does
    /// nothing.
    pub(crate) fn help_act(&mut self, action: Action, n: usize) -> Option<Vec<Effect>> {
        let last = self.help_lines().len().saturating_sub(1);
        let scroll = &mut self.help.scroll;
        match action {
            Action::Up => *scroll = scroll.saturating_sub(n),
            Action::Down => *scroll = (*scroll + n).min(last),
            Action::HalfPageUp | Action::PageUp => *scroll = scroll.saturating_sub(10 * n),
            Action::HalfPageDown | Action::PageDown => *scroll = (*scroll + 10 * n).min(last),
            Action::FirstStep => *scroll = 0,
            Action::LastStep => *scroll = last,
            Action::Escape => self.page = self.help.back,
            Action::CommandLine
            | Action::TogglePlay
            | Action::PlaySong
            | Action::PlayFill
            | Action::Quit
            | Action::GotoMixer
            | Action::GotoPattern
            | Action::GotoSong
            | Action::GotoInstrument
            | Action::GotoEffects => return None,
            _ => self.does_nothing(action),
        }
        Some(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{Grid, Step, Track};

    fn editor() -> Editor {
        let track = Track {
            name: "t".into(),
            steps: vec![Step::default(); 16],
            ..Track::default()
        };
        Editor::new(
            Grid {
                name: "help".into(),
                steps_per_beat: 4,
                beats_per_bar: 4,
                grouping: vec![],
                tracks: vec![track],
                groups: vec![],
            },
            120.0,
        )
    }

    fn keys_for(lines: &[HelpLine], what: &str) -> Vec<String> {
        lines
            .iter()
            .filter_map(|l| match l {
                HelpLine::Item { keys, what: w, .. } if w.starts_with(what) => Some(keys.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn lists_every_key_and_command() {
        let e = editor();
        let lines = e.help_lines();
        assert!(keys_for(&lines, "mute / unmute in this pattern").contains(&"tM".to_string()));
        assert!(keys_for(&lines, "mixer page").iter().any(|k| k == "gm"));
        for spec in COMMANDS {
            assert!(
                lines.iter().any(
                    |l| matches!(l, HelpLine::Item { keys, .. } if keys.starts_with(&format!(":{} ", spec.name)) || *keys == format!(":{}", spec.name))
                ),
                "{}",
                spec.name
            );
        }
        // Each action once, with all its keys.
        let left = keys_for(&lines, crate::keymap::Action::Left.describe());
        assert_eq!(left[0], "h left");
    }

    #[test]
    fn opens_scrolls_and_goes_back() {
        let mut e = editor();
        crate::command::run(&mut e, "help").unwrap();
        assert_eq!(e.page, Page::Help);
        e.handle(Key::Char('j'));
        e.handle(Key::Char('j'));
        assert_eq!(e.help.scroll, 2);
        // Editing keys do nothing here.
        e.handle(Key::Char('i'));
        assert_eq!(e.mode, Mode::Normal);
        e.handle(Key::Char('g'));
        e.handle(Key::Char('e'));
        assert_eq!(e.help.scroll, e.help_lines().len() - 1);
        e.handle(Key::Esc);
        assert_eq!(e.page, Page::Pattern);
    }
}
