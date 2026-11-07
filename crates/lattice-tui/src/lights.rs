//! Lights the keys of a keyboard that lights them one by one, through `qs-rgbd`, a service that
//! shares the keyboard between the programs that want it. What each key means comes from the
//! editor ([`Editor::lights`]); here it's given a colour, with the transport's added, and sent.
//!
//! Talking to the service waits for its answers, so it's done on a thread of its own, sent the
//! whole picture each time it changes, rather than ever holding up a frame. Without the
//! service, on another machine say, nothing lights and nothing goes wrong.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use lattice_editor::lights::{Glow, Kind, Playhead};
use lattice_editor::{Editor, Key};
use lattice_engine::Status;

#[cfg(unix)]
use crate::keyboard_lights::Keyboard;
use crate::log::HEALTH;

/// What the layer's called, for `qs-rgb list`.
const LAYER: &str = "lattice";

/// Where applications' layers go: over the battery and notifications, under the volume bar.
const PRIORITY: u16 = 20;

/// How long a key flashes after something goes wrong.
const FLASH: Duration = Duration::from_millis(1_500);

/// A key flashing to say something went wrong.
const WARNING: Light = Light {
    colour: "#ff0000",
    moves: Some(("blink", 0.25)),
};

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
/// these are the keyboard's own. What a key could do is dim, a colour for each kind of action;
/// what it does now, or what's happening, is bright.
fn light(glow: Glow) -> Light {
    match glow {
        Glow::Does(Kind::Move) => solid("#0028c0"),
        Glow::Does(Kind::Page) => solid("#007878"),
        Glow::Does(Kind::Select) => solid("#907000"),
        Glow::Does(Kind::Edit) => solid("#a00018"),
        Glow::Does(Kind::Mode) => solid("#606060"),
        Glow::Does(Kind::Track) => solid("#900070"),
        Glow::Does(Kind::Play) => solid("#00a020"),
        Glow::Menu => solid("#c06000"),
        Glow::Count => solid("#181818"),
        Glow::Dark => solid("#000000"),
        Glow::Root => solid("#ff5000"),
        Glow::InScale => solid("#a0a0a0"),
        Glow::OutOfScale => solid("#081840"),
        Glow::Sounding => solid("#00ffff"),
        Glow::Pad => solid("#00a0ff"),
        Glow::Choice => solid("#ffaa00"),
        Glow::Beat => solid("#40ff80"),
        Glow::Bar => solid("#003010"),
        Glow::Recorded => solid("#a040ff"),
        Glow::Recording => Light {
            colour: "#ff0020",
            moves: Some(("blink", 1.0)),
        },
        Glow::Insert => solid("#00ff60"),
        Glow::Select => solid("#ffd000"),
        Glow::Command => solid("#ffffff"),
    }
}

/// The keys lit, by the service's names for them.
type Picture = Vec<(String, Light)>;

/// Keeps the keyboard showing what's going on.
pub struct Lights {
    to: Sender<Picture>,
    shown: Picture,
    /// The output blocks missed, and input frames dropped, as last seen.
    missed: u64,
    dropped: u64,
    /// Until when `space` flashes for a glitch in the sound, and right Alt for input lost.
    glitched: Option<Instant>,
    lost: Option<Instant>,
}

impl Lights {
    /// Starts the thread that talks to the service, if it's there.
    pub fn start() -> Self {
        let (to, from) = channel();
        std::thread::spawn(move || send(&from));
        Self {
            to,
            shown: vec![],
            missed: HEALTH.missed.load(Ordering::Relaxed),
            dropped: HEALTH.dropped.load(Ordering::Relaxed),
            glitched: None,
            lost: None,
        }
    }

