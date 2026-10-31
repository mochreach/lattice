//! Keys, the actions they trigger, and the default bindings, modelled on Helix.

use std::collections::HashMap;
use std::fmt;

/// A key press, independent of any terminal library. Shifted letters arrive as uppercase chars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Ctrl(char),
    Alt(char),
    Esc,
    Enter,
    Backspace,
    Delete,
    Tab,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    /// A numpad digit, when the terminal tells the numpad apart.
    Pad(u8),
}

impl fmt::Display for Key {
    /// Helix-style names: `g`, `C-a`, `A-;`, `esc`.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Key::Char(' ') => f.write_str("space"),
            Key::Char(c) => write!(f, "{c}"),
            Key::Ctrl(c) => write!(f, "C-{c}"),
            Key::Alt(c) => write!(f, "A-{c}"),
            Key::Esc => f.write_str("esc"),
            Key::Enter => f.write_str("ret"),
            Key::Backspace => f.write_str("backspace"),
            Key::Delete => f.write_str("del"),
            Key::Tab => f.write_str("tab"),
            Key::Up => f.write_str("up"),
            Key::Down => f.write_str("down"),
            Key::Left => f.write_str("left"),
            Key::Right => f.write_str("right"),
            Key::PageUp => f.write_str("pageup"),
            Key::PageDown => f.write_str("pagedown"),
            Key::Home => f.write_str("home"),
            Key::End => f.write_str("end"),
            Key::Pad(n) => write!(f, "pad{n}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Left,
    Right,
    Up,
    Down,
    HalfPageUp,
    HalfPageDown,
    PageUp,
    PageDown,
    FirstStep,
    LastStep,
    FirstTrack,
    LastTrack,
    GotoMixer,
    GotoPattern,
    /// The mixer page, on the instrument of the cursor's track.
    GotoInstrument,
    /// The mixer page, on the effects of the cursor's track.
    GotoEffects,
    NextBeat,
    PrevBeat,
    NextBar,
    PrevBar,
    NextNote,
    PrevNote,
    /// Select whole rows across every track, or extend a row selection downwards.
    SelectRows,
    SelectAll,
    /// Select every step of the tracks the selections reach.
    SelectTracks,
    Collapse,
    Flip,
    SelectMode,
    /// Add a copy of each selection just below it, or just above.
    CopyDown,
    CopyUp,
    /// Split each selection into one per row.
    SplitRows,
    /// Merge every selection into one covering them all.
    MergeAll,
    KeepPrimary,
    RemovePrimary,
    /// Make the next or previous selection the primary.
    RotateForward,
    RotateBackward,
    /// Prompt for notes, and select each matching step inside the selections.
    SelectNotes,
    /// Prompt for notes, and keep only the selections with a match, or without one.
    KeepNotes,
    RemoveNotes,
    Insert,
    Append,
    Change,
    CommandLine,
    Escape,
    /// Copy, then clear.
    Delete,
    /// Clear without copying.
    Clear,
    /// Insert mode: clear the step and move on.
    ClearAdvance,
    Yank,
    /// Paste over the selection, from its first step: a tracker's paste.
    Paste,
    /// Paste at the selection, pushing the steps there down.
    PasteInsert,
    /// Fill the selection with the next note played.
    Replace,
    Undo,
    Redo,
    NoteUp,
    NoteDown,
    NoteOctaveUp,
    NoteOctaveDown,
    Longer,
    Shorter,
    KeyboardOctaveUp,
    KeyboardOctaveDown,
    /// Mute or unmute every track the selections reach.
    ToggleMute,
    ToggleSolo,
    AddTrack,
    /// Copy, then remove, every track the selections reach.
    RemoveTrack,
    YankTracks,
    PasteTracksAfter,
    PasteTracksBefore,
    MoveTrackLeft,
    MoveTrackRight,
    /// Group the tracks the selections reach, or ungroup the group they're in.
    ToggleGroup,
    /// Fold or unfold the groups the selections reach.
    ToggleFold,
    TogglePlay,
    /// Arm or disarm recording at the playhead.
    ToggleRecord,
    ToggleView,
    Quit,
}

impl Action {
    /// One-line description, for the infobox and help.
    pub fn describe(self) -> &'static str {
        match self {
            Action::Left => "previous track",
            Action::Right => "next track",
            Action::Up => "previous step",
            Action::Down => "next step",
            Action::HalfPageUp => "up half a page",
            Action::HalfPageDown => "down half a page",
            Action::PageUp => "up a page",
            Action::PageDown => "down a page",
            Action::FirstStep => "first step",
            Action::LastStep => "last step",
            Action::FirstTrack => "first track",
            Action::LastTrack => "last track",
            Action::GotoMixer => "mixer page",
            Action::GotoPattern => "pattern page",
            Action::GotoInstrument => "instrument, on the mixer",
            Action::GotoEffects => "effects, on the mixer",
            Action::NextBeat => "next beat",
            Action::PrevBeat => "previous beat",
            Action::NextBar => "next bar",
            Action::PrevBar => "previous bar",
            Action::NextNote => "next note",
            Action::PrevNote => "previous note",
            Action::SelectRows => "select rows",
            Action::SelectAll => "select everything",
            Action::SelectTracks => "select whole tracks",
            Action::Collapse => "collapse selection",
            Action::Flip => "flip selection",
            Action::SelectMode => "select mode",
            Action::CopyDown => "copy selection onto next rows",
            Action::CopyUp => "copy selection onto previous rows",
            Action::SplitRows => "split selection into rows",
            Action::MergeAll => "merge selections",
            Action::KeepPrimary => "keep primary selection",
            Action::RemovePrimary => "remove primary selection",
            Action::RotateForward => "next selection primary",
            Action::RotateBackward => "previous selection primary",
            Action::SelectNotes => "select matching notes",
            Action::KeepNotes => "keep selections with matching notes",
            Action::RemoveNotes => "remove selections with matching notes",
            Action::Insert => "insert at start",
            Action::Append => "insert after end",
            Action::Change => "clear and insert",
            Action::CommandLine => "command line",
            Action::Escape => "back to normal mode",
            Action::Delete => "delete (and copy)",
            Action::Clear => "delete without copying",
            Action::ClearAdvance => "clear step and move on",
            Action::Yank => "copy",
            Action::Paste => "paste",
            Action::PasteInsert => "paste, pushing the rest down",
            Action::Replace => "fill with a note",
            Action::Undo => "undo",
            Action::Redo => "redo",
            Action::NoteUp => "note up a semitone",
            Action::NoteDown => "note down a semitone",
            Action::NoteOctaveUp => "note up an octave",
            Action::NoteOctaveDown => "note down an octave",
            Action::Longer => "longer note",
            Action::Shorter => "shorter note",
            Action::KeyboardOctaveUp => "keyboard octave up",
            Action::KeyboardOctaveDown => "keyboard octave down",
            Action::ToggleMute => "mute / unmute",
            Action::ToggleSolo => "solo / unsolo",
            Action::AddTrack => "add a track after this one",
            Action::RemoveTrack => "remove tracks (and copy)",
            Action::YankTracks => "copy tracks",
            Action::PasteTracksAfter => "paste tracks after",
            Action::PasteTracksBefore => "paste tracks before",
            Action::MoveTrackLeft => "move track left",
            Action::MoveTrackRight => "move track right",
            Action::ToggleGroup => "group / ungroup tracks",
            Action::ToggleFold => "fold / unfold group",
            Action::TogglePlay => "play / stop",
            Action::ToggleRecord => "record on / off",
            Action::ToggleView => "written or playing pattern",
            Action::Quit => "quit",
        }
    }
}

