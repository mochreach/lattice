//! Modal editor state for Lattice, independent of any frontend.
//!
//! Editing follows Helix: there is always a selection, a rectangle of steps across one or more
//! tracks, and actions apply to it. Select first, then act, so you see what will change before it
//! does. Motions move the cursor and collapse the selection onto it; in select mode they extend it.
//!
//! The frontend turns its key events into [`Key`]s and calls [`Editor::handle`], which updates
//! the song and selection and returns [`Effect`]s (send a track to the engine, play, quit) for
//! the frontend to carry out. Nothing here knows about terminals or audio, so it's tested by
//! feeding in key sequences.

mod command;
mod keymap;

pub use command::{COMMANDS, Completion, Param, Spec};
pub use keymap::{Action, Binding, Key, Keymap, Menu};

use lattice_core::{Grid, MAX_STEPS, Note, Step, Track};

const UNDO_LIMIT: usize = 500;
const PAGE: usize = 16;
const DEFAULT_VELOCITY: u8 = 100;
pub const BEATS_PER_BAR: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// Motions extend the selection instead of moving it.
    Select,
    /// Typing on the piano keys enters notes.
    Insert,
    /// Typing a `:` command.
    Command(Prompt),
}

/// The `:` command line being typed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Prompt {
    pub line: String,
    /// The completion `tab` has filled in, while cycling through them.
    pub selected: Option<usize>,
    /// What was typed before cycling, which completions are matched against.
    typed: String,
}

/// Which version of the pattern the grid shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// As written.
    #[default]
    Source,
    /// As realised for the pass currently playing, after the track's rules.
    Live,
}

/// Something for the frontend to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// The track at this index changed; send it to the engine.
    TrackChanged(usize),
    /// Tracks were added, removed or reordered. Bring the engine's tracks in line with
    /// [`Editor::track_ids`]: remove those whose ids have gone, move those that have moved, and
    /// add the new ones (each with the contents of its track). Any `TrackChanged` effects that
    /// follow use the new indices.
    TracksChanged,
    TogglePlay,
    Bpm(f64),
    Click(bool),
    /// Audition a note on a track's instrument.
    Preview {
        track: usize,
        pitch: u8,
    },
    Quit,
}

/// Identifies a track while it's added, removed, moved and brought back by undo, so a frontend
/// can keep each track's instrument with it. Never reused within an editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrackId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    pub track: usize,
    pub step: usize,
}

/// The selection as a rectangle, inclusive at both ends. Steps past the end of a shorter track
/// are ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub first_track: usize,
    pub last_track: usize,
    pub first_step: usize,
    pub last_step: usize,
}

impl Region {
    pub fn contains(&self, track: usize, step: usize) -> bool {
        (self.first_track..=self.last_track).contains(&track)
            && (self.first_step..=self.last_step).contains(&step)
    }

    pub fn rows(&self) -> usize {
        self.last_step - self.first_step + 1
    }
}

/// A key that's waiting for the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Menu(Key),
    /// `r`: the next piano key fills the selection.
    Replace,
}

/// What the infobox shows while a key is pending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Infobox {
    pub title: &'static str,
    /// Key and description, in display order.
    pub items: Vec<(String, &'static str)>,
}

#[derive(Debug, Clone)]
struct Snapshot {
    grid: Grid,
    ids: Vec<TrackId>,
    cursor: Cursor,
    anchor: Cursor,
}