    /// Lights the keys for `editor` and the engine's `status`, if they've changed.
    pub fn show(&mut self, editor: &Editor, status: &Status) {
        // The transport first, on the keys that start it, as it matters most: space playing,
        // red while it records; right Alt, the loop pedal, as the looper waits or records.
        let mut picture: Picture = vec![];
        // A glitch first of all, flashing for a moment: space if the sound broke up, right Alt
        // if the input lost some of what it heard, which the looper would have recorded.
        let now = Instant::now();
        let (missed, dropped) = (
            HEALTH.missed.load(Ordering::Relaxed),
            HEALTH.dropped.load(Ordering::Relaxed),
        );
        if missed > self.missed {
            self.glitched = Some(now + FLASH);
        }
        if dropped > self.dropped {
            self.lost = Some(now + FLASH);
        }
        (self.missed, self.dropped) = (missed, dropped);
        if self.glitched.is_some_and(|until| now < until) {
            picture.push(("space".into(), WARNING));
        }
        if self.lost.is_some_and(|until| now < until) {
            picture.push(("rightalt".into(), WARNING));
        }
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
        let playhead = status.playing().then(|| Playhead {
            step: status.pattern_step(),
            tracks: (0..editor.grid.tracks.len())
                .map(|t| status.position(t))
                .collect(),
        });
        picture.extend(
            (editor.lights(playhead.as_ref()).into_iter())
                .filter_map(|(key, glow)| Some((name(key)?, light(glow)))),
        );
        // A key can mean several things, `u` and `U` say; the first said wins.
        let mut seen = HashSet::new();
        picture.retain(|(key, _)| seen.insert(key.clone()));
        if picture != self.shown {
            // Only gone if the thread has, when there's nothing to light anyway.
            let _ = self.to.send(picture.clone());
            self.shown = picture;
        }
    }
}

/// Lights each picture sent, the newest, until Lattice stops: the keys that changed alone, if
/// it's the same keys lit, as it is when a note sounds or the beat moves on, or else the whole
/// layer afresh, so no key's left lit from the last.
fn send(from: &Receiver<Picture>) {
    let mut keyboard = Keyboard::connect(LAYER);
    let mut shown: HashMap<String, Light> = HashMap::new();
    while let Ok(mut picture) = from.recv() {
        // Only the newest matters, if several came while the last was being sent.
        while let Ok(newer) = from.try_recv() {
            picture = newer;
        }
        // The service may have started since, or come back.
        if !keyboard.is_connected() {
            keyboard = Keyboard::connect(LAYER);
            shown.clear();
        }
        let same_keys =
            shown.len() == picture.len() && picture.iter().all(|(key, _)| shown.contains_key(key));
        let changed: Vec<&(String, Light)> = (picture.iter())
            .filter(|(key, light)| !same_keys || shown.get(key) != Some(light))
            .collect();
        let still: Vec<(&str, &str)> = (changed.iter())
            .filter(|(_, light)| light.moves.is_none())
            .map(|(key, light)| (key.as_str(), light.colour))
            .collect();
        // A missing service is no error; a write that fails stops the client trying.
        if !same_keys {
            let _ = keyboard.set(PRIORITY, &still);
        } else if !still.is_empty() {
            let _ = keyboard.patch(&still);
        }
        for (key, light) in &changed {
            if let Some((effect, period)) = light.moves {
                let _ = keyboard.animate(key, light.colour, effect, period);
            }
        }
        shown = picture.into_iter().collect();
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

/// Without Unix sockets there's no service to ask, so nothing lights.
#[cfg(not(unix))]
struct Keyboard;

#[cfg(not(unix))]
impl Keyboard {
    fn connect(_layer: &str) -> Self {
        Self
    }

    fn is_connected(&self) -> bool {
        false
    }

    fn set(&mut self, _priority: u16, _keys: &[(&str, &str)]) -> std::io::Result<()> {
        Ok(())
    }

    fn patch(&mut self, _keys: &[(&str, &str)]) -> std::io::Result<()> {
        Ok(())
    }

    fn animate(
        &mut self,
        _key: &str,
        _colour: &str,
        _effect: &str,
        _period: f32,
    ) -> std::io::Result<()> {
        Ok(())
    }
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
