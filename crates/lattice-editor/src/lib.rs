//! Modal editor state for Lattice, independent of any frontend.
//!
//! Editing follows Helix: there is always a selection, a rectangle of steps across one or more
//! tracks, and actions apply to it. Select first, then act, so you see what will change before it
//! does. Motions move the cursor and collapse the selection onto it; in select mode they extend it.
//! There can be several selections at once, as in Helix: every motion and action applies to each
//! of them, and the view follows the primary one.
//!
//! The frontend turns its key events into [`Key`]s and calls [`Editor::handle`], which updates
//! the song and selection and returns [`Effect`]s (send a track to the engine, play, save, quit)
//! for the frontend to carry out. Nothing here knows about terminals, audio or files, so it's
//! tested by feeding in key sequences.

mod command;
mod group;
mod keymap;
mod mixer;
mod pattern;

pub use command::{COMMANDS, Completion, Kind, Param, Problem, Spec, Value};
pub use keymap::{Action, Binding, Key, Keymap, Menu};
pub use mixer::{Field, MixerCursor, PARAM_COLUMNS, Page, Row, Shown, Strip};

use std::path::{Path, PathBuf};

use lattice_core::{Buses, Grid, MAX_STEPS, MAX_TRACKS, Note, Song, Step, Track};

use crate::pattern::Pattern;

const UNDO_LIMIT: usize = 500;
const PAGE: usize = 16;
const DEFAULT_VELOCITY: u8 = 100;

/// The digit a numpad key types.
fn pad_digit(n: u8) -> char {
    char::from(b'0' + n.min(9))
}
pub const BEATS_PER_BAR: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// Motions extend the selection instead of moving it.
    Select,
    /// Typing on the piano keys enters notes.
    Insert,
    /// Typing on the prompt: a `:` command, or notes to select.
    Command(Prompt),
}

/// What the prompt is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptKind {
    /// A `:` command.
    #[default]
    Command,
    /// `s`: the notes to select within the selections.
    Select,
    /// `K`: keep the selections containing one of these notes.
    Keep,
    /// `A-K`: remove the selections containing one of these notes.
    Remove,
}

impl PromptKind {
    /// Shown before what's typed, like Helix's `select:`.
    pub fn label(self) -> &'static str {
        match self {
            PromptKind::Command => ":",
            PromptKind::Select => "select:",
            PromptKind::Keep => "keep:",
            PromptKind::Remove => "remove:",
        }
    }
}

/// The prompt being typed on the status line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Prompt {
    pub kind: PromptKind,
    pub line: String,
    /// The completion `tab` has filled in, while cycling through them.
    pub selected: Option<usize>,
    /// What was typed before cycling, which completions are matched against.
    typed: String,
    /// The selections when a note prompt opened: what it matches within, and what `esc` restores.
    saved: Vec<Selection>,
    saved_primary: usize,
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
    /// Effect chains or bus levels changed: bring the engine's in line with the tracks' effects
    /// and [`Editor::buses`].
    MixerChanged,
    TogglePlay,
    Bpm(f64),
    Click(bool),
    /// Audition a note on a track's instrument.
    Preview {
        track: usize,
        pitch: u8,
    },
    /// Write [`Editor::song`] here, then call [`Editor::saved`]. If the save fails, drop the
    /// effects that follow it, so `:wq` doesn't quit.
    Save(PathBuf),
    /// Read the song here and pass it to [`Editor::load`], or start a new song if there's no such
    /// file.
    Open(PathBuf),
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

/// A selection as a rectangle, inclusive at both ends. Steps past the end of a shorter track
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

    fn overlaps(&self, other: &Region) -> bool {
        self.first_track <= other.last_track
            && other.first_track <= self.last_track
            && self.first_step <= other.last_step
            && other.first_step <= self.last_step
    }
}

/// One selection: the rectangle between a fixed anchor and the moving cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    /// The fixed end; equal to `cursor` when the selection is collapsed.
    pub anchor: Cursor,
    /// The moving end.
    pub cursor: Cursor,
}

impl Selection {
    pub fn point(at: Cursor) -> Self {
        Self {
            anchor: at,
            cursor: at,
        }
    }

    pub fn region(&self) -> Region {
        Region {
            first_track: self.anchor.track.min(self.cursor.track),
            last_track: self.anchor.track.max(self.cursor.track),
            first_step: self.anchor.step.min(self.cursor.step),
            last_step: self.anchor.step.max(self.cursor.step),
        }
    }

    /// The rectangle covering both, with the cursor at the same corner as this one's.
    fn merge(self, other: Selection) -> Selection {
        let (a, b) = (self.region(), other.region());
        let tracks = (
            a.first_track.min(b.first_track),
            a.last_track.max(b.last_track),
        );
        let steps = (a.first_step.min(b.first_step), a.last_step.max(b.last_step));
        let order = |forwards: bool, (first, last): (usize, usize)| {
            if forwards {
                (first, last)
            } else {
                (last, first)
            }
        };
        let (anchor_track, cursor_track) = order(self.cursor.track >= self.anchor.track, tracks);
        let (anchor_step, cursor_step) = order(self.cursor.step >= self.anchor.step, steps);
        Selection {
            anchor: Cursor {
                track: anchor_track,
                step: anchor_step,
            },
            cursor: Cursor {
                track: cursor_track,
                step: cursor_step,
            },
        }
    }
}

/// Where a paste goes, relative to each selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    After,
    Before,
    Over,
    Insert,
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
    buses: Buses,
    ids: Vec<TrackId>,
    selections: Vec<Selection>,
    primary: usize,
}

/// What one selection copied: a column of steps per track.
type Clip = Vec<Vec<Step>>;

/// The song as last saved or opened, to tell whether it's been modified since.
#[derive(Debug, Clone)]
struct Saved {
    grid: Grid,
    buses: Buses,
    bpm: f64,
}

