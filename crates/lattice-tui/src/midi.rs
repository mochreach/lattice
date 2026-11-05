//! MIDI in. Every port that sends MIDI is listened to as it appears, so a keyboard plugged in
//! while Lattice runs plays at once; one unplugged is let go. Its messages come to the UI thread,
//! which plays and enters notes as the piano keys do (see [`Editor::play_note`]), and does what
//! the controls are bound to (see [`Editor::control`]). The bindings are kept in `midi.toml`,
//! beside the config.
//!
//! [`Editor::play_note`]: lattice_editor::Editor::play_note
//! [`Editor::control`]: lattice_editor::Editor::control

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use anyhow::Result;
use lattice_editor::Editor;
use midir::{Ignore, MidiInput, MidiInputConnection};
use toml::{Table, Value};

use crate::config;
use crate::log::log;

/// How often to look for ports come and gone.
const SCAN: Duration = Duration::from_secs(1);

/// What Lattice calls itself to other MIDI software.
const CLIENT: &str = "Lattice";

/// A message from a port, as Lattice uses it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Message {
    /// A key down, `velocity` 1 to 127.
    NoteOn { pitch: u8, velocity: u8 },
    /// A key up.
    NoteOff { pitch: u8 },
    /// A controller number, from a pedal, button or knob, moved to `value`, 0 to 127.
    Control { number: u8, value: u8 },
}

impl Message {
    /// The message in `bytes`, if it's one Lattice uses. Every channel is listened to, so a
    /// keyboard plays whatever channel it's set to.
    pub fn parse(bytes: &[u8]) -> Option<Message> {
        let &[status, data1, data2, ..] = bytes else {
            return None;
        };
        let pitch = data1 & 0x7f;
        let velocity = data2 & 0x7f;
        match status & 0xf0 {
            // A note on at no velocity is how many keyboards say a key's come up.
            0x90 if velocity > 0 => Some(Message::NoteOn { pitch, velocity }),
            0x80 | 0x90 => Some(Message::NoteOff { pitch }),
            0xb0 => Some(Message::Control {
                number: data1 & 0x7f,
                value: data2 & 0x7f,
            }),
            _ => None,
        }
    }
}

/// A port being listened to.
struct Input {
    id: String,
    name: String,
    /// Keeps the port open; dropping it closes it.
    _connection: MidiInputConnection<()>,
}

/// The ports listened to, and what they've sent.
pub struct Midi {
    inputs: Vec<Input>,
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    /// Come in, not yet taken.
    came: Vec<Message>,
    scanned: Option<Instant>,
}

impl Midi {
    pub fn new() -> Self {
        let (sender, receiver) = channel();
        Self {
            inputs: vec![],
            sender,
            receiver,
            came: vec![],
            scanned: None,
        }
    }

    /// Whether any port is listened to, so it's worth looking for what's come in often.
    pub fn listening(&self) -> bool {
        !self.inputs.is_empty()
    }

    /// The ports listened to, by name.
    pub fn names(&self) -> Vec<String> {
        self.inputs
            .iter()
            .map(|i| short(&i.name).to_string())
            .collect()
    }

    /// Whether anything's come in that hasn't been taken.
    pub fn waiting(&mut self) -> bool {
        self.came.extend(self.receiver.try_iter());
        !self.came.is_empty()
    }

    /// What's come in since last taken.
    pub fn take(&mut self) -> Vec<Message> {
        self.came.extend(self.receiver.try_iter());
        std::mem::take(&mut self.came)
    }