#[derive(Debug, Clone)]
pub struct Editor {
    pub grid: Grid,
    pub bpm: f64,
    /// The moving end of the selection.
    pub cursor: Cursor,
    /// The fixed end of the selection; equal to `cursor` when it's collapsed.
    pub anchor: Cursor,
    pub mode: Mode,
    pub view: View,
    /// Octave of the lower piano row in insert mode.
    pub octave: u8,
    /// Steps to move after entering a note.
    pub auto_step: usize,
    pub follow: bool,
    pub click: bool,
    /// Feedback for the status line; cleared on the next key.
    pub message: Option<String>,
    pub keymap: Keymap,
    /// One per track in `grid`, in the same order.
    ids: Vec<TrackId>,
    next_id: u32,
    count: Option<usize>,
    pending: Option<Pending>,
    /// One column of steps per track copied.
    clipboard: Vec<Vec<Step>>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Editor {
    pub fn new(grid: Grid, bpm: f64) -> Self {
        assert!(!grid.tracks.is_empty(), "need at least one track");
        let tracks = grid.tracks.len() as u32;
        Self {
            ids: (0..tracks).map(TrackId).collect(),
            next_id: tracks,
            grid,
            bpm,
            cursor: Cursor::default(),
            anchor: Cursor::default(),
            mode: Mode::Normal,
            view: View::Source,
            octave: 4,
            auto_step: 1,
            follow: true,
            click: true,
            message: None,
            keymap: Keymap::default(),
            count: None,
            pending: None,
            clipboard: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Each track's id, in the order of `grid.tracks`.
    pub fn track_ids(&self) -> &[TrackId] {
        &self.ids
    }

    /// A pending count typed before a command, e.g. the `4` in `4j`.
    pub fn count(&self) -> Option<usize> {
        self.count
    }

    pub fn selection(&self) -> Region {
        Region {
            first_track: self.anchor.track.min(self.cursor.track),
            last_track: self.anchor.track.max(self.cursor.track),
            first_step: self.anchor.step.min(self.cursor.step),
            last_step: self.anchor.step.max(self.cursor.step),
        }
    }

    /// The keys available after a prefix key, while it waits for the next one.
    pub fn infobox(&self) -> Option<Infobox> {
        match self.pending? {
            Pending::Menu(key) => {
                let Some(Binding::Menu(menu)) = self.keymap.normal.get(&key) else {
                    return None;
                };
                Some(Infobox {
                    title: menu.title,
                    items: menu
                        .items
                        .iter()
                        .map(|(key, action)| (key.to_string(), action.describe()))
                        .collect(),
                })
            }
            Pending::Replace => Some(Infobox {
                title: "replace",
                items: vec![
                    ("z–m q–u".into(), "fill with this note"),
                    ("esc".into(), "cancel"),
                ],
            }),
        }
    }

    /// Commands matching what's typed on the command line, best first. Empty once the command
    /// name is finished.
    pub fn completions(&self) -> Vec<Completion> {
        match &self.mode {
            Mode::Command(prompt) if !prompt.line.contains(' ') => command::complete(&prompt.typed),
            _ => vec![],
        }
    }

    /// Placeholders for the parameters still to type, shown faintly after the cursor.
    pub fn command_hint(&self) -> Option<String> {
        match &self.mode {
            Mode::Command(prompt) => command::hint(&prompt.line),
            _ => None,
        }
    }

    /// The command named on the command line, once there is one.
    pub fn command_spec(&self) -> Option<&'static Spec> {
        match &self.mode {
            Mode::Command(prompt) => command::find(prompt.line.split(' ').next()?),
            _ => None,
        }
    }

    pub fn handle(&mut self, key: Key) -> Vec<Effect> {
        self.message = None;
        match self.mode {
            Mode::Command(_) => self.command_key(key),
            Mode::Insert => self.insert_key(key),
            Mode::Normal | Mode::Select => self.normal_key(key),
        }
    }

    fn normal_key(&mut self, key: Key) -> Vec<Effect> {
        match self.pending.take() {
            Some(Pending::Replace) => {
                // Octave keys work while choosing the note.
                if let Some(&action @ (Action::KeyboardOctaveUp | Action::KeyboardOctaveDown)) =
                    self.keymap.insert.get(&key)
                {
                    self.pending = Some(Pending::Replace);
                    return self.act(action, 1);
                }
                return self.replace(key);
            }
            Some(Pending::Menu(prefix)) => {
                let n = self.count.take().unwrap_or(1);
                let action = match self.keymap.normal.get(&prefix) {
                    Some(Binding::Menu(menu)) => {
                        menu.items.iter().find(|(k, _)| *k == key).map(|&(_, a)| a)
                    }
                    _ => None,
                };
                return action.map_or_else(Vec::new, |a| self.act(a, n));
            }
            None => {}
        }
        if let Key::Char(c @ '0'..='9') = key
            && (c != '0' || self.count.is_some())
        {
            let digit = c.to_digit(10).unwrap() as usize;
            self.count = Some((self.count.unwrap_or(0) * 10 + digit).min(9_999));
            return vec![];
        }
        match self.keymap.normal.get(&key) {
            // Keep the count for the menu's action.
            Some(Binding::Menu(_)) => {
                self.pending = Some(Pending::Menu(key));
                vec![]
            }
            Some(&Binding::Action(action)) => {
                let n = self.count.take().unwrap_or(1);
                self.act(action, n)
            }
            None => {
                self.count = None;
                vec![]
            }
        }
    }

    /// The pitch a piano key plays at the keyboard octave.
    fn piano_pitch(&self, key: Key) -> Option<u8> {
        let Key::Char(c) = key else {
            return None;
        };
        let semitone = *self.keymap.piano.get(&c)?;
        let pitch = (u16::from(self.octave) + 1) * 12 + u16::from(semitone);
        u8::try_from(pitch).ok().filter(|&p| p <= 127)
    }

    fn insert_key(&mut self, key: Key) -> Vec<Effect> {
        if let Some(pitch) = self.piano_pitch(key) {
            self.checkpoint();
            let Cursor { track, step } = self.cursor;
            self.grid.tracks[track].steps[step].note = Some(Note {
                pitch,
                velocity: DEFAULT_VELOCITY,
                length: 1,
            });
            self.advance();
            return vec![
                Effect::TrackChanged(track),
                Effect::Preview { track, pitch },
            ];
        }
        match self.keymap.insert.get(&key).copied() {
            Some(action) => self.act(action, 1),
            None => vec![],
        }
    }

    fn command_key(&mut self, key: Key) -> Vec<Effect> {
        let Mode::Command(prompt) = &mut self.mode else {
            return vec![];
        };
        match key {
            Key::Esc => self.mode = Mode::Normal,
            Key::Backspace => {
                if prompt.line.pop().is_none() {
                    self.mode = Mode::Normal;
                } else {
                    prompt.typed = prompt.line.clone();
                    prompt.selected = None;
                }
            }
            Key::Char(c) => {
                prompt.line.push(c);
                prompt.typed = prompt.line.clone();
                prompt.selected = None;
            }
            // Cycle through the completions, filling each in.
            Key::Tab if !prompt.line.contains(' ') => {
                let completions = command::complete(&prompt.typed);
                if !completions.is_empty() {
                    let next = prompt.selected.map_or(0, |i| (i + 1) % completions.len());
                    prompt.selected = Some(next);
                    prompt.line = completions[next].spec.name.into();
                }
            }
            Key::Enter => {
                let line = std::mem::take(&mut prompt.line);
                self.mode = Mode::Normal;
                match command::run(self, &line) {
                    Ok(effects) => return effects,
                    Err(message) => self.message = Some(message),
                }
            }
            _ => {}
        }
        vec![]
    }

    fn act(&mut self, action: Action, n: usize) -> Vec<Effect> {
        let region = self.selection();
        if matches!(
            action,
            Action::Delete
                | Action::Clear
                | Action::Yank
                | Action::PasteAfter
                | Action::PasteBefore
                | Action::ReplaceWithYanked
                | Action::Replace
        ) && self.mode == Mode::Select
        {
            // Like Helix, these finish the selection.
            self.mode = Mode::Normal;
        }
        let last_track = self.grid.tracks.len() - 1;
        let by = n as isize;
        match action {
            Action::Left => self.move_by(-by, 0),
            Action::Right => self.move_by(by, 0),
            Action::Up => self.move_by(0, -by),
            Action::Down => self.move_by(0, by),
            Action::HalfPageUp => self.move_by(0, -by * (PAGE / 2) as isize),
            Action::HalfPageDown => self.move_by(0, by * (PAGE / 2) as isize),
            Action::PageUp => self.move_by(0, -by * PAGE as isize),
            Action::PageDown => self.move_by(0, by * PAGE as isize),
            Action::FirstStep => self.move_to(self.cursor.track, 0),
            Action::LastStep => self.move_to(self.cursor.track, usize::MAX),
            Action::FirstTrack => self.move_to(0, self.cursor.step),
            Action::LastTrack => self.move_to(last_track, self.cursor.step),
            Action::NextBeat => self.move_to(self.cursor.track, self.next_multiple(self.beat(), n)),
            Action::PrevBeat => self.move_to(self.cursor.track, self.prev_multiple(self.beat(), n)),
            Action::NextBar => self.move_to(
                self.cursor.track,
                self.next_multiple(self.beat() * BEATS_PER_BAR, n),
            ),
            Action::PrevBar => self.move_to(
                self.cursor.track,
                self.prev_multiple(self.beat() * BEATS_PER_BAR, n),
            ),
            Action::NextNote => {
                for _ in 0..n {
                    if let Some(step) = self.find_note(true) {
                        self.move_to(self.cursor.track, step);
                    }
                }
            }
            Action::PrevNote => {
                for _ in 0..n {
                    if let Some(step) = self.find_note(false) {
                        self.move_to(self.cursor.track, step);
                    }
                }
            }
            Action::SelectRows => {
                // Like Helix's `x`: select whole rows, or extend a selection that already is.
                let extend = region.first_track == 0 && region.last_track == last_track;
                let last_step = region.last_step + if extend { n } else { n - 1 };
                self.select(
                    Cursor {
                        track: 0,
                        step: region.first_step,
                    },
                    Cursor {
                        track: last_track,
                        step: last_step,
                    },
                );
            }
            Action::SelectAll => self.select(
                Cursor::default(),
                Cursor {
                    track: last_track,
                    step: usize::MAX,
                },
            ),
            Action::Collapse => self.anchor = self.cursor,
            Action::Flip => std::mem::swap(&mut self.anchor, &mut self.cursor),
            Action::SelectMode => {
                self.mode = match self.mode {
                    Mode::Select => Mode::Normal,
                    _ => Mode::Select,
                }
            }
            Action::Insert => self.insert_at(region.first_track, region.first_step),
            Action::Append => self.insert_at(region.first_track, region.last_step + 1),
            Action::Change => {
                self.yank(region);
                let effects = self.edit(|s| *s = Step::default());
                self.insert_at(region.first_track, region.first_step);
                return effects;
            }
            Action::CommandLine => self.mode = Mode::Command(Prompt::default()),
            Action::Escape => self.mode = Mode::Normal,
            Action::Delete | Action::Clear => {
                if action == Action::Delete {
                    self.yank(region);
                }
                let effects = self.edit(|s| *s = Step::default());
                self.select_point(region.first_track, region.first_step);
                return effects;
            }
            Action::ClearAdvance => {
                self.checkpoint();
                let track = self.cursor.track;
                self.grid.tracks[track].steps[self.cursor.step] = Step::default();
                self.advance();
                return vec![Effect::TrackChanged(track)];
            }
            Action::Yank => {
                self.yank(region);
                let steps: usize = self.clipboard.iter().map(Vec::len).sum();
                self.message = Some(format!("copied {steps} steps"));
            }
            Action::PasteAfter => {
                return self.paste(region.first_track, region.last_step as isize + 1, n);
            }
            Action::PasteBefore => {
                let rows = self.clipboard.first().map_or(0, Vec::len) * n;
                return self.paste(
                    region.first_track,
                    region.first_step as isize - rows as isize,
                    n,
                );
            }
            Action::ReplaceWithYanked => {
                return self.paste(region.first_track, region.first_step as isize, n);
            }
            Action::Replace => self.pending = Some(Pending::Replace),
            Action::Undo => return self.restore(true),
            Action::Redo => return self.restore(false),
            Action::NoteUp => return self.transpose(n as i32),
            Action::NoteDown => return self.transpose(-(n as i32)),
            Action::NoteOctaveUp => return self.transpose(12 * n as i32),
            Action::NoteOctaveDown => return self.transpose(-12 * n as i32),
            Action::Longer => return self.lengthen(n as i32),
            Action::Shorter => return self.lengthen(-(n as i32)),
            Action::KeyboardOctaveUp => {
                self.octave = (self.octave + 1).min(9);
                self.message = Some(format!("octave {}", self.octave));
            }
            Action::KeyboardOctaveDown => {
                self.octave = self.octave.saturating_sub(1);
                self.message = Some(format!("octave {}", self.octave));
            }
            Action::TogglePlay => return vec![Effect::TogglePlay],
            Action::ToggleView => {
                self.view = match self.view {
                    View::Source => View::Live,
                    View::Live => View::Source,
                }
            }
            Action::Quit => return vec![Effect::Quit],
        }
        vec![]
    }

    fn track_len(&self, track: usize) -> usize {
        self.grid.tracks[track].steps.len()
    }

    /// The last step the cursor can reach: the end of the longest track the selection spans.
    fn step_limit(&self, anchor: Cursor, cursor: Cursor) -> usize {
        let (a, b) = (
            anchor.track.min(cursor.track),
            anchor.track.max(cursor.track),
        );
        (a..=b).map(|t| self.track_len(t)).max().unwrap_or(1) - 1
    }

    /// Sets the selection, keeping it on the grid.
    fn select(&mut self, anchor: Cursor, cursor: Cursor) {
        let last_track = self.grid.tracks.len() - 1;
        let clamp_track = |c: Cursor| Cursor {
            track: c.track.min(last_track),
            step: c.step,
        };
        let (anchor, mut cursor) = (clamp_track(anchor), clamp_track(cursor));
        cursor.step = cursor.step.min(self.step_limit(anchor, cursor));
        let anchor = Cursor {
            step: anchor.step.min(self.step_limit(anchor, cursor)),
            ..anchor
        };
        self.anchor = anchor;
        self.cursor = cursor;
    }

    fn select_point(&mut self, track: usize, step: usize) {
        let point = Cursor { track, step };
        self.select(point, point);
    }

    /// Moves the cursor, collapsing the selection onto it unless in select mode.
    fn move_to(&mut self, track: usize, step: usize) {
        let target = Cursor { track, step };
        if self.mode == Mode::Select {
            self.select(self.anchor, target);
        } else {
            self.select(target, target);
        }
    }

    fn move_by(&mut self, tracks: isize, steps: isize) {
        let last_track = self.grid.tracks.len() - 1;
        let track = self
            .cursor
            .track
            .saturating_add_signed(tracks)
            .min(last_track);
        self.move_to(track, self.cursor.step.saturating_add_signed(steps));
    }

    fn beat(&self) -> usize {
        (self.grid.steps_per_beat as usize).max(1)
    }

    /// The `n`th step after the cursor that's a multiple of `unit`.
    fn next_multiple(&self, unit: usize, n: usize) -> usize {
        (self.cursor.step / unit + n) * unit
    }

    /// The `n`th step before the cursor that's a multiple of `unit`.
    fn prev_multiple(&self, unit: usize, n: usize) -> usize {
        let start = self.cursor.step.div_ceil(unit);
        start.saturating_sub(n) * unit
    }

    /// The next or previous step with a note on the cursor's track.
    fn find_note(&self, forwards: bool) -> Option<usize> {
        let steps = &self.grid.tracks[self.cursor.track].steps;
        let has_note = |&i: &usize| steps[i].note.is_some();
        if forwards {
            (self.cursor.step + 1..steps.len()).find(has_note)
        } else {
            (0..self.cursor.step).rev().find(has_note)
        }
    }

    fn insert_at(&mut self, track: usize, step: usize) {
        self.mode = Mode::Insert;
        self.select_point(track, step);
    }

    /// Moves on by the auto-step amount, wrapping round the track.
    fn advance(&mut self) {
        let step = (self.cursor.step + self.auto_step) % self.track_len(self.cursor.track);
        self.select_point(self.cursor.track, step);
    }

    /// The selected steps of each selected track, clipped to the track's length.
    fn columns(&self, region: Region) -> impl Iterator<Item = (usize, std::ops::Range<usize>)> {
        (region.first_track..=region.last_track).map(move |t| {
            let len = self.track_len(t);
            (
                t,
                region.first_step.min(len)..(region.last_step + 1).min(len),
            )
        })
    }

    fn yank(&mut self, region: Region) {
        self.clipboard = self
            .columns(region)
            .map(|(t, steps)| self.grid.tracks[t].steps[steps].to_vec())
            .collect();
    }

    /// Applies `f` to every selected step. The selection stays, so the edit can be repeated.
    fn edit(&mut self, mut f: impl FnMut(&mut Step)) -> Vec<Effect> {
        self.checkpoint();
        let region = self.selection();
        let columns: Vec<_> = self.columns(region).collect();
        for (t, steps) in &columns {
            for step in &mut self.grid.tracks[*t].steps[steps.clone()] {
                f(step);
            }
        }
        columns
            .into_iter()
            .map(|(t, _)| Effect::TrackChanged(t))
            .collect()
    }

    /// Writes the clipboard `n` times down from `step`, one column per track from `track`, then
    /// selects what was written. Steps off the grid are skipped.
    fn paste(&mut self, track: usize, step: isize, n: usize) -> Vec<Effect> {
        let rows = self.clipboard.first().map_or(0, Vec::len);
        if rows == 0 {
            self.message = Some("nothing to paste".into());
            return vec![];
        }
        self.checkpoint();
        let mut effects = vec![];
        for (i, column) in self.clipboard.iter().enumerate() {
            let Some(steps) = self.grid.tracks.get_mut(track + i).map(|t| &mut t.steps) else {
                break;
            };
            for (j, value) in column.iter().cycle().take(rows * n).enumerate() {
                if let Some(slot) = usize::try_from(step + j as isize)
                    .ok()
                    .and_then(|s| steps.get_mut(s))
                {
                    *slot = value.clone();
                }
            }
            effects.push(Effect::TrackChanged(track + i));
        }
        let first = step.max(0) as usize;
        let last = (step + (rows * n) as isize - 1).max(0) as usize;
        self.select(
            Cursor { track, step: first },
            Cursor {
                track: track + self.clipboard.len() - 1,
                step: last,
            },
        );
        effects
    }

    /// `r` then a piano key: every selected step becomes that note.
    fn replace(&mut self, key: Key) -> Vec<Effect> {
        let Some(pitch) = self.piano_pitch(key) else {
            return vec![];
        };
        let mut effects = self.edit(|s| {
            s.note = Some(Note {
                pitch,
                velocity: DEFAULT_VELOCITY,
                length: 1,
            })
        });
        effects.push(Effect::Preview {
            track: self.cursor.track,
            pitch,
        });
        effects
    }

    fn transpose(&mut self, by: i32) -> Vec<Effect> {
        let mut last = None;
        let mut effects = self.edit(|s| {
            if let Some(note) = &mut s.note {
                note.pitch = (i32::from(note.pitch) + by).clamp(0, 127) as u8;
                last = Some(note.pitch);
            }
        });
        if let Some(pitch) = last {
            effects.push(Effect::Preview {
                track: self.cursor.track,
                pitch,
            });
        }
        effects
    }

    fn lengthen(&mut self, by: i32) -> Vec<Effect> {
        self.edit(|s| {
            if let Some(note) = &mut s.note {
                note.length = (i32::from(note.length) + by).clamp(1, MAX_STEPS as i32) as u16;
            }
        })
    }

    /// Keeps the selection on the grid after tracks change length.
    pub(crate) fn clamp_selection(&mut self) {
        self.select(self.anchor, self.cursor);
    }

    /// Adds a track at `index`, with a new id.
    pub(crate) fn insert_track(&mut self, index: usize, track: Track) {
        self.grid.tracks.insert(index, track);
        self.ids.insert(index, TrackId(self.next_id));
        self.next_id += 1;
    }

    pub(crate) fn remove_track(&mut self, index: usize) {
        self.grid.tracks.remove(index);
        self.ids.remove(index);
    }

    pub(crate) fn move_track(&mut self, from: usize, to: usize) {
        let track = self.grid.tracks.remove(from);
        self.grid.tracks.insert(to, track);
        let id = self.ids.remove(from);
        self.ids.insert(to, id);
    }

    /// Saves the current state for undo. Call before every change to the song.
    pub(crate) fn checkpoint(&mut self) {
        self.undo.push(Snapshot {
            grid: self.grid.clone(),
            ids: self.ids.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore(&mut self, undo: bool) -> Vec<Effect> {
        let (from, to) = if undo {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let Some(snapshot) = from.pop() else {
            self.message = Some(
                if undo {
                    "nothing to undo"
                } else {
                    "nothing to redo"
                }
                .into(),
            );
            return vec![];
        };
        let tracks_changed = snapshot.ids != self.ids;
        to.push(Snapshot {
            grid: std::mem::replace(&mut self.grid, snapshot.grid),
            ids: std::mem::replace(&mut self.ids, snapshot.ids),
            cursor: self.cursor,
            anchor: self.anchor,
        });
        self.cursor = snapshot.cursor;
        self.anchor = snapshot.anchor;
        self.mode = Mode::Normal;
        // Line the tracks up first, so the indices that follow are the restored ones.
        tracks_changed
            .then_some(Effect::TracksChanged)
            .into_iter()
            .chain((0..self.grid.tracks.len()).map(Effect::TrackChanged))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{Condition, MAX_TRACKS, Rule, Transform};

    fn editor() -> Editor {
        let track = |name: &str, len| Track {
            name: name.into(),
            steps: vec![Step::default(); len],
            rules: vec![],
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![track("a", 16), track("b", 8)],
        };
        Editor::new(grid, 120.0)
    }

    fn keys(e: &mut Editor, s: &str) -> Vec<Effect> {
        s.chars().flat_map(|c| e.handle(Key::Char(c))).collect()
    }

    fn pitches(e: &Editor, track: usize) -> Vec<Option<u8>> {
        e.grid.tracks[track]
            .steps
            .iter()
            .map(|s| s.note.map(|n| n.pitch))
            .collect()
    }

    /// Enters notes from the top of a track, then returns to normal mode.
    fn notes(e: &mut Editor, piano: &str) {
        keys(e, "i");
        keys(e, piano);
        e.handle(Key::Esc);
    }

    #[test]
    fn insert_mode_enters_notes_and_advances() {
        let mut e = editor();
        let effects = keys(&mut e, "izxc");
        assert_eq!(&pitches(&e, 0)[..4], [Some(60), Some(62), Some(64), None]);
        assert_eq!(e.cursor.step, 3);
        assert!(effects.contains(&Effect::Preview {
            track: 0,
            pitch: 64
        }));
        e.handle(Key::Esc);
        assert_eq!(e.mode, Mode::Normal);
    }

    #[test]
    fn counts_repeat_motions() {
        let mut e = editor();
        keys(&mut e, "5j");
        assert_eq!(e.cursor.step, 5);
        keys(&mut e, "12j");
        assert_eq!(e.cursor.step, 15, "clamped to the track");
        keys(&mut e, "l");
        assert_eq!(
            e.cursor,
            Cursor { track: 1, step: 7 },
            "clamped to the shorter track"
        );
        assert_eq!(e.anchor, e.cursor, "motions collapse the selection");
    }

    #[test]
    fn goto_menu() {
        let mut e = editor();
        keys(&mut e, "g");
        let infobox = e.infobox().unwrap();
        assert_eq!(infobox.title, "goto");
        assert!(infobox.items.contains(&("e".into(), "last step")));
        keys(&mut e, "e");
        assert_eq!(e.cursor.step, 15);
        assert_eq!(e.infobox(), None);
        keys(&mut e, "gl");
        assert_eq!(e.cursor, Cursor { track: 1, step: 7 });
        keys(&mut e, "ghgg");
        assert_eq!(e.cursor, Cursor::default());
        keys(&mut e, "gq");
        assert_eq!(e.infobox(), None, "unbound key closes the menu");
    }

    #[test]
    fn select_mode_extends() {
        let mut e = editor();
        keys(&mut e, "vjjl");
        assert_eq!(
            e.selection(),
            Region {
                first_track: 0,
                last_track: 1,
                first_step: 0,
                last_step: 2
            }
        );
        keys(&mut e, "v");
        assert_eq!(e.mode, Mode::Normal);
        assert_eq!(e.selection().rows(), 3, "leaving select mode keeps it");
        keys(&mut e, ";");
        assert_eq!(e.anchor, e.cursor);
    }

    #[test]
    fn undo_and_redo() {
        let mut e = editor();
        notes(&mut e, "z");
        keys(&mut e, "kd");
        assert_eq!(pitches(&e, 0)[0], None);
        keys(&mut e, "u");
        assert_eq!(pitches(&e, 0)[0], Some(60));
        keys(&mut e, "u");
        assert_eq!(pitches(&e, 0)[0], None);
        keys(&mut e, "U");
        assert_eq!(pitches(&e, 0)[0], Some(60));
    }

    #[test]
    fn yank_and_paste_select_what_was_pasted() {
        let mut e = editor();
        notes(&mut e, "zxc");
        keys(&mut e, "ggvjjy");
        keys(&mut e, "p");
        assert_eq!(&pitches(&e, 0)[3..7], [Some(60), Some(62), Some(64), None]);
        assert_eq!((e.selection().first_step, e.selection().last_step), (3, 5));
        keys(&mut e, ";gg10jR");
        assert_eq!(&pitches(&e, 0)[10..13], [Some(60), Some(62), Some(64)]);
    }

    #[test]
    fn delete_copies_and_collapses() {
        let mut e = editor();
        notes(&mut e, "zx");
        keys(&mut e, "ggvjd");
        assert_eq!(&pitches(&e, 0)[..2], [None, None]);
        assert_eq!(e.anchor, e.cursor);
        keys(&mut e, "4jp");
        assert_eq!(&pitches(&e, 0)[5..7], [Some(60), Some(62)]);
    }

    #[test]
    fn rows_span_every_track() {
        let mut e = editor();
        notes(&mut e, "zx");
        keys(&mut e, "lgg");
        notes(&mut e, "c");
        keys(&mut e, "ghggx");
        assert_eq!(
            e.selection(),
            Region {
                first_track: 0,
                last_track: 1,
                first_step: 0,
                last_step: 0
            }
        );
        keys(&mut e, "x");
        assert_eq!(e.selection().last_step, 1, "x again extends a row");
        e.handle(Key::Ctrl('a'));
        assert_eq!(&pitches(&e, 0)[..2], [Some(61), Some(63)]);
        assert_eq!(pitches(&e, 1)[0], Some(65));
    }

    #[test]
    fn transpose_and_lengthen_keep_the_selection() {
        let mut e = editor();
        notes(&mut e, "zx");
        keys(&mut e, "ggvj");
        e.handle(Key::Ctrl('a'));
        e.handle(Key::Ctrl('a'));
        assert_eq!(&pitches(&e, 0)[..2], [Some(62), Some(64)]);
        keys(&mut e, "3+");
        assert_eq!(e.grid.tracks[0].steps[0].note.unwrap().length, 4);
        keys(&mut e, ">");
        assert_eq!(pitches(&e, 0)[1], Some(76));
    }

    #[test]
    fn replace_fills_the_selection() {
        let mut e = editor();
        keys(&mut e, "vjjr");
        assert_eq!(e.infobox().unwrap().title, "replace");
        keys(&mut e, "q");
        assert_eq!(&pitches(&e, 0)[..4], [Some(72), Some(72), Some(72), None]);
    }

    #[test]
    fn beats_and_bars() {
        let mut e = editor();
        keys(&mut e, "]b");
        assert_eq!(e.cursor.step, 4);
        keys(&mut e, "j]b");
        assert_eq!(e.cursor.step, 8);
        keys(&mut e, "[b");
        assert_eq!(e.cursor.step, 4);
        keys(&mut e, "j[b");
        assert_eq!(e.cursor.step, 4, "back to the start of this beat first");
        keys(&mut e, "]m");
        assert_eq!(e.cursor.step, 15, "clamped to the track");
        keys(&mut e, "[m");
        assert_eq!(e.cursor.step, 0);
        keys(&mut e, "2]b");
        assert_eq!(e.cursor.step, 8);
        assert_eq!(
            e.octave, 4,
            "brackets don't change the octave in normal mode"
        );
        keys(&mut e, "i]");
        assert_eq!(e.octave, 5);
    }

    #[test]
    fn tab_completes_commands() {
        let mut e = editor();
        keys(&mut e, ":f");
        assert_eq!(e.completions()[0].spec.name, "follow");
        e.handle(Key::Tab);
        let Mode::Command(prompt) = &e.mode else {
            panic!("left command mode")
        };
        assert_eq!(prompt.line, "follow");
        assert_eq!(
            e.completions()[0].spec.name,
            "follow",
            "list stays while cycling"
        );
        keys(&mut e, " ");
        assert!(e.completions().is_empty());
        assert_eq!(e.command_spec().unwrap().name, "follow");
        e.handle(Key::Enter);
        assert!(!e.follow);
    }

    #[test]
    fn next_and_previous_note() {
        let mut e = editor();
        keys(&mut e, "4j");
        notes(&mut e, "z");
        keys(&mut e, "9j");
        notes(&mut e, "x");
        keys(&mut e, "gg");
        keys(&mut e, "w");
        assert_eq!(e.cursor.step, 4);
        keys(&mut e, "w");
        assert_eq!(e.cursor.step, 14);
        keys(&mut e, "w");
        assert_eq!(e.cursor.step, 14, "stays put after the last note");
        keys(&mut e, "b");
        assert_eq!(e.cursor.step, 4);
    }

    fn command(e: &mut Editor, line: &str) -> Vec<Effect> {
        e.handle(Key::Char(':'));
        keys(e, line);
        e.handle(Key::Enter)
    }

    #[test]
    fn commands_add_rules() {
        let mut e = editor();
        assert_eq!(command(&mut e, "rev every 2 1"), [Effect::TrackChanged(0)]);
        command(&mut e, "tr +5 pass 3");
        command(&mut e, "drift -1");
        assert_eq!(
            e.grid.tracks[0].rules,
            [
                Rule {
                    transform: Transform::Reverse,
                    when: Condition::Every { n: 2, offset: 1 }
                },
                Rule {
                    transform: Transform::Transpose(5),
                    when: Condition::Pass(3)
                },
                Rule {
                    transform: Transform::Drift(-1),
                    when: Condition::Always
                },
            ]
        );
        command(&mut e, "pop");
        assert_eq!(e.grid.tracks[0].rules.len(), 2);
    }

    #[test]
    fn commands_report_errors() {
        let mut e = editor();
        command(&mut e, "tr lots");
        assert_eq!(e.message.as_deref(), Some("not a number: lots"));
        command(&mut e, "bpm 1000");
        assert!(e.message.unwrap().starts_with("expected a number"));
        assert_eq!(e.mode, Mode::Normal);
    }

    #[test]
    fn length_and_tempo() {
        let mut e = editor();
        keys(&mut e, "ge");
        assert_eq!(command(&mut e, "len 4"), [Effect::TrackChanged(0)]);
        assert_eq!(e.grid.tracks[0].steps.len(), 4);
        assert_eq!(e.cursor.step, 3);
        assert_eq!(command(&mut e, "bpm 90"), [Effect::Bpm(90.0)]);
        command(&mut e, "follow");
        assert!(!e.follow);
    }

    fn names(e: &Editor) -> Vec<&str> {
        e.grid.tracks.iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn tracks_are_added_after_the_cursor() {
        let mut e = editor();
        keys(&mut e, "5j");
        assert_eq!(command(&mut e, "track-add"), [Effect::TracksChanged]);
        assert_eq!(names(&e), ["a", "track 3", "b"]);
        assert_eq!(e.grid.tracks[1].steps.len(), 16);
        assert_eq!(e.cursor, Cursor { track: 1, step: 5 }, "onto the new track");
        assert_eq!(e.track_ids(), [TrackId(0), TrackId(2), TrackId(1)]);
        command(&mut e, "ta drums and bass");
        assert_eq!(names(&e), ["a", "track 3", "drums and bass", "b"]);
        command(&mut e, "ta");
        assert_eq!(names(&e)[3], "track 5");
    }

    #[test]
    fn tracks_are_limited() {
        let mut e = editor();
        for _ in 2..MAX_TRACKS {
            command(&mut e, "track-add");
        }
        assert_eq!(e.grid.tracks.len(), MAX_TRACKS);
        assert_eq!(command(&mut e, "track-add"), []);
        assert_eq!(e.message.as_deref(), Some("at most 16 tracks"));
        assert_eq!(e.grid.tracks.len(), MAX_TRACKS);
    }

    #[test]
    fn removing_a_track_keeps_the_cursor_on_the_grid() {
        let mut e = editor();
        keys(&mut e, "l12j");
        assert_eq!(command(&mut e, "track-remove"), [Effect::TracksChanged]);
        assert_eq!(names(&e), ["a"]);
        assert_eq!(e.track_ids(), [TrackId(0)]);
        assert_eq!(e.cursor, Cursor { track: 0, step: 7 });
        assert_eq!(e.anchor, e.cursor);
        assert_eq!(command(&mut e, "trm"), []);
        assert_eq!(e.message.as_deref(), Some("can't remove the only track"));
        assert_eq!(names(&e), ["a"]);
    }

    #[test]
    fn moving_tracks() {
        let mut e = editor();
        command(&mut e, "ta c");
        assert_eq!(command(&mut e, "track-move -1"), [Effect::TracksChanged]);
        assert_eq!(names(&e), ["c", "a", "b"]);
        assert_eq!(e.cursor.track, 0, "the cursor goes with it");
        command(&mut e, "tmv +5");
        assert_eq!(names(&e), ["a", "b", "c"]);
        assert_eq!(command(&mut e, "tmv 1"), [], "already at the end");
        command(&mut e, "tmv left");
        assert_eq!(e.message.as_deref(), Some("not a number: left"));
    }

    #[test]
    fn undo_and_redo_across_adding_and_removing_tracks() {
        let mut e = editor();
        notes(&mut e, "z");
        keys(&mut e, "l");
        command(&mut e, "ta c");
        command(&mut e, "track-move -2");
        keys(&mut e, "l");
        command(&mut e, "track-remove");
        assert_eq!(names(&e), ["c", "b"]);
        let ids = |e: &Editor| e.track_ids().to_vec();
        assert_eq!(ids(&e), [TrackId(2), TrackId(1)]);

        let all = |n| {
            std::iter::once(Effect::TracksChanged)
                .chain((0..n).map(Effect::TrackChanged))
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&mut e, "u"), all(3), "brings back the removed track");
        assert_eq!(names(&e), ["c", "a", "b"]);
        assert_eq!(ids(&e), [TrackId(2), TrackId(0), TrackId(1)], "with its id");
        assert_eq!(pitches(&e, 1)[0], Some(60), "and its notes");
        assert_eq!(e.cursor.track, 1);
        keys(&mut e, "u");
        assert_eq!(names(&e), ["a", "b", "c"]);
        keys(&mut e, "u");
        assert_eq!(names(&e), ["a", "b"]);
        assert_eq!(e.cursor.track, 1);
        assert_eq!(
            keys(&mut e, "u"),
            [Effect::TrackChanged(0), Effect::TrackChanged(1)]
        );
        assert_eq!(pitches(&e, 0)[0], None);

        keys(&mut e, "UUU");
        assert_eq!(names(&e), ["c", "a", "b"]);
        assert_eq!(keys(&mut e, "U"), all(2));
        assert_eq!(names(&e), ["c", "b"]);
        assert_eq!(ids(&e), [TrackId(2), TrackId(1)]);
        assert_eq!(e.cursor.track, 1);
    }
}