#[derive(Debug, Clone)]
pub struct Editor {
    pub grid: Grid,
    /// The send and master buses.
    pub buses: Buses,
    pub bpm: f64,
    /// Where the song is saved; `None` until it's given a file name.
    pub path: Option<PathBuf>,
    saved: Saved,
    pub mode: Mode,
    pub view: View,
    pub page: Page,
    /// Where the cursor is on the mixer page; see [`Editor::mixer_cursor`].
    mixer: MixerCursor,
    /// Octave of the lower piano row in insert mode.
    pub octave: u8,
    /// Steps to move after entering a note.
    pub auto_step: usize,
    pub follow: bool,
    pub click: bool,
    /// Whether the frontend shows the keys as they're pressed, for screencasts.
    pub show_keys: bool,
    /// Feedback for the status line; cleared on the next key.
    pub message: Option<String>,
    pub keymap: Keymap,
    /// Never empty, never overlapping, and ordered down the grid then across it.
    selections: Vec<Selection>,
    /// Which selection the view follows.
    primary: usize,
    /// One per track in `grid`, in the same order.
    ids: Vec<TrackId>,
    next_id: u32,
    count: Option<usize>,
    pending: Option<Pending>,
    /// One clip per selection copied.
    clipboard: Vec<Clip>,
    /// Whole tracks copied, instruments and all.
    track_clipboard: Vec<Track>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

/// One `TrackChanged` for each track touched, in order.
fn changed(tracks: impl IntoIterator<Item = usize>) -> Vec<Effect> {
    let mut tracks: Vec<usize> = tracks.into_iter().collect();
    tracks.sort_unstable();
    tracks.dedup();
    tracks.into_iter().map(Effect::TrackChanged).collect()
}

/// The `n`th multiple of `unit` after `step`.
fn next_multiple(step: usize, unit: usize, n: usize) -> usize {
    (step / unit + n) * unit
}

/// The `n`th multiple of `unit` before `step`.
fn prev_multiple(step: usize, unit: usize, n: usize) -> usize {
    step.div_ceil(unit).saturating_sub(n) * unit
}

impl Editor {
    pub fn new(grid: Grid, bpm: f64) -> Self {
        assert!(!grid.tracks.is_empty(), "need at least one track");
        let tracks = grid.tracks.len() as u32;
        Self {
            ids: (0..tracks).map(TrackId).collect(),
            next_id: tracks,
            saved: Saved {
                grid: grid.clone(),
                buses: Buses::default(),
                bpm,
            },
            grid,
            buses: Buses::default(),
            bpm,
            path: None,
            mode: Mode::Normal,
            view: View::Source,
            page: Page::Pattern,
            mixer: MixerCursor::default(),
            octave: 4,
            auto_step: 1,
            follow: true,
            click: true,
            show_keys: false,
            message: None,
            keymap: Keymap::default(),
            selections: vec![Selection::default()],
            primary: 0,
            count: None,
            pending: None,
            clipboard: Vec::new(),
            track_clipboard: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Starts with these buses, as saved.
    pub fn with_buses(mut self, buses: Buses) -> Self {
        self.saved.buses = buses.clone();
        self.buses = buses;
        self
    }

    /// Each track's id, in the order of `grid.tracks`.
    pub fn track_ids(&self) -> &[TrackId] {
        &self.ids
    }

    /// Whether the song differs from when it was last saved or opened, tracks added, removed
    /// and moved included. Undoing back to that point counts as unmodified.
    pub fn modified(&self) -> bool {
        self.grid != self.saved.grid || self.buses != self.saved.buses || self.bpm != self.saved.bpm
    }

    /// The song to save, named after its file.
    pub fn song(&self) -> Song {
        let name = self
            .path
            .as_deref()
            .and_then(Path::file_stem)
            .map_or_else(|| self.grid.name.clone(), |s| s.to_string_lossy().into());
        Song {
            name,
            bpm: self.bpm,
            grids: vec![self.grid.clone()],
            order: vec![0],
            buses: self.buses.clone(),
        }
    }

    /// The frontend has written [`Editor::song`] to `path`.
    pub fn saved(&mut self, path: PathBuf) {
        self.message = Some(format!("written {}", path.display()));
        self.path = Some(path);
        self.saved = Saved {
            grid: self.grid.clone(),
            buses: self.buses.clone(),
            bpm: self.bpm,
        };
    }

    /// Replaces the song with one read from `path`, starting afresh: its tracks get new ids, there's
    /// one selection at the top left, and the undo history is cleared. Only the first grid in the
    /// song's order is edited so far. Returns the effects that send the new song to the engine.
    pub fn load(&mut self, song: Song, path: PathBuf) -> Vec<Effect> {
        let first = song.first_grid();
        let others = song.grids.len().saturating_sub(1);
        let Some(grid) = song.grids.into_iter().nth(first) else {
            self.message = Some("the song has no grids".into());
            return vec![];
        };
        assert!(!grid.tracks.is_empty(), "need at least one track");
        self.message = Some(if others > 0 {
            format!("opened {}; only its first grid so far", path.display())
        } else {
            format!("opened {}", path.display())
        });
        let tracks = grid.tracks.len() as u32;
        self.ids = (self.next_id..self.next_id + tracks).map(TrackId).collect();
        self.next_id += tracks;
        self.saved = Saved {
            grid: grid.clone(),
            buses: song.buses.clone(),
            bpm: song.bpm,
        };
        self.grid = grid;
        self.buses = song.buses;
        self.bpm = song.bpm;
        self.path = Some(path);
        self.selections = vec![Selection::default()];
        self.primary = 0;
        self.mode = Mode::Normal;
        self.count = None;
        self.pending = None;
        self.undo.clear();
        self.redo.clear();
        // Every id is new, so the engine rebuilds every track with its contents.
        vec![
            Effect::TracksChanged,
            Effect::MixerChanged,
            Effect::Bpm(self.bpm),
        ]
    }

    /// A pending count typed before a command, e.g. the `4` in `4j`.
    pub fn count(&self) -> Option<usize> {
        self.count
    }

    /// Every selection, ordered down the grid then across it.
    pub fn selections(&self) -> &[Selection] {
        &self.selections
    }

    /// The index of the primary selection in [`Editor::selections`].
    pub fn primary(&self) -> usize {
        self.primary
    }

    /// The primary selection's moving end, which the view follows.
    pub fn cursor(&self) -> Cursor {
        self.selections[self.primary].cursor
    }

    /// The primary selection's fixed end.
    pub fn anchor(&self) -> Cursor {
        self.selections[self.primary].anchor
    }

    /// The primary selection.
    pub fn selection(&self) -> Region {
        self.selections[self.primary].region()
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
            Mode::Command(prompt)
                if prompt.kind == PromptKind::Command && !prompt.line.contains(' ') =>
            {
                command::complete(&prompt.typed)
            }
            _ => vec![],
        }
    }

    /// Placeholders for the parameters still to type, shown faintly after the cursor.
    pub fn command_hint(&self) -> Option<String> {
        match &self.mode {
            Mode::Command(prompt) if prompt.kind == PromptKind::Command => {
                command::hint(&prompt.line)
            }
            Mode::Command(prompt) if prompt.line.is_empty() => Some("NOTE…".into()),
            _ => None,
        }
    }

    /// What's wrong with the command line so far. A word that more typing could make valid,
    /// such as `4` on the way to `40`, isn't flagged until it can't.
    pub fn command_problem(&self) -> Option<Problem> {
        match &self.mode {
            Mode::Command(prompt) if prompt.kind == PromptKind::Command => {
                command::problem(&prompt.line)
            }
            _ => None,
        }
    }

    /// The command named on the command line, once there is one.
    pub fn command_spec(&self) -> Option<&'static Spec> {
        match &self.mode {
            Mode::Command(prompt) if prompt.kind == PromptKind::Command => {
                command::find(prompt.line.split(' ').next()?)
            }
            _ => None,
        }
    }

    pub fn handle(&mut self, key: Key) -> Vec<Effect> {
        self.message = None;
        match (key, &self.mode) {
            // On the command line, the numpad types digits.
            (Key::Pad(n), Mode::Command(_)) => return self.command_key(Key::Char(pad_digit(n))),
            (Key::Pad(n), _) => return self.pad(n),
            (Key::Char(c @ '1'..='9'), Mode::Insert) if self.in_folded_group() => {
                return self.pad(c as u8 - b'0');
            }
            _ => {}
        }
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
            let track = self.cursor().track;
            let cursors: Vec<Cursor> = self.selections.iter().map(|s| s.cursor).collect();
            for c in &cursors {
                if let Some(step) = self.grid.tracks[c.track].steps.get_mut(c.step) {
                    step.note = Some(Note {
                        pitch,
                        velocity: DEFAULT_VELOCITY,
                        length: 1,
                    });
                }
            }
            self.advance();
            let mut effects = changed(cursors.iter().map(|c| c.track));
            effects.push(Effect::Preview { track, pitch });
            return effects;
        }
        match self.keymap.insert.get(&key).copied() {
            Some(action) => self.act(action, 1),
            None => vec![],
        }
    }

    fn open_prompt(&mut self, kind: PromptKind) {
        self.mode = Mode::Command(Prompt {
            kind,
            saved: self.selections.clone(),
            saved_primary: self.primary,
            ..Prompt::default()
        });
    }

    /// Leaves the prompt, putting back the selections a note prompt started from.
    fn cancel_prompt(&mut self) {
        if let Mode::Command(prompt) = std::mem::replace(&mut self.mode, Mode::Normal)
            && prompt.kind != PromptKind::Command
        {
            self.set_selections(prompt.saved, prompt.saved_primary);
        }
    }

