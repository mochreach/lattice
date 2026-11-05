//! MIDI controls: a controller's pedals, buttons, pads and knobs, bound to what Lattice does.
//! A pedal or button does an action or runs a command as it goes down, as a key would; a knob
//! turns one of the macros of the track the piano keys play. `:midi-learn` binds the next
//! control moved, and the frontend keeps the bindings in a file of their own.

use std::fmt;

use crate::keymap::{Action, Binding};
use crate::{Editor, Effect};

/// A control on a MIDI controller. Channels aren't told apart, so a control does the same
/// whichever channel the controller sends it on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Control {
    /// A controller number: a pedal, button or knob.
    Cc(u8),
    /// A note: a pad, or a key given over to a control.
    Note(u8),
}

impl Control {
    /// `cc 64` or `note 36`, as it's written.
    pub fn parse(text: &str) -> Option<Control> {
        let (kind, number) = text.trim().split_once(' ')?;
        let number = number.trim().parse::<u8>().ok().filter(|&n| n <= 127)?;
        match kind {
            "cc" => Some(Control::Cc(number)),
            "note" => Some(Control::Note(number)),
            _ => None,
        }
    }
}

impl fmt::Display for Control {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Control::Cc(n) => write!(f, "cc {n}"),
            Control::Note(n) => write!(f, "note {n}"),
        }
    }
}

/// What a control does.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// An action or command, as it goes down.
    Bound(Binding),
    /// The macro numbered from 1 on the track the piano keys play, turned through its range.
    Macro(usize),
}

impl Target {
    /// An action's name, a command after a `:`, or `macro 2`.
    pub fn parse(text: &str) -> Result<Target, String> {
        let text = text.trim();
        if let Some(n) = text.strip_prefix("macro") {
            return match n.trim().parse::<usize>() {
                Ok(n @ 1..) => Ok(Target::Macro(n)),
                _ => Err(format!(
                    "{text:?}: a macro by its number, from 1, as in macro 2"
                )),
            };
        }
        match Binding::parse(text)? {
            Some(binding @ (Binding::Action(_) | Binding::Command(_))) => {
                Ok(Target::Bound(binding))
            }
            _ => Err(format!(
                "{text:?}: an action's name, a :command, or macro and a number"
            )),
        }
    }
}

impl fmt::Display for Target {
    /// As [`Target::parse`] reads it.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Target::Bound(Binding::Action(action)) => f.write_str(&action.name()),
            Target::Bound(Binding::Command(line)) => write!(f, ":{line}"),
            Target::Bound(Binding::Menu(menu)) => f.write_str(&menu.title),
            Target::Macro(n) => write!(f, "macro {n}"),
        }
    }
}

/// The controls bound, and what's being learnt.
#[derive(Debug, Clone, Default)]
pub struct Controls {
    /// In the order they were bound.
    pub bound: Vec<(Control, Target)>,
    /// What `:midi-learn` will bind the next control moved to; `Some(None)` to unbind it.
    learning: Option<Option<Target>>,
    /// The controls down, so a pedal acts once as it goes down, however many messages it sends.
    down: Vec<Control>,
    /// The macro a knob last turned, by track and macro, while nothing else has changed since,
    /// so a turn is one undo rather than one for every message.
    turning: Option<(usize, usize)>,
}

impl Controls {
    /// The bindings in `entries`, control and target as they're written, with what's wrong
    /// with any that aren't understood.
    pub fn read(entries: &[(String, String)]) -> (Controls, Vec<String>) {
        let mut controls = Controls::default();
        let mut problems = vec![];
        for (control, target) in entries {
            let Some(c) = Control::parse(control) else {
                problems.push(format!("{control:?}: a control is cc or note and a number"));
                continue;
            };
            match Target::parse(target) {
                Ok(t) => controls.bind(c, Some(t)),
                Err(e) => problems.push(e),
            }
        }
        (controls, problems)
    }

    /// As they're written, for the frontend to keep.
    pub fn entries(&self) -> Vec<(String, String)> {
        (self.bound.iter())
            .map(|(c, t)| (c.to_string(), t.to_string()))
            .collect()
    }

