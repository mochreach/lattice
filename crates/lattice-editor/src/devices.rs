//! Choosing the audio devices: `:input` for what the looper records, `:output` for what's heard.
//! The frontend finds the devices and fills in [`Devices`]; while `:input ` or `:output ` is
//! typed, the infobox lists them, numbered, and the command takes a number or part of a name.

use crate::{Editor, Effect, Infobox, Mode, PromptKind};

/// Which way audio goes through a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Input,
    Output,
}

impl Direction {
    pub fn name(self) -> &'static str {
        match self {
            Direction::Input => "input",
            Direction::Output => "output",
        }
    }
}

/// What the id of an input that records an app's own sound, rather than a device's, starts with.
pub const APP: &str = "app:";

/// An audio device the frontend found, or an app it can record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// What the frontend knows it by, kept from one run to the next.
    pub id: String,
    pub name: String,
}

/// The devices one way, as the frontend last found them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Devices {
    pub found: Vec<Device>,
    /// The system's default's name, if there is one.
    pub default: Option<String>,
    /// The device in use, by id; `None` for the system's default, whichever that is.
    pub chosen: Option<String>,
    /// Whether they've been looked for yet.
    pub looked: bool,
}

impl Devices {
    /// The device in use's name.
    pub fn current(&self) -> String {
        match &self.chosen {
            Some(id) => (self.found.iter().find(|d| &d.id == id))
                .map_or_else(|| id.clone(), |d| d.name.clone()),
            None => match &self.default {
                Some(name) => format!("the default ({name})"),
                None => "the default".into(),
            },
        }
    }

    /// The device `typed` picks: 0 for the default, a number from the list, or part of one
    /// name. `Ok(None)` is the default.
    fn pick(&self, typed: &str, direction: Direction) -> Result<Option<String>, String> {
        let way = direction.name();
        if let Ok(n) = typed.parse::<usize>() {
            return match n {
                0 => Ok(None),
                n if n <= self.found.len() => (self.found.get(n - 1))
                    .map(|d| Some(d.id.clone()))
                    .ok_or_else(|| format!("no {way} {n}")),
                _ if !self.looked => Err(format!("still looking for {way}s")),
                n => Err(format!("no {way} {n}; there are {}", self.found.len())),
            };
        }
        if typed.eq_ignore_ascii_case("default") {
            return Ok(None);
        }
        let lower = typed.to_lowercase();
        let matching: Vec<_> = (self.found.iter())
            .filter(|d| d.name.to_lowercase().contains(&lower))
            .collect();
        match matching[..] {
            [device] => Ok(Some(device.id.clone())),
            [] => Err(format!("no {way} called {typed:?}")),
            _ => {
                // A name that's all of one device's and part of others' picks the one.
                match (matching.iter()).find(|d| d.name.eq_ignore_ascii_case(typed)) {
                    Some(device) => Ok(Some(device.id.clone())),
                    None => Err(format!(
                        "{} {way}s match {typed:?}; type more, or its number",
                        matching.len()
                    )),
                }
            }
        }
    }

    /// The list the infobox shows: the default first, then each device, the one in use marked.
    fn infobox(&self, direction: Direction) -> Infobox {
        let mark = |chosen: bool| if chosen { "● " } else { "  " };
        let default = match &self.default {
            Some(name) => format!("default: {name}"),
            None => "default".into(),
        };
        let mut items = vec![(
            "0".into(),
            format!("{}{default}", mark(self.chosen.is_none())),
        )];
        for (i, device) in self.found.iter().enumerate() {
            let chosen = self.chosen.as_ref() == Some(&device.id);
            items.push((
                (i + 1).to_string(),
                format!("{}{}", mark(chosen), device.name),
            ));
        }
        if !self.looked {
            items.push(("…".into(), "  looking".into()));
        }
        Infobox {
            title: format!("{}s", direction.name()),
            items,
        }
    }
}

impl Editor {
    pub fn devices(&self, direction: Direction) -> &Devices {
        match direction {
            Direction::Input => &self.inputs,
            Direction::Output => &self.outputs,
        }
    }

    pub fn devices_mut(&mut self, direction: Direction) -> &mut Devices {
        match direction {
            Direction::Input => &mut self.inputs,
            Direction::Output => &mut self.outputs,
        }
    }