    fn command_key(&mut self, key: Key) -> Vec<Effect> {
        let Mode::Command(prompt) = &mut self.mode else {
            return vec![];
        };
        match key {
            Key::Esc => self.cancel_prompt(),
            Key::Backspace => {
                if prompt.line.pop().is_none() {
                    self.cancel_prompt();
                } else {
                    prompt.typed = prompt.line.clone();
                    prompt.selected = None;
                    self.preview();
                }
            }
            Key::Char(c) => {
                prompt.line.push(c);
                prompt.typed = prompt.line.clone();
                prompt.selected = None;
                self.preview();
            }
            // Cycle through the completions, filling each in.
            Key::Tab if prompt.kind == PromptKind::Command && !prompt.line.contains(' ') => {
                let completions = command::complete(&prompt.typed);
                if !completions.is_empty() {
                    let next = prompt.selected.map_or(0, |i| (i + 1) % completions.len());
                    prompt.selected = Some(next);
                    prompt.line = completions[next].spec.name.into();
                }
            }
            Key::Enter => {
                let prompt = std::mem::take(prompt);
                self.mode = Mode::Normal;
                if prompt.kind == PromptKind::Command {
                    match command::run(self, &prompt.line) {
                        Ok(effects) => return effects,
                        Err(message) => self.message = Some(message),
                    }
                } else {
                    match self.filter(&prompt) {
                        Ok((selections, primary)) => self.set_selections(selections, primary),
                        Err(message) => {
                            self.set_selections(prompt.saved, prompt.saved_primary);
                            self.message = Some(message);
                        }
                    }
                }
            }
            _ => {}
        }
        vec![]
    }

    /// Shows what a note prompt will select while it's typed, as Helix does, or the selections
    /// it started from while what's typed matches nothing.
    fn preview(&mut self) {
        let Mode::Command(prompt) = &self.mode else {
            return;
        };
        if prompt.kind == PromptKind::Command {
            return;
        }
        let (selections, primary) = self
            .filter(prompt)
            .unwrap_or_else(|_| (prompt.saved.clone(), prompt.saved_primary));
        self.set_selections(selections, primary);
    }

    /// The selections a note prompt leaves, and which is primary: the first at or after the
    /// primary it started from.
    fn filter(&self, prompt: &Prompt) -> Result<(Vec<Selection>, usize), String> {
        let pattern = Pattern::parse(&prompt.line)?;
        let mut selections = Vec::new();
        let mut primary = 0;
        for (i, s) in prompt.saved.iter().enumerate() {
            if i == prompt.saved_primary {
                primary = selections.len();
            }
            let matches = self.matching(s.region(), &pattern);
            match prompt.kind {
                PromptKind::Select => selections.extend(matches.into_iter().map(Selection::point)),
                PromptKind::Keep if !matches.is_empty() => selections.push(*s),
                PromptKind::Remove if matches.is_empty() => selections.push(*s),
                _ => {}
            }
        }
        if selections.is_empty() {
            return Err(match prompt.kind {
                PromptKind::Remove => "that would remove every selection".into(),
                _ => "no matches".into(),
            });
        }
        let primary = primary.min(selections.len() - 1);
        Ok((selections, primary))
    }

    /// The steps in `region` whose notes match.
    fn matching(&self, region: Region, pattern: &Pattern) -> Vec<Cursor> {
        self.columns(region)
            .flat_map(|(track, steps)| steps.map(move |step| Cursor { track, step }))
            .filter(|c| {
                self.grid.tracks[c.track].steps[c.step]
                    .note
                    .is_some_and(|n| pattern.matches(n.pitch))
            })
            .collect()
    }

