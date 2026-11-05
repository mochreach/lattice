//! MIDI controls: a controller's pedals, buttons, pads and knobs, bound to what Lattice does.
//! A pedal or button does an action or runs a command as it goes down, as a key would; a knob
//! turns one of the macros of the track the piano keys play. `:midi-learn` binds the next
//! control moved, and the frontend keeps the bindings in a file of their own.
//!
//! A knob can also be bound to one setting of one track, with `:midi-learn this` on it. That's
//! the song's, kept with the track as a [`Knob`], and comes before what the frontend's file
//! binds the knob to.

use std::fmt;

use lattice_core::macros::setting_info;
use lattice_core::{Knob, Lock, MACRO};

use crate::keymap::{Action, Binding};
use crate::locks::setting_name;
use crate::{Editor, Effect, TrackId};

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

/// What `:midi-learn` binds the next control moved to.
#[derive(Debug, Clone, PartialEq)]
enum Learning {
    /// A target, or nothing, to unbind it.
    Bind(Option<Target>),
    /// One setting of a track, the effect it's on and its id, as a [`Knob`] has them.
    Knob(TrackId, Option<u8>, u8),
}

/// What a knob is turning, while nothing else has changed since.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Turning {
    /// A macro, by track and macro.
    Macro(usize, usize),
    /// The settings bound to a knob in the song, by its controller number.
    Knob(u8),
}

/// The controls bound, and what's being learnt.
#[derive(Debug, Clone, Default)]
pub struct Controls {
    /// In the order they were bound.
    pub bound: Vec<(Control, Target)>,
    learning: Option<Learning>,
    /// The controls down, so a pedal acts once as it goes down, however many messages it sends.
    down: Vec<Control>,
    /// What a knob last turned, so a turn is one undo rather than one for every message.
    turning: Option<Turning>,
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
    /// `none`, or with `this`, a knob to the setting under the cursor; with nothing, stops
    /// learning.
    pub(crate) fn midi_learn(&mut self, target: Option<&str>) -> Result<(), String> {
        let learning = match target.map(str::trim) {
            None | Some("") => {
                self.controls.learning = None;
                self.message = Some("stopped learning".into());
                return Ok(());
            }
            Some("none") => Learning::Bind(None),
            Some("this") => {
                let (t, fx, param) = match self.inst_macro() {
                    Some((t, m)) => (t, Some(MACRO), m as u8),
                    None => self.setting_under_cursor().ok_or(
                        "this is the setting under the cursor: go to one on the mixer (gm), fx \
                         (gf) or instrument (gi) page",
                    )?,
                };
                let id = *self.ids.get(t).ok_or("no such track")?;
                Learning::Knob(id, fx, param)
            }
            Some(text) => Learning::Bind(Some(Target::parse(text)?)),
        };
        self.message = Some(match &learning {
            Learning::Bind(Some(target)) => {
                format!("move a pedal, button, pad or knob to bind it to {target}")
            }
            Learning::Bind(None) => "move a pedal, button, pad or knob to unbind it".into(),
            Learning::Knob(id, fx, param) => {
                let name = (self.track_index(*id))
                    .and_then(|t| self.grid.tracks.get(t))
                    .and_then(|track| Some((track, setting_name(track, *fx, *param)?)))
                    .map_or("it".into(), |(track, setting)| {
                        format!("{}'s {setting}", track.name)
                    });
                format!("move a knob or fader to have it turn {name}")
            }
        });
        self.controls.learning = Some(learning);
        Ok(())
    }

    /// Binds knob `cc` to the setting `fx` and `param` of the track `id`, in the song: in place
    /// of whatever it turned in the song before, and of any other knob on the setting.
    fn learn_knob(&mut self, cc: u8, id: TrackId, fx: Option<u8>, param: u8) -> Vec<Effect> {
        let Some(t) = self.track_index(id) else {
            self.message = Some("the track's gone".into());
            return vec![];
        };
        self.checkpoint();
        let mut effects = self.unbind_knob(cc);
        let Some(track) = self.grid.tracks.get_mut(t) else {
            return effects;
        };
        track.knobs.retain(|k| !k.is(fx, param));
        track.knobs.push(Knob { cc, fx, param });
        let setting = setting_name(track, fx, param).unwrap_or_default();
        let over = match self.controls.target(Control::Cc(cc)) {
            Some(target) => format!(" in this song, over {target}"),
            None => String::new(),
        };
        self.message = Some(format!("cc {cc} turns {}'s {setting}{over}", track.name));
        if !effects.contains(&Effect::TrackChanged(t)) {
            effects.push(Effect::TrackChanged(t));
        }
        effects
    }

