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

// Tests may unwrap and panic: that's how they fail.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::indexing_slicing
    )
)]

mod chains;
mod command;
mod devices;
mod export;
mod fields;
mod fx_page;
mod group;
mod help;
mod inst_page;
mod keymap;
mod locks;
pub use fields::StepField;
pub use locks::{ColumnLabel, lock_text};
mod looper;
mod macros;
mod mixer;
mod pads;
mod pattern;
mod patterns;
mod presets;
mod rearrange;
mod record;
mod song;
mod suggest;
mod tutor;

pub use command::{COMMANDS, Kind, Param, Problem, Spec, Value};
pub use devices::{Device, Devices, Direction};
pub use export::{Export, Part, SAMPLES, file_name, scratch};
pub use fx_page::{FxCursor, FxLine};
pub use help::{Help, HelpLine};
pub use inst_page::{InstCursor, InstLine};
pub use keymap::{Action, Binding, Key, Keymap, Menu, NoteLayout};
pub use macros::target_name;
pub use mixer::{Field, MixerCursor, PARAM_COLUMNS, Page, Row, Shown, Strip};
pub use pads::{HARD, MEDIUM, PadMode, SCALES, SOFT, Scale};
pub use patterns::Next;
pub use record::Transport;
pub use suggest::{Completion, word_start};
pub use tutor::TutorView;

use std::path::{Path, PathBuf};

use lattice_core::{
    Arrangement, Buses, FxChain, Grid, Lock, MAX_STEPS, MAX_TRACKS, Note, Preset, Song, Step, Track,
};

use crate::pattern::Pattern;
use crate::record::Held;

const UNDO_LIMIT: usize = 500;
const PAGE: usize = 16;
const DEFAULT_VELOCITY: u8 = 100;

/// The digit a numpad key types.
fn pad_digit(n: u8) -> char {
    char::from(b'0' + n.min(9))
}

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

/// Most commands the command line's history keeps.
pub const MAX_HISTORY: usize = 500;

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
    /// The command from the history shown, while going through it with `↑` `↓`.
    history: Option<usize>,
    /// What was typed before going through the history: the start the commands shown share.
    prefix: String,
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
    /// The meter changed: send it to the engine, before the tempo. `groups` has a bit for each
    /// beat that starts a group, as [`Grid::group_starts`](lattice_core::Grid::group_starts).
    Meter {
        steps_per_beat: u32,
        beats_per_bar: u32,
        groups: u32,
    },
    /// Audition a note on a track's instrument, briefly.
    Preview {
        track: usize,
        pitch: u8,
    },
    /// Audition a note until a [`Effect::Release`] for it: a note key played while the frontend
    /// reports key releases ([`Editor::releases`]).
    Hold {
        track: usize,
        pitch: u8,
    },
    /// Audition a note again on every step that's a multiple of `every`, in time, until a
    /// [`Effect::Release`] for it; `now` also plays it at once.
    Repeat {
        track: usize,
        pitch: u8,
        every: usize,
        now: bool,
    },
    /// Hold a setting on a track now, as its step's lock would: a lock recorded as it plays.
    Lock {
        track: usize,
        lock: Lock,
    },
    /// The player's muted or soloed this track themselves, so the conductor's mute and solo
    /// on it are over.
    Unconduct(usize),
    /// A fill starts, or ends.
    Fill(bool),
    /// The key holding this note has come up: end it.
    Release {
        track: usize,
        pitch: u8,
    },
    /// Write [`Editor::song`] here, then call [`Editor::saved`]. If the save fails, drop the
    /// effects that follow it, so `:wq` doesn't quit.
    Save(PathBuf),
    /// Read the song here and pass it to [`Editor::load`], or start a new song if there's no such
    /// file.
    Open(PathBuf),
    /// Render this to a WAV file.
    Export(Box<Export>),
    /// Allow this many more milliseconds of latency when recording the input, or fewer.
    Latency(i32),
    /// Switch to this device, by id, or the system's default; see [`devices`].
    Device(Direction, Option<String>),
    /// Save this chain under this name, for `:chain-load` to put on any strip.
    SaveChain(String, Box<FxChain>),
    /// Read the chain saved under this name, and give it to [`Editor::put_chain`].
    LoadChain(String),
    /// Save this preset under this name, for `:preset` to put on any track.
    SavePreset(String, Box<Preset>),
    /// Read the preset saved under this name, and give it to [`Editor::put_preset`], for the
    /// track the preset commands work on.
    LoadPreset(String),
    /// Read this text file and give it to [`Editor::put_face_art`] for this track.
    LoadFaceArt(usize, PathBuf),
    /// Stop the song if it's playing, and silence everything: every instrument and effect
    /// starts afresh, so no note, echo or tail rings on.
    Hush,
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
    /// `f`: the next key picks a field of the open track.
    Field,
    /// `r`: the next piano key fills the selection.
    Replace,
}

/// What the infobox shows while a key is pending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Infobox {
    pub title: String,
    /// Key and description, in display order.
    pub items: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct Snapshot {
    grid: Grid,
    arrangement: Arrangement,
    buses: Buses,
    ids: Vec<TrackId>,
    selections: Vec<Selection>,
    primary: usize,
    /// The song page's loop, which goes by entries.
    looped: Option<(usize, usize)>,
    /// The tempo, which only `:meter` changes along with the rest, so only comes back with it.
    bpm: f64,
}

/// What one selection copied: a column of steps per track.
type Clip = Vec<Vec<Step>>;

/// The song as last saved or opened, to tell whether it's been modified since.
#[derive(Debug, Clone)]
struct Saved {
    grid: Grid,
    arrangement: Arrangement,
    buses: Buses,
    bpm: f64,
}