/// A prefix key's menu, shown in the infobox while it waits for the next key.
#[derive(Debug, Clone)]
pub struct Menu {
    pub title: &'static str,
    /// In display order.
    pub items: Vec<(Key, Action)>,
}

#[derive(Debug, Clone)]
pub enum Binding {
    Action(Action),
    Menu(Menu),
}

/// The two-row piano layout used by most trackers: Z–M is the lower octave, Q–U the upper.
const PIANO: [(char, u8); 29] = [
    ('z', 0),
    ('s', 1),
    ('x', 2),
    ('d', 3),
    ('c', 4),
    ('v', 5),
    ('g', 6),
    ('b', 7),
    ('h', 8),
    ('n', 9),
    ('j', 10),
    ('m', 11),
    (',', 12),
    ('l', 13),
    ('.', 14),
    (';', 15),
    ('/', 16),
    ('q', 12),
    ('2', 13),
    ('w', 14),
    ('3', 15),
    ('e', 16),
    ('r', 17),
    ('5', 18),
    ('t', 19),
    ('6', 20),
    ('y', 21),
    ('7', 22),
    ('u', 23),
];

#[derive(Debug, Clone)]
pub struct Keymap {
    /// Normal and select mode. Select mode uses the same keys; motions extend instead of move.
    pub normal: HashMap<Key, Binding>,
    pub insert: HashMap<Key, Action>,
    /// Insert mode and `r`: key → semitones above the keyboard octave's C.
    pub piano: HashMap<char, u8>,
}