    fn bind(&mut self, control: Control, target: Option<Target>) {
        self.bound.retain(|(c, _)| *c != control);
        if let Some(target) = target {
            self.bound.push((control, target));
        }
    }

    fn target(&self, control: Control) -> Option<&Target> {
        (self.bound.iter())
            .find(|(c, _)| *c == control)
            .map(|(_, t)| t)
    }
}

impl Editor {
    /// `:midi-learn TARGET`: the next control moved is bound to `target`, or unbound if it's
    /// `none`; with nothing, stops learning.
    pub(crate) fn midi_learn(&mut self, target: Option<&str>) -> Result<(), String> {
        let learning = match target.map(str::trim) {
            None | Some("") => {
                self.controls.learning = None;
                self.message = Some("stopped learning".into());
                return Ok(());
            }
            Some("none") => None,
            Some(text) => Some(Target::parse(text)?),
        };
        self.message = Some(match &learning {
            Some(target) => format!("move a pedal, button, pad or knob to bind it to {target}"),
            None => "move a pedal, button, pad or knob to unbind it".into(),
        });
        self.controls.learning = Some(learning);
        Ok(())
    }

    /// A control has moved to `value`, 0 to 127: a note's velocity, or 0 as it comes up.
    /// `None` if it isn't bound, so a note plays as a note.
    pub fn control(&mut self, control: Control, value: u8) -> Option<Vec<Effect>> {
        let down = value >= 64 || matches!(control, Control::Note(_)) && value > 0;
        if let Some(learning) = self.controls.learning.take() {
            // A note coming up, or a pedal let go, isn't a control being moved to learn.
            if !down && !matches!(learning, Some(Target::Macro(_))) {
                self.controls.learning = Some(learning);
                return self.controls.target(control).map(|_| vec![]);
            }
            self.message = Some(match &learning {
                Some(target) => format!("{control} does {target}"),
                None => format!("{control} does nothing now"),
            });
            self.controls.bind(control, learning);
            self.controls.down.retain(|&c| c != control);
            if down {
                self.controls.down.push(control);
            }
            return Some(vec![Effect::MidiControls(self.controls.entries())]);
        }
        let target = self.controls.target(control)?.clone();
        let was_down = self.controls.down.contains(&control);
        self.controls.down.retain(|&c| c != control);
        if down {
            self.controls.down.push(control);
        }
        match target {
            Target::Macro(n) => {
                let track = self.played_track();
                let m = n - 1;
                if self.macro_of(track, m).is_none() {
                    self.message = Some(format!("{control}: this track has no macro {n}"));
                    return Some(vec![]);
                }
                let value = f32::from(value) / 127.0;
                Some(self.turn_macro(track, m, value))
            }
            Target::Bound(binding) if down && !was_down => {
                self.controls.turning = None;
                Some(self.bound(&binding, 1))
            }
            // A fill held on a pedal ends as it comes up.
            Target::Bound(Binding::Action(Action::PlayFill)) if !down && was_down && self.fill => {
                self.fill = false;
                Some(vec![Effect::Fill(false)])
            }
            Target::Bound(_) => Some(vec![]),
        }
    }

    /// Turns macro `m` of `track` from a knob: the first message of a turn is an undo step,
    /// and the rest go into it, until something else changes.
    fn turn_macro(&mut self, track: usize, m: usize, value: f32) -> Vec<Effect> {
        if self.recording() || self.controls.turning != Some((track, m)) {
            let effects = self.set_macro(track, m, value);
            // Only once it's changed is there an undo step to go on in.
            if !effects.is_empty() {
                self.controls.turning = Some((track, m));
            }
            return effects;
        }
        match self.macro_of_mut(track, m) {
            Some(mac) if mac.value != value => {
                mac.value = value;
                vec![Effect::TrackChanged(track)]
            }
            _ => vec![],
        }
    }

    /// Something other than a knob has changed the song, so the next turn is an undo step of
    /// its own.
    pub(crate) fn stop_turning(&mut self) {
        self.controls.turning = None;
    }

