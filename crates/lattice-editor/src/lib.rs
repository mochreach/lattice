//! Modal editor state for Lattice, independent of any frontend.
//!
//! The frontend turns its key events into [`Key`]s and calls [`Editor::handle`], which updates
//! the song and cursor and returns [`Effect`]s (send a track to the engine, play, quit) for the
//! frontend to carry out. Nothing here knows about terminals or audio, so it's tested by feeding
//! in key sequences.

mod command;
mod keymap;

pub use command::HELP;
pub use keymap::{Action, Key, Keymap};

use lattice_core::{Grid, MAX_STEPS, Note, Step};

const UNDO_LIMIT: usize = 500;
const PAGE: usize = 16;
const DEFAULT_VELOCITY: u8 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// Typing on the piano keys enters notes.
    Insert,
    /// Selecting steps in the cursor's track, from `anchor` to the cursor.
    Visual {
        anchor: usize,
    },
    /// Typing a `:` command.
    Command(String),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    pub track: usize,
    pub step: usize,
}

#[derive(Debug, Clone)]
struct Snapshot {
    grid: Grid,
    cursor: Cursor,
}

#[derive(Debug, Clone)]
pub struct Editor {
    pub grid: Grid,
    pub bpm: f64,
    pub cursor: Cursor,
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
    count: Option<usize>,
    clipboard: Vec<Step>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Editor {
    pub fn new(grid: Grid, bpm: f64) -> Self {
        assert!(!grid.tracks.is_empty(), "need at least one track");
        Self {
            grid,
            bpm,
            cursor: Cursor::default(),
            mode: Mode::Normal,
            view: View::Source,
            octave: 4,
            auto_step: 1,
            follow: true,
            click: true,
            message: None,
            keymap: Keymap::default(),
            count: None,
            clipboard: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// A pending count typed before a command, e.g. the `4` in `4j`.
    pub fn count(&self) -> Option<usize> {
        self.count
    }

    /// Steps covered by the selection, or just the cursor's step outside visual mode.
    pub fn selection(&self) -> (usize, usize) {
        match self.mode {
            Mode::Visual { anchor } => (anchor.min(self.cursor.step), anchor.max(self.cursor.step)),
            _ => (self.cursor.step, self.cursor.step),
        }
    }

    pub fn handle(&mut self, key: Key) -> Vec<Effect> {
        self.message = None;
        match self.mode {
            Mode::Command(_) => self.command_key(key),
            Mode::Insert => self.insert_key(key),
            Mode::Normal | Mode::Visual { .. } => self.normal_key(key),
        }
    }

    fn normal_key(&mut self, key: Key) -> Vec<Effect> {
        if let Key::Char(c @ '0'..='9') = key
            && (c != '0' || self.count.is_some())
        {
            let digit = c.to_digit(10).unwrap() as usize;
            self.count = Some((self.count.unwrap_or(0) * 10 + digit).min(9_999));
            return vec![];
        }
        let n = self.count.take().unwrap_or(1);
        match self.keymap.normal.get(&key).copied() {
            Some(action) => self.act(action, n),
            None => vec![],
        }
    }

    fn insert_key(&mut self, key: Key) -> Vec<Effect> {
        if let Key::Char(c) = key
            && let Some(&semitone) = self.keymap.piano.get(&c)
        {
            let pitch = (u16::from(self.octave) + 1) * 12 + u16::from(semitone);
            let Ok(pitch) = u8::try_from(pitch) else {
                return vec![];
            };
            if pitch > 127 {
                return vec![];
            }
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
        let Mode::Command(line) = &mut self.mode else {
            return vec![];
        };
        match key {
            Key::Esc => self.mode = Mode::Normal,
            Key::Backspace => {
                if line.pop().is_none() {
                    self.mode = Mode::Normal;
                }
            }
            Key::Char(c) => line.push(c),
            Key::Enter => {
                let line = std::mem::take(line);
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
        let track = self.cursor.track;
        match action {
            Action::Left => self.move_track(-(n as isize)),
            Action::Right => self.move_track(n as isize),
            Action::Up => self.move_step(-(n as isize)),
            Action::Down => self.move_step(n as isize),
            Action::PageUp => self.move_step(-((n * PAGE) as isize)),
            Action::PageDown => self.move_step((n * PAGE) as isize),
            Action::Top => self.cursor.step = 0,
            Action::Bottom => self.cursor.step = self.track_len() - 1,
            Action::Insert => self.mode = Mode::Insert,
            Action::Visual => {
                self.mode = match self.mode {
                    Mode::Visual { .. } => Mode::Normal,
                    _ => Mode::Visual {
                        anchor: self.cursor.step,
                    },
                }
            }
            Action::CommandLine => self.mode = Mode::Command(String::new()),
            Action::Escape => self.mode = Mode::Normal,
            Action::Clear => {
                self.edit_selection(|s| *s = Step::default());
                return vec![Effect::TrackChanged(track)];
            }
            Action::ClearAdvance => {
                self.checkpoint();
                self.grid.tracks[track].steps[self.cursor.step] = Step::default();
                self.advance();
                return vec![Effect::TrackChanged(track)];
            }
            Action::Yank => {
                let (start, end) = self.selection();
                self.clipboard = self.grid.tracks[track].steps[start..=end].to_vec();
                self.message = Some(format!("copied {} steps", self.clipboard.len()));
                self.mode = Mode::Normal;
                self.cursor.step = start;
            }
            Action::Paste => {
                if self.clipboard.is_empty() {
                    self.message = Some("nothing to paste".into());
                    return vec![];
                }
                self.checkpoint();
                let start = self.cursor.step;
                let steps = &mut self.grid.tracks[track].steps;
                for (i, step) in self
                    .clipboard
                    .iter()
                    .cycle()
                    .take(self.clipboard.len() * n)
                    .enumerate()
                {
                    if let Some(slot) = steps.get_mut(start + i) {
                        *slot = step.clone();
                    }
                }
                return vec![Effect::TrackChanged(track)];
            }
            Action::Undo => return self.undo(),
            Action::Redo => return self.redo(),
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
            Action::ToggleFollow => {
                self.follow = !self.follow;
                self.message = Some(format!("follow {}", if self.follow { "on" } else { "off" }));
            }
            Action::Quit => return vec![Effect::Quit],
        }
        vec![]
    }

    fn track_len(&self) -> usize {
        self.grid.tracks[self.cursor.track].steps.len()
    }

    fn move_track(&mut self, by: isize) {
        if matches!(self.mode, Mode::Visual { .. }) {
            return; // A selection stays within one track.
        }
        let last = self.grid.tracks.len() - 1;
        self.cursor.track = self.cursor.track.saturating_add_signed(by).min(last);
        self.cursor.step = self.cursor.step.min(self.track_len() - 1);
    }

    fn move_step(&mut self, by: isize) {
        self.cursor.step = self
            .cursor
            .step
            .saturating_add_signed(by)
            .min(self.track_len() - 1);
    }

    /// Moves on by the auto-step amount, wrapping round the track.
    fn advance(&mut self) {
        self.cursor.step = (self.cursor.step + self.auto_step) % self.track_len();
    }

    fn edit_selection(&mut self, mut f: impl FnMut(&mut Step)) {
        self.checkpoint();
        let (start, end) = self.selection();
        for step in &mut self.grid.tracks[self.cursor.track].steps[start..=end] {
            f(step);
        }
        self.mode = Mode::Normal;
        // Like vim, the cursor goes to the start of what was changed.
        self.cursor.step = start;
    }

    fn transpose(&mut self, by: i32) -> Vec<Effect> {
        let mut last = None;
        self.edit_selection(|s| {
            if let Some(note) = &mut s.note {
                note.pitch = (i32::from(note.pitch) + by).clamp(0, 127) as u8;
                last = Some(note.pitch);
            }
        });
        let track = self.cursor.track;
        let mut effects = vec![Effect::TrackChanged(track)];
        if let Some(pitch) = last {
            effects.push(Effect::Preview { track, pitch });
        }
        effects
    }

    fn lengthen(&mut self, by: i32) -> Vec<Effect> {
        self.edit_selection(|s| {
            if let Some(note) = &mut s.note {
                note.length = (i32::from(note.length) + by).clamp(1, MAX_STEPS as i32) as u16;
            }
        });
        vec![Effect::TrackChanged(self.cursor.track)]
    }

    /// Saves the current state for undo. Call before every change to the song.
    pub(crate) fn checkpoint(&mut self) {
        self.undo.push(Snapshot {
            grid: self.grid.clone(),
            cursor: self.cursor,
        });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn undo(&mut self) -> Vec<Effect> {
        self.restore(true)
    }

    fn redo(&mut self) -> Vec<Effect> {
        self.restore(false)
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
        to.push(Snapshot {
            grid: std::mem::replace(&mut self.grid, snapshot.grid),
            cursor: self.cursor,
        });
        self.cursor = snapshot.cursor;
        self.mode = Mode::Normal;
        (0..self.grid.tracks.len())
            .map(Effect::TrackChanged)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{Condition, Rule, Track, Transform};

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
    }

    #[test]
    fn undo_and_redo() {
        let mut e = editor();
        keys(&mut e, "iz");
        e.handle(Key::Esc);
        keys(&mut e, "kx");
        assert_eq!(pitches(&e, 0)[0], None);
        keys(&mut e, "u");
        assert_eq!(pitches(&e, 0)[0], Some(60));
        keys(&mut e, "u");
        assert_eq!(pitches(&e, 0)[0], None);
        e.handle(Key::Ctrl('r'));
        assert_eq!(pitches(&e, 0)[0], Some(60));
    }

    #[test]
    fn visual_yank_and_paste() {
        let mut e = editor();
        keys(&mut e, "izxc");
        e.handle(Key::Esc);
        keys(&mut e, "gvjjy8jp");
        assert_eq!(&pitches(&e, 0)[8..12], [Some(60), Some(62), Some(64), None]);
    }

    #[test]
    fn transpose_and_lengthen_selection() {
        let mut e = editor();
        keys(&mut e, "izx");
        e.handle(Key::Esc);
        keys(&mut e, "gvj");
        e.handle(Key::Ctrl('a'));
        assert_eq!(&pitches(&e, 0)[..2], [Some(61), Some(63)]);
        keys(&mut e, "3+");
        assert_eq!(e.grid.tracks[0].steps[0].note.unwrap().length, 4);
        keys(&mut e, ">");
        assert_eq!(pitches(&e, 0)[0], Some(73));
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
        keys(&mut e, "G");
        assert_eq!(command(&mut e, "len 4"), [Effect::TrackChanged(0)]);
        assert_eq!(e.grid.tracks[0].steps.len(), 4);
        assert_eq!(e.cursor.step, 3);
        assert_eq!(command(&mut e, "bpm 90"), [Effect::Bpm(90.0)]);
    }
}
