//! The mixer page, in two halves. Above, the overview: a strip for each track, then the groups'
//! buses and the send and master buses, each with its level, pan and meter. `h` `l` move between
//! them and `C-a` `C-x` set the level, so a balance is a matter of moving sideways. Below, the
//! detail of the strip under the cursor: where it goes, its mix (level, pan, send and choke), its
//! chain from instrument to effects, and the settings of the device on the chain under the
//! cursor, always shown, in a grid.
//!
//! `j` `k` move from the overview down through the detail's rows; there, `h` `l` move along the
//! row, and `(` `)` change strip. `tab` jumps between the overview and the chain, and `gi` `gf`
//! jump to the instrument and the effects, from the pattern page too. `C-a` `C-x` (and `+` `-`)
//! nudge the value under the cursor, as Helix increments numbers, and `c` types one in.

use lattice_core::{
    Bus, Fx, FxKind, InstKind, Instrument, Kind, MAX_CHOKE, MAX_DB, MAX_EFFECTS, MIN_DB, ParamInfo,
};

use crate::keymap::Action;
use crate::{Editor, Effect, Mode, PromptKind};

/// What a nudge moves a level or send by, in dB.
const LEVEL_STEP: f32 = 0.5;
/// What a nudge moves pan by, out of -1 to 1.
const PAN_STEP: f32 = 0.05;
/// Columns in the grid of settings, filled a row at a time.
pub const PARAM_COLUMNS: usize = 3;

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
    /// A group's bus, by its index in the grid's groups.
    Group(usize),
    Send,
    Master,
}

impl Strip {
    pub fn is_bus(self) -> bool {
        !matches!(self, Strip::Track(_))
    }

    /// The mix fields it has, left to right.
    pub fn fields(self) -> &'static [Field] {
        if self.is_bus() {
            &[Field::Level]
        } else {
            &[Field::Level, Field::Pan, Field::Send, Field::Choke]
        }
    }
}

/// A value on the detail's mix row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Level,
    /// Tracks only.
    Pan,
    /// Tracks only.
    Send,
    /// Tracks only: the choke group, 0 for none.
    Choke,
}

/// Where the mixer cursor is, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Row {
    /// The overview, on a strip's level.
    #[default]
    Strips,
    /// The detail's mix row.
    Mix(Field),
    /// The detail's chain: the instrument or effect the cursor is on.
    Chain,
    /// A setting of the device on the chain, by its id.
    Param(usize),
}

/// The instrument or effect whose settings are shown in the detail.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shown<'a> {
    pub strip: Strip,
    /// The effect's slot in the chain, or `None` for the track's instrument.
    pub slot: Option<usize>,
    pub name: &'static str,
    pub params: &'static [ParamInfo],
    pub values: &'a [f32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixerCursor {
    /// Tracks in order, then the groups' buses, then the send bus, then the master.
    pub strip: usize,
    pub row: Row,
    /// The device on the strip's chain whose settings are shown: the track's instrument, or
    /// this effect.
    pub effect: usize,
    pub instrument: bool,
}

impl Default for MixerCursor {
    fn default() -> Self {
        Self {
            strip: 0,
            row: Row::Strips,
            effect: 0,
            instrument: true,
        }
    }
}

impl Editor {
    /// Strips on the mixer page: the tracks, the groups' buses, then the send and master buses.
    pub fn strips(&self) -> usize {
        self.grid.tracks.len() + self.grid.groups.len() + 2
    }

    pub fn strip(&self, index: usize) -> Strip {
        let tracks = self.grid.tracks.len();
        let groups = self.grid.groups.len();
        match index {
            i if i < tracks => Strip::Track(i),
            i if i < tracks + groups => Strip::Group(i - tracks),
            i if i == tracks + groups => Strip::Send,
            _ => Strip::Master,
        }
    }