    fn act(&mut self, action: Action, n: usize) -> Vec<Effect> {
        if self.page == Page::Mixer
            && let Some(effects) = self.mixer_act(action, n)
        {
            return effects;
        }
        if matches!(
            action,
            Action::Delete
                | Action::Clear
                | Action::Yank
                | Action::Paste
                | Action::PasteInsert
                | Action::Replace
        ) && self.mode == Mode::Select
        {
            // Like Helix, these finish the selection.
            self.mode = Mode::Normal;
        }
        let last_track = self.grid.tracks.len() - 1;
        let by = n as isize;
        let at_step = |step: usize| move |_: &Self, c: Cursor| Cursor { step, ..c };
        let at_track = |track: usize| move |_: &Self, c: Cursor| Cursor { track, ..c };
        match action {
            Action::Left => self.motion(|e, c| e.offset(c, -by, 0)),
            Action::Right => self.motion(|e, c| e.offset(c, by, 0)),
            Action::Up => self.motion(|e, c| e.offset(c, 0, -by)),
            Action::Down => self.motion(|e, c| e.offset(c, 0, by)),
            Action::HalfPageUp => self.motion(|e, c| e.offset(c, 0, -by * (PAGE / 2) as isize)),
            Action::HalfPageDown => self.motion(|e, c| e.offset(c, 0, by * (PAGE / 2) as isize)),
            Action::PageUp => self.motion(|e, c| e.offset(c, 0, -by * PAGE as isize)),
            Action::PageDown => self.motion(|e, c| e.offset(c, 0, by * PAGE as isize)),
            Action::FirstStep => self.motion(at_step(0)),
            Action::LastStep => self.motion(at_step(usize::MAX)),
            Action::FirstTrack => self.motion(at_track(0)),
            Action::LastTrack => self.motion(at_track(last_track)),
            Action::NextBeat => self.motion(|e, c| Cursor {
                step: next_multiple(c.step, e.beat(), n),
                ..c
            }),
            Action::PrevBeat => self.motion(|e, c| Cursor {
                step: prev_multiple(c.step, e.beat(), n),
                ..c
            }),
            Action::NextBar => self.motion(|e, c| Cursor {
                step: next_multiple(c.step, e.beat() * BEATS_PER_BAR, n),
                ..c
            }),
            Action::PrevBar => self.motion(|e, c| Cursor {
                step: prev_multiple(c.step, e.beat() * BEATS_PER_BAR, n),
                ..c
            }),
            Action::NextNote | Action::PrevNote => self.motion(|e, mut c| {
                for _ in 0..n {
                    if let Some(step) = e.find_note(c, action == Action::NextNote) {
                        c.step = step;
                    }
                }
                c
            }),
            Action::SelectRows => self.map_selections(|e, s| {
                // Like Helix's `x`: select whole rows, or extend a selection that already is.
                let region = s.region();
                let extend = region.first_track == 0 && region.last_track == last_track;
                let last_step = region.last_step + if extend { n } else { n - 1 };
                e.clamped(
                    Cursor {
                        track: 0,
                        step: region.first_step,
                    },
                    Cursor {
                        track: last_track,
                        step: last_step,
                    },
                )
            }),
            Action::SelectAll => self.set_selections(
                vec![self.clamped(
                    Cursor::default(),
                    Cursor {
                        track: last_track,
                        step: usize::MAX,
                    },
                )],
                0,
            ),
            Action::SelectTracks => self.map_selections(|e, s| {
                let region = s.region();
                e.clamped(
                    Cursor {
                        track: region.first_track,
                        step: 0,
                    },
                    Cursor {
                        track: region.last_track,
                        step: usize::MAX,
                    },
                )
            }),
            Action::Collapse => self.map_selections(|_, s| Selection::point(s.cursor)),
            Action::Flip => self.map_selections(|_, s| Selection {
                anchor: s.cursor,
                cursor: s.anchor,
            }),
            Action::SelectMode => {
                self.mode = match self.mode {
                    Mode::Select => Mode::Normal,
                    _ => Mode::Select,
                }
            }
            Action::CopyDown => self.copy_rows(by),
            Action::CopyUp => self.copy_rows(-by),
            Action::SplitRows => self.split_rows(),
            Action::MergeAll => {
                let merged = self
                    .selections
                    .iter()
                    .fold(self.selections[self.primary], |m, s| m.merge(*s));
                self.set_selections(vec![merged], 0);
            }
            Action::KeepPrimary => self.set_selections(vec![self.selections[self.primary]], 0),
            Action::RemovePrimary => {
                if self.selections.len() == 1 {
                    self.message = Some("can't remove the only selection".into());
                } else {
                    self.selections.remove(self.primary);
                    self.primary = self.primary.min(self.selections.len() - 1);
                }
            }
            Action::RotateForward => {
                self.primary = (self.primary + n) % self.selections.len();
            }
            Action::RotateBackward => {
                let len = self.selections.len();
                self.primary = (self.primary + len - n % len) % len;
            }
            Action::SelectNotes => self.open_prompt(PromptKind::Select),
            Action::KeepNotes => self.open_prompt(PromptKind::Keep),
            Action::RemoveNotes => self.open_prompt(PromptKind::Remove),
            Action::Insert => self.insert_at(false),
            Action::Append => self.insert_at(true),
            Action::Change => {
                self.yank();
                let effects = self.edit(|s| *s = Step::default());
                self.insert_at(false);
                return effects;
            }
            Action::CommandLine => self.open_prompt(PromptKind::Command),
            Action::Escape => self.mode = Mode::Normal,
            Action::Delete | Action::Clear => {
                if action == Action::Delete {
                    self.yank();
                }
                let effects = self.edit(|s| *s = Step::default());
                self.map_selections(|e, s| {
                    let region = s.region();
                    let start = Cursor {
                        track: region.first_track,
                        step: region.first_step,
                    };
                    e.clamped(start, start)
                });
                return effects;
            }
            Action::ClearAdvance => {
                self.checkpoint();
                let cursors: Vec<Cursor> = self.selections.iter().map(|s| s.cursor).collect();
                for c in &cursors {
                    if let Some(step) = self.grid.tracks[c.track].steps.get_mut(c.step) {
                        *step = Step::default();
                    }
                }
                self.advance();
                return changed(cursors.iter().map(|c| c.track));
            }
            Action::Yank => {
                self.yank();
                self.message = Some(match self.clipboard.as_slice() {
                    [clip] => format!("copied {} steps", clip.iter().map(Vec::len).sum::<usize>()),
                    clips => format!("copied {} selections", clips.len()),
                });
            }
            Action::Paste => return self.paste(Place::Over, n),
            Action::PasteInsert => return self.paste(Place::Insert, n),
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
            Action::ToggleMute => return self.run_command("mute"),
            Action::ToggleSolo => return self.run_command("solo"),
            Action::AddTrack => return self.run_command("track-add"),
            Action::RemoveTrack => return self.delete_tracks(),
            Action::YankTracks => self.yank_tracks(),
            Action::PasteTracksAfter => return self.paste_tracks(Place::After),
            Action::PasteTracksBefore => return self.paste_tracks(Place::Before),
            Action::MoveTrackLeft => return self.run_command("track-move -1"),
            Action::MoveTrackRight => return self.run_command("track-move 1"),
            Action::ToggleGroup => return self.toggle_group(),
            Action::ToggleFold => return self.toggle_fold(),
            Action::GotoMixer => {
                // From the pattern, onto the cursor's track.
                if self.page == Page::Pattern {
                    self.mixer.strip = self.cursor().track;
                }
                self.page = Page::Mixer;
                self.mode = Mode::Normal;
            }
            Action::GotoInstrument => self.goto_chain(true),
            Action::GotoEffects => self.goto_chain(false),
            Action::GotoPattern => self.page = Page::Pattern,
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

    /// Copies every track the selections reach.
    fn yank_tracks(&mut self) {
        self.track_clipboard = self
            .selected_tracks()
            .into_iter()
            .map(|t| self.grid.tracks[t].clone())
            .collect();
        self.message = Some(match self.track_clipboard.len() {
            1 => "copied 1 track".into(),
            n => format!("copied {n} tracks"),
        });
    }

    /// Copies, then removes, every track the selections reach, leaving the cursor on the track
    /// that took the first one's place.
    fn delete_tracks(&mut self) -> Vec<Effect> {
        let tracks = self.selected_tracks();
        if tracks.len() == self.grid.tracks.len() {
            self.message = Some("can't remove every track".into());
            return vec![];
        }
        self.yank_tracks();
        self.checkpoint();
        for &t in tracks.iter().rev() {
            self.remove_track(t);
        }
        let first = tracks[0].min(self.grid.tracks.len() - 1);
        self.select_point(first, self.cursor().step);
        vec![Effect::TracksChanged]
    }

    /// Pastes the copied tracks after or before the cursor's track, and moves onto the first.
    fn paste_tracks(&mut self, place: Place) -> Vec<Effect> {
        if self.track_clipboard.is_empty() {
            self.message = Some("no tracks copied".into());
            return vec![];
        }
        if self.grid.tracks.len() + self.track_clipboard.len() > MAX_TRACKS {
            self.message = Some(format!("at most {MAX_TRACKS} tracks"));
            return vec![];
        }
        let cursor = self.cursor();
        let at = match place {
            Place::Before => cursor.track,
            _ => cursor.track + 1,
        };
        self.checkpoint();
        for (i, track) in self.track_clipboard.clone().into_iter().enumerate() {
            self.insert_track(at + i, track);
        }
        self.select_point(at, cursor.step);
        vec![Effect::TracksChanged]
    }

    /// Runs a command line for a key, showing any error as `:` would.
    fn run_command(&mut self, line: &str) -> Vec<Effect> {
        command::run(self, line).unwrap_or_else(|message| {
            self.message = Some(message);
            vec![]
        })
    }

    fn track_len(&self, track: usize) -> usize {
        self.grid.tracks[track].steps.len()
    }

    /// The last step a cursor can reach: the end of the longest track the selection spans.
    fn step_limit(&self, anchor: Cursor, cursor: Cursor) -> usize {
        let (a, b) = (
            anchor.track.min(cursor.track),
            anchor.track.max(cursor.track),
        );
        (a..=b).map(|t| self.track_len(t)).max().unwrap_or(1) - 1
    }

    /// A selection between these ends, kept on the grid.
    fn clamped(&self, anchor: Cursor, cursor: Cursor) -> Selection {
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
        Selection { anchor, cursor }
    }

    /// Replaces the selections, merging any that overlap into the rectangle covering them, as
    /// Helix does, and putting them in order.
    fn set_selections(&mut self, mut selections: Vec<Selection>, primary: usize) {
        let main = selections[primary].cursor;
        // Merging grows a rectangle, which can make it overlap one it didn't before.
        loop {
            let before = selections.len();
            let mut merged: Vec<Selection> = Vec::with_capacity(before);
            for s in selections {
                match merged.iter_mut().find(|m| m.region().overlaps(&s.region())) {
                    Some(m) => *m = m.merge(s),
                    None => merged.push(s),
                }
            }
            selections = merged;
            if selections.len() == before {
                break;
            }
        }
        selections.sort_by_key(|s| {
            let region = s.region();
            (region.first_step, region.first_track)
        });
        self.primary = selections
            .iter()
            .position(|s| s.region().contains(main.track, main.step))
            .unwrap_or(0);
        self.selections = selections;
    }

    /// Collapses to a single selection on this step, kept on the grid.
    fn select_point(&mut self, track: usize, step: usize) {
        let point = Cursor { track, step };
        self.set_selections(vec![self.clamped(point, point)], 0);
    }

    /// Replaces each selection with what `f` makes of it.
    fn map_selections(&mut self, f: impl Fn(&Self, Selection) -> Selection) {
        let selections = self.selections.iter().map(|&s| f(self, s)).collect();
        self.set_selections(selections, self.primary);
    }

    /// Moves every selection's cursor to where `target` says, collapsing the selection onto it
    /// unless in select mode.
    fn motion(&mut self, target: impl Fn(&Self, Cursor) -> Cursor) {
        let extend = self.mode == Mode::Select;
        self.map_selections(|e, s| {
            let to = target(e, s.cursor);
            e.clamped(if extend { s.anchor } else { to }, to)
        });
    }

    fn offset(&self, c: Cursor, tracks: isize, steps: isize) -> Cursor {
        Cursor {
            track: c
                .track
                .saturating_add_signed(tracks)
                .min(self.grid.tracks.len() - 1),
            step: c.step.saturating_add_signed(steps),
        }
    }

    fn beat(&self) -> usize {
        (self.grid.steps_per_beat as usize).max(1)
    }

    /// The next or previous step with a note on the cursor's track.
    fn find_note(&self, cursor: Cursor, forwards: bool) -> Option<usize> {
        let steps = &self.grid.tracks[cursor.track].steps;
        let has_note = |&i: &usize| steps[i].note.is_some();
        if forwards {
            (cursor.step + 1..steps.len()).find(has_note)
        } else {
            (0..cursor.step.min(steps.len())).rev().find(has_note)
        }
    }

    /// Like Helix's `C`: adds `n` copies of each selection, each the selection's own height
    /// further down (or up, if `by` is negative). The primary's furthest copy becomes primary.
    fn copy_rows(&mut self, by: isize) {
        let mut selections = self.selections.clone();
        let mut primary = self.primary;
        for (i, s) in self.selections.iter().enumerate() {
            let height = s.region().rows() as isize;
            for k in 1..=by.unsigned_abs() as isize {
                let shift = height * k * by.signum();
                let moved = |c: Cursor| {
                    c.step
                        .checked_add_signed(shift)
                        .map(|step| Cursor { step, ..c })
                };
                let (Some(anchor), Some(cursor)) = (moved(s.anchor), moved(s.cursor)) else {
                    break;
                };
                if anchor.step.min(cursor.step) > self.step_limit(anchor, cursor) {
                    break;
                }
                selections.push(self.clamped(anchor, cursor));
                if i == self.primary {
                    primary = selections.len() - 1;
                }
            }
        }
        self.set_selections(selections, primary);
    }

    /// Like Helix's `A-s`: splits each selection into one selection per row.
    fn split_rows(&mut self) {
        let mut selections = Vec::new();
        let mut primary = 0;
        for (i, s) in self.selections.iter().enumerate() {
            if i == self.primary {
                primary = selections.len();
            }
            let region = s.region();
            selections.extend(
                (region.first_step..=region.last_step).map(|step| Selection {
                    anchor: Cursor {
                        track: region.first_track,
                        step,
                    },
                    cursor: Cursor {
                        track: region.last_track,
                        step,
                    },
                }),
            );
        }
        self.set_selections(selections, primary);
    }

    /// Collapses each selection onto its first step, or the step after its last, for typing.
    fn insert_at(&mut self, after: bool) {
        self.mode = Mode::Insert;
        self.map_selections(|e, s| {
            let region = s.region();
            let at = Cursor {
                track: region.first_track,
                step: if after {
                    region.last_step + 1
                } else {
                    region.first_step
                },
            };
            e.clamped(at, at)
        });
    }

    /// Moves every cursor on by the auto-step amount, wrapping round its track.
    fn advance(&mut self) {
        self.map_selections(|e, s| {
            let c = s.cursor;
            let at = Cursor {
                step: (c.step + e.auto_step) % e.track_len(c.track),
                ..c
            };
            e.clamped(at, at)
        });
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

    /// Copies each selection's steps as its own clip, as Helix yanks one value per selection.
    fn yank(&mut self) {
        self.clipboard = self
            .selections
            .iter()
            .map(|s| {
                self.columns(s.region())
                    .map(|(t, steps)| self.grid.tracks[t].steps[steps].to_vec())
                    .collect()
            })
            .collect();
    }

    /// Applies `f` to every selected step. The selections stay, so the edit can be repeated.
    fn edit(&mut self, mut f: impl FnMut(&mut Step)) -> Vec<Effect> {
        self.checkpoint();
        let columns: Vec<_> = self
            .selections
            .iter()
            .flat_map(|s| self.columns(s.region()))
            .collect();
        for (t, steps) in &columns {
            for step in &mut self.grid.tracks[*t].steps[steps.clone()] {
                f(step);
            }
        }
        changed(columns.into_iter().map(|(t, _)| t))
    }

    /// Writes a clip `n` times down from each selection's first step, one column per track, then
    /// selects what was written. As in Helix, the selections take the clips in order, and the last
    /// clip is repeated if there are more selections than clips. `Place::Over` writes over the
    /// steps there; `Place::Insert` pushes them down first, and what's pushed past the end of the
    /// track falls off. Steps off the grid are skipped.
    fn paste(&mut self, place: Place, n: usize) -> Vec<Effect> {
        if self
            .clipboard
            .iter()
            .all(|clip| clip.iter().all(Vec::is_empty))
        {
            self.message = Some("nothing to paste".into());
            return vec![];
        }
        self.checkpoint();
        let mut tracks = vec![];
        let mut selections = self.selections.clone();
        // What each selection wrote: its tracks, its first step and how many rows.
        let mut writes = vec![];
        // Lowest first, so inserting doesn't move a step another selection has yet to paste at.
        let mut order: Vec<usize> = (0..selections.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(selections[i].region().first_step));
        for i in order {
            let clip = &self.clipboard[i.min(self.clipboard.len() - 1)];
            let rows = clip.first().map_or(0, Vec::len);
            if rows == 0 {
                continue;
            }
            let region = selections[i].region();
            let (track, step, total) = (region.first_track, region.first_step, rows * n);
            for (j, column) in clip.iter().enumerate() {
                let Some(steps) = self.grid.tracks.get_mut(track + j).map(|t| &mut t.steps) else {
                    break;
                };
                if step >= steps.len() {
                    continue;
                }
                let values = column.iter().cycle().take(total).cloned();
                if place == Place::Insert {
                    let len = steps.len();
                    steps.splice(step..step, values);
                    steps.truncate(len);
                } else {
                    for (slot, value) in steps[step..].iter_mut().zip(values) {
                        *slot = value;
                    }
                }
                tracks.push(track + j);
            }
            let last_track = track + clip.len() - 1;
            writes.push((i, track..=last_track, step, total));
        }
        for &(i, ref span, step, total) in &writes {
            // Moved down by what was inserted above it on the same track.
            let shift: usize = writes
                .iter()
                .filter(|w| place == Place::Insert && w.1.contains(span.start()) && w.2 < step)
                .map(|w| w.3)
                .sum();
            let first = step + shift;
            selections[i] = self.clamped(
                Cursor {
                    track: *span.start(),
                    step: first,
                },
                Cursor {
                    track: *span.end(),
                    step: first + total - 1,
                },
            );
        }
        self.set_selections(selections, self.primary);
        changed(tracks)
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
            track: self.cursor().track,
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
                track: self.cursor().track,
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

    /// Keeps the selections on the grid after tracks change length.
    pub(crate) fn clamp_selection(&mut self) {
        self.map_selections(|e, s| e.clamped(s.anchor, s.cursor));
    }

    /// Adds a track at `index`, with a new id.
    pub(crate) fn insert_track(&mut self, index: usize, track: Track) {
        self.grid.insert_track(index, track);
        self.ids.insert(index, TrackId(self.next_id));
        self.next_id += 1;
    }

    pub(crate) fn remove_track(&mut self, index: usize) {
        self.grid.remove_track(index);
        self.ids.remove(index);
    }

    pub(crate) fn move_track(&mut self, from: usize, to: usize) {
        self.grid.move_track(from, to);
        let id = self.ids.remove(from);
        self.ids.insert(to, id);
    }

    /// Saves the current state for undo. Call before every change to the song.
    /// Every track a selection reaches, in order.
    pub(crate) fn selected_tracks(&self) -> Vec<usize> {
        let mut tracks: Vec<usize> = self
            .selections()
            .iter()
            .flat_map(|s| s.region().first_track..=s.region().last_track)
            .collect();
        tracks.sort_unstable();
        tracks.dedup();
        tracks
    }

    pub(crate) fn checkpoint(&mut self) {
        self.undo.push(Snapshot {
            grid: self.grid.clone(),
            buses: self.buses.clone(),
            ids: self.ids.clone(),
            selections: self.selections.clone(),
            primary: self.primary,
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
            buses: std::mem::replace(&mut self.buses, snapshot.buses),
            ids: std::mem::replace(&mut self.ids, snapshot.ids),
            selections: std::mem::replace(&mut self.selections, snapshot.selections),
            primary: std::mem::replace(&mut self.primary, snapshot.primary),
        });
        self.mode = Mode::Normal;
        // Line the tracks up first, so the indices that follow are the restored ones.
        tracks_changed
            .then_some(Effect::TracksChanged)
            .into_iter()
            .chain((0..self.grid.tracks.len()).map(Effect::TrackChanged))
            .chain([Effect::MixerChanged])
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
            mix: Default::default(),
            instrument: Default::default(),
            effects: vec![],
            choke: 0,
            rules: vec![],
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![track("a", 16), track("b", 8)],
            groups: vec![],
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
        assert_eq!(e.cursor().step, 3);
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
        assert_eq!(e.cursor().step, 5);
        keys(&mut e, "12j");
        assert_eq!(e.cursor().step, 15, "clamped to the track");
        keys(&mut e, "l");
        assert_eq!(
            e.cursor(),
            Cursor { track: 1, step: 7 },
            "clamped to the shorter track"
        );
        assert_eq!(e.anchor(), e.cursor(), "motions collapse the selection");
    }

    #[test]
    fn goto_menu() {
        let mut e = editor();
        keys(&mut e, "g");
        let infobox = e.infobox().unwrap();
        assert_eq!(infobox.title, "goto");
        assert!(infobox.items.contains(&("e".into(), "last step")));
        keys(&mut e, "e");
        assert_eq!(e.cursor().step, 15);
        assert_eq!(e.infobox(), None);
        keys(&mut e, "gl");
        assert_eq!(e.cursor(), Cursor { track: 1, step: 7 });
        keys(&mut e, "ghgg");
        assert_eq!(e.cursor(), Cursor::default());
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
        assert_eq!(e.anchor(), e.cursor());
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
        keys(&mut e, ";gg10jp");
        assert_eq!(
            &pitches(&e, 0)[10..14],
            [Some(60), Some(62), Some(64), None]
        );
        assert_eq!(
            (e.selection().first_step, e.selection().last_step),
            (10, 12)
        );
        keys(&mut e, ";gg");
        notes(&mut e, "v");
        keys(&mut e, "ggp");
        assert_eq!(
            &pitches(&e, 0)[..3],
            [Some(60), Some(62), Some(64)],
            "onto the first step"
        );
    }

    #[test]
    fn paste_inserting_pushes_the_rest_down() {
        let mut e = editor();
        notes(&mut e, "zx");
        let len = e.grid.tracks[0].steps.len();
        e.grid.tracks[0].steps[len - 1] = e.grid.tracks[0].steps[0].clone();
        keys(&mut e, "ggvjy");
        keys(&mut e, "ggP");
        assert_eq!(
            &pitches(&e, 0)[..4],
            [Some(60), Some(62), Some(60), Some(62)],
            "onto the first step, pushing the rest down"
        );
        assert_eq!(pitches(&e, 0)[len - 1], None, "the end falls off");
        assert_eq!(e.grid.tracks[0].steps.len(), len);
        assert_eq!((e.selection().first_step, e.selection().last_step), (0, 1));

        // Two selections on one track: each lands where it was, and the lower one moves down
        // by what went in above it.
        keys(&mut e, ";gg");
        e.handle(Key::Alt('d'));
        keys(&mut e, "y4jC");
        keys(&mut e, "4jP");
        let s = e.selections();
        assert_eq!(
            (s[0].region().first_step, s[1].region().first_step),
            (8, 10)
        );
    }

    #[test]
    fn delete_copies_and_collapses() {
        let mut e = editor();
        notes(&mut e, "zx");
        keys(&mut e, "ggvjd");
        assert_eq!(&pitches(&e, 0)[..2], [None, None]);
        assert_eq!(e.anchor(), e.cursor());
        keys(&mut e, "4jp");
        assert_eq!(&pitches(&e, 0)[4..6], [Some(60), Some(62)]);
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
        assert_eq!(e.cursor().step, 4);
        keys(&mut e, "j]b");
        assert_eq!(e.cursor().step, 8);
        keys(&mut e, "[b");
        assert_eq!(e.cursor().step, 4);
        keys(&mut e, "j[b");
        assert_eq!(e.cursor().step, 4, "back to the start of this beat first");
        keys(&mut e, "]m");
        assert_eq!(e.cursor().step, 15, "clamped to the track");
        keys(&mut e, "[m");
        assert_eq!(e.cursor().step, 0);
        keys(&mut e, "2]b");
        assert_eq!(e.cursor().step, 8);
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
        keys(&mut e, ":fo");
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
        assert_eq!(e.cursor().step, 4);
        keys(&mut e, "w");
        assert_eq!(e.cursor().step, 14);
        keys(&mut e, "w");
        assert_eq!(e.cursor().step, 14, "stays put after the last note");
        keys(&mut e, "b");
        assert_eq!(e.cursor().step, 4);
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
        assert_eq!(e.message.as_deref(), Some("semitones is -128 to 127"));
        command(&mut e, "bpm 1000");
        assert_eq!(e.message.as_deref(), Some("bpm is 20 to 400"));
        assert_eq!(e.mode, Mode::Normal);
        command(&mut e, "bpm");
        assert_eq!(
            e.message.as_deref(),
            Some("bpm is 20 to 400"),
            "missing values are flagged on enter"
        );
        assert_eq!(e.bpm, 120.0);
    }

    #[test]
    fn command_problems_show_while_typing() {
        let mut e = editor();
        assert_eq!(e.command_problem(), None, "not on the command line");
        keys(&mut e, ":bpm 4");
        assert_eq!(e.command_problem(), None, "could still become 40");
        keys(&mut e, "000");
        assert_eq!(
            e.command_problem(),
            Some(Problem {
                range: 4..8,
                message: "bpm is 20 to 400".into()
            })
        );
        for _ in 0..3 {
            e.handle(Key::Backspace);
        }
        keys(&mut e, "0 ");
        assert_eq!(e.command_problem(), None);
        e.handle(Key::Enter);
        assert_eq!(e.bpm, 40.0);
    }

    #[test]
    fn length_and_tempo() {
        let mut e = editor();
        keys(&mut e, "ge");
        assert_eq!(command(&mut e, "len 4"), [Effect::TrackChanged(0)]);
        assert_eq!(e.grid.tracks[0].steps.len(), 4);
        assert_eq!(e.cursor().step, 3);
        assert_eq!(command(&mut e, "bpm 90"), [Effect::Bpm(90.0)]);
        assert_eq!(command(&mut e, "bpm 122.5"), [Effect::Bpm(122.5)]);
        command(&mut e, "follow");
        assert!(!e.follow);
    }

    #[test]
    fn mixer_commands_change_every_selected_track() {
        let mut e = editor();
        let all = [Effect::TrackChanged(0), Effect::TrackChanged(1)];
        keys(&mut e, "%");
        assert_eq!(command(&mut e, "level -6.5"), all);
        assert_eq!(command(&mut e, "pan -40"), all);
        assert_eq!(command(&mut e, "send -12"), all);
        let mix = e.grid.tracks[1].mix;
        assert_eq!((mix.level, mix.pan, mix.send), (-6.5, -0.4, -12.0));

        // Just the cursor's track, then both: one was already muted, so both end up muted.
        keys(&mut e, ";gh");
        assert_eq!(command(&mut e, "mute"), [Effect::TrackChanged(0)]);
        keys(&mut e, "%");
        command(&mut e, "mute");
        assert!(e.grid.tracks.iter().all(|t| t.mix.mute));
        command(&mut e, "mute");
        assert!(e.grid.tracks.iter().all(|t| !t.mix.mute));
        command(&mut e, "solo");
        assert!(e.grid.tracks.iter().all(|t| t.mix.solo));

        keys(&mut e, "u");
        assert!(e.grid.tracks.iter().all(|t| !t.mix.solo), "undoable");
        // `t` opens the track menu: `tm` mutes, `ts` solos.
        keys(&mut e, ";gh");
        assert_eq!(keys(&mut e, "tm"), [Effect::TrackChanged(0)]);
        assert!(e.grid.tracks[0].mix.mute && !e.grid.tracks[1].mix.mute);
        keys(&mut e, "%ts");
        assert!(e.grid.tracks.iter().all(|t| t.mix.solo));

        // `tt` selects whole tracks.
        keys(&mut e, ";ghjjjtt");
        let region = e.selections()[0].region();
        assert_eq!((region.first_track, region.last_track), (0, 0));
        assert_eq!((region.first_step, region.last_step), (0, 15));

        // `ty` and `tp` copy and paste whole tracks.
        keys(&mut e, "%ty");
        assert_eq!(e.message.as_deref(), Some("copied 2 tracks"));
        keys(&mut e, ";gltp");
        assert_eq!(e.grid.tracks.len(), 4);
        assert_eq!(e.cursor().track, 2, "on the first pasted track");
        let names: Vec<&str> = e.grid.tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "a", "b"]);
        keys(&mut e, "ghtP");
        assert_eq!(e.grid.tracks.len(), 6);
        assert_eq!(e.grid.tracks[0].name, "a");
        assert_eq!(e.cursor().track, 0);
        keys(&mut e, "%td");
        assert_eq!(e.message.as_deref(), Some("can't remove every track"));
        // `td` copies what it removes.
        keys(&mut e, ";ghtd");
        assert_eq!(e.grid.tracks.len(), 5);
        keys(&mut e, "gltp");
        assert_eq!(e.grid.tracks.len(), 6);
        assert_eq!(e.grid.tracks[5].name, "a");
        keys(&mut e, "uuuu");
        assert_eq!(e.grid.tracks.len(), 2, "all undone");

        // And `ta`, `td`, `th`, `tl` add, remove and move tracks.
        keys(&mut e, ";gh");
        assert_eq!(keys(&mut e, "ta"), [Effect::TracksChanged]);
        assert_eq!(e.grid.tracks.len(), 3);
        assert_eq!(e.cursor().track, 1, "on the new track");
        keys(&mut e, "tl");
        assert_eq!(e.grid.tracks[2].name, "track 3");
        keys(&mut e, "th");
        assert_eq!(e.grid.tracks[1].name, "track 3");
        keys(&mut e, "td");
        assert_eq!(e.grid.tracks.len(), 2);
        keys(&mut e, "tdtd");
        assert_eq!(e.grid.tracks.len(), 1);
        assert!(e.message.is_some(), "can't remove the only track");

        e.handle(Key::Char(':'));
        keys(&mut e, "level 7");
        assert!(e.command_problem().is_some(), "6 dB at most");
    }

    #[test]
    fn edits_mark_the_song_modified_until_saved() {
        let mut e = editor();
        assert!(!e.modified());
        notes(&mut e, "z");
        assert!(e.modified());
        keys(&mut e, "u");
        assert!(!e.modified(), "undone back to how it was");
        command(&mut e, "bpm 90");
        assert!(e.modified(), "tempo is part of the song");
        e.saved("song.lattice".into());
        assert!(!e.modified());
        assert_eq!(e.path, Some("song.lattice".into()));
        assert_eq!(e.song().name, "song");
        assert_eq!(e.song().bpm, 90.0);

        command(&mut e, "track-add");
        assert!(e.modified(), "adding a track");
        keys(&mut e, "u");
        assert!(!e.modified());
        command(&mut e, "track-move 1");
        assert!(e.modified(), "moving a track");
        keys(&mut e, "u");
        command(&mut e, "track-remove");
        assert!(e.modified(), "removing a track");
    }

    #[test]
    fn write_needs_a_file_name_once() {
        let mut e = editor();
        assert_eq!(command(&mut e, "w"), []);
        assert_eq!(e.message.as_deref(), Some("no file name yet: :w PATH"));
        assert_eq!(
            command(&mut e, "w my song.lattice"),
            [Effect::Save("my song.lattice".into())],
            "paths can contain spaces"
        );
        e.saved("my song.lattice".into());
        assert_eq!(
            command(&mut e, "write"),
            [Effect::Save("my song.lattice".into())]
        );
        assert_eq!(
            command(&mut e, "wq other.lattice"),
            [Effect::Save("other.lattice".into()), Effect::Quit]
        );
    }

    #[test]
    fn quit_and_open_refuse_to_lose_changes() {
        let mut e = editor();
        assert_eq!(command(&mut e, "q"), [Effect::Quit], "nothing to lose");
        notes(&mut e, "z");
        assert_eq!(command(&mut e, "q"), []);
        assert!(e.message.as_ref().unwrap().starts_with("unsaved changes"));
        assert_eq!(command(&mut e, "o b.lattice"), []);
        assert!(e.message.as_ref().unwrap().starts_with("unsaved changes"));
        assert_eq!(
            command(&mut e, "o! b.lattice"),
            [Effect::Open("b.lattice".into())]
        );
        assert_eq!(command(&mut e, "quit!"), [Effect::Quit]);
        assert_eq!(command(&mut e, "q!"), [Effect::Quit]);
        command(&mut e, "open!");
        assert_eq!(e.message.as_deref(), Some("expected a path"));
    }

    #[test]
    fn loading_replaces_the_song() {
        let mut e = editor();
        notes(&mut e, "zx");
        keys(&mut e, "lC");
        let before = e.track_ids().to_vec();
        let mut song = editor().song();
        song.bpm = 140.0;
        song.grids[0].tracks.truncate(1);
        let effects = e.load(song, "new.lattice".into());
        assert_eq!(
            effects,
            [
                Effect::TracksChanged,
                Effect::MixerChanged,
                Effect::Bpm(140.0)
            ]
        );
        assert_eq!(e.grid.tracks.len(), 1);
        assert!(
            !before.contains(&e.track_ids()[0]),
            "loaded tracks get new ids"
        );
        assert_eq!(pitches(&e, 0)[0], None);
        assert_eq!(e.selections().len(), 1);
        assert_eq!(e.cursor(), Cursor::default());
        assert!(!e.modified());
        assert_eq!(e.path, Some("new.lattice".into()));
        keys(&mut e, "u");
        assert_eq!(e.message.as_deref(), Some("nothing to undo"));
    }

    fn steps(e: &Editor) -> Vec<(usize, usize)> {
        e.selections()
            .iter()
            .map(|s| (s.cursor.track, s.cursor.step))
            .collect()
    }

    #[test]
    fn copy_selection_onto_next_rows() {
        let mut e = editor();
        keys(&mut e, "C");
        assert_eq!(steps(&e), [(0, 0), (0, 1)]);
        assert_eq!(e.primary(), 1, "the new copy is primary");
        keys(&mut e, "2j");
        assert_eq!(steps(&e), [(0, 2), (0, 3)], "motions move every selection");
        assert_eq!(e.cursor().step, 3);
        keys(&mut e, "gg");
        assert_eq!(steps(&e), [(0, 0)], "selections that meet merge");
        keys(&mut e, "vjC");
        let regions: Vec<_> = e
            .selections()
            .iter()
            .map(|s| (s.region().first_step, s.region().last_step))
            .collect();
        assert_eq!(
            regions,
            [(0, 1), (2, 3)],
            "copied by the selection's height"
        );
        e.handle(Key::Alt('C'));
        assert_eq!(e.selections().len(), 2, "nothing above the first row");
        keys(&mut e, "v;gg20C");
        assert_eq!(e.selections().len(), 16, "stops at the end of the track");
        assert_eq!(e.cursor().step, 15);
    }

    #[test]
    fn edits_apply_to_every_selection() {
        let mut e = editor();
        notes(&mut e, "zxcv");
        keys(&mut e, "ggCC");
        e.handle(Key::Ctrl('a'));
        assert_eq!(
            &pitches(&e, 0)[..4],
            [Some(61), Some(63), Some(65), Some(65)]
        );
        assert_eq!(e.selections().len(), 3, "edits keep the selections");
        keys(&mut e, "u");
        assert_eq!(
            &pitches(&e, 0)[..4],
            [Some(60), Some(62), Some(64), Some(65)]
        );
        assert_eq!(e.selections().len(), 3, "one undo step for all of them");
        keys(&mut e, "d");
        assert_eq!(&pitches(&e, 0)[..4], [None, None, None, Some(65)]);
        keys(&mut e, "rq");
        assert_eq!(
            &pitches(&e, 0)[..4],
            [Some(72), Some(72), Some(72), Some(65)]
        );
    }

    #[test]
    fn select_notes_within_the_selections() {
        let mut e = editor();
        notes(&mut e, "zxz");
        keys(&mut e, "lgg");
        notes(&mut e, ",");
        keys(&mut e, "%s");
        assert!(matches!(&e.mode, Mode::Command(p) if p.kind == PromptKind::Select));
        keys(&mut e, "C");
        assert_eq!(
            steps(&e),
            [(0, 0), (1, 0), (0, 2)],
            "previews while typing, in order down the grid"
        );
        e.handle(Key::Enter);
        assert_eq!(e.mode, Mode::Normal);
        e.handle(Key::Ctrl('a'));
        assert_eq!(&pitches(&e, 0)[..3], [Some(61), Some(62), Some(61)]);
        assert_eq!(pitches(&e, 1)[0], Some(73));

        keys(&mut e, "%sC#5");
        assert_eq!(steps(&e), [(1, 0)]);
        e.handle(Key::Esc);
        assert_eq!(e.selections().len(), 1);
        assert_eq!(e.selection().last_step, 15, "esc puts the selection back");

        keys(&mut e, "s*");
        assert_eq!(e.selections().len(), 4);
        keys(&mut e, "9");
        assert_eq!(e.selections().len(), 1, "no match previews the original");
        e.handle(Key::Enter);
        assert_eq!(e.message.as_deref(), Some("no matches"));
        assert_eq!(e.selection().last_step, 15);
    }

    #[test]
    fn keep_and_remove_by_note() {
        let mut e = editor();
        notes(&mut e, "zx");
        keys(&mut e, "ggCC");
        keys(&mut e, "KC");
        e.handle(Key::Enter);
        assert_eq!(steps(&e), [(0, 0)]);
        keys(&mut e, "CC");
        e.handle(Key::Alt('K'));
        keys(&mut e, "*");
        e.handle(Key::Enter);
        assert_eq!(steps(&e), [(0, 2)], "removes the ones with any note");
    }

    #[test]
    fn primary_selection() {
        let mut e = editor();
        keys(&mut e, "CC");
        assert_eq!(e.primary(), 2);
        keys(&mut e, ")");
        assert_eq!(e.cursor().step, 0, "rotates round");
        keys(&mut e, "((");
        assert_eq!(e.cursor().step, 1);
        e.handle(Key::Alt(','));
        assert_eq!(steps(&e), [(0, 0), (0, 2)]);
        assert_eq!(e.cursor().step, 2, "the next one becomes primary");
        keys(&mut e, ",");
        assert_eq!(steps(&e), [(0, 2)]);
        e.handle(Key::Alt(','));
        assert_eq!(e.selections().len(), 1, "the last one stays");
    }

    #[test]
    fn collapse_flip_split_and_merge_each() {
        let mut e = editor();
        keys(&mut e, "vjC");
        e.handle(Key::Alt(';'));
        assert_eq!(steps(&e), [(0, 0), (0, 2)]);
        keys(&mut e, ";");
        assert!(e.selections().iter().all(|s| s.anchor == s.cursor));
        e.handle(Key::Alt('-'));
        assert_eq!(e.selections().len(), 1);
        assert_eq!((e.selection().first_step, e.selection().last_step), (0, 2));
        e.handle(Key::Alt('s'));
        assert_eq!(e.selections().len(), 3, "a selection per row");
    }

    #[test]
    fn yank_and_paste_one_value_per_selection() {
        let mut e = editor();
        notes(&mut e, "zx");
        keys(&mut e, "ggCy");
        assert_eq!(e.message.as_deref(), Some("copied 2 selections"));
        keys(&mut e, "4jCp");
        assert_eq!(
            &pitches(&e, 0)[4..7],
            [Some(60), Some(62), Some(62)],
            "in order, repeating the last"
        );
        keys(&mut e, ",8jp");
        assert_eq!(
            &pitches(&e, 0)[13..15],
            [None, Some(60)],
            "one selection takes the first"
        );
    }

    #[test]
    fn insert_at_every_cursor() {
        let mut e = editor();
        keys(&mut e, "lCi");
        assert_eq!(steps(&e), [(1, 0), (1, 1)]);
        keys(&mut e, "z");
        assert_eq!(&pitches(&e, 1)[..3], [Some(60), Some(60), None]);
        assert_eq!(steps(&e), [(1, 1), (1, 2)]);
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
        assert_eq!(
            e.cursor(),
            Cursor { track: 1, step: 5 },
            "onto the new track"
        );
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
        assert_eq!(e.cursor(), Cursor { track: 0, step: 7 });
        assert_eq!(e.anchor(), e.cursor());
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
        assert_eq!(e.cursor().track, 0, "the cursor goes with it");
        command(&mut e, "tmv +5");
        assert_eq!(names(&e), ["a", "b", "c"]);
        assert_eq!(command(&mut e, "tmv 1"), [], "already at the end");
        command(&mut e, "tmv left");
        assert_eq!(e.message.as_deref(), Some("tracks to move is -16 to 16"));
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
                .chain([Effect::MixerChanged])
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&mut e, "u"), all(3), "brings back the removed track");
        assert_eq!(names(&e), ["c", "a", "b"]);
        assert_eq!(ids(&e), [TrackId(2), TrackId(0), TrackId(1)], "with its id");
        assert_eq!(pitches(&e, 1)[0], Some(60), "and its notes");
        assert_eq!(e.cursor().track, 1);
        keys(&mut e, "u");
        assert_eq!(names(&e), ["a", "b", "c"]);
        keys(&mut e, "u");
        assert_eq!(names(&e), ["a", "b"]);
        assert_eq!(e.cursor().track, 1);
        assert_eq!(
            keys(&mut e, "u"),
            [
                Effect::TrackChanged(0),
                Effect::TrackChanged(1),
                Effect::MixerChanged
            ]
        );
        assert_eq!(pitches(&e, 0)[0], None);

        keys(&mut e, "UUU");
        assert_eq!(names(&e), ["c", "a", "b"]);
        assert_eq!(keys(&mut e, "U"), all(2));
        assert_eq!(names(&e), ["c", "b"]);
        assert_eq!(ids(&e), [TrackId(2), TrackId(1)]);
        assert_eq!(e.cursor().track, 1);
    }

    #[test]
    fn track_commands_collapse_multiple_selections() {
        let mut e = editor();
        keys(&mut e, "CC");
        command(&mut e, "track-add");
        assert_eq!(steps(&e), [(1, 2)]);
        keys(&mut e, "u");
        assert_eq!(e.selections().len(), 3, "undo brings them back");
    }

    #[test]
    fn note_prompts_are_not_commands() {
        let mut e = editor();
        keys(&mut e, "%s");
        assert_eq!(e.command_hint().as_deref(), Some("NOTE…"));
        keys(&mut e, "bpm");
        assert_eq!(e.command_problem(), None);
        assert!(e.completions().is_empty());
        assert_eq!(e.command_spec(), None);
    }
}