#[derive(Debug, Clone)]
pub struct Editor {
    /// The tracks, with the lanes of the pattern being edited.
    pub grid: Grid,
    /// The patterns and the order they play in.
    pub arrangement: Arrangement,
    /// The pattern to change to when the playing one ends, if one's been picked while playing.
    pub cue: Option<usize>,
    /// While the song plays from the arrangement: which entry, and which repeat of it.
    pub song_position: Option<(usize, u32)>,
    /// The send and master buses.
    pub buses: Buses,
    pub bpm: f64,
    /// Where the song is saved; `None` until it's given a file name.
    pub path: Option<PathBuf>,
    /// The last selection exported, which `:sample` plays by default.
    last_export: Option<PathBuf>,
    /// The track armed to record the audio input; see [`crate::looper`].
    armed: Option<looper::Armed>,
    /// Milliseconds added to the latency the looper allows for, or taken off.
    pub latency: i32,
    /// The `:` commands run, oldest first, for `↑` `↓` on the command line. The frontend keeps
    /// them from one run to the next.
    pub history: Vec<String>,
    /// The chains `:chain-load` can load, by name, as the frontend found them.
    pub chains: Vec<String>,
    /// The presets `:preset` can load, by name, as the frontend found them.
    pub presets: Vec<String>,
    /// The audio devices, as the frontend found them.
    pub inputs: Devices,
    pub outputs: Devices,
    saved: Saved,
    pub mode: Mode,
    pub view: View,
    pub page: Page,
    /// Where the cursor is on the mixer page; see [`Editor::mixer_cursor`].
    mixer: MixerCursor,
    fx: FxCursor,
    inst: InstCursor,
    /// Where the cursor is on the song page; see [`Editor::song_cursor`].
    song: song::SongCursor,
    /// Where the help page is scrolled to.
    pub help: Help,
    /// After a first `L` on the mixer page, the settings to lock the changes to.
    locking: Option<locks::Locking>,
    /// Tracks `te` keeps open, showing their fields, as well as the cursor's.
    open: Vec<TrackId>,
    /// The field the cursor's in on an open track; see [`Editor::field`].
    field: StepField,
    /// The conductor's mutes and solos (`M2`, `S-2`) over the tracks' own while the song plays:
    /// shown, but not the song's.
    conducted: Vec<(TrackId, Option<bool>, Option<bool>)>,
    /// What's been typed into a field so far.
    typing: fields::Typing,
    /// Values copied from a field, one list per selection, `None` where there was none.
    field_clipboard: Vec<Vec<Option<fields::Value>>>,
    /// The lock last recorded from the mixer, and its track, to nudge on from.
    live_lock: Option<(TrackId, Lock)>,
    /// The lesson, while the tutor's running.
    tutor: Option<tutor::Tutor>,
    /// Octave of the lower piano row in insert mode.
    pub octave: u8,
    /// Steps to move after entering a note.
    pub auto_step: usize,
    /// How hard notes are played and entered.
    pub velocity: u8,
    /// What the numpad's pads do.
    pub pad_mode: PadMode,
    /// The scale pads play outside a group, as an index into [`SCALES`], and its root above C.
    pub scale: usize,
    pub root: u8,
    pub follow: bool,
    /// Whether the metronome clicks, saved with the song. Changing it alone doesn't count as an
    /// unsaved change: it's how the player listens, not the music.
    pub click: bool,
    /// Whether the frontend shows the keys as they're pressed, for screencasts.
    pub show_keys: bool,
    /// Whether notes played in insert mode while the song plays go in at the playhead.
    pub record: bool,
    /// Bars of clicks before the song starts, when record is armed.
    pub count_in: u32,
    /// Where playback is; see [`Editor::set_transport`].
    transport: Option<Transport>,
    /// The pass being recorded, all one undo.
    take: Option<u64>,
    /// Recorded notes whose keys are still down.
    held: Vec<Held>,
    /// Whether the frontend reports key releases, so previews are held until their key comes up.
    pub releases: bool,
    /// A fill's playing: while the fill key's held, or until it's pressed again.
    pub fill: bool,
    /// Previewed notes, by the key that's holding each, and whether each is repeating.
    sounding: Vec<(Key, TrackId, u8, bool)>,
    /// Whether the numpad's `0` is held, so pads repeat.
    repeat: bool,
    /// How often pads repeat, in steps.
    pub repeat_every: usize,
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
    /// Tracks with faces whose instrument page column shows all their settings instead.
    inst_opened: std::collections::BTreeSet<TrackId>,
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

/// The cursor needs a track to be on, so a song without one gets one.
fn give_track(grid: &mut Grid, arrangement: &mut Arrangement) {
    if grid.tracks.is_empty() {
        let track = Track {
            name: "track 1".into(),
            steps: vec![Step::default(); command::NEW_TRACK_STEPS],
            ..Track::default()
        };
        arrangement.insert_track(0, track.steps.len());
        grid.tracks.push(track);
    }
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
    pub fn new(mut grid: Grid, bpm: f64) -> Self {
        let mut arrangement = Arrangement::default();
        give_track(&mut grid, &mut arrangement);
        let tracks = grid.tracks.len() as u32;
        Self {
            ids: (0..tracks).map(TrackId).collect(),
            inst_opened: Default::default(),
            next_id: tracks,
            saved: Saved {
                grid: grid.clone(),
                arrangement: arrangement.clone(),
                buses: Buses::default(),
                bpm,
            },
            grid,
            arrangement,
            cue: None,
            song_position: None,
            buses: Buses::default(),
            bpm,
            path: None,
            last_export: None,
            armed: None,
            latency: 0,
            history: vec![],
            chains: lattice_core::CHAINS.iter().map(|p| p.to_string()).collect(),
            presets: lattice_core::PRESETS
                .iter()
                .map(|p| p.to_string())
                .collect(),
            inputs: Devices::default(),
            outputs: Devices::default(),
            mode: Mode::Normal,
            view: View::Source,
            page: Page::Pattern,
            mixer: MixerCursor::default(),
            fx: FxCursor::default(),
            inst: InstCursor::default(),
            song: Default::default(),
            help: Help::default(),
            locking: None,
            open: vec![],
            field: StepField::Note,
            conducted: vec![],
            typing: Default::default(),
            field_clipboard: vec![],
            live_lock: None,
            tutor: None,
            octave: 4,
            auto_step: 1,
            velocity: DEFAULT_VELOCITY,
            pad_mode: PadMode::Play,
            scale: 0,
            root: 0,
            follow: true,
            click: true,
            show_keys: false,
            record: false,
            count_in: 1,
            transport: None,
            take: None,
            held: Vec::new(),
            releases: false,
            fill: false,
            sounding: Vec::new(),
            repeat: false,
            repeat_every: 1,
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

    /// Starts with these patterns, the grid's tracks holding the current one's lanes.
    pub fn with_arrangement(mut self, arrangement: Arrangement) -> Self {
        self.saved.arrangement = arrangement.clone();
        self.arrangement = arrangement;
        self
    }

    /// Each track's id, in the order of `grid.tracks`.
    pub fn track_ids(&self) -> &[TrackId] {
        &self.ids
    }

    /// Whether the song differs from when it was last saved or opened, tracks added, removed
    /// and moved included. Undoing back to that point counts as unmodified.
    pub fn modified(&self) -> bool {
        // The tutor's song is only for practice.
        !self.tutoring()
            && (self.grid != self.saved.grid
                || self.arrangement != self.saved.arrangement
                || self.buses != self.saved.buses
                || self.bpm != self.saved.bpm)
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
            grid: self.grid.clone(),
            arrangement: self.arrangement.clone(),
            buses: self.buses.clone(),
            click: self.click,
        }
    }

    /// The frontend has written [`Editor::song`] to `path`.
    pub fn saved(&mut self, path: PathBuf) {
        self.message = Some(format!("written {}", path.display()));
        self.path = Some(path);
        self.saved = Saved {
            grid: self.grid.clone(),
            arrangement: self.arrangement.clone(),
            buses: self.buses.clone(),
            bpm: self.bpm,
        };
    }

    /// Replaces the song with one read from `path`, starting afresh: its tracks get new ids, there's
    /// one selection at the top left, and the undo history is cleared. Returns the effects that
    /// send the new song to the engine.
    pub fn load(&mut self, song: Song, path: PathBuf) -> Vec<Effect> {
        let effects = self.replace_song(song, Some(path.clone()));
        self.message = Some(format!("opened {}", path.display()));
        self.tutor = None;
        self.armed = None;
        effects
    }

    /// `:new [PATH]`: an empty song, of one track of sixteen steps, at the tempo and steps per
    /// beat there are now, to be saved at `path` if it's given.
    pub(crate) fn new_song(&mut self, path: &str) -> Result<Vec<Effect>, String> {
        let path = (!path.is_empty()).then(|| PathBuf::from(path));
        if let Some(path) = &path
            && path.exists()
        {
            return Err(format!(
                "{} is already there: :o to open it",
                path.display()
            ));
        }
        let track = Track {
            name: "track 1".into(),
            steps: vec![Step::default(); 16],
            ..Track::default()
        };
        let song = Song {
            name: "new".into(),
            bpm: self.bpm,
            grid: Grid {
                name: "new".into(),
                steps_per_beat: self.grid.steps_per_beat,
                beats_per_bar: self.grid.beats_per_bar,
                grouping: self.grid.grouping.clone(),
                tracks: vec![track],
                groups: vec![],
            },
            arrangement: Default::default(),
            buses: Default::default(),
            click: self.click,
        };
        let effects = self.replace_song(song, path.clone());
        self.page = Page::Pattern;
        self.mode = Mode::Normal;
        self.tutor = None;
        self.armed = None;
        self.message = Some(match path {
            Some(path) => format!("new song: :w saves it as {}", path.display()),
            None => "new song: :w PATH saves it".into(),
        });
        Ok(effects)
    }

    /// Counts the song as it is now as saved.
    pub(crate) fn mark_saved(&mut self) {
        self.saved = Saved {
            grid: self.grid.clone(),
            arrangement: self.arrangement.clone(),
            buses: self.buses.clone(),
            bpm: self.bpm,
        };
    }

    /// Replaces the song, as [`Editor::load`] does, from `path` if it came from one.
    pub(crate) fn replace_song(&mut self, song: Song, path: Option<PathBuf>) -> Vec<Effect> {
        let (mut grid, mut arrangement) = (song.grid, song.arrangement);
        give_track(&mut grid, &mut arrangement);
        let tracks = grid.tracks.len() as u32;
        self.ids = (self.next_id..self.next_id + tracks).map(TrackId).collect();
        self.next_id += tracks;
        self.saved = Saved {
            grid: grid.clone(),
            arrangement: arrangement.clone(),
            buses: song.buses.clone(),
            bpm: song.bpm,
        };
        self.grid = grid;
        self.arrangement = arrangement;
        self.cue = None;
        self.song_position = None;
        self.song = Default::default();
        self.buses = song.buses;
        self.bpm = song.bpm;
        self.click = song.click;
        self.path = path;
        self.selections = vec![Selection::default()];
        self.primary = 0;
        self.mode = Mode::Normal;
        self.count = None;
        self.pending = None;
        self.undo.clear();
        self.redo.clear();
        self.reconcile();
        // Every id is new, so the engine rebuilds every track with its contents.
        vec![
            Effect::TracksChanged,
            Effect::MixerChanged,
            self.meter(),
            Effect::Bpm(self.bpm),
            Effect::Click(self.click),
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
        self.primary_selection().cursor
    }

    /// The primary selection's fixed end.
    pub fn anchor(&self) -> Cursor {
        self.primary_selection().anchor
    }

    /// The primary selection.
    pub fn selection(&self) -> Region {
        self.primary_selection().region()
    }

    /// The primary selection. There's always one, as `set_selections` keeps `primary` on one;
    /// were there not, the first step would stand in, as where a song starts.
    fn primary_selection(&self) -> Selection {
        (self.selections.get(self.primary).copied()).unwrap_or_default()
    }

    /// The keys available after a prefix key, while it waits for the next one.
    pub fn infobox(&self) -> Option<Infobox> {
        let Some(pending) = self.pending else {
            return self.device_infobox();
        };
        match pending {
            // The open track's fields, by the keys that go to them.
            Pending::Field => {
                let t = self.cursor().track;
                let columns = self.lock_columns(t);
                Some(Infobox {
                    title: "field".into(),
                    items: [
                        ("n", "note"),
                        ("v", "velocity"),
                        ("g", "gestures"),
                        ("G", "second gestures"),
                    ]
                    .map(|(k, f)| (k.to_string(), f.to_string()))
                    .into_iter()
                    .chain(columns.into_iter().map(|c| (c.key.to_string(), c.full)))
                    .chain([("C".into(), "the conductor".into())])
                    .collect(),
                })
            }
            Pending::Menu(key) => {
                let Some(Binding::Menu(menu)) = self.keymap.normal.get(&key) else {
                    return None;
                };
                Some(Infobox {
                    title: menu.title.clone(),
                    items: (menu.items.iter())
                        .map(|(key, binding)| (key.to_string(), binding.describe()))
                        .collect(),
                })
            }
            Pending::Replace => Some(Infobox {
                title: "replace".into(),
                items: vec![
                    ("z–]".into(), "fill with this note".into()),
                    ("esc".into(), "cancel".into()),
                ],
            }),
        }
    }

    /// Suggestions for the word being typed on the command line, best first: commands, then
    /// the values that fit the command's next word. None once the word's typed out in full.
    pub fn completions(&self) -> Vec<Completion> {
        let Mode::Command(prompt) = &self.mode else {
            return vec![];
        };
        if prompt.kind != PromptKind::Command {
            return vec![];
        }
        // While cycling, the suggestions for what was typed before.
        let line = match prompt.selected {
            Some(_) => &prompt.typed,
            None => &prompt.line,
        };
        let found = self.suggest(line);
        let values: Vec<&Completion> = found.iter().filter(|c| !c.any).collect();
        match &values[..] {
            [only] if only.text == line[word_start(line)..] => vec![],
            _ => found,
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
        // Only note keys hold: they're plain keys, so their releases match. An edit's preview,
        // `C-a` say, stays brief, as Ctrl might come up first.
        let holds = self.releases && self.plays(key);
        let mut effects = self.handle_key(key);
        // Leaving the mixer leaves locking, putting the settings back.
        if !matches!(self.page, Page::Mixer | Page::Fx | Page::Instrument) && self.locking.is_some()
        {
            effects.extend(self.cancel_locking());
        }
        if holds {
            self.hold_previews(key, effects)
        } else {
            effects
        }
    }

    /// Holds a note key's previews until it comes up, ending any it was still holding.
    fn hold_previews(&mut self, key: Key, effects: Vec<Effect>) -> Vec<Effect> {
        let mut out = self.release_previews(key, false);
        for effect in effects {
            out.push(match effect {
                Effect::Preview { track, pitch } => {
                    if let Some(&id) = self.ids.get(track) {
                        self.sounding.push((key, id, pitch, false));
                    }
                    Effect::Hold { track, pitch }
                }
                Effect::Repeat { track, pitch, .. } => {
                    if let Some(&id) = self.ids.get(track) {
                        self.sounding.push((key, id, pitch, true));
                    }
                    effect
                }
                effect => effect,
            });
        }
        out
    }

    /// Ends the previews `key` is holding, and if `repeats`, every repeat.
    fn release_previews(&mut self, key: Key, repeats: bool) -> Vec<Effect> {
        let mut released = vec![];
        let ids = &self.ids;
        self.sounding.retain(|&(k, id, pitch, repeating)| {
            let up = k == key || (repeats && repeating);
            // A track that's gone took its notes with it.
            if up && let Some(track) = ids.iter().position(|&i| i == id) {
                released.push(Effect::Release { track, pitch });
            }
            !up
        });
        released
    }

    fn handle_key(&mut self, key: Key) -> Vec<Effect> {
        self.message = None;
        if let Some(effects) = self.tutor_key(key) {
            return effects;
        }
        match (key, &self.mode) {
            // On the command line, the numpad types digits.
            (Key::Pad(n), Mode::Command(_)) => return self.command_key(Key::Char(pad_digit(n))),
            (Key::PadOp(c), Mode::Command(_)) => return self.command_key(Key::Char(c)),
            (Key::PadOp(c), _) => return self.pad_op(c),
            (Key::Pad(n), _) => return self.pad(n, key),
            (Key::Char(c @ '1'..='9'), Mode::Insert) if self.in_folded_group() => {
                return self.pad(c as u8 - b'0', key);
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
            Some(Pending::Field) => {
                self.goto_field(key);
                return vec![];
            }
            Some(Pending::Menu(prefix)) => {
                let n = self.count.take().unwrap_or(1);
                let binding = match self.keymap.normal.get(&prefix) {
                    Some(Binding::Menu(menu)) => (menu.items.iter())
                        .find(|(k, _)| *k == key)
                        .map(|(_, b)| b.clone()),
                    _ => None,
                };
                return binding.map_or_else(Vec::new, |b| self.bound(&b, n));
            }
            None => {}
        }
        if let Key::Char(c @ '0'..='9') = key
            && (c != '0' || self.count.is_some())
        {
            let digit = c as usize - '0' as usize;
            self.count = Some((self.count.unwrap_or(0) * 10 + digit).min(9_999));
            return vec![];
        }
        if self.page == Page::Song
            && let Some(effects) = self.song_key(key)
        {
            self.count = None;
            return effects;
        }
        match self.keymap.normal.get(&key).cloned() {
            // Keep the count for the menu's action.
            Some(Binding::Menu(_)) => {
                self.pending = Some(Pending::Menu(key));
                vec![]
            }
            Some(binding) => {
                let n = self.count.take().unwrap_or(1);
                self.bound(&binding, n)
            }
            None => {
                self.count = None;
                vec![]
            }
        }
    }

    /// Does what a key's bound to, `n` times for an action. A command runs once.
    fn bound(&mut self, binding: &Binding, n: usize) -> Vec<Effect> {
        match binding {
            Binding::Action(action) => self.act(*action, n),
            Binding::Command(line) => self.run_command(line),
            Binding::Menu(_) => vec![],
        }
    }

    /// Whether `key` plays a note now: a pad, or a piano key in insert mode. Holding one down
    /// shouldn't play it again and again, so the frontend drops its repeats.
    pub fn plays(&self, key: Key) -> bool {
        match (key, &self.mode) {
            (_, Mode::Command(_)) => false,
            (Key::Pad(_), _) => true,
            _ if self.typing_field() => false,
            (Key::Char('1'..='9'), Mode::Insert) if self.in_folded_group() => true,
            (_, Mode::Insert) => self.piano_pitch(key).is_some(),
            _ => false,
        }
    }

    /// The pitch a piano key plays at the keyboard octave.
    fn piano_pitch(&self, key: Key) -> Option<u8> {
        let Key::Char(c) = key else {
            return None;
        };
        let semitone = *self.keymap.piano.get(&c)?;
        let pitch = (i16::from(self.octave) + 1) * 12 + semitone;
        u8::try_from(pitch).ok().filter(|&p| p <= 127)
    }

    fn insert_key(&mut self, key: Key) -> Vec<Effect> {
        if self.page == Page::Instrument {
            return self.play_key(key);
        }
        if let Some(field) = self.on_field()
            && let Some(effects) = self.type_field(key, field)
        {
            return effects;
        }
        if let Some(pitch) = self.piano_pitch(key) {
            let track = self.cursor().track;
            let cursors: Vec<Cursor> = self.selections.iter().map(|s| s.cursor).collect();
            let mut tracks: Vec<usize> = cursors.iter().map(|c| c.track).collect();
            // The primary's track first, as it sets the take.
            tracks.sort_unstable_by_key(|&t| t != track);
            tracks.dedup();
            if let Some(effects) = self.record_note(key, &tracks, pitch, 0) {
                return effects;
            }
            self.checkpoint();
            for c in &cursors {
                if let Some(step) =
                    (self.grid.tracks.get_mut(c.track)).and_then(|t| t.steps.get_mut(c.step))
                {
                    step.note = Some(Note {
                        pitch,
                        velocity: self.velocity,
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
                    prompt.selected = None;
                    prompt.history = None;
                    self.preview();
                }
            }
            Key::Char(c) => {
                prompt.line.push(c);
                prompt.selected = None;
                prompt.history = None;
                self.preview();
            }
            Key::Tab if prompt.kind == PromptKind::Command => self.cycle_completions(),
            Key::Up | Key::Ctrl('p') if prompt.kind == PromptKind::Command => {
                self.browse_history(true);
            }
            Key::Down | Key::Ctrl('n') if prompt.kind == PromptKind::Command => {
                self.browse_history(false);
            }
            Key::Enter => {
                let prompt = std::mem::take(prompt);
                self.mode = Mode::Normal;
                if prompt.kind == PromptKind::Command {
                    self.remember(&prompt.line);
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

    /// Fills in the next suggestion for the word being typed, going round them.
    fn cycle_completions(&mut self) {
        let Mode::Command(prompt) = &self.mode else {
            return;
        };
        let typed = match prompt.selected {
            Some(_) => prompt.typed.clone(),
            None => prompt.line.clone(),
        };
        let found: Vec<Completion> = (self.suggest(&typed).into_iter())
            .filter(|c| !c.any)
            .collect();
        let Mode::Command(prompt) = &mut self.mode else {
            return;
        };
        if found.is_empty() {
            return;
        }
        let next = prompt.selected.map_or(0, |i| (i + 1) % found.len());
        let (Some(start), Some(choice)) = (typed.get(..word_start(&typed)), found.get(next)) else {
            return;
        };
        prompt.line = format!("{start}{}", choice.text);
        prompt.typed = typed;
        prompt.selected = Some(next);
    }

    /// Shows the next older command run (or newer, back to what was typed) that starts with
    /// what was typed.
    fn browse_history(&mut self, older: bool) {
        let Mode::Command(prompt) = &mut self.mode else {
            return;
        };
        if prompt.history.is_none() {
            prompt.prefix = prompt.line.clone();
        }
        let history = &self.history;
        let fits = |i: &usize| {
            (history.get(*i)).is_some_and(|h| h.starts_with(&prompt.prefix) && *h != prompt.line)
        };
        let at = prompt.history.unwrap_or(history.len());
        let next = if older {
            (0..at).rev().find(fits)
        } else {
            (at + 1..history.len()).find(fits)
        };
        match next.and_then(|i| Some((i, history.get(i)?))) {
            Some((i, line)) => {
                prompt.history = Some(i);
                prompt.line = line.clone();
            }
            None if !older => {
                prompt.history = None;
                prompt.line = prompt.prefix.clone();
            }
            None => {}
        }
        prompt.selected = None;
    }

    /// Adds a command run to the history, moving it to the end if it's there already.
    fn remember(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        self.history.retain(|h| h != line);
        self.history.push(line.into());
        let over = self.history.len().saturating_sub(MAX_HISTORY);
        self.history.drain(..over);
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
        let primary = primary.min(selections.len().saturating_sub(1));
        Ok((selections, primary))
    }

    /// The steps in `region` whose notes match.
    fn matching(&self, region: Region, pattern: &Pattern) -> Vec<Cursor> {
        self.columns(region)
            .flat_map(|(track, steps)| steps.map(move |step| Cursor { track, step }))
            .filter(|c| {
                (self.grid.tracks.get(c.track))
                    .and_then(|t| t.steps.get(c.step))
                    .and_then(|s| s.note)
                    .is_some_and(|n| pattern.matches(n.pitch))
            })
            .collect()
    }

    fn act(&mut self, action: Action, n: usize) -> Vec<Effect> {
        // The same on every page.
        match action {
            Action::KeyboardOctaveUp => {
                self.octave = (self.octave + 1).min(9);
                self.message = Some(format!("octave {}", self.octave));
                return vec![];
            }
            Action::KeyboardOctaveDown => {
                self.octave = self.octave.saturating_sub(1);
                self.message = Some(format!("octave {}", self.octave));
                return vec![];
            }
            Action::PlaySong => return self.play_song(true),
            Action::PlayFromHere => return self.play_song(false),
            Action::Hush => return self.hush(),
            _ => {}
        }
        if self.page == Page::Mixer
            && let Some(effects) = self.mixer_act(action, n)
        {
            return effects;
        }
        if self.page == Page::Fx
            && let Some(effects) = self.fx_act(action, n)
        {
            return effects;
        }
        if self.page == Page::Instrument
            && let Some(effects) = self.inst_act(action, n)
        {
            return effects;
        }
        if self.page == Page::Song
            && let Some(effects) = self.song_act(action, n)
        {
            return effects;
        }
        if self.page == Page::Help
            && let Some(effects) = self.help_act(action, n)
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
        let last_track = self.grid.tracks.len().saturating_sub(1);
        let by = n as isize;
        let at_step = |step: usize| move |_: &Self, c: Cursor| Cursor { step, ..c };
        let at_track = |track: usize| move |_: &Self, c: Cursor| Cursor { track, ..c };
        if let Some(field) = self.on_field() {
            match action {
                Action::NoteUp | Action::NoteDown => {
                    let by = if action == Action::NoteUp {
                        n
                    } else {
                        n.wrapping_neg()
                    };
                    return self.nudge_field(field, by as i32);
                }
                Action::NoteOctaveUp => return self.nudge_field(field, 10 * n as i32),
                Action::NoteOctaveDown => return self.nudge_field(field, -10 * n as i32),
                Action::Delete | Action::Clear => {
                    if action == Action::Delete {
                        self.yank_field(field);
                    }
                    return self.clear_field(field);
                }
                Action::ClearAdvance => {
                    let effects = self.clear_field(field);
                    self.advance();
                    return effects;
                }
                Action::Change => {
                    self.yank_field(field);
                    let effects = self.clear_field(field);
                    self.insert_at(false);
                    return effects;
                }
                Action::Yank => {
                    self.yank_field(field);
                    return vec![];
                }
                Action::Paste | Action::PasteInsert => return self.paste_field(field, n),
                Action::Replace => {
                    self.message = Some("in a field, i and two digits set its value".into());
                    return vec![];
                }
                _ => {}
            }
        }
        match action {
            Action::Left => self.step_field(false, n),
            Action::Right => self.step_field(true, n),
            Action::PrevTrack => self.motion(|e, c| e.offset(c, -by, 0)),
            Action::NextTrack => self.motion(|e, c| e.offset(c, by, 0)),
            Action::GotoField => self.pending = Some(Pending::Field),
            Action::Fill => return self.fill_field(),
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
                step: next_multiple(c.step, e.bar(), n),
                ..c
            }),
            Action::PrevBar => self.motion(|e, c| Cursor {
                step: prev_multiple(c.step, e.bar(), n),
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
            Action::ReverseSteps => return self.reverse_steps(),
            Action::RepeatSteps => return self.repeat_steps(n),
            Action::RotateStepsForward => return self.rotate_steps(by),
            Action::RotateStepsBackward => return self.rotate_steps(-by),
            Action::SplitRows => self.split_rows(),
            Action::MergeAll => {
                let merged = self
                    .selections
                    .iter()
                    .fold(self.primary_selection(), |m, s| m.merge(*s));
                self.set_selections(vec![merged], 0);
            }
            Action::KeepPrimary => self.set_selections(vec![self.primary_selection()], 0),
            Action::RemovePrimary => {
                if self.selections.len() == 1 {
                    self.message = Some("can't remove the only selection".into());
                } else if self.primary < self.selections.len() {
                    self.selections.remove(self.primary);
                    self.primary = self.primary.min(self.selections.len().saturating_sub(1));
                }
            }
            Action::RotateForward => {
                self.primary = (self.primary + n) % self.selections.len().max(1);
            }
            Action::RotateBackward => {
                let len = self.selections.len().max(1);
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
                    if let Some(step) =
                        (self.grid.tracks.get_mut(c.track)).and_then(|t| t.steps.get_mut(c.step))
                    {
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
            // Handled above, for every page.
            Action::KeyboardOctaveUp
            | Action::KeyboardOctaveDown
            | Action::PlaySong
            | Action::PlayFromHere
            | Action::Hush => {}
            Action::NextPattern => return self.step_pattern(n as isize),
            Action::PrevPattern => return self.step_pattern(-(n as isize)),
            Action::ToggleMute => return self.run_command("mute"),
            Action::TogglePatternMute => return self.run_command("pattern-mute"),
            Action::LockSetting => {
                self.message = Some("L locks a setting from the mixer (gm) or fx page (gf)".into());
            }
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
            Action::ToggleOpen => self.toggle_open(),
            Action::GotoMixer => {
                // From the pattern, onto the cursor's track.
                if self.page == Page::Pattern {
                    self.mixer.strip = self.cursor().track;
                }
                self.page = Page::Mixer;
                self.mode = Mode::Normal;
            }
            Action::GotoInstrument => self.goto_instrument(),
            Action::GotoEffects => self.goto_fx(),
            Action::GotoPattern => self.page = Page::Pattern,
            Action::GotoSong => {
                // Onto the entry playing, or the first playing the pattern being edited.
                let entries = &self.arrangement.entries;
                if let Some(entry) = self.song_position.map(|(e, _)| e).or_else(|| {
                    entries
                        .iter()
                        .position(|e| e.pattern == self.arrangement.current)
                }) {
                    self.song_to_entry(entry);
                }
                self.page = Page::Song;
                self.mode = Mode::Normal;
            }
            // Song page only.
            Action::OpenBelow | Action::OpenAbove | Action::Open => {}
            Action::TogglePlay => return vec![Effect::TogglePlay],
            Action::PlayFill => return self.play_fill(),
            Action::ToggleRecord => self.toggle_record(),
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
            .filter_map(|t| self.grid.tracks.get(t).cloned())
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
        // The selected tracks are there, so each comes out.
        for &t in tracks.iter().rev() {
            let _ = self.remove_track(t);
        }
        let first =
            (tracks.first().copied().unwrap_or(0)).min(self.grid.tracks.len().saturating_sub(1));
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
        let at = at.min(self.grid.tracks.len());
        for (i, track) in self.track_clipboard.clone().into_iter().enumerate() {
            // Each goes in after the one before, so there's room.
            let _ = self.insert_track(at + i, track);
        }
        self.select_point(at, cursor.step);
        vec![Effect::TracksChanged]
    }

    /// Runs a command line as `:` would, showing any error.
    pub fn run_command(&mut self, line: &str) -> Vec<Effect> {
        command::run(self, line).unwrap_or_else(|message| {
            self.message = Some(message);
            vec![]
        })
    }

    /// How many steps `track` has: none if there's no such track. In the conductor, left of the
    /// first track, as many as the conductor has.
    fn track_len(&self, track: usize) -> usize {
        if track == 0 && self.in_conductor() {
            return self.conductor_len();
        }
        self.grid.tracks.get(track).map_or(0, |t| t.steps.len())
    }

    /// The last step a cursor can reach: the end of the longest track the selection spans.
    fn step_limit(&self, anchor: Cursor, cursor: Cursor) -> usize {
        let (a, b) = (
            anchor.track.min(cursor.track),
            anchor.track.max(cursor.track),
        );
        (a..=b)
            .map(|t| self.track_len(t))
            .max()
            .unwrap_or(1)
            .saturating_sub(1)
    }

    /// A selection between these ends, kept on the grid.
    fn clamped(&self, anchor: Cursor, cursor: Cursor) -> Selection {
        let last_track = self.grid.tracks.len().saturating_sub(1);
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
        // Without a primary the cursor would be nowhere, so the selections stay as they were.
        let Some(main) = selections.get(primary).map(|s| s.cursor) else {
            return;
        };
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
    pub(crate) fn select_point(&mut self, track: usize, step: usize) {
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
        let track = self.cursor().track;
        let conducting = self.in_conductor();
        self.map_selections(|e, s| {
            let to = target(e, s.cursor);
            e.clamped(if extend { s.anchor } else { to }, to)
        });
        // On another track, the cursor's on its notes, kept to the track's length if it's left
        // the conductor's.
        if self.cursor().track != track {
            self.field = StepField::Note;
            if conducting {
                self.clamp_selection();
            }
        }
    }

    fn offset(&self, c: Cursor, tracks: isize, steps: isize) -> Cursor {
        Cursor {
            track: c
                .track
                .saturating_add_signed(tracks)
                .min(self.grid.tracks.len().saturating_sub(1)),
            step: c.step.saturating_add_signed(steps),
        }
    }

    fn beat(&self) -> usize {
        (self.grid.steps_per_beat as usize).max(1)
    }

    /// Steps in a bar.
    fn bar(&self) -> usize {
        self.grid.steps_per_bar() as usize
    }

    /// The meter, to send to the engine.
    pub(crate) fn meter(&self) -> Effect {
        Effect::Meter {
            steps_per_beat: self.grid.steps_per_beat,
            beats_per_bar: self.grid.beats_per_bar,
            groups: self.grid.group_starts(),
        }
    }

    /// The next or previous step with a note on the cursor's track.
    fn find_note(&self, cursor: Cursor, forwards: bool) -> Option<usize> {
        let steps = &self.grid.tracks.get(cursor.track)?.steps;
        let has_note = |&i: &usize| steps.get(i).is_some_and(|s| s.note.is_some());
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
                // An empty track has only its first step to be on.
                step: (c.step + e.auto_step)
                    .checked_rem(e.track_len(c.track))
                    .unwrap_or(0),
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
                    // `columns` keeps to the track, and a missing one has no steps to copy.
                    .map(|(t, steps)| {
                        (self.grid.tracks.get(t))
                            .and_then(|t| t.steps.get(steps))
                            .map_or(vec![], <[Step]>::to_vec)
                    })
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
            let Some(steps) =
                (self.grid.tracks.get_mut(*t)).and_then(|t| t.steps.get_mut(steps.clone()))
            else {
                continue;
            };
            for step in steps {
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
        let mut order: Vec<(usize, Region)> =
            selections.iter().map(|s| s.region()).enumerate().collect();
        order.sort_by_key(|(_, region)| std::cmp::Reverse(region.first_step));
        for (i, region) in order {
            // The last clip goes to any selections past the clips.
            let Some(clip) = self.clipboard.get(i).or(self.clipboard.last()) else {
                continue;
            };
            let rows = clip.first().map_or(0, Vec::len);
            if rows == 0 {
                continue;
            }
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
                    for (slot, value) in steps.iter_mut().skip(step).zip(values) {
                        *slot = value;
                    }
                }
                tracks.push(track + j);
            }
            let last_track = track + clip.len().saturating_sub(1);
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
            let pasted = self.clamped(
                Cursor {
                    track: *span.start(),
                    step: first,
                },
                Cursor {
                    track: *span.end(),
                    step: (first + total).saturating_sub(1),
                },
            );
            if let Some(selection) = selections.get_mut(i) {
                *selection = pasted;
            }
        }
        self.set_selections(selections, self.primary);
        changed(tracks)
    }

    /// `r` then a piano key: every selected step becomes that note.
    fn replace(&mut self, key: Key) -> Vec<Effect> {
        let Some(pitch) = self.piano_pitch(key) else {
            return vec![];
        };
        let velocity = self.velocity;
        let mut effects = self.edit(|s| {
            s.note = Some(Note {
                pitch,
                velocity,
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
    /// Adds a track at `index`, with a new id. False, changing nothing, if `index` is past the
    /// end.
    #[must_use]
    pub(crate) fn insert_track(&mut self, index: usize, track: Track) -> bool {
        let len = track.steps.len();
        if !self.grid.insert_track(index, track) {
            return false;
        }
        self.arrangement.insert_track(index, len);
        // One id per track, so `index` is in range; were it not, the id would go on the end.
        self.ids
            .insert(index.min(self.ids.len()), TrackId(self.next_id));
        self.next_id += 1;
        for entry in &mut self.song.yanked {
            entry.insert_track(index);
        }
        self.reconcile();
        true
    }

    /// Removes the track at `index`. False, changing nothing, if there's no such track.
    #[must_use]
    pub(crate) fn remove_track(&mut self, index: usize) -> bool {
        if self.grid.remove_track(index).is_none() {
            return false;
        }
        self.arrangement.remove_track(index);
        if index < self.ids.len() {
            self.ids.remove(index);
        }
        for entry in &mut self.song.yanked {
            entry.remove_track(index);
        }
        self.reconcile();
        true
    }

    /// Moves the track at `from` to `to`. False, changing nothing, if either isn't a track.
    #[must_use]
    pub(crate) fn move_track(&mut self, from: usize, to: usize) -> bool {
        if !self.grid.move_track(from, to) {
            return false;
        }
        self.arrangement.move_track(from, to);
        // One id per track, so these are in range.
        if from < self.ids.len() {
            let id = self.ids.remove(from);
            self.ids.insert(to.min(self.ids.len()), id);
        }
        for entry in &mut self.song.yanked {
            entry.move_track(from, to);
        }
        self.reconcile();
        true
    }

    /// Where the track with `id` is now, if it's still there.
    /// `te`: keeps the tracks the selections reach open, showing their fields after the cursor
    /// leaves; again, closes them.
    fn toggle_open(&mut self) {
        if !self.open.is_empty() {
            self.open.clear();
            return;
        }
        let tracks: Vec<usize> = (self.selections.iter())
            .flat_map(|s| self.columns(s.region()).map(|(t, _)| t))
            .collect();
        self.open = tracks
            .iter()
            .filter_map(|&t| self.ids.get(t).copied())
            .collect();
        self.open.sort_unstable();
        self.open.dedup();
    }

    /// Whether track `t` shows its fields: velocity and lock columns beside its notes. The
    /// cursor's track always does.
    pub fn is_open(&self, t: usize) -> bool {
        t == self.cursor().track || self.ids.get(t).is_some_and(|id| self.open.contains(id))
    }

    pub(crate) fn track_index(&self, id: TrackId) -> Option<usize> {
        self.ids.iter().position(|&i| i == id)
    }

    /// Lets go of whatever points into the song where the song no longer reaches, after it
    /// changes underneath: tracks added, removed or moved, or an undo or redo. The cursors
    /// keep themselves on the song as they're read; this is for the rest.
    fn reconcile(&mut self) {
        let patterns = self.arrangement.patterns.len();
        let current = self.arrangement.current;
        self.cue = self.cue.filter(|&p| p < patterns && p != current);
        let entries = self.arrangement.entries.len();
        self.song_position = self.song_position.filter(|&(e, _)| e < entries);
        self.song.looped = self.song.looped.filter(|&(a, b)| a <= b && b < entries);
        let tracks = self.grid.tracks.len();
        let every = (1u32 << tracks) - 1;
        self.song.yanked.retain(|e| e.pattern < patterns);
        for entry in &mut self.song.yanked {
            entry.mutes &= every;
        }
        if self.locking.as_ref().is_some_and(|l| !l.matches(self)) {
            self.locking = None;
        }
        let ids = &self.ids;
        self.live_lock = self.live_lock.filter(|(id, _)| ids.contains(id));
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

    pub(crate) fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Forgets the undo steps after the first `len`.
    pub(crate) fn truncate_undo(&mut self, len: usize) {
        self.undo.truncate(len);
        // What was undone came after, so it can't be redone.
        self.redo.clear();
    }

    pub(crate) fn checkpoint(&mut self) {
        self.take = None;
        self.undo.push(Snapshot {
            grid: self.grid.clone(),
            arrangement: self.arrangement.clone(),
            buses: self.buses.clone(),
            ids: self.ids.clone(),
            selections: self.selections.clone(),
            primary: self.primary,
            looped: self.song.looped,
            bpm: self.bpm,
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
        let grid = &snapshot.grid;
        let meter_changed = (grid.steps_per_beat, grid.beats_per_bar, &grid.grouping)
            != (
                self.grid.steps_per_beat,
                self.grid.beats_per_bar,
                &self.grid.grouping,
            );
        // A `:meter` changed the tempo with the steps per beat, so they come back together.
        let bpm = if meter_changed {
            snapshot.bpm
        } else {
            self.bpm
        };
        self.take = None;
        self.held.clear();
        to.push(Snapshot {
            grid: std::mem::replace(&mut self.grid, snapshot.grid),
            arrangement: std::mem::replace(&mut self.arrangement, snapshot.arrangement),
            buses: std::mem::replace(&mut self.buses, snapshot.buses),
            ids: std::mem::replace(&mut self.ids, snapshot.ids),
            selections: std::mem::replace(&mut self.selections, snapshot.selections),
            primary: std::mem::replace(&mut self.primary, snapshot.primary),
            looped: std::mem::replace(&mut self.song.looped, snapshot.looped),
            bpm: std::mem::replace(&mut self.bpm, bpm),
        });
        self.mode = Mode::Normal;
        self.reconcile();
        // Line the tracks up first, so the indices that follow are the restored ones.
        let meter = meter_changed.then(|| [self.meter(), Effect::Bpm(self.bpm)]);
        tracks_changed
            .then_some(Effect::TracksChanged)
            .into_iter()
            .chain((0..self.grid.tracks.len()).map(Effect::TrackChanged))
            .chain([Effect::MixerChanged])
            .chain(meter.into_iter().flatten())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{Condition, MAX_TRACKS, Rule, SongGesture, Transform};

    fn editor() -> Editor {
        let track = |name: &str, len| Track {
            face: None,
            pattern_mute: false,
            columns: vec![],
            name: name.into(),
            steps: vec![Step::default(); len],
            mix: Default::default(),
            instrument: Default::default(),
            effects: Default::default(),
            choke: 0,
            macros: vec![],
            rules: vec![],
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            beats_per_bar: 4,
            grouping: vec![],
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
        keys(&mut e, "tl");
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
        assert!(infobox.items.contains(&("e".into(), "last step".into())));
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
        keys(&mut e, "vjjtl");
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
        e.grid.tracks[0].steps[len - 1] = e.grid.tracks[0].steps[0];
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
        keys(&mut e, "tlgg");
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
        assert_eq!(e.octave, 4, "brackets don't change the octave");
    }

    #[test]
    fn braces_change_the_octave_on_every_page_and_mode() {
        let mut e = editor();
        keys(&mut e, "}");
        assert_eq!(e.octave, 5);
        keys(&mut e, "i}");
        assert_eq!(e.octave, 6, "in insert mode too");
        e.handle(Key::Esc);
        keys(&mut e, "gi{");
        assert_eq!(e.octave, 5, "on the instrument page");
        keys(&mut e, "i{{");
        assert_eq!(e.octave, 3, "and playing it");
    }

    #[test]
    fn the_piano_reaches_down_to_b_and_up_past_the_top_row() {
        let mut e = editor();
        notes(&mut e, "\\zi]");
        // At octave 4: the B under C-4, C-4, C-6 and the G over it.
        assert_eq!(
            pitches(&e, 0)[..4],
            [Some(59), Some(60), Some(84), Some(91)]
        );
    }

    #[test]
    fn tab_completes_commands() {
        let mut e = editor();
        keys(&mut e, ":fo");
        assert_eq!(e.completions()[0].text, "follow");
        e.handle(Key::Tab);
        let Mode::Command(prompt) = &e.mode else {
            panic!("left command mode")
        };
        assert_eq!(prompt.line, "follow");
        assert_eq!(
            e.completions()[0].text,
            "follow",
            "list stays while cycling"
        );
        keys(&mut e, " ");
        assert!(e.completions().is_empty());
        assert_eq!(e.command_spec().unwrap().name, "follow");
        e.handle(Key::Enter);
        assert!(!e.follow);
    }

    /// Types `s`, with `\n` for enter.
    fn typing(e: &mut Editor, s: &str) {
        for c in s.chars() {
            e.handle(if c == '\n' { Key::Enter } else { Key::Char(c) });
        }
    }

    fn line(e: &Editor) -> String {
        match &e.mode {
            Mode::Command(prompt) => prompt.line.clone(),
            _ => panic!("left command mode"),
        }
    }

    #[test]
    fn values_are_suggested_as_they_are_typed() {
        let mut e = editor();
        let texts = |e: &Editor| {
            e.completions()
                .into_iter()
                .map(|c| c.text)
                .collect::<Vec<_>>()
        };
        typing(&mut e, ":fx-add ");
        assert_eq!(
            texts(&e).len(),
            lattice_core::FxKind::ALL.len(),
            "every effect"
        );
        // Their first letters narrow them down, and letters in order find them otherwise.
        typing(&mut e, "r");
        assert_eq!(texts(&e), ["reverb"]);
        e.handle(Key::Backspace);
        typing(&mut e, "rvb");
        assert_eq!(texts(&e), ["reverb"]);
        e.handle(Key::Tab);
        assert_eq!(line(&e), "fx-add reverb");
        // Typed out in full, there's nothing left to suggest.
        e.handle(Key::Enter);
        typing(&mut e, ":fx-add reverb");
        assert!(e.completions().is_empty());
        e.handle(Key::Esc);

        // A track's settings, then a setting's choices.
        typing(&mut e, ":lock rev.mi");
        assert_eq!(texts(&e), ["reverb.mix"]);
        e.handle(Key::Esc);
        typing(&mut e, ":fx-add filter\n:lock filter.type ");
        assert!(texts(&e).contains(&"band".to_string()), "{:?}", texts(&e));
        // Tab goes round them.
        let first = texts(&e)[0].clone();
        let n = texts(&e).len();
        for _ in 0..n {
            e.handle(Key::Tab);
        }
        e.handle(Key::Tab);
        assert_eq!(line(&e), format!("lock filter.type {first}"));
    }

    #[test]
    fn rules_suggest_their_amounts_and_when_they_apply() {
        let mut e = editor();
        let suggested = |e: &Editor| {
            (e.completions().into_iter())
                .map(|c| format!("{} {}", c.text, c.describe))
                .collect::<Vec<_>>()
        };
        typing(&mut e, ":tr ");
        assert_eq!(suggested(&e)[0], "12 an octave up");
        // Last, a reminder that any number will do, which tab passes by.
        let all = e.completions();
        assert!(all.last().unwrap().any);
        typing(&mut e, "13");
        assert_eq!(e.completions().len(), 1, "still there when nothing matches");
        e.handle(Key::Tab);
        assert_eq!(line(&e), "tr 13");
        e.handle(Key::Backspace);
        e.handle(Key::Backspace);
        typing(&mut e, "7 ");
        assert_eq!(suggested(&e).len(), 2, "every or pass");
        typing(&mut e, "e");
        e.handle(Key::Tab);
        assert_eq!(line(&e), "tr 7 every");
        typing(&mut e, " ");
        assert_eq!(suggested(&e)[0], "2 every other pass");
        typing(&mut e, "4 ");
        assert_eq!(suggested(&e)[1], "1 the 2nd pass of every 4");
        typing(&mut e, "3\n");
        assert_eq!(
            e.grid.tracks[0].rules.last().unwrap().when,
            Condition::Every { n: 4, offset: 3 }
        );
        typing(&mut e, ":rot ");
        assert!(suggested(&e).contains(&"4 a beat later".to_string()));
        typing(&mut e, "1 pass ");
        assert_eq!(suggested(&e)[0], "0 the 1st pass only");
        e.handle(Key::Esc);
        typing(&mut e, ":rev ");
        assert_eq!(suggested(&e).len(), 2, "rev takes no amount");
        e.handle(Key::Esc);
        assert_eq!(super::suggest::ordinal(12), "12th");
        assert_eq!(super::suggest::ordinal(22), "22nd");
    }

    #[test]
    fn up_and_down_go_through_the_commands_run() {
        let mut e = editor();
        typing(&mut e, ":bpm 100\n:follow\n:bpm 120\n:follow\n");
        assert_eq!(
            e.history,
            ["bpm 100", "bpm 120", "follow"],
            "once each, latest last"
        );
        typing(&mut e, ":");
        e.handle(Key::Up);
        assert_eq!(line(&e), "follow");
        e.handle(Key::Up);
        assert_eq!(line(&e), "bpm 120");
        e.handle(Key::Ctrl('n'));
        assert_eq!(line(&e), "follow");
        e.handle(Key::Down);
        assert_eq!(line(&e), "", "back to what was typed");
        // Only those starting with what's typed.
        typing(&mut e, "b");
        e.handle(Key::Up);
        e.handle(Key::Up);
        assert_eq!(line(&e), "bpm 100");
        e.handle(Key::Up);
        assert_eq!(line(&e), "bpm 100", "the oldest");
        e.handle(Key::Enter);
        assert_eq!(e.bpm, 100.0);
        assert_eq!(e.history.last().unwrap(), "bpm 100");
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
    fn rules_can_be_typed_as_they_are_shown() {
        let mut e = editor();
        for line in ["rev /2+1", "tr 5 /4", "rot 1 @3"] {
            command(&mut e, line);
        }
        let shown: Vec<String> = e.grid.tracks[0]
            .rules
            .iter()
            .map(|r| r.to_string())
            .collect();
        assert_eq!(shown, ["rev/2+1", "tr+5/4", "rot+1@3"]);
    }

    #[test]
    fn selected_steps_reverse_repeat_and_rotate() {
        let mut e = editor();
        notes(&mut e, "zxcv");
        let (c, d, e_, f) = (Some(60), Some(62), Some(64), Some(65));
        keys(&mut e, "ggv3j~");
        assert_eq!(&pitches(&e, 0)[..4], [f, e_, d, c]);
        assert_eq!(e.grid.tracks[1].steps, vec![Step::default(); 8]);
        keys(&mut e, "2*");
        assert_eq!(&pitches(&e, 0)[..5], [f, e_, f, e_, None]);
        e.handle(Key::Alt(')'));
        assert_eq!(&pitches(&e, 0)[..4], [e_, f, e_, f]);
        e.handle(Key::Alt('('));
        assert_eq!(&pitches(&e, 0)[..4], [f, e_, f, e_]);
        keys(&mut e, "u");
        assert_eq!(&pitches(&e, 0)[..4], [e_, f, e_, f]);
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

        // And `ta`, `td`, `tH`, `tL` add, remove and move tracks.
        keys(&mut e, ";gh");
        assert_eq!(keys(&mut e, "ta"), [Effect::TracksChanged]);
        assert_eq!(e.grid.tracks.len(), 3);
        assert_eq!(e.cursor().track, 1, "on the new track");
        keys(&mut e, "tL");
        assert_eq!(e.grid.tracks[2].name, "track 3");
        keys(&mut e, "tH");
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
    fn new_starts_an_empty_song() {
        let mut e = editor();
        notes(&mut e, "z");
        assert_eq!(command(&mut e, "new"), []);
        assert!(e.message.as_ref().unwrap().starts_with("unsaved changes"));
        let effects = command(&mut e, "new! fresh.lattice");
        assert!(!effects.is_empty(), "{:?}", e.message);
        assert_eq!(e.grid.tracks.len(), 1);
        assert!(e.grid.tracks[0].steps.iter().all(|s| s.note.is_none()));
        assert_eq!(
            e.path.as_deref(),
            Some(std::path::Path::new("fresh.lattice"))
        );
        assert!(!e.modified());
        assert_eq!(
            command(&mut e, "write"),
            [Effect::Save("fresh.lattice".into())]
        );
        command(&mut e, "new Cargo.toml");
        assert!(e.message.as_ref().unwrap().contains("already there"));
    }

    #[test]
    fn meter_keeps_the_music_where_it_was() {
        let mut e = editor();
        e.grid.steps_per_beat = 2;
        e.bpm = 200.0;
        command(&mut e, "bars 14");
        let current = e.arrangement.current;
        e.arrangement.patterns[current].conductor.steps =
            vec![[Some(SongGesture::Tempo(200)), Some(SongGesture::Slide(144))]];
        assert_eq!(e.arrangement.steps(&e.grid, current), Some(112));

        // The Scarlatti: two beats of four steps a bar, at half the tempo, sounds the same.
        let effects = command(&mut e, "meter 2 4");
        assert_eq!((e.grid.beats_per_bar, e.grid.steps_per_beat), (2, 4));
        assert_eq!(e.bpm, 100.0);
        assert_eq!(
            effects,
            [
                Effect::Meter {
                    steps_per_beat: 4,
                    beats_per_bar: 2,
                    groups: 1
                },
                Effect::Bpm(100.0)
            ]
        );
        assert_eq!(e.arrangement.patterns[current].bars, Some(14));
        assert_eq!(e.arrangement.steps(&e.grid, current), Some(112));
        assert_eq!(
            e.arrangement.patterns[current].conductor.steps[0],
            [Some(SongGesture::Tempo(100)), Some(SongGesture::Slide(72))]
        );
        assert!(e.message.as_ref().unwrap().contains("100.0 BPM"));

        // In threes, 112 steps aren't whole bars, so nothing changes.
        command(&mut e, "meter 3");
        assert!(
            e.message.as_ref().unwrap().contains("isn't 1 to"),
            "{:?}",
            e.message
        );
        assert_eq!(e.grid.beats_per_bar, 2);

        // Undo brings the tempo back with the steps per beat.
        let effects = keys(&mut e, "u");
        assert_eq!(
            (e.grid.beats_per_bar, e.grid.steps_per_beat, e.bpm),
            (4, 2, 200.0)
        );
        assert!(effects.contains(&Effect::Bpm(200.0)), "{effects:?}");
        // While undoing an edit leaves the tempo as it is.
        command(&mut e, "bpm 150");
        notes(&mut e, "z");
        keys(&mut e, "u");
        assert_eq!(e.bpm, 150.0);

        command(&mut e, "meter");
        assert_eq!(e.message.as_deref(), Some("4 beats a bar, 2 steps a beat"));
    }

    #[test]
    fn meter_groups_an_odd_bar() {
        let mut e = editor();
        e.grid.steps_per_beat = 2;
        let effects = command(&mut e, "meter 2+2+3");
        assert_eq!(
            (e.grid.beats_per_bar, &e.grid.grouping[..]),
            (7, &[2, 2, 3][..])
        );
        assert_eq!(
            effects[0],
            Effect::Meter {
                steps_per_beat: 2,
                beats_per_bar: 7,
                groups: 0b10101
            }
        );
        assert!(
            (e.message.as_deref().unwrap()).starts_with("7 beats a bar (2+2+3), 2 steps a beat"),
            "{:?}",
            e.message
        );

        // A plain number of beats takes the groups away.
        command(&mut e, "meter 7");
        assert!(e.grid.grouping.is_empty());
        assert_eq!(e.grid.group_starts(), 1);
        keys(&mut e, "u");
        assert_eq!(e.grid.grouping, [2, 2, 3]);

        for bad in ["meter 2+0+3", "meter 9+9", "meter 2+x"] {
            command(&mut e, bad);
            assert_eq!(e.grid.grouping, [2, 2, 3], "{bad}");
        }
    }

    #[test]
    fn loading_replaces_the_song() {
        let mut e = editor();
        notes(&mut e, "zx");
        keys(&mut e, "lC");
        let before = e.track_ids().to_vec();
        let mut song = editor().song();
        song.bpm = 140.0;
        song.click = false;
        song.grid.tracks.truncate(1);
        let effects = e.load(song, "new.lattice".into());
        assert_eq!(
            effects,
            [
                Effect::TracksChanged,
                Effect::MixerChanged,
                Effect::Meter {
                    steps_per_beat: 4,
                    beats_per_bar: 4,
                    groups: 1
                },
                Effect::Bpm(140.0),
                Effect::Click(false)
            ]
        );
        assert!(!e.click && !e.song().click, "the song keeps its click off");
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
        keys(&mut e, "tlgg");
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
        keys(&mut e, "tlCi");
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
        keys(&mut e, "tl12j");
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
    fn previews_end_on_their_track_wherever_it_moves() {
        let mut e = editor();
        e.sounding.push((Key::Char('z'), TrackId(1), 60, false));
        e.sounding.push((Key::Char('z'), TrackId(0), 62, false));
        assert!(e.move_track(1, 0));
        assert!(e.remove_track(1));
        // Track 0, a, has gone, and its note with it; b's ends where b is now.
        assert_eq!(
            e.release_previews(Key::Char('z'), false),
            [Effect::Release {
                track: 0,
                pitch: 60
            }]
        );
        assert!(e.sounding.is_empty());
    }

    #[test]
    fn undoing_while_locking_forgets_what_was_undone() {
        let mut e = editor();
        notes(&mut e, "z");
        keys(&mut e, "u");
        assert!(!e.redo.is_empty());
        e.truncate_undo(0);
        assert!(e.redo.is_empty());
    }

    #[test]
    fn undo_and_redo_across_adding_and_removing_tracks() {
        let mut e = editor();
        notes(&mut e, "z");
        keys(&mut e, "tl");
        command(&mut e, "ta c");
        command(&mut e, "track-move -2");
        keys(&mut e, "tl");
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

    #[test]
    fn only_note_keys_play() {
        let mut e = editor();
        assert!(e.plays(Key::Pad(1)));
        assert!(!e.plays(Key::Char('z')), "a motion in normal mode");
        keys(&mut e, "i");
        assert!(e.plays(Key::Char('z')));
        assert!(
            !e.plays(Key::Char('1')),
            "not a piano key, outside a folded group"
        );
        assert!(!e.plays(Key::Backspace));
        e.handle(Key::Esc);
        keys(&mut e, ":");
        assert!(
            !e.plays(Key::Pad(1)),
            "the numpad types on the command line"
        );
    }

    #[test]
    fn note_keys_hold_their_notes_until_they_come_up() {
        let mut e = editor();
        e.releases = true;
        let hold = |pitch| Effect::Hold { track: 0, pitch };
        let release = |pitch| Effect::Release { track: 0, pitch };
        // A chord: each note held by its own key.
        assert!(keys(&mut e, "iz").contains(&hold(60)));
        let effects = keys(&mut e, "c");
        assert!(effects.contains(&hold(64)) && !effects.contains(&release(60)));
        assert_eq!(e.release(Key::Char('c')), [release(64)]);
        assert_eq!(e.release(Key::Char('z')), [release(60)]);
        assert!(e.release(Key::Char('z')).is_empty());

        // An edit's preview is brief, with nothing to release.
        e.handle(Key::Esc);
        keys(&mut e, "k");
        let effects = e.handle(Key::Ctrl('a'));
        assert!(effects.contains(&Effect::Preview {
            track: 0,
            pitch: 65
        }));
        assert!(e.release(Key::Ctrl('a')).is_empty());

        // Without releases from the frontend, note keys' previews are brief too.
        e.releases = false;
        assert!(keys(&mut e, "ix").contains(&Effect::Preview {
            track: 0,
            pitch: 62
        }));
    }
}
