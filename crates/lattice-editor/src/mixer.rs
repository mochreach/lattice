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

use std::sync::LazyLock;

use lattice_core::instrument::chop;
use lattice_core::{
    Bus, Fx, FxChain, FxKind, InstKind, Instrument, Kind, MAX_CHOKE, MAX_DB, MAX_EFFECTS, MIN_DB,
    ParamInfo, Step, Track,
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
    /// One strip's chain of effects, in lanes.
    Fx,
    /// Every track's instrument, side by side.
    Instrument,
    Song,
    /// Every key and command.
    Help,
    /// Everything, explained.
    Manual,
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
            Strip::Group(g) => self.grid.groups.get(g).map(|g| &g.bus),
            Strip::Send => Some(&self.buses.send),
            Strip::Master => Some(&self.buses.master),
        }
    }

    fn bus_mut(&mut self, strip: Strip) -> Option<&mut Bus> {
        match strip {
            Strip::Track(_) => None,
            Strip::Group(g) => self.grid.groups.get_mut(g).map(|g| &mut g.bus),
            Strip::Send => Some(&mut self.buses.send),
            Strip::Master => Some(&mut self.buses.master),
        }
    }

    /// A strip's chain; for a track or group there isn't, an empty one, as there's nothing on
    /// it to show.
    pub fn chain(&self, strip: Strip) -> &FxChain {
        static NONE: LazyLock<FxChain> = LazyLock::new(FxChain::default);
        self.chain_of(strip).unwrap_or(&NONE)
    }

    /// A strip's chain, if there's such a strip.
    pub(crate) fn chain_of(&self, strip: Strip) -> Option<&FxChain> {
        match strip {
            Strip::Track(i) => self.grid.tracks.get(i).map(|t| &t.effects),
            Strip::Group(g) => self.grid.groups.get(g).map(|g| &g.bus.effects),
            Strip::Send => Some(&self.buses.send.effects),
            Strip::Master => Some(&self.buses.master.effects),
        }
    }

    /// A strip's chain to change, if there's such a strip.
    pub(crate) fn chain_mut_of(&mut self, strip: Strip) -> Option<&mut FxChain> {
        match strip {
            Strip::Track(i) => self.grid.tracks.get_mut(i).map(|t| &mut t.effects),
            Strip::Group(g) => self.grid.groups.get_mut(g).map(|g| &mut g.bus.effects),
            Strip::Send => Some(&mut self.buses.send.effects),
            Strip::Master => Some(&mut self.buses.master.effects),
        }
    }

    /// The track's instrument, for a track's strip.
    pub fn instrument(&self, strip: Strip) -> Option<&Instrument> {
        match strip {
            Strip::Track(i) => self.grid.tracks.get(i).map(|t| &t.instrument),
            _ => None,
        }
    }

    /// A strip's level, in dB; 0 for a strip there isn't, as for a bus.
    pub fn strip_level(&self, strip: Strip) -> f32 {
        match strip {
            Strip::Track(i) => self.grid.tracks.get(i).map_or(0.0, |t| t.mix.level),
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
                Some(shown) => Row::Param(p.min(shown.params.len().saturating_sub(1))),
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
            Row::Param(p) if p / PARAM_COLUMNS < params.saturating_sub(1) / PARAM_COLUMNS => {
                Row::Param(params.saturating_sub(1))
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
                if let Some(&field) = fields.get(step(at, fields.len())) {
                    cursor.row = Row::Mix(field);
                }
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
                    Some(shown) => Row::Param(shown.params.len().saturating_sub(1)),
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
            Action::Escape if self.locking().is_some() => {
                self.message = Some("locking cancelled; settings put back".into());
                return Some(self.cancel_locking());
            }
            Action::LockSetting => {
                return Some(self.toggle_locking().unwrap_or_else(|message| {
                    self.message = Some(message);
                    vec![]
                }));
            }
            Action::Undo
            | Action::Redo
            | Action::CommandLine
            | Action::TogglePlay
            | Action::PlaySong
            | Action::PlayFill
            | Action::Escape
            | Action::Quit
            | Action::ToggleMute
            | Action::TogglePatternMute
            | Action::ToggleRecord
            | Action::ToggleSolo
            | Action::GotoMixer
            | Action::GotoPattern
            | Action::GotoSong
            | Action::GotoInstrument
            | Action::GotoEffects => return None,
            // Pattern editing has nothing to act on here.
            _ => {}
        }
        Some(vec![])
    }

    /// The instrument's or effect's setting under the cursor, on the mixer, fx or instrument
    /// page, so `:set` can take one of its choices by name.
    pub(crate) fn param_under_cursor(&self) -> Option<ParamInfo> {
        let page = matches!(self.page, Page::Mixer | Page::Instrument)
            || (self.page == Page::Fx && self.on_fx_param());
        if !page || self.inst_macro().is_some() {
            return None;
        }
        let Row::Param(p) = self.mixer_cursor().row else {
            return None;
        };
        self.shown()?.params.get(p).copied()
    }

    /// The value under the mixer cursor, and the range it can take, in the units `:set` takes.
    fn cursor_value(&self) -> Option<(f32, f32, f32)> {
        let cursor = self.mixer_cursor();
        let strip = self.strip(cursor.strip);
        let track = match strip {
            Strip::Track(i) => self.grid.tracks.get(i),
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
                let info = *shown.params.get(p)?;
                // Recording locks, nudges go on from the last one.
                let value = match self.live_lock {
                    Some((id, lock))
                        if self.record
                            && self.transport.is_some()
                            && self.track_index(id).map(Strip::Track) == Some(shown.strip)
                            && lock.fx == shown.slot.map(|s| s as u8)
                            && u32::from(lock.param) == info.id =>
                    {
                        lock.value
                    }
                    _ => *shown.values.get(p)?,
                };
                Some((value, info.min, info.max))
            }
        }
    }

    /// Sets the value under the mixer cursor, in the units [`Editor::cursor_value`] gives.
    pub(crate) fn set_mixer_value(&mut self, value: f32) -> Result<Vec<Effect>, String> {
        if !matches!(self.page, Page::Mixer | Page::Fx | Page::Instrument) {
            return Err("set works on the mixer (gm), fx (gf) and instrument (gi) pages".into());
        }
        if self.page == Page::Fx && !self.on_fx_param() {
            return self.set_fx_value(value);
        }
        if let Some((t, m)) = self.inst_macro() {
            if !(0.0..=1.0).contains(&value) {
                return Err("a macro goes from 0 to 1".into());
            }
            return Ok(self.set_macro(t, m, value));
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
        if let Some(effects) = self.record_lock(value) {
            return Ok(effects);
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
                let track = self.grid.tracks.get_mut(i).ok_or("no such track")?;
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
                let instrument = &mut self
                    .grid
                    .tracks
                    .get_mut(i)
                    .ok_or("no such track")?
                    .instrument;
                let info = *instrument.params().get(p).ok_or("no such setting")?;
                instrument.set(info.id, info.clamp(value));
            }
            (Row::Param(p), _) => {
                let fx = (self.chain_mut_of(strip))
                    .and_then(|chain| chain.get_mut(cursor.effect))
                    .ok_or("no such effect")?;
                *fx.values.get_mut(p).ok_or("no such setting")? = value;
            }
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
            Row::Param(p) => match self.shown().and_then(|shown| shown.params.get(p).copied()) {
                Some(info) => info.nudge(old, by),
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
            Page::Fx => {
                let strip = self.fx_strip();
                if self.chain_of(strip).ok_or("no such strip")?.len() >= MAX_EFFECTS {
                    return Err(format!("at most {MAX_EFFECTS} effects in a chain"));
                }
                self.checkpoint();
                let at = self.fx_insert_at();
                let chain = self.chain_mut_of(strip).ok_or("no such strip")?;
                let index = chain.insert_at(at, Fx::new(kind));
                self.fx_onto(at);
                return Ok(self.chain_changed(strip, index, true));
            }
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
            Page::Instrument => {
                let track = self.inst_cursor().track;
                (Strip::Track(track), self.chain(Strip::Track(track)).len())
            }
            Page::Pattern | Page::Song | Page::Help | Page::Manual => {
                let track = self.cursor().track;
                (Strip::Track(track), self.chain(Strip::Track(track)).len())
            }
        };
        if self.chain_of(strip).ok_or("no such track")?.len() >= MAX_EFFECTS {
            return Err(format!("at most {MAX_EFFECTS} effects in a chain"));
        }
        self.checkpoint();
        (self.chain_mut_of(strip).ok_or("no such track")?).insert(index, Fx::new(kind));
        if self.page == Page::Mixer {
            self.mixer.row = Row::Chain;
            self.mixer.effect = index;
            self.mixer.instrument = false;
        }
        Ok(self.chain_changed(strip, index, true))
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
            .filter(|&t| (self.grid.tracks.get(t)).is_some_and(|t| t.instrument.kind != kind))
            .collect();
        if tracks.is_empty() {
            return Ok(vec![]);
        }
        self.checkpoint();
        let mut effects = vec![Effect::MixerChanged];
        for t in tracks {
            let Some(track) = self.grid.tracks.get_mut(t) else {
                continue;
            };
            track.instrument = Instrument::new(kind);
            track.face = None;
            // The old instrument's settings mean nothing to the new one.
            if self.retain_locks(t, |lock| lock.fx.is_some())
                | self.retain_targets(t, |target| target.fx.is_some())
            {
                effects.push(Effect::TrackChanged(t));
            }
        }
        if self.page == Page::Mixer {
            self.mixer.row = Row::Chain;
            self.mixer.instrument = true;
        }
        Ok(effects)
    }

    /// `:sample [PATH]`: the tracks play the WAV file at `path`, or the last selection exported,
    /// on a sampler, keeping its settings if they're already on one.
    pub(crate) fn set_sample(&mut self, path: &str) -> Result<Vec<Effect>, String> {
        let path = self.sample_path(path)?;
        let tracks = self.mix_targets()?;
        self.checkpoint();
        for &t in &tracks {
            let Some(track) = self.grid.tracks.get_mut(t) else {
                continue;
            };
            if track.instrument.kind != InstKind::Sampler {
                track.instrument = Instrument::new(InstKind::Sampler);
                track.face = None;
            }
            track.instrument.file = Some(path.clone());
        }
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into());
        self.message = Some(match self.only_track(&tracks) {
            Some(track) => format!("{} plays {name}", track.name),
            None => format!("{} tracks play {name}", tracks.len()),
        });
        if self.page == Page::Mixer {
            self.mixer.row = Row::Chain;
            self.mixer.instrument = true;
        }
        Ok(vec![Effect::MixerChanged])
    }

    /// `:chop [PATH]`: the tracks play the WAV file at `path`, or the last selection exported,
    /// cut into as many pieces as each has steps, with each step playing its own piece, so it
    /// sounds as recorded until it's rearranged.
    pub(crate) fn set_chop(&mut self, path: &str) -> Result<Vec<Effect>, String> {
        let path = self.sample_path(path)?;
        let tracks = self.mix_targets()?;
        self.checkpoint();
        for &t in &tracks {
            self.chop_track(t, path.clone());
        }
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into());
        self.message = Some(match self.only_track(&tracks) {
            Some(track) => format!(
                "{} plays {name}, in {} pieces",
                track.name,
                track.steps.len().min(chop::MAX_PIECES)
            ),
            None => format!("{} tracks play {name}, in pieces", tracks.len()),
        });
        let mut effects: Vec<Effect> = tracks.iter().map(|&t| Effect::TrackChanged(t)).collect();
        effects.push(Effect::MixerChanged);
        Ok(effects)
    }

    /// Track `t` plays `path` on a chop track, cut into a piece for each step, each step playing
    /// its own; locks are kept.
    pub(crate) fn chop_track(&mut self, t: usize, path: std::path::PathBuf) {
        let Some(track) = self.grid.tracks.get_mut(t) else {
            return;
        };
        if track.instrument.kind != InstKind::Chop {
            track.instrument = Instrument::new(InstKind::Chop);
            // A face shows the old instrument's settings.
            track.face = None;
        }
        track.instrument.file = Some(path);
        let pieces = track.steps.len().clamp(1, chop::MAX_PIECES);
        track.instrument.set(chop::PIECES, pieces as f32);
        for (i, step) in track.steps.iter_mut().enumerate() {
            step.note = Step::note(chop::pitch(i % pieces)).note;
        }
    }

    /// The track, when `tracks` is just the one.
    fn only_track(&self, tracks: &[usize]) -> Option<&Track> {
        match tracks {
            [t] => self.grid.tracks.get(*t),
            _ => None,
        }
    }

    /// The file `:sample` or `:chop` names, made absolute, or the last selection exported.
    fn sample_path(&self, path: &str) -> Result<std::path::PathBuf, String> {
        let path = match path {
            "" => self
                .last_export
                .clone()
                .ok_or("which file? nothing's been exported yet")?,
            path => std::path::PathBuf::from(path),
        };
        // The file is read when the instrument's built, which says if it can't be; this just
        // catches a mistyped name at once. The last export may still be rendering, and is read
        // again once it's done.
        if !path.is_file() && self.last_export.as_ref() != Some(&path) {
            return Err(format!("no file {}", path.display()));
        }
        std::path::absolute(&path).map_err(|e| e.to_string())
    }

    /// Removes the effect under the mixer cursor.
    fn remove_effect(&mut self) -> Vec<Effect> {
        let cursor = self.mixer_cursor();
        let strip = self.strip(cursor.strip);
        if cursor.row != Row::Chain
            || cursor.instrument
            || self.chain(strip).get(cursor.effect).is_none()
        {
            self.message = Some("move onto an effect on the chain to remove it".into());
            return vec![];
        }
        self.checkpoint();
        if let Some(chain) = self.chain_mut_of(strip) {
            chain.remove(cursor.effect);
        }
        self.mixer = self.mixer_cursor();
        self.chain_changed(strip, cursor.effect, false)
    }

    /// After an effect's been added to or removed from a chain at `index`: a track's locks
    /// follow their effects.
    pub(crate) fn chain_changed(
        &mut self,
        strip: Strip,
        index: usize,
        inserted: bool,
    ) -> Vec<Effect> {
        let mut effects = vec![Effect::MixerChanged];
        if let Strip::Track(t) = strip
            && (self.shift_locks(t, index, inserted) | self.shift_targets(t, index, inserted))
        {
            effects.push(Effect::TrackChanged(t));
        }
        effects
    }

    /// The tracks `:level`, `:pan`, `:send`, `:mute`, `:solo` and `:inst` change: on the mixer
    /// page, the cursor's strip; on the pattern page, every track the selections reach.
    pub(crate) fn mix_targets(&self) -> Result<Vec<usize>, String> {
        match self.page {
            Page::Pattern => return Ok(self.selected_tracks()),
            Page::Song => {
                let (_, columns) = self.song_selection();
                let tracks: Vec<usize> = columns.filter_map(|c| c.checked_sub(1)).collect();
                return match tracks.is_empty() {
                    true => Err("move onto a track's column first".into()),
                    false => Ok(tracks),
                };
            }
            _ => {}
        }
        match self.strip(self.mixer_cursor().strip) {
            Strip::Track(i) => Ok(vec![i]),
            _ => Err("buses only have a level and effects".into()),
        }
    }

    /// The track a command such as `:name` goes to: the one under the page's cursor, the
    /// mixer's strip, the instrument page's track or the song page's column. `None` on a bus,
    /// or on the song page's entries.
    pub(crate) fn page_track(&self) -> Option<usize> {
        match self.page {
            Page::Mixer | Page::Fx | Page::Instrument => {
                match self.strip(self.mixer_cursor().strip) {
                    Strip::Track(t) => Some(t),
                    _ => None,
                }
            }
            Page::Song => self.song_column().checked_sub(1),
            _ => Some(self.cursor().track),
        }
    }

    /// Puts the page's cursor on track `track`, after it's been added or moved.
    pub(crate) fn point_page_at(&mut self, track: usize) {
        self.select_point(track, self.cursor().step);
        match self.page {
            Page::Mixer | Page::Fx => self.mixer.strip = track,
            Page::Instrument => self.inst.track = track,
            Page::Song => self.song.column = track + 1,
            _ => {}
        }
    }

    /// The bus under the mixer cursor, for `:level`.
    pub(crate) fn bus_under_cursor(&self) -> Option<Strip> {
        let strip = self.strip(self.mixer_cursor().strip);
        (matches!(self.page, Page::Mixer | Page::Fx) && strip.is_bus()).then_some(strip)
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

    #[test]
    fn chop_gives_each_step_its_own_piece() {
        let mut e = editor();
        e.grid.tracks[1].steps.truncate(12);
        e.grid.tracks[1].face = Some(lattice_core::Face::new("Old Synth"));
        e.grid.tracks[1].steps[3].locks.set(lattice_core::Lock {
            fx: None,
            param: 0,
            value: 1.0,
            ..Default::default()
        });
        // Any file will do: it's read when the instrument's built.
        let file = std::env::temp_dir().join(format!("lattice-chop-{}.wav", std::process::id()));
        std::fs::write(&file, b"").unwrap();
        keys(&mut e, "l");
        let effects = e.set_chop(file.to_str().unwrap()).unwrap();
        std::fs::remove_file(&file).unwrap();
        assert_eq!(effects, [Effect::TrackChanged(1), Effect::MixerChanged]);
        let track = &e.grid.tracks[1];
        assert_eq!(track.instrument.kind, InstKind::Chop);
        assert!(track.face.is_none(), "the old instrument's face goes");
        assert_eq!(track.instrument.get(chop::PIECES), 12.0, "a piece a step");
        let pieces: Vec<usize> = (track.steps.iter())
            .map(|s| chop::piece(s.note.unwrap().pitch, 12))
            .collect();
        assert_eq!(pieces, (0..12).collect::<Vec<_>>());
        assert!(!track.steps[3].locks.is_empty(), "locks are kept");
        assert!(e.set_chop("/no/such/file.wav").is_err());
        keys(&mut e, "u");
        assert_eq!(e.grid.tracks[1].instrument.kind, InstKind::Synth);
    }

    fn editor() -> Editor {
        let track = |name: &str| Track {
            name: name.into(),
            steps: vec![Step::default(); 16],
            ..Track::default()
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            beats_per_bar: 4,
            grouping: vec![],
            tracks: vec![track("a"), track("b")],
            groups: vec![],
        };
        let buses = Buses {
            send: Bus {
                effects: vec![Fx::new(FxKind::Delay)].into(),
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
    fn gi_gm_and_gf_open_on_the_cursors_track() {
        let mut e = editor();
        keys(&mut e, "gptl");
        command(&mut e, "fx-add delay");
        // gi opens the instrument page, with the mixer's cursor on the track's instrument.
        keys(&mut e, "gi");
        assert_eq!(
            (e.page, at(&e)),
            (Page::Instrument, (Strip::Track(1), Row::Param(0)))
        );
        assert_eq!(e.shown().unwrap().name, "synth");
        // gf opens the fx page, on the effect at the top of the chain.
        keys(&mut e, "gphgf");
        assert_eq!((e.page, at(&e)), (Page::Fx, (Strip::Track(0), Row::Chain)));
        keys(&mut e, "gplgf");
        assert_eq!(e.shown().unwrap().slot, Some(0));
        keys(&mut e, "gphgm");
        assert_eq!(at(&e).0, Strip::Track(0));
        // From a bus, the instrument page stays on the track it was last on.
        keys(&mut e, "))gi");
        assert_eq!((e.page, at(&e).0), (Page::Instrument, Strip::Track(1)));
    }

    #[test]
    fn the_fx_page_sets_a_bus_level_and_records_settings_as_the_mixer_does() {
        let mut e = editor();
        keys(&mut e, "ggll");
        e.page = Page::Fx;
        assert_eq!(command(&mut e, "level -9"), [Effect::MixerChanged]);
        assert_eq!(e.buses.send.level, -9.0);
        keys(&mut e, "R");
        assert_eq!(e.mode, Mode::Normal, "recording settings, not notes");
        assert!(e.message.as_deref().unwrap().contains("locked"));
    }

    #[test]
    fn set_takes_a_choice_by_name() {
        use lattice_core::instrument::switched;
        let mut e = editor();
        command(&mut e, "inst switched");
        let params = e.grid.tracks[0].instrument.params();
        let stack = params.iter().position(|p| p.id == switched::STACK).unwrap();
        e.mixer.row = Row::Param(stack);
        command(&mut e, "set fifths");
        assert_eq!(e.grid.tracks[0].instrument.get(switched::STACK), 4.0);
        command(&mut e, "set 1");
        assert_eq!(e.grid.tracks[0].instrument.get(switched::STACK), 1.0);
        command(&mut e, "set legion");
        assert_eq!(
            e.message.as_deref(),
            Some("stack is one of one, pair, swarm, stops, fifths")
        );
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
        assert_eq!(e.message.as_deref(), Some("time goes from 0.01s to 2s"));
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
        command(&mut e, "fx-add plate");
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
        keys(&mut e, "gptl");
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