impl Default for Keymap {
    fn default() -> Self {
        use Action::*;
        use Key::*;

        // Shared by normal and insert mode.
        let common = [
            (Key::Left, Action::Left),
            (Key::Right, Action::Right),
            (Key::Up, Action::Up),
            (Key::Down, Action::Down),
            (Key::PageUp, Action::PageUp),
            (Key::PageDown, Action::PageDown),
            (Home, FirstStep),
            (End, LastStep),
            (Esc, Escape),
            (Char(' '), TogglePlay),
            (Tab, ToggleView),
            (Char('R'), ToggleRecord),
            (Ctrl('c'), Quit),
        ];

        let goto = Menu {
            title: "goto",
            items: vec![
                (Char('g'), FirstStep),
                (Char('e'), LastStep),
                (Char('h'), FirstTrack),
                (Char('l'), LastTrack),
                (Char('m'), GotoMixer),
                (Char('p'), GotoPattern),
                (Char('i'), GotoInstrument),
                (Char('f'), GotoEffects),
            ],
        };

        let next = Menu {
            title: "next",
            items: vec![
                (Char('b'), NextBeat),
                (Char('m'), NextBar),
                (Char('n'), NextNote),
            ],
        };
        let previous = Menu {
            title: "previous",
            items: vec![
                (Char('b'), PrevBeat),
                (Char('m'), PrevBar),
                (Char('n'), PrevNote),
            ],
        };

        // Not Helix's `t` (select till a character), which has no use on a grid.
        let track = Menu {
            title: "track",
            items: vec![
                (Char('t'), SelectTracks),
                (Char('m'), ToggleMute),
                (Char('s'), ToggleSolo),
                (Char('a'), AddTrack),
                (Char('d'), RemoveTrack),
                (Char('y'), YankTracks),
                (Char('p'), PasteTracksAfter),
                (Char('P'), PasteTracksBefore),
                (Char('h'), MoveTrackLeft),
                (Char('l'), MoveTrackRight),
                (Char('g'), ToggleGroup),
                (Char('z'), ToggleFold),
            ],
        };

        let normal = common
            .into_iter()
            .chain([
                (Char('h'), Action::Left),
                (Char('l'), Action::Right),
                (Char('k'), Action::Up),
                (Char('j'), Action::Down),
                (Ctrl('u'), HalfPageUp),
                (Ctrl('d'), HalfPageDown),
                (Ctrl('b'), Action::PageUp),
                (Ctrl('f'), Action::PageDown),
                (Char('w'), NextNote),
                (Char('b'), PrevNote),
                (Char('x'), SelectRows),
                (Char('%'), SelectAll),
                (Char(';'), Collapse),
                (Alt(';'), Flip),
                (Char('v'), SelectMode),
                (Char('C'), CopyDown),
                (Alt('C'), CopyUp),
                (Alt('s'), SplitRows),
                (Alt('-'), MergeAll),
                (Char(','), KeepPrimary),
                (Alt(','), RemovePrimary),
                (Char(')'), RotateForward),
                (Char('('), RotateBackward),
                (Char('s'), SelectNotes),
                (Char('K'), KeepNotes),
                (Alt('K'), RemoveNotes),
                (Char('i'), Insert),
                (Char('a'), Append),
                (Char('c'), Change),
                (Char(':'), CommandLine),
                (Char('d'), Action::Delete),
                (Alt('d'), Clear),
                (Key::Delete, Clear),
                (Char('y'), Yank),
                (Char('p'), Paste),
                (Char('P'), PasteInsert),
                (Char('r'), Replace),
                (Char('u'), Undo),
                (Char('U'), Redo),
                (Ctrl('a'), NoteUp),
                (Ctrl('x'), NoteDown),
                (Char('>'), NoteOctaveUp),
                (Char('<'), NoteOctaveDown),
                (Char('+'), Longer),
                (Char('='), Longer),
                (Char('-'), Shorter),
            ])
            .map(|(key, action)| (key, Binding::Action(action)))
            .chain([
                (Char('g'), Binding::Menu(goto)),
                (Char(']'), Binding::Menu(next)),
                (Char('['), Binding::Menu(previous)),
                (Char('t'), Binding::Menu(track)),
            ])
            .collect();

        let insert = common
            .into_iter()
            .chain([
                (Backspace, ClearAdvance),
                (Key::Delete, ClearAdvance),
                (Char('['), KeyboardOctaveDown),
                (Char(']'), KeyboardOctaveUp),
            ])
            .collect();

        Self {
            normal,
            insert,
            piano: PIANO.into_iter().collect(),
        }
    }
}
