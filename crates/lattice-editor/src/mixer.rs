//! The mixer page: a strip for each track, then the send and master buses, each with its level
//! (and a track's pan and send) and its chain of effects, and below them the settings of the
//! effect under the cursor.
//!
//! The cursor moves over strips with `h` `l` and over a strip's rows with `j` `k`. `C-a` `C-x`
//! (and `+` `-`) nudge the value under it, as Helix increments numbers, and `c` types one in.

use lattice_core::{Fx, FxKind, MAX_DB, MAX_EFFECTS, MIN_DB};

use crate::keymap::Action;
use crate::{Editor, Effect, Mode, PromptKind};

/// What a nudge moves a level or send by, in dB.
const LEVEL_STEP: f32 = 0.5;
/// What a nudge moves pan by, out of -1 to 1.
const PAN_STEP: f32 = 0.05;

/// Which page the editor shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Pattern,
    Mixer,
}

/// A column of the mixer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strip {
    Track(usize),
    Send,
    Master,
}

impl Strip {
    pub fn is_bus(self) -> bool {
        !matches!(self, Strip::Track(_))
    }
}

/// A row of a strip, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Row {
    #[default]
    Level,
    /// Tracks only.
    Pan,
    /// Tracks only.
    Send,
    /// A slot in the chain; slot 0 is there even when the chain is empty, to add to.
    Effect(usize),
    /// A setting of the effect whose settings are shown, by its id.
    Param(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MixerCursor {
    /// Tracks in order, then the send bus, then the master.
    pub strip: usize,
    pub row: Row,
    /// The effect in the strip's chain whose settings are shown.
    pub effect: usize,
}

impl Editor {
    /// Strips on the mixer page: the tracks, then the send and master buses.
    pub fn strips(&self) -> usize {
        self.grid.tracks.len() + 2
    }

    pub fn strip(&self, index: usize) -> Strip {
        let tracks = self.grid.tracks.len();
        match index {
            i if i < tracks => Strip::Track(i),
            i if i == tracks => Strip::Send,
            _ => Strip::Master,
        }
    }

    pub fn chain(&self, strip: Strip) -> &[Fx] {
        match strip {
            Strip::Track(i) => &self.grid.tracks[i].effects,
            Strip::Send => &self.buses.send.effects,
            Strip::Master => &self.buses.master.effects,
        }
    }

    fn chain_mut(&mut self, strip: Strip) -> &mut Vec<Fx> {
        match strip {
            Strip::Track(i) => &mut self.grid.tracks[i].effects,
            Strip::Send => &mut self.buses.send.effects,
            Strip::Master => &mut self.buses.master.effects,
        }
    }

    /// The mixer cursor, kept within the strips, rows and effects there are now.
    pub fn mixer_cursor(&self) -> MixerCursor {
        let strip = self.mixer.strip.min(self.strips() - 1);
        let kind = self.strip(strip);
        let effects = self.chain(kind).len();
        let mut effect = self.mixer.effect.min(effects.saturating_sub(1));
        let row = match self.mixer.row {
            Row::Pan | Row::Send if kind.is_bus() => Row::Level,
            Row::Effect(i) => {
                let i = i.min(effects.saturating_sub(1));
                effect = i;
                Row::Effect(i)
            }
            Row::Param(p) => match self.chain(kind).get(effect) {
                Some(fx) => Row::Param(p.min(fx.params().len() - 1)),
                None => Row::Effect(0),
            },
            row => row,
        };
        MixerCursor { strip, row, effect }
    }

    /// The effect whose settings are shown under the strips, if the cursor's strip has any.
    pub fn shown_effect(&self) -> Option<(Strip, usize, &Fx)> {
        let cursor = self.mixer_cursor();
        let strip = self.strip(cursor.strip);
        let fx = self.chain(strip).get(cursor.effect)?;
        Some((strip, cursor.effect, fx))
    }

    /// The rows the cursor can be on in a strip, top to bottom, with the settings of `effect`.
    fn rows(&self, strip: Strip, effect: usize) -> Vec<Row> {
        let mut rows = vec![Row::Level];
        if !strip.is_bus() {
            rows.extend([Row::Pan, Row::Send]);
        }
        let chain = self.chain(strip);
        rows.extend((0..chain.len().max(1)).map(Row::Effect));
        if let Some(fx) = chain.get(effect) {
            rows.extend((0..fx.params().len()).map(Row::Param));
        }
        rows
    }

    /// Moves the cursor `by` rows, or to the first or last.
    fn move_row(&mut self, to: impl FnOnce(usize, usize) -> usize) {
        let mut cursor = self.mixer_cursor();
        let rows = self.rows(self.strip(cursor.strip), cursor.effect);
        let at = rows.iter().position(|&r| r == cursor.row).unwrap_or(0);
        cursor.row = rows[to(at, rows.len() - 1).min(rows.len() - 1)];
        if let Row::Effect(i) = cursor.row {
            cursor.effect = i;
        }
        self.mixer = cursor;
    }

    fn move_strip(&mut self, to: impl FnOnce(usize, usize) -> usize) {
        let mut cursor = self.mixer_cursor();
        cursor.strip = to(cursor.strip, self.strips() - 1).min(self.strips() - 1);
        self.mixer = cursor;
        self.mixer = self.mixer_cursor();
    }

    /// Handles a key's action on the mixer page. `None` for the actions both pages share.
    pub(crate) fn mixer_act(&mut self, action: Action, n: usize) -> Option<Vec<Effect>> {
        let by = n as i32;
        match action {
            Action::Left => self.move_strip(|s, _| s.saturating_sub(n)),
            Action::Right => self.move_strip(|s, _| s + n),
            Action::FirstTrack => self.move_strip(|_, _| 0),
            Action::LastTrack => self.move_strip(|_, last| last),
            Action::Up => self.move_row(|r, _| r.saturating_sub(n)),
            Action::Down => self.move_row(|r, _| r + n),
            Action::FirstStep => self.move_row(|_, _| 0),
            Action::LastStep => self.move_row(|_, last| last),
            Action::NoteUp | Action::Longer => return Some(self.nudge(by)),
            Action::NoteDown | Action::Shorter => return Some(self.nudge(-by)),
            Action::Change => {
                self.open_prompt(PromptKind::Command);
                if let Mode::Command(prompt) = &mut self.mode {
                    prompt.line = "set ".into();
                    prompt.typed = prompt.line.clone();
                }
            }
            Action::Delete | Action::Clear => return Some(self.remove_effect()),
            Action::Undo
            | Action::Redo
            | Action::CommandLine
            | Action::TogglePlay
            | Action::Escape
            | Action::Quit
            | Action::ToggleMute
            | Action::ToggleSolo
            | Action::GotoMixer
            | Action::GotoPattern => return None,
            // Pattern editing has nothing to act on here.
            _ => {}
        }
        Some(vec![])
    }

    /// The value under the mixer cursor, and the range it can take, in the units `:set` takes.
    fn cursor_value(&self) -> Option<(f32, f32, f32)> {
        let cursor = self.mixer_cursor();
        let strip = self.strip(cursor.strip);
        let track = match strip {
            Strip::Track(i) => Some(&self.grid.tracks[i].mix),
            _ => None,
        };
        let bus = match strip {
            Strip::Send => Some(&self.buses.send),
            Strip::Master => Some(&self.buses.master),
            Strip::Track(_) => None,
        };
        match cursor.row {
            Row::Level => {
                let level = track.map_or_else(|| bus.map_or(0.0, |b| b.level), |m| m.level);
                Some((level, MIN_DB, MAX_DB))
            }
            Row::Pan => Some((track?.pan * 100.0, -100.0, 100.0)),
            Row::Send => Some((track?.send, MIN_DB, MAX_DB)),
            Row::Effect(_) => None,
            Row::Param(p) => {
                let (_, _, fx) = self.shown_effect()?;
                let info = fx.params()[p];
                Some((fx.values[p], info.min, info.max))
            }
        }
    }

    /// Sets the value under the mixer cursor, in the units [`Editor::cursor_value`] gives.
    pub(crate) fn set_mixer_value(&mut self, value: f32) -> Result<Vec<Effect>, String> {
        if self.page != Page::Mixer {
            return Err("set works on the mixer page (gm)".into());
        }
        let Some((old, min, max)) = self.cursor_value() else {
            return Err("nothing to set here: move onto a level or setting".into());
        };
        if !(min..=max).contains(&value) {
            return Err(format!("this goes from {min} to {max}"));
        }
        if value == old {
            return Ok(vec![]);
        }
        self.checkpoint();
        let cursor = self.mixer_cursor();
        let strip = self.strip(cursor.strip);
        match (cursor.row, strip) {
            (Row::Level, Strip::Track(i)) => self.grid.tracks[i].mix.level = value,
            (Row::Level, Strip::Send) => self.buses.send.level = value,
            (Row::Level, Strip::Master) => self.buses.master.level = value,
            (Row::Pan, Strip::Track(i)) => self.grid.tracks[i].mix.pan = value / 100.0,
            (Row::Send, Strip::Track(i)) => self.grid.tracks[i].mix.send = value,
            (Row::Param(p), _) => self.chain_mut(strip)[cursor.effect].values[p] = value,
            _ => {}
        }
        Ok(match (cursor.row, strip) {
            (Row::Level | Row::Pan | Row::Send, Strip::Track(i)) => vec![Effect::TrackChanged(i)],
            _ => vec![Effect::MixerChanged],
        })
    }

    /// Moves the value under the mixer cursor `by` steps.
    fn nudge(&mut self, by: i32) -> Vec<Effect> {
        let cursor = self.mixer_cursor();
        let Some((old, min, max)) = self.cursor_value() else {
            return vec![];
        };
        let new = match cursor.row {
            Row::Level | Row::Send => old + by as f32 * LEVEL_STEP,
            // Kept to whole percent, so repeated nudges don't drift.
            Row::Pan => (old + by as f32 * PAN_STEP * 100.0).round(),
            Row::Param(p) => match self.shown_effect() {
                Some((_, _, fx)) => fx.params()[p].nudge(old, by),
                None => return vec![],
            },
            Row::Effect(_) => return vec![],
        };
        self.set_mixer_value(new.clamp(min, max))
            .unwrap_or_default()
    }

    /// Adds an effect after the one under the mixer cursor, or at the end of the cursor's
    /// track's chain on the pattern page, and moves onto it.
    pub(crate) fn add_effect(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        let kind = FxKind::named(name).ok_or_else(|| {
            let names: Vec<&str> = FxKind::ALL.iter().map(|k| k.name()).collect();
            format!("no effect called {name:?}; there's {}", names.join(", "))
        })?;
        let cursor = self.mixer_cursor();
        let (strip, index) = match self.page {
            Page::Mixer => {
                let strip = self.strip(cursor.strip);
                let len = self.chain(strip).len();
                let index = match cursor.row {
                    _ if len == 0 => 0,
                    Row::Effect(_) | Row::Param(_) => cursor.effect + 1,
                    _ => len,
                };
                (strip, index)
            }
            Page::Pattern => {
                let track = self.cursor().track;
                (Strip::Track(track), self.grid.tracks[track].effects.len())
            }
        };
        if self.chain(strip).len() >= MAX_EFFECTS {
            return Err(format!("at most {MAX_EFFECTS} effects in a chain"));
        }
        self.checkpoint();
        self.chain_mut(strip).insert(index, Fx::new(kind));
        if self.page == Page::Mixer {
            self.mixer.row = Row::Effect(index);
            self.mixer.effect = index;
        }
        Ok(vec![Effect::MixerChanged])
    }

    /// Removes the effect under the mixer cursor.
    fn remove_effect(&mut self) -> Vec<Effect> {
        let cursor = self.mixer_cursor();
        let strip = self.strip(cursor.strip);
        if !matches!(cursor.row, Row::Effect(_)) || self.chain(strip).is_empty() {
            self.message = Some("move onto an effect to remove it".into());
            return vec![];
        }
        self.checkpoint();
        self.chain_mut(strip).remove(cursor.effect);
        self.mixer = self.mixer_cursor();
        vec![Effect::MixerChanged]
    }

    /// The tracks `:level`, `:pan`, `:send`, `:mute` and `:solo` change: on the mixer page, the
    /// cursor's strip; on the pattern page, every track the selections reach.
    pub(crate) fn mix_targets(&self) -> Result<Vec<usize>, String> {
        if self.page == Page::Pattern {
            return Ok(self.selected_tracks());
        }
        match self.strip(self.mixer_cursor().strip) {
            Strip::Track(i) => Ok(vec![i]),
            _ => Err("buses only have a level and effects".into()),
        }
    }

    /// The bus under the mixer cursor, for `:level`.
    pub(crate) fn bus_under_cursor(&self) -> Option<Strip> {
        let strip = self.strip(self.mixer_cursor().strip);
        (self.page == Page::Mixer && strip.is_bus()).then_some(strip)
    }

    pub(crate) fn set_bus_level(&mut self, bus: Strip, level: f32) -> Vec<Effect> {
        self.checkpoint();
        match bus {
            Strip::Send => self.buses.send.level = level,
            _ => self.buses.master.level = level,
        }
        vec![Effect::MixerChanged]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;
    use lattice_core::fx::delay::{FEEDBACK, TIME};
    use lattice_core::{Bus, Buses, Grid, Step, Track};

    fn editor() -> Editor {
        let track = |name: &str| Track {
            name: name.into(),
            steps: vec![Step::default(); 16],
            ..Track::default()
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![track("a"), track("b")],
        };
        let buses = Buses {
            send: Bus {
                effects: vec![Fx::new(FxKind::Delay)],
                ..Bus::default()
            },
            ..Buses::default()
        };
        let mut e = Editor::new(grid, 120.0).with_buses(buses);
        keys(&mut e, "gm");
        e
    }

    fn keys(e: &mut Editor, s: &str) -> Vec<Effect> {
        s.chars().flat_map(|c| e.handle(key(c))).collect()
    }

    fn key(c: char) -> Key {
        match c {
            'A' => Key::Ctrl('a'),
            'X' => Key::Ctrl('x'),
            '\n' => Key::Enter,
            c => Key::Char(c),
        }
    }

    fn command(e: &mut Editor, line: &str) -> Vec<Effect> {
        keys(e, &format!(":{line}\n"))
    }

    fn at(e: &Editor) -> (Strip, Row) {
        let cursor = e.mixer_cursor();
        (e.strip(cursor.strip), cursor.row)
    }

    #[test]
    fn the_cursor_walks_strips_and_rows() {
        let mut e = editor();
        assert_eq!(e.page, Page::Mixer);
        assert_eq!(at(&e), (Strip::Track(0), Row::Level));
        keys(&mut e, "jjj");
        assert_eq!(at(&e), (Strip::Track(0), Row::Effect(0)), "an empty slot");
        keys(&mut e, "j");
        assert_eq!(at(&e).1, Row::Effect(0), "nothing below it");
        keys(&mut e, "ll");
        assert_eq!(at(&e), (Strip::Send, Row::Effect(0)), "on the delay");
        keys(&mut e, "jj");
        assert_eq!(at(&e), (Strip::Send, Row::Param(FEEDBACK as usize)));
        let (strip, slot, fx) = e.shown_effect().unwrap();
        assert_eq!((strip, slot, fx.kind), (Strip::Send, 0, FxKind::Delay));
        keys(&mut e, "h");
        assert_eq!(
            at(&e),
            (Strip::Track(1), Row::Effect(0)),
            "no settings there"
        );
        keys(&mut e, "glgg");
        assert_eq!(at(&e), (Strip::Master, Row::Level));
        keys(&mut e, "gp");
        assert_eq!(e.page, Page::Pattern);
    }

    #[test]
    fn nudging_and_setting_values() {
        let mut e = editor();
        assert_eq!(keys(&mut e, "A"), [Effect::TrackChanged(0)]);
        assert_eq!(e.grid.tracks[0].mix.level, 0.5);
        keys(&mut e, "4X");
        assert_eq!(e.grid.tracks[0].mix.level, -1.5);
        keys(&mut e, "j3-");
        assert_eq!(e.grid.tracks[0].mix.pan, -0.15);
        keys(&mut e, "c-40\n");
        assert_eq!(e.grid.tracks[0].mix.pan, -0.4);
        keys(&mut e, "u");
        assert_eq!(e.grid.tracks[0].mix.pan, -0.15, "undoable");

        // The send bus's level, then the delay's feedback.
        keys(&mut e, "ggll");
        assert_eq!(keys(&mut e, "X"), [Effect::MixerChanged]);
        assert_eq!(e.buses.send.level, -0.5);
        assert_eq!(command(&mut e, "level -12"), [Effect::MixerChanged]);
        assert_eq!(e.buses.send.level, -12.0);
        keys(&mut e, "jj");
        assert_eq!(at(&e).1, Row::Param(TIME as usize));
        command(&mut e, "set 0.5");
        assert_eq!(e.buses.send.effects[0].values[TIME as usize], 0.5);
        command(&mut e, "set 9");
        assert_eq!(e.message.as_deref(), Some("this goes from 0.01 to 2"));
        assert!(e.modified());
    }

    #[test]
    fn effects_are_added_and_removed() {
        let mut e = editor();
        keys(&mut e, "jjj");
        assert_eq!(command(&mut e, "fx-add delay"), [Effect::MixerChanged]);
        command(&mut e, "fx-add delay");
        assert_eq!(e.grid.tracks[0].effects.len(), 2);
        assert_eq!(at(&e), (Strip::Track(0), Row::Effect(1)), "on the new one");
        command(&mut e, "fx-add reverb");
        assert!(e.message.as_deref().unwrap().contains("there's delay"));

        keys(&mut e, "k");
        assert_eq!(keys(&mut e, "d"), [Effect::MixerChanged]);
        assert_eq!(e.grid.tracks[0].effects.len(), 1);
        keys(&mut e, "dd");
        assert!(e.grid.tracks[0].effects.is_empty());
        assert_eq!(
            e.message.as_deref(),
            Some("move onto an effect to remove it")
        );
        keys(&mut e, "uu");
        assert_eq!(e.grid.tracks[0].effects.len(), 2);

        // On the pattern page, it goes at the end of the cursor's track's chain.
        keys(&mut e, "gpl");
        command(&mut e, "fx-add delay");
        assert_eq!(e.grid.tracks[1].effects.len(), 1);
    }

    #[test]
    fn mix_commands_follow_the_mixer_cursor() {
        let mut e = editor();
        keys(&mut e, "l");
        assert_eq!(keys(&mut e, "tm"), [Effect::TrackChanged(1)]);
        assert!(e.grid.tracks[1].mix.mute && !e.grid.tracks[0].mix.mute);
        keys(&mut e, "l");
        command(&mut e, "pan 20");
        assert_eq!(
            e.message.as_deref(),
            Some("buses only have a level and effects")
        );
        // Pattern keys do nothing here.
        assert!(keys(&mut e, "ix%").is_empty());
        assert_eq!(e.mode, Mode::Normal);
    }
}