    /// Listens to ports that have appeared and lets go of those gone, once a second. Says
    /// what changed, for the status line.
    pub fn scan(&mut self, now: Instant) -> Option<String> {
        if self.scanned.is_some_and(|at| now.duration_since(at) < SCAN) {
            return None;
        }
        self.scanned = Some(now);
        let Ok(lister) = MidiInput::new(CLIENT) else {
            return None;
        };
        let ports: Vec<_> = (lister.ports().into_iter())
            .filter_map(|port| Some((lister.port_name(&port).ok()?, port)))
            .filter(|(name, _)| wanted(name))
            .collect();
        let mut said = vec![];
        self.inputs.retain(|input| {
            let here = ports.iter().any(|(_, port)| port.id() == input.id);
            if !here {
                log!("midi in gone: {}", input.name);
                said.push(format!("{} gone", short(&input.name)));
            }
            here
        });
        for (name, port) in ports {
            if self.inputs.iter().any(|i| i.id == port.id()) {
                continue;
            }
            let Ok(mut input) = MidiInput::new(CLIENT) else {
                continue;
            };
            input.ignore(Ignore::All);
            let sender = self.sender.clone();
            let connected = input.connect(
                &port,
                "in",
                move |_, bytes, _| {
                    if let Some(message) = Message::parse(bytes) {
                        // The UI's gone only as Lattice quits.
                        let _ = sender.send(message);
                    }
                },
                (),
            );
            match connected {
                Ok(connection) => {
                    log!("midi in: {name}");
                    said.push(short(&name).to_string());
                    self.inputs.push(Input {
                        id: port.id(),
                        name,
                        _connection: connection,
                    });
                }
                Err(e) => log!("can't listen to {name}: {e}"),
            }
        }
        (!said.is_empty()).then(|| format!("midi in: {}", said.join(", ")))
    }
}

/// Where the MIDI controls' bindings are kept: `midi.toml`, beside the config.
fn controls_path() -> Option<PathBuf> {
    config::default_path().map(|p| p.with_file_name("midi.toml"))
}

/// Reads the controls' bindings into the editor, saying on the status line what's wrong with
/// any it can't use. No file is no bindings.
pub fn load_controls(editor: &mut Editor) {
    let Some(path) = controls_path() else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let mut problems = vec![];
    let mut entries = vec![];
    match text.parse::<Table>() {
        Ok(table) => match table.get("controls") {
            Some(Value::Table(controls)) => {
                for (control, target) in controls {
                    match target {
                        Value::String(target) => entries.push((control.clone(), target.clone())),
                        _ => problems.push(format!("{control:?}: what it does goes in quotes")),
                    }
                }
            }
            Some(_) => problems.push("[controls] should be a table".into()),
            None => {}
        },
        Err(e) => problems.push(e.message().to_string()),
    }
    problems.extend(editor.set_controls(&entries));
    if let Some(first) = problems.first() {
        editor.message = Some(format!("{}: {first}", path.display()));
    }
}

/// Keeps the controls' bindings, control and target as written, in the order they were bound.
pub fn save_controls(entries: &[(String, String)]) -> Result<PathBuf> {
    let path = controls_path().ok_or_else(|| anyhow::anyhow!("nowhere to keep them: no HOME"))?;
    let mut text = String::from(
        "# What MIDI controllers' pedals, buttons, pads and knobs do. :midi-learn writes it, \
         and it can be edited.\n# A control is \"cc N\" or \"note N\"; what it does is an \
         action's name, a :command, or \"macro N\".\n[controls]\n",
    );
    for (control, target) in entries {
        let control = Value::String(control.clone());
        let target = Value::String(target.clone());
        text += &format!("{control} = {target}\n");
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, text)?;
    Ok(path)
}

/// Whether to listen to the port `name`. Not a loopback, which would hand Lattice back what it
/// sends, nor Lattice's own.
fn wanted(name: &str) -> bool {
    !name.contains("Through") && !name.starts_with(CLIENT)
}

/// A port's name without what ALSA adds: "KeyStep 37:KeyStep 37 MIDI 1 20:0" is "KeyStep 37".
pub fn short(name: &str) -> &str {
    name.split(':').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_notes_on_any_channel() {
        assert_eq!(
            Message::parse(&[0x93, 60, 100]),
            Some(Message::NoteOn {
                pitch: 60,
                velocity: 100
            })
        );
        assert_eq!(
            Message::parse(&[0x90, 60, 0]),
            Some(Message::NoteOff { pitch: 60 })
        );
        assert_eq!(
            Message::parse(&[0x80, 61, 64]),
            Some(Message::NoteOff { pitch: 61 })
        );
        assert_eq!(Message::parse(&[0xf8]), None, "clock");
        assert_eq!(Message::parse(&[0x90, 60]), None, "cut short");
    }

    #[test]
    fn names_ports_shortly_and_skips_loopbacks() {
        assert_eq!(short("KeyStep 37:KeyStep 37 MIDI 1 20:0"), "KeyStep 37");
        assert!(!wanted("Midi Through:Midi Through Port-0 14:0"));
        assert!(wanted("KeyStep 37:KeyStep 37 MIDI 1 20:0"));
    }
}
