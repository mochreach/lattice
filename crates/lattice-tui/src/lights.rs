//! Lights the keys of a keyboard that lights them one by one, through `qs-rgbd`, a service that
//! shares the keyboard between the programs that want it. What each key means comes from the
//! editor ([`Editor::lights`]); here it's given a colour, with the transport's added, and sent.
//!
//! Talking to the service waits for its answers, so it's done on a thread of its own, sent the
//! whole picture each time it changes, rather than ever holding up a frame. Without the
//! service, on another machine say, nothing lights and nothing goes wrong.

use std::sync::mpsc::{Receiver, Sender, channel};

use lattice_editor::{Editor, Glow, Key};
use lattice_engine::Status;

use crate::keyboard_lights::Keyboard;

/// What the layer's called, for `qs-rgb list`.
const LAYER: &str = "lattice";

/// Where applications' layers go: over the battery and notifications, under the volume bar.
const PRIORITY: u16 = 20;

/// How a key lights: a colour, and how it moves, if it does.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Light {
    colour: &'static str,
    /// An effect the service knows, `breathe`, `pulse` or `blink`, and its period in seconds.
    moves: Option<(&'static str, f32)>,
}

const fn solid(colour: &'static str) -> Light {
    Light {
        colour,
        moves: None,
    }
}

/// The colour for what a key means. The terminal's palette can't be asked for its colours, so
/// these are the keyboard's own, after the theme's roles.
fn light(glow: Glow) -> Light {
    match glow {
        Glow::Root => solid("#ff5000"),
        Glow::InScale => solid("#909090"),
        Glow::OutOfScale => solid("#081840"),
        Glow::Pad => solid("#00a0ff"),
        Glow::Choice => solid("#ffaa00"),
        Glow::Insert => solid("#00ff60"),
        Glow::Select => solid("#ffd000"),
        Glow::Command => solid("#a040ff"),
    }
}

/// The keys lit, by the service's names for them.
type Picture = Vec<(String, Light)>;

/// Keeps the keyboard showing what's going on.
pub struct Lights {
    to: Sender<Picture>,
    shown: Picture,
}

impl Lights {
    /// Starts the thread that talks to the service, if it's there.
    pub fn start() -> Self {
        let (to, from) = channel();
        std::thread::spawn(move || send(&from));
        Self { to, shown: vec![] }
    }

    /// Lights the keys for `editor` and the engine's `status`, if they've changed.
    pub fn show(&mut self, editor: &Editor, status: &Status) {
        let mut picture: Picture = (editor.lights().into_iter())
            .filter_map(|(key, glow)| Some((name(key)?, light(glow))))
            .collect();
        // The transport, on the keys that start it: space playing, red while it records; right
        // Alt, the loop pedal, as the looper waits or records.
        let recording = editor.record || status.recording();
        if status.playing() {
            picture.push((
                "space".into(),
                solid(if recording { "#ff0020" } else { "#00ff40" }),
            ));
        }
        if editor.armed().is_some() {
            let pedal = match () {
                () if status.recording() => Light {
                    colour: "#ff0020",
                    moves: Some(("breathe", 1.0)),
                },
                () if status.listening() => Light {
                    colour: "#ffaa00",
                    moves: Some(("pulse", 1.0)),
                },
                () => Light {
                    colour: "#ffaa00",
                    moves: Some(("breathe", 2.0)),
                },
            };
            picture.push(("rightalt".into(), pedal));
        }
        // A key can mean two things, as a choice and a note; the first said wins.
        let mut seen = std::collections::HashSet::new();
        picture.retain(|(key, _)| seen.insert(key.clone()));
        if picture != self.shown {
            // Only gone if the thread has, when there's nothing to light anyway.
            let _ = self.to.send(picture.clone());
            self.shown = picture;
        }
    }
}

/// Lights each picture sent, the newest, until Lattice stops.
fn send(from: &Receiver<Picture>) {
    let mut keyboard = Keyboard::connect(LAYER);
    while let Ok(mut picture) = from.recv() {
        // Only the newest matters, if several came while the last was being sent.
        while let Ok(newer) = from.try_recv() {
            picture = newer;
        }
        // The service may have started since, or come back.
        if !keyboard.is_connected() {
            keyboard = Keyboard::connect(LAYER);
        }
        let still: Vec<(&str, &str)> = (picture.iter())
            .filter(|(_, light)| light.moves.is_none())
            .map(|(key, light)| (key.as_str(), light.colour))
            .collect();
        // A missing service is no error; a write that fails stops the client trying.
        let _ = keyboard.set(PRIORITY, &still);
        for (key, light) in &picture {
            if let Some((effect, period)) = light.moves {
                let _ = keyboard.animate(key, light.colour, effect, period);
            }
        }
    }
}

/// What the service calls the key `key` is typed on, on a British keyboard, as the laptop's
/// is: a shifted symbol lights the key it's on.
fn name(key: Key) -> Option<String> {
    let name = match key {
        Key::Char(' ') | Key::Ctrl(' ') | Key::Alt(' ') => "space".into(),
        Key::Char(c) | Key::Ctrl(c) | Key::Alt(c) => {
            let base = match c {
                '!' => '1',
                '"' => '2',
                '£' => '3',
                '$' => '4',
                '%' => '5',
                '^' => '6',
                '&' => '7',
                '*' => '8',
                '(' => '9',
                ')' => '0',
                '_' => '-',
                '+' => '=',
                '{' => '[',
                '}' => ']',
                ':' => ';',
                '@' => '\'',
                '~' => '#',
                '<' => ',',
                '>' => '.',
                '?' => '/',
                '|' => '\\',
                '¬' => '`',
                c => c.to_ascii_lowercase(),
            };
            base.to_string()
        }
        Key::Esc => "esc".into(),
        Key::Enter => "enter".into(),
        Key::Backspace => "backspace".into(),
        Key::Delete => "delete".into(),
        Key::Tab => "tab".into(),
        Key::Up => "up".into(),
        Key::Down => "down".into(),
        Key::Left => "left".into(),
        Key::Right => "right".into(),
        Key::Home => "home".into(),
        Key::End => "end".into(),
        Key::F(n @ 1..=12) => format!("f{n}"),
        Key::RightAlt => "rightalt".into(),
        // No numpad on the laptop, and no keys for the rest.
        _ => return None,
    };
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shifted_symbol_lights_the_key_its_on() {
        assert_eq!(name(Key::Char('G')).as_deref(), Some("g"));
        assert_eq!(name(Key::Char('{')).as_deref(), Some("["));
        assert_eq!(name(Key::Char('"')).as_deref(), Some("2"));
        assert_eq!(name(Key::Ctrl('a')).as_deref(), Some("a"));
        assert_eq!(name(Key::Char(' ')).as_deref(), Some("space"));
        assert_eq!(name(Key::PageUp), None);
    }
}