    /// The bus behind a bus's strip.
    pub fn bus(&self, strip: Strip) -> Option<&Bus> {
        match strip {
            Strip::Track(_) => None,
            Strip::Group(g) => Some(&self.grid.groups[g].bus),
            Strip::Send => Some(&self.buses.send),
            Strip::Master => Some(&self.buses.master),
        }
    }

    fn bus_mut(&mut self, strip: Strip) -> Option<&mut Bus> {
        match strip {
            Strip::Track(_) => None,
            Strip::Group(g) => Some(&mut self.grid.groups[g].bus),
            Strip::Send => Some(&mut self.buses.send),
            Strip::Master => Some(&mut self.buses.master),
        }
    }

    pub fn chain(&self, strip: Strip) -> &[Fx] {
        match strip {
            Strip::Track(i) => &self.grid.tracks[i].effects,
            _ => &self.bus(strip).expect("a bus").effects,
        }
    }

    fn chain_mut(&mut self, strip: Strip) -> &mut Vec<Fx> {
        match strip {
            Strip::Track(i) => &mut self.grid.tracks[i].effects,
            _ => &mut self.bus_mut(strip).expect("a bus").effects,
        }
    }

    /// The track's instrument, for a track's strip.
    pub fn instrument(&self, strip: Strip) -> Option<&Instrument> {
        match strip {
            Strip::Track(i) => Some(&self.grid.tracks[i].instrument),
            _ => None,
        }
    }

    /// A strip's level, in dB.
    pub fn strip_level(&self, strip: Strip) -> f32 {
        match strip {
            Strip::Track(i) => self.grid.tracks[i].mix.level,
            bus => self.bus(bus).map_or(0.0, |b| b.level),
        }
    }

    /// The mixer cursor, kept within the strips, fields, effects and settings there are now.
    pub fn mixer_cursor(&self) -> MixerCursor {
        let strip = self.mixer.strip.min(self.strips() - 1);
        let kind = self.strip(strip);
        let effects = self.chain(kind).len();
        let effect = self.mixer.effect.min(effects.saturating_sub(1));
        // A track with no effects shows its instrument.
        let instrument = !kind.is_bus() && (self.mixer.instrument || effects == 0);
        let mut cursor = MixerCursor {
            strip,
            row: Row::Strips,
            effect,
            instrument,
        };
        cursor.row = match self.mixer.row {
            Row::Mix(field) if !kind.fields().contains(&field) => Row::Mix(Field::Level),
            Row::Param(p) => match self.shown_at(cursor) {
                Some(shown) => Row::Param(p.min(shown.params.len() - 1)),
                None => Row::Chain,
            },
            row => row,
        };
        cursor
    }