    /// The controls bound, from the frontend's file, with what's wrong with any it can't use.
    pub fn set_controls(&mut self, entries: &[(String, String)]) -> Vec<String> {
        let (controls, problems) = Controls::read(entries);
        self.controls = controls;
        problems
    }

    /// The controls bound, for `:midi` and the help page.
    pub fn controls(&self) -> &[(Control, Target)] {
        &self.controls.bound
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Page, keymap::Key};
    use lattice_core::{Grid, Step, Track};

    fn editor() -> Editor {
        let track = Track {
            name: "t".into(),
            steps: vec![Step::default(); 16],
            ..Track::default()
        };
        Editor::new(
            Grid {
                name: "midi".into(),
                steps_per_beat: 4,
                beats_per_bar: 4,
                grouping: vec![],
                tracks: vec![track],
                groups: vec![],
            },
            120.0,
        )
    }

    #[test]
    fn reads_and_writes_bindings() {
        let entries = [
            ("cc 64".to_string(), "toggle_record".to_string()),
            ("note 36".to_string(), ":arm 1".to_string()),
            ("cc 21".to_string(), "macro 2".to_string()),
            ("cc 300".to_string(), "toggle_play".to_string()),
            ("cc 1".to_string(), "fly".to_string()),
        ];
        let (controls, problems) = Controls::read(&entries);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert_eq!(controls.entries(), entries[..3]);
    }

    #[test]
    fn a_pedal_acts_once_as_it_goes_down() {
        let mut e = editor();
        e.set_controls(&[("cc 64".into(), "toggle_record".into())]);
        assert_eq!(e.control(Control::Cc(64), 127), Some(vec![]));
        assert!(e.record);
        // Pedals send more than one message going down, and one coming up.
        e.control(Control::Cc(64), 127);
        e.control(Control::Cc(64), 0);
        assert!(e.record);
        e.control(Control::Cc(64), 127);
        assert!(!e.record);
        // An unbound control is left to play.
        assert_eq!(e.control(Control::Note(60), 100), None);
    }

    #[test]
    fn learns_the_next_control_moved() {
        let mut e = editor();
        crate::command::run(&mut e, "midi-learn toggle_play").unwrap();
        let effects = e.control(Control::Note(36), 90).unwrap();
        assert_eq!(e.message.as_deref(), Some("note 36 does toggle_play"));
        assert_eq!(
            effects,
            [Effect::MidiControls(vec![(
                "note 36".into(),
                "toggle_play".into()
            )])]
        );
        // It's learnt, not played; coming up does nothing.
        assert_eq!(e.control(Control::Note(36), 0), Some(vec![]));
        assert_eq!(
            e.control(Control::Note(36), 90),
            Some(vec![Effect::TogglePlay])
        );
        crate::command::run(&mut e, "midi-learn none").unwrap();
        e.control(Control::Note(36), 90);
        assert!(e.controls().is_empty());
        assert!(crate::command::run(&mut e, "midi-learn fly").is_err());
    }

    #[test]
    fn a_knob_turns_a_macro_in_one_undo() {
        let mut e = editor();
        crate::command::run(&mut e, "macro m").unwrap();
        e.set_controls(&[("cc 21".into(), "macro 1".into())]);
        let before = e.grid.tracks[0].macros[0].value;
        for v in [10, 40, 127] {
            assert_eq!(
                e.control(Control::Cc(21), v),
                Some(vec![Effect::TrackChanged(0)])
            );
        }
        assert_eq!(e.grid.tracks[0].macros[0].value, 1.0);
        e.handle(Key::Char('u'));
        assert_eq!(e.grid.tracks[0].macros[0].value, before);
        e.set_controls(&[("cc 22".into(), "macro 2".into())]);
        e.control(Control::Cc(22), 64);
        assert_eq!(
            e.message.as_deref(),
            Some("cc 22: this track has no macro 2")
        );
        assert_eq!(e.page, Page::Pattern);
    }
}
