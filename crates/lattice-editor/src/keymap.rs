//! Keys, the actions they trigger, and the default bindings.

use std::collections::HashMap;

/// A key press, independent of any terminal library. Shifted letters arrive as uppercase chars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Ctrl(char),
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Left,
    Right,
    Up,
    Down,
    Top,
    Bottom,
    PageUp,
    PageDown,
    Insert,
    Visual,
    CommandLine,
    Escape,
    Clear,
    /// Insert mode: clear the step and move on.
    ClearAdvance,
    Yank,
    Paste,
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
    TogglePlay,
    ToggleView,
    ToggleFollow,
    Quit,
}

impl Action {
    /// One-line description, for help and hints.
    pub fn describe(self) -> &'static str {
        match self {
            Action::Left => "previous track",
            Action::Right => "next track",
            Action::Up => "previous step",
            Action::Down => "next step",
            Action::Top => "first step",
            Action::Bottom => "last step",
            Action::PageUp => "up 16 steps",
            Action::PageDown => "down 16 steps",
            Action::Insert => "insert mode: play notes into the track",
            Action::Visual => "select steps",
            Action::CommandLine => "command line",
            Action::Escape => "back to normal mode",
            Action::Clear => "clear steps",
            Action::ClearAdvance => "clear step and move on",
            Action::Yank => "copy steps",
            Action::Paste => "paste steps",
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
            Action::TogglePlay => "play / stop",
            Action::ToggleView => "show written or playing pattern",
            Action::ToggleFollow => "follow the playhead",
            Action::Quit => "quit",
        }
    }
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
    pub normal: HashMap<Key, Action>,
    pub insert: HashMap<Key, Action>,
    /// Insert mode: key → semitones above the keyboard octave's C.
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
            (Home, Top),
            (End, Bottom),
            (Esc, Escape),
            (Char(' '), TogglePlay),
            (Tab, ToggleView),
            (Char('['), KeyboardOctaveDown),
            (Char(']'), KeyboardOctaveUp),
            (Ctrl('c'), Quit),
        ];

        let normal = common
            .into_iter()
            .chain([
                (Char('h'), Action::Left),
                (Char('l'), Action::Right),
                (Char('k'), Action::Up),
                (Char('j'), Action::Down),
                (Char('g'), Top),
                (Char('G'), Bottom),
                (Ctrl('u'), Action::PageUp),
                (Ctrl('d'), Action::PageDown),
                (Char('i'), Insert),
                (Char('v'), Visual),
                (Char(':'), CommandLine),
                (Char('x'), Clear),
                (Char('d'), Clear),
                (Delete, Clear),
                (Char('y'), Yank),
                (Char('p'), Paste),
                (Char('u'), Undo),
                (Ctrl('r'), Redo),
                (Ctrl('a'), NoteUp),
                (Ctrl('x'), NoteDown),
                (Char('>'), NoteOctaveUp),
                (Char('<'), NoteOctaveDown),
                (Char('+'), Longer),
                (Char('='), Longer),
                (Char('-'), Shorter),
                (Char('f'), ToggleFollow),
            ])
            .collect();

        let insert = common
            .into_iter()
            .chain([(Backspace, ClearAdvance), (Delete, ClearAdvance)])
            .collect();

        Self {
            normal,
            insert,
            piano: PIANO.into_iter().collect(),
        }
    }
}