    /// Which devices are being chosen on the command line, if any: `:input ` or `:output `
    /// typed, for the frontend to keep the list fresh and the infobox to show it.
    pub fn choosing_device(&self) -> Option<Direction> {
        let Mode::Command(prompt) = &self.mode else {
            return None;
        };
        if prompt.kind != PromptKind::Command {
            return None;
        }
        match prompt.line.split_once(' ')?.0 {
            "input" => Some(Direction::Input),
            "output" => Some(Direction::Output),
            _ => None,
        }
    }

    pub(crate) fn device_infobox(&self) -> Option<Infobox> {
        let direction = self.choosing_device().or(self.listing)?;
        Some(self.devices(direction).infobox(direction))
    }

    /// `:input` and `:output`: with nothing typed, says which is in use and leaves the list up
    /// until the next key; otherwise asks the frontend to switch to the device picked.
    pub(crate) fn choose_device(
        &mut self,
        direction: Direction,
        typed: &str,
    ) -> Result<Vec<Effect>, String> {
        let devices = self.devices(direction);
        let typed = typed.trim();
        if typed.is_empty() {
            self.message = Some(format!("{}: {}", direction.name(), devices.current()));
            self.listing = Some(direction);
            return Ok(vec![]);
        }
        let id = devices.pick(typed, direction)?;
        Ok(vec![Effect::Device(direction, id)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;
    use lattice_core::{Grid, Track};

    fn devices() -> Devices {
        let device = |id: &str, name: &str| Device {
            id: id.into(),
            name: name.into(),
        };
        Devices {
            found: vec![
                device("a", "Headset Microphone"),
                device("b", "Digital Microphone"),
                device("c", "Monitor of Speaker"),
            ],
            default: Some("Digital Microphone".into()),
            chosen: Some("c".into()),
            looked: true,
        }
    }

    #[test]
    fn a_device_is_picked_by_number_or_name() {
        let d = devices();
        let pick = |typed| d.pick(typed, Direction::Input);
        assert_eq!(pick("0"), Ok(None));
        assert_eq!(pick("default"), Ok(None));
        assert_eq!(pick("2"), Ok(Some("b".into())));
        assert_eq!(pick("HEAD"), Ok(Some("a".into())));
        assert!(pick("microphone").unwrap_err().contains("2 inputs match"));
        assert!(pick("4").is_err());
        assert!(pick("guitar").is_err());
    }

    #[test]
    fn typing_the_command_lists_the_devices() {
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            beats_per_bar: 4,
            grouping: vec![],
            tracks: vec![Track::default()],
            groups: vec![],
        };
        let mut e = Editor::new(grid, 120.0);
        e.inputs = devices();
        e.handle(Key::Char(':'));
        for c in "input".chars() {
            e.handle(Key::Char(c));
        }
        assert_eq!(e.infobox(), None, "not while the command's name is typed");
        e.handle(Key::Char(' '));
        let infobox = e.infobox().unwrap();
        assert_eq!(infobox.title, "inputs");
        assert_eq!(infobox.items[0].1, "  default: Digital Microphone");
        assert_eq!(
            infobox.items[3],
            ("3".into(), "● Monitor of Speaker".into())
        );
        assert_eq!(
            e.choose_device(Direction::Input, "1"),
            Ok(vec![Effect::Device(Direction::Input, Some("a".into()))])
        );
        e.handle(Key::Esc);
        for c in ":input".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        assert_eq!(e.message.as_deref(), Some("input: Monitor of Speaker"));
        assert_eq!(e.mode, Mode::Normal);
        assert_eq!(e.infobox().unwrap().title, "inputs", "the list's left up");
        e.handle(Key::Char('j'));
        assert_eq!(e.infobox(), None, "until the next key");
    }

    #[test]
    fn apps_playing_are_listed_after_the_devices_but_suggested_first() {
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            beats_per_bar: 4,
            grouping: vec![],
            tracks: vec![Track::default()],
            groups: vec![],
        };
        let mut e = Editor::new(grid, 120.0);
        e.inputs = devices();
        e.inputs.found.push(Device {
            id: format!("{APP}Firefox"),
            name: "app: Firefox".into(),
        });
        for c in ":input ".chars() {
            e.handle(Key::Char(c));
        }
        assert_eq!(e.infobox().unwrap().items[4].1, "  app: Firefox");
        let first = &e.completions()[0];
        assert_eq!(
            (first.text.as_str(), first.describe.as_str()),
            ("app: Firefox", "device 4")
        );
        assert_eq!(
            e.choose_device(Direction::Input, "firefox"),
            Ok(vec![Effect::Device(
                Direction::Input,
                Some("app:Firefox".into())
            )])
        );
    }
}