    /// Takes knob `cc` off the settings it turns in the song.
    fn unbind_knob(&mut self, cc: u8) -> Vec<Effect> {
        let mut effects = vec![];
        for (t, track) in self.grid.tracks.iter_mut().enumerate() {
            let before = track.knobs.len();
            track.knobs.retain(|k| k.cc != cc);
            if track.knobs.len() != before {
                effects.push(Effect::TrackChanged(t));
            }
        }
        effects
    }

    /// The tracks and settings knob `cc` turns in the song.
    fn knobs_on(&self, cc: u8) -> Vec<(usize, Knob)> {
        (self.grid.tracks.iter().enumerate())
            .flat_map(|(t, track)| track.knobs.iter().map(move |&k| (t, k)))
            .filter(|(_, k)| k.cc == cc)
            .collect()
    }

    /// The knobs bound in the song: each track's name, and the control and setting.
    pub fn knobs(&self) -> Vec<(String, String)> {
        (self.grid.tracks.iter())
            .flat_map(|track| {
                (track.knobs.iter()).map(move |k| {
                    let setting = setting_name(track, k.fx, k.param).unwrap_or_default();
                    (
                        format!("cc {}", k.cc),
                        format!("{}'s {setting}", track.name),
                    )
                })
            })
            .collect()
    }

    /// Turns the settings knob `cc` is bound to in the song to `value`, 0 to 127, through each
    /// one's range; with record armed and playing, locks them on the step playing.
    fn turn_knob(&mut self, cc: u8, knobs: &[(usize, Knob)], value: u8) -> Vec<Effect> {
        let fraction = f32::from(value) / 127.0;
        let mut effects = vec![];
        let mut set = vec![];
        for &(t, knob) in knobs {
            if knob.fx == Some(MACRO) {
                effects.extend(self.turn_macro(t, usize::from(knob.param), fraction));
                continue;
            }
            let Some(track) = self.grid.tracks.get(t) else {
                continue;
            };
            let info = setting_info(
                &track.instrument,
                &track.effects,
                &track.macros,
                knob.fx,
                knob.param,
            );
            let Some(info) = info else {
                continue;
            };
            let value = info.clamp(info.at(fraction));
            let lock = Lock {
                fx: knob.fx,
                param: knob.param,
                value,
                ..Default::default()
            };
            match self.record_lock_of(t, lock) {
                Some(locked) => effects.extend(locked),
                None => set.push((t, knob, value)),
            }
        }
        if set.is_empty() {
            return effects;
        }
        if self.controls.turning != Some(Turning::Knob(cc)) {
            self.checkpoint();
            self.controls.turning = Some(Turning::Knob(cc));
        }
        for (t, knob, value) in set {
            let Some(track) = self.grid.tracks.get_mut(t) else {
                continue;
            };
            match knob.fx {
                None => {
                    track.instrument.set(u32::from(knob.param), value);
                }
                Some(fx) => {
                    let slot = (track.effects.get_mut(usize::from(fx)))
                        .and_then(|fx| fx.values.get_mut(usize::from(knob.param)));
                    if let Some(slot) = slot {
                        *slot = value;
                    }
                }
            }
        }
        effects.push(Effect::MixerChanged);
        effects
    }