    /// The instrument or effect whose settings are shown in the detail, if the cursor's strip
    /// has one: the one on the chain the cursor is on, or was last on in this strip.
    pub fn shown(&self) -> Option<Shown<'_>> {
        self.shown_at(self.mixer_cursor())
    }

    fn shown_at(&self, cursor: MixerCursor) -> Option<Shown<'_>> {
        let strip = self.strip(cursor.strip);
        if cursor.instrument {
            let instrument = self.instrument(strip)?;
            return Some(Shown {
                strip,
                slot: None,
                name: instrument.kind.name(),
                params: instrument.params(),
                values: &instrument.values,
            });
        }
        let fx = self.chain(strip).get(cursor.effect)?;
        Some(Shown {
            strip,
            slot: Some(cursor.effect),
            name: fx.kind.name(),
            params: fx.params(),
            values: &fx.values,
        })
    }

    /// The device under `cursor` on its strip's chain, counting the instrument as the first, and
    /// how many there are.
    fn chain_position(&self, cursor: MixerCursor) -> (usize, usize) {
        let strip = self.strip(cursor.strip);
        let first = usize::from(!strip.is_bus());
        let len = first + self.chain(strip).len();
        let at = if cursor.instrument {
            0
        } else {
            first + cursor.effect
        };
        (at, len)
    }

    fn set_chain_position(&self, cursor: &mut MixerCursor, at: usize) {
        let strip = self.strip(cursor.strip);
        if strip.is_bus() {
            cursor.effect = at;
        } else {
            cursor.instrument = at == 0;
            cursor.effect = at.saturating_sub(1);
        }
    }

    /// One row down: from the overview to the mix, the chain, then down the settings.
    fn row_down(&self, mut cursor: MixerCursor) -> MixerCursor {
        let params = self.shown_at(cursor).map_or(0, |s| s.params.len());
        cursor.row = match cursor.row {
            Row::Strips => Row::Mix(Field::Level),
            Row::Mix(_) => Row::Chain,
            Row::Chain if params > 0 => Row::Param(0),
            Row::Param(p) if p + PARAM_COLUMNS < params => Row::Param(p + PARAM_COLUMNS),
            // Onto the last row's last setting, if this column ends a row higher.
            Row::Param(p) if p / PARAM_COLUMNS < (params - 1) / PARAM_COLUMNS => {
                Row::Param(params - 1)
            }
            row => row,
        };
        cursor
    }

    fn row_up(&self, mut cursor: MixerCursor) -> MixerCursor {
        cursor.row = match cursor.row {
            Row::Strips | Row::Mix(_) => Row::Strips,
            Row::Chain => Row::Mix(Field::Level),
            Row::Param(p) if p >= PARAM_COLUMNS => Row::Param(p - PARAM_COLUMNS),
            Row::Param(_) => Row::Chain,
        };
        cursor
    }

    /// One step along the row: strips in the overview, fields, devices or settings below.
    fn along(&self, mut cursor: MixerCursor, right: bool) -> MixerCursor {
        let step = |at: usize, len: usize| {
            if right {
                (at + 1).min(len.saturating_sub(1))
            } else {
                at.saturating_sub(1)
            }
        };
        match cursor.row {
            Row::Strips => cursor.strip = step(cursor.strip, self.strips()),
            Row::Mix(field) => {
                let fields = self.strip(cursor.strip).fields();
                let at = fields.iter().position(|&f| f == field).unwrap_or(0);
                cursor.row = Row::Mix(fields[step(at, fields.len())]);
            }
            Row::Chain => {
                let (at, len) = self.chain_position(cursor);
                self.set_chain_position(&mut cursor, step(at, len));
            }
            Row::Param(p) => {
                let params = self.shown_at(cursor).map_or(0, |s| s.params.len());
                let row = p / PARAM_COLUMNS * PARAM_COLUMNS;
                let column = step(p - row, PARAM_COLUMNS);
                cursor.row = Row::Param((row + column).min(params.saturating_sub(1)));
            }
        }
        cursor
    }

    fn move_mixer(&mut self, n: usize, mv: impl Fn(&Self, MixerCursor) -> MixerCursor) {
        let mut cursor = self.mixer_cursor();
        for _ in 0..n {
            cursor = mv(self, cursor);
        }
        self.mixer = cursor;
        self.mixer = self.mixer_cursor();
    }

    /// Changes strip, keeping to the same row where the new strip has it.
    fn move_strip(&mut self, to: impl FnOnce(usize, usize) -> usize) {
        let mut cursor = self.mixer_cursor();
        cursor.strip = to(cursor.strip, self.strips() - 1).min(self.strips() - 1);
        self.mixer = cursor;
        self.mixer = self.mixer_cursor();
    }

    /// `gi` and `gf`: opens the mixer page on the pattern cursor's track, if coming from the
    /// pattern, at the chain: on the instrument, or the first effect.
    pub(crate) fn goto_chain(&mut self, instrument: bool) {
        if self.page == Page::Pattern {
            self.mixer.strip = self.cursor().track;
            self.page = Page::Mixer;
            self.mode = Mode::Normal;
        }
        let mut cursor = self.mixer_cursor();
        cursor.row = Row::Chain;
        cursor.instrument = instrument;
        cursor.effect = 0;
        self.mixer = cursor;
        self.mixer = self.mixer_cursor();
        if instrument && !self.mixer.instrument {
            self.message = Some("buses have no instrument".into());
        }
    }

    /// Handles a key's action on the mixer page. `None` for the actions both pages share.
    pub(crate) fn mixer_act(&mut self, action: Action, n: usize) -> Option<Vec<Effect>> {
        let by = n as i32;
        match action {
            Action::Left => self.move_mixer(n, |e, c| e.along(c, false)),
            Action::Right => self.move_mixer(n, |e, c| e.along(c, true)),
            Action::Up => self.move_mixer(n, Self::row_up),
            Action::Down => self.move_mixer(n, Self::row_down),
            Action::RotateBackward => self.move_strip(|s, _| s.saturating_sub(n)),
            Action::RotateForward => self.move_strip(|s, _| s + n),
            Action::FirstTrack => self.move_strip(|_, _| 0),
            Action::LastTrack => self.move_strip(|_, last| last),
            Action::FirstStep => self.mixer.row = Row::Strips,
            Action::LastStep => {
                self.mixer.row = match self.shown() {
                    Some(shown) => Row::Param(shown.params.len() - 1),
                    None => Row::Chain,
                };
            }
            Action::ToggleView => {
                self.mixer.row = match self.mixer_cursor().row {
                    Row::Strips => Row::Chain,
                    _ => Row::Strips,
                };
            }
            Action::NoteUp | Action::Longer => return Some(self.nudge(by)),
            Action::NoteDown | Action::Shorter => return Some(self.nudge(-by)),
            Action::Change => {
                // On the instrument, choose another; anywhere else, type a value.
                let cursor = self.mixer_cursor();
                let line = match cursor.row {
                    Row::Chain if cursor.instrument => "inst ",
                    _ => "set ",
                };
                self.open_prompt(PromptKind::Command);
                if let Mode::Command(prompt) = &mut self.mode {
                    prompt.line = line.into();
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
            | Action::GotoPattern
            | Action::GotoInstrument
            | Action::GotoEffects => return None,
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
            Strip::Track(i) => Some(&self.grid.tracks[i]),
            _ => None,
        };
        match cursor.row {
            Row::Strips | Row::Mix(Field::Level) => Some((self.strip_level(strip), MIN_DB, MAX_DB)),
            Row::Mix(Field::Pan) => Some((track?.mix.pan * 100.0, -100.0, 100.0)),
            Row::Mix(Field::Send) => Some((track?.mix.send, MIN_DB, MAX_DB)),
            Row::Mix(Field::Choke) => Some((f32::from(track?.choke), 0.0, f32::from(MAX_CHOKE))),
            Row::Chain => None,
            Row::Param(p) => {
                let shown = self.shown()?;
                let info = shown.params[p];
                Some((shown.values[p], info.min, info.max))
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
        let row = match cursor.row {
            Row::Strips => Row::Mix(Field::Level),
            row => row,
        };
        match (row, strip) {
            (Row::Mix(field), Strip::Track(i)) => {
                let track = &mut self.grid.tracks[i];
                match field {
                    Field::Level => track.mix.level = value,
                    Field::Pan => track.mix.pan = value / 100.0,
                    Field::Send => track.mix.send = value,
                    Field::Choke => track.choke = value.round() as u8,
                }
                return Ok(vec![Effect::TrackChanged(i)]);
            }
            (Row::Mix(_), bus) => {
                if let Some(bus) = self.bus_mut(bus) {
                    bus.level = value;
                }
            }
            (Row::Param(p), Strip::Track(i)) if cursor.instrument => {
                let info = self.grid.tracks[i].instrument.params()[p];
                self.grid.tracks[i].instrument.values[p] = info.clamp(value);
            }
            (Row::Param(p), _) => self.chain_mut(strip)[cursor.effect].values[p] = value,
            _ => {}
        }
        Ok(vec![Effect::MixerChanged])
    }

    /// Moves the value under the mixer cursor `by` steps.
    fn nudge(&mut self, by: i32) -> Vec<Effect> {
        let cursor = self.mixer_cursor();
        let Some((old, min, max)) = self.cursor_value() else {
            return vec![];
        };
        let new = match cursor.row {
            Row::Strips | Row::Mix(Field::Level | Field::Send) => old + by as f32 * LEVEL_STEP,
            // Kept to whole percent, so repeated nudges don't drift.
            Row::Mix(Field::Pan) => (old + by as f32 * PAN_STEP * 100.0).round(),
            Row::Mix(Field::Choke) => old + by as f32,
            Row::Param(p) => match self.shown() {
                Some(shown) => shown.params[p].nudge(old, by),
                None => return vec![],
            },
            Row::Chain => return vec![],
        };
        self.set_mixer_value(new.clamp(min, max))
            .unwrap_or_default()
    }

    /// Adds an effect after the device under the mixer cursor (the instrument counts as first),
    /// or at the end of the cursor's track's chain on the pattern page, and moves onto it.
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
                    Row::Chain | Row::Param(_) if cursor.instrument => 0,
                    Row::Chain | Row::Param(_) => cursor.effect + 1,
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
            self.mixer.row = Row::Chain;
            self.mixer.effect = index;
            self.mixer.instrument = false;
        }
        Ok(vec![Effect::MixerChanged])
    }

    /// Puts a new instrument, at its default settings, on the tracks `:inst` changes. Tracks
    /// that already have that kind keep their settings.
    pub(crate) fn set_instrument(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        let kind = InstKind::named(name).ok_or_else(|| {
            let names: Vec<&str> = InstKind::ALL.iter().map(|k| k.name()).collect();
            format!(
                "no instrument called {name:?}; there's {}",
                names.join(", ")
            )
        })?;
        let tracks: Vec<usize> = self
            .mix_targets()?
            .into_iter()
            .filter(|&t| self.grid.tracks[t].instrument.kind != kind)
            .collect();
        if tracks.is_empty() {
            return Ok(vec![]);
        }
        self.checkpoint();
        for t in tracks {
            self.grid.tracks[t].instrument = Instrument::new(kind);
        }
        if self.page == Page::Mixer {
            self.mixer.row = Row::Chain;
            self.mixer.instrument = true;
        }
        Ok(vec![Effect::MixerChanged])
    }

    /// Removes the effect under the mixer cursor.
    fn remove_effect(&mut self) -> Vec<Effect> {
        let cursor = self.mixer_cursor();
        let strip = self.strip(cursor.strip);
        if cursor.row != Row::Chain || cursor.instrument || self.chain(strip).is_empty() {
            self.message = Some("move onto an effect on the chain to remove it".into());
            return vec![];
        }
        self.checkpoint();
        self.chain_mut(strip).remove(cursor.effect);
        self.mixer = self.mixer_cursor();
        vec![Effect::MixerChanged]
    }

    /// The tracks `:level`, `:pan`, `:send`, `:mute`, `:solo` and `:inst` change: on the mixer
    /// page, the cursor's strip; on the pattern page, every track the selections reach.
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
        if let Some(bus) = self.bus_mut(bus) {
            bus.level = level;
        }
        vec![Effect::MixerChanged]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;
    use lattice_core::fx::delay::{FEEDBACK, TIME};
    use lattice_core::instrument::kick;
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
            groups: vec![],
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
            '\t' => Key::Tab,
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
    fn the_cursor_walks_the_overview_and_the_detail() {
        let mut e = editor();
        assert_eq!(e.page, Page::Mixer);
        assert_eq!(at(&e), (Strip::Track(0), Row::Strips));
        assert_eq!(
            e.shown().unwrap().name,
            "synth",
            "the instrument's settings show"
        );
        keys(&mut e, "ll");
        assert_eq!(at(&e), (Strip::Send, Row::Strips));
        assert_eq!(e.shown().unwrap().name, "delay");

        // Down into the detail, along the mix row, and back out.
        keys(&mut e, "hhj");
        assert_eq!(at(&e), (Strip::Track(0), Row::Mix(Field::Level)));
        keys(&mut e, "lll");
        assert_eq!(at(&e).1, Row::Mix(Field::Choke));
        assert_eq!(command(&mut e, "set 3"), [Effect::TrackChanged(0)]);
        assert_eq!(e.grid.tracks[0].choke, 3);
        command(&mut e, "set 9");
        assert_eq!(e.message.as_deref(), Some("this goes from 0 to 8"));
        keys(&mut e, "l");
        assert_eq!(at(&e).1, Row::Mix(Field::Choke), "nothing past it");

        // `(` `)` change strip and keep the row where they can; a bus has only a level.
        keys(&mut e, ")");
        assert_eq!(at(&e), (Strip::Track(1), Row::Mix(Field::Choke)));
        keys(&mut e, ")");
        assert_eq!(at(&e), (Strip::Send, Row::Mix(Field::Level)));

        // The chain, then a grid of settings: the synth has nine, in rows of three.
        keys(&mut e, "((j");
        assert_eq!(at(&e), (Strip::Track(0), Row::Chain));
        keys(&mut e, "j");
        assert_eq!(at(&e).1, Row::Param(0));
        keys(&mut e, "lj");
        assert_eq!(at(&e).1, Row::Param(1 + PARAM_COLUMNS));
        keys(&mut e, "llll");
        assert_eq!(
            at(&e).1,
            Row::Param(2 * PARAM_COLUMNS - 1),
            "within its row"
        );
        keys(&mut e, "9j");
        assert_eq!(at(&e).1, Row::Param(8), "the last");
        keys(&mut e, "gg");
        assert_eq!(at(&e).1, Row::Strips);
        keys(&mut e, "ge");
        assert_eq!(at(&e).1, Row::Param(8));
        keys(&mut e, "kkkk");
        assert_eq!(at(&e).1, Row::Mix(Field::Level));

        // Tab jumps between the overview and the chain.
        keys(&mut e, "\t");
        assert_eq!(at(&e).1, Row::Strips);
        keys(&mut e, "\t");
        assert_eq!(at(&e).1, Row::Chain);
        keys(&mut e, "glgg");
        assert_eq!(at(&e), (Strip::Master, Row::Strips));
        assert!(e.shown().is_none(), "nothing on its chain");
        keys(&mut e, "gp");
        assert_eq!(e.page, Page::Pattern);
    }

    #[test]
    fn gi_gf_and_gm_open_the_mixer_on_the_cursors_track() {
        let mut e = editor();
        keys(&mut e, "gpl");
        command(&mut e, "fx-add delay");
        keys(&mut e, "gi");
        assert_eq!(
            (e.page, at(&e)),
            (Page::Mixer, (Strip::Track(1), Row::Chain))
        );
        assert_eq!(e.shown().unwrap().name, "synth");
        keys(&mut e, "gphgf");
        assert_eq!(at(&e), (Strip::Track(0), Row::Chain));
        assert_eq!(
            e.shown().unwrap().name,
            "synth",
            "no effects, so the instrument"
        );
        keys(&mut e, "gplgf");
        assert_eq!(e.shown().unwrap().slot, Some(0));
        keys(&mut e, "gphgm");
        assert_eq!(at(&e).0, Strip::Track(0));
        keys(&mut e, "))gi");
        assert_eq!(e.message.as_deref(), Some("buses have no instrument"));
    }

    #[test]
    fn nudging_and_setting_values() {
        let mut e = editor();
        assert_eq!(keys(&mut e, "A"), [Effect::TrackChanged(0)]);
        assert_eq!(e.grid.tracks[0].mix.level, 0.5);
        keys(&mut e, "4X");
        assert_eq!(e.grid.tracks[0].mix.level, -1.5);
        keys(&mut e, "jl3-");
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
        keys(&mut e, "jjj");
        assert_eq!(at(&e).1, Row::Param(TIME as usize));
        command(&mut e, "set 0.5");
        assert_eq!(e.buses.send.effects[0].values[TIME as usize], 0.5);
        command(&mut e, "set 9");
        assert_eq!(e.message.as_deref(), Some("this goes from 0.01 to 2"));
        keys(&mut e, "l");
        assert_eq!(at(&e).1, Row::Param(FEEDBACK as usize));
        assert!(e.modified());
    }

    #[test]
    fn effects_are_added_and_removed() {
        let mut e = editor();
        keys(&mut e, "\t");
        assert_eq!(at(&e).1, Row::Chain);
        assert_eq!(command(&mut e, "fx-add delay"), [Effect::MixerChanged]);
        assert_eq!(e.shown().unwrap().slot, Some(0), "on the new one");
        command(&mut e, "fx-add drive");
        assert_eq!(e.grid.tracks[0].effects[1].kind, FxKind::Drive);
        assert_eq!(at(&e), (Strip::Track(0), Row::Chain));
        assert_eq!(e.shown().unwrap().slot, Some(1));
        command(&mut e, "fx-add reverb");
        assert!(e.message.as_deref().unwrap().contains("there's delay"));
        // After the instrument means first.
        keys(&mut e, "hh");
        command(&mut e, "fx-add comp");
        assert_eq!(e.grid.tracks[0].effects[0].kind, FxKind::Comp);

        assert_eq!(keys(&mut e, "d"), [Effect::MixerChanged]);
        assert_eq!(e.grid.tracks[0].effects.len(), 2);
        keys(&mut e, "hd");
        assert_eq!(
            e.message.as_deref(),
            Some("move onto an effect on the chain to remove it")
        );
        keys(&mut e, "ldd");
        assert!(e.grid.tracks[0].effects.is_empty());
        assert!(e.shown().unwrap().slot.is_none(), "back on the instrument");
        keys(&mut e, "uu");
        assert_eq!(e.grid.tracks[0].effects.len(), 2);

        // On the pattern page, it goes at the end of the cursor's track's chain.
        keys(&mut e, "gpl");
        command(&mut e, "fx-add delay");
        assert_eq!(e.grid.tracks[1].effects.len(), 1);
    }

    #[test]
    fn instruments_are_chosen_and_set() {
        let mut e = editor();
        keys(&mut e, "l\t");
        assert_eq!(at(&e), (Strip::Track(1), Row::Chain));
        keys(&mut e, "c");
        let Mode::Command(prompt) = &e.mode else {
            panic!("c opens the command line")
        };
        assert_eq!(prompt.line, "inst ");
        assert_eq!(keys(&mut e, "kick\n"), [Effect::MixerChanged]);
        assert_eq!(e.grid.tracks[1].instrument.kind, InstKind::Kick);
        assert_eq!(e.grid.tracks[0].instrument.kind, InstKind::Synth);
        assert!(command(&mut e, "inst kick").is_empty(), "already a kick");

        // Its settings show below, and nudge like an effect's.
        keys(&mut e, "j");
        assert_eq!(at(&e).1, Row::Param(kick::MODEL as usize));
        keys(&mut e, "A");
        assert_eq!(
            e.grid.tracks[1].instrument.get(kick::MODEL),
            2.0,
            "the next model"
        );
        keys(&mut e, "ll");
        assert_eq!(at(&e).1, Row::Param(kick::PUNCH as usize));
        keys(&mut e, "5A");
        assert!((e.grid.tracks[1].instrument.get(kick::PUNCH) - 0.55).abs() < 1e-6);
        command(&mut e, "inst cowbell");
        assert!(e.message.as_deref().unwrap().contains("kick, snare, hat"));

        // On the pattern page, every track the selections reach.
        keys(&mut e, "gp%");
        command(&mut e, "inst hat");
        assert!(
            e.grid
                .tracks
                .iter()
                .all(|t| t.instrument.kind == InstKind::Hat)
        );
        keys(&mut e, "u");
        assert_eq!(e.grid.tracks[1].instrument.kind, InstKind::Kick, "undoable");
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