    /// A control has moved to `value`, 0 to 127: a note's velocity, or 0 as it comes up.
    /// `None` if it isn't bound, so a note plays as a note.
    pub fn control(&mut self, control: Control, value: u8) -> Option<Vec<Effect>> {
        let down = value >= 64 || matches!(control, Control::Note(_)) && value > 0;
        match self.controls.learning.take() {
            Some(Learning::Knob(id, fx, param)) => {
                let Control::Cc(cc) = control else {
                    // A note isn't a knob: it plays, unless it's bound, and learning goes on.
                    self.controls.learning = Some(Learning::Knob(id, fx, param));
                    if down {
                        self.message = Some(format!(
                            "{control} is a pad or key: move a knob or fader to turn a setting"
                        ));
                    }
                    return self.controls.target(control).map(|_| vec![]);
                };
                return Some(self.learn_knob(cc, id, fx, param));
            }
            // A note coming up, or a pedal let go, isn't a control being moved to learn.
            Some(Learning::Bind(learning))
                if !down && !matches!(learning, Some(Target::Macro(_))) =>
            {
                self.controls.learning = Some(Learning::Bind(learning));
                return self.controls.target(control).map(|_| vec![]);
            }
            Some(Learning::Bind(learning)) => {
                self.message = Some(match &learning {
                    Some(target) => format!("{control} does {target}"),
                    None => format!("{control} does nothing now"),
                });
                let mut effects = vec![];
                // Unbinding takes a knob off the song's settings too.
                if let (None, Control::Cc(cc)) = (&learning, control)
                    && !self.knobs_on(cc).is_empty()
                {
                    self.checkpoint();
                    effects = self.unbind_knob(cc);
                }
                self.controls.bind(control, learning);
                self.controls.down.retain(|&c| c != control);
                if down {
                    self.controls.down.push(control);
                }
                effects.push(Effect::MidiControls(self.controls.entries()));
                return Some(effects);
            }
            None => {}
        }
        // The song's knobs come first.
        if let Control::Cc(cc) = control {
            let knobs = self.knobs_on(cc);
            if !knobs.is_empty() {
                return Some(self.turn_knob(cc, &knobs, value));
            }
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
        let turning = Turning::Macro(track, m);
        if self.recording() || self.controls.turning != Some(turning) {
            let effects = self.set_macro(track, m, value);
            // Only once it's changed is there an undo step to go on in.
            if !effects.is_empty() {
                self.controls.turning = Some(turning);
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

    fn press(e: &mut Editor, s: &str) {
        for c in s.chars() {
            e.handle(match c {
                '\n' => Key::Enter,
                c => Key::Char(c),
            });
        }
    }

    #[test]
    fn a_knob_learns_a_setting_and_turns_it_wherever_the_cursor_is() {
        let mut e = editor();
        e.set_controls(&[("cc 21".into(), "macro 1".into())]);
        // A filter's cutoff, after a delay.
        press(
            &mut e,
            "gf:fx-add delay\n:fx-add filter\njj:midi-learn this\n",
        );
        assert_eq!(
            e.message.as_deref(),
            Some("move a knob or fader to have it turn t's filter.cutoff")
        );
        // A pad isn't a knob, so learning goes on.
        e.control(Control::Note(36), 100);
        assert!(e.grid.tracks[0].knobs.is_empty());
        e.control(Control::Cc(21), 0);
        assert_eq!(
            e.message.as_deref(),
            Some("cc 21 turns t's filter.cutoff in this song, over macro 1")
        );
        let knob = e.grid.tracks[0].knobs[0];
        assert_eq!((knob.cc, knob.fx), (21, Some(1)));
        assert_eq!(e.knobs(), [("cc 21".into(), "t's filter.cutoff".into())]);

        // From the pattern page, a turn is one undo.
        press(&mut e, "gp");
        let track = &e.grid.tracks[0];
        let info =
            setting_info(&track.instrument, &track.effects, &[], knob.fx, knob.param).unwrap();
        let cutoff = |e: &Editor| e.grid.tracks[0].effects[1].values[usize::from(knob.param)];
        let before = cutoff(&e);
        for v in [10, 64, 127] {
            assert_eq!(
                e.control(Control::Cc(21), v),
                Some(vec![Effect::MixerChanged])
            );
        }
        assert_eq!(cutoff(&e), info.max);
        press(&mut e, "u");
        assert_eq!(cutoff(&e), before);

        // Taking the delay out, the knob goes with the filter.
        e.grid.tracks[0].effects.remove(0);
        assert!(e.shift_targets(0, 0, false));
        assert_eq!(e.grid.tracks[0].knobs[0].fx, Some(0));

        // Unbinding takes it off the setting.
        press(&mut e, ":midi-learn none\n");
        e.control(Control::Cc(21), 64);
        assert!(e.grid.tracks[0].knobs.is_empty());
        assert!(e.controls().is_empty());
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
