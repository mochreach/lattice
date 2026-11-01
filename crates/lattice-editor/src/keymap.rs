//! Keys, the actions they trigger, and the default bindings, modelled on Helix.

use std::collections::HashMap;
use std::fmt;

use crate::pads::{SCALES, Scale};

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
    /// The numpad's `.` `/` `*` `-` `+`.
    PadOp(char),
}

impl fmt::Display for Key {
    /// Helix-style names: `g`, `C-a`, `A-;`, `esc`.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Key::Char(' ') => f.write_str("space"),
            Key::Char(c) => write!(f, "{c}"),
            Key::Ctrl(' ') => f.write_str("C-space"),
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
            Key::PadOp(c) => write!(f, "pad{c}"),
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
    GotoSong,
    /// The mixer page, on the instrument of the cursor's track.
    GotoInstrument,
    /// The mixer page, on the effects of the cursor's track.
    GotoEffects,
    NextBeat,
    PrevBeat,
    NextBar,
    NextPattern,
    PrevPattern,
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
    /// Song page: add an entry after the cursor's, or before.
    OpenBelow,
    OpenAbove,
    /// Song page: edit the entry's pattern.
    Open,
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
    /// The same, in this pattern only.
    TogglePatternMute,
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
    /// Play the song from the entry playing this pattern, or the song page's cursor.
    PlaySong,
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
            Action::GotoSong => "song page",
            Action::GotoInstrument => "instrument, on the mixer",
            Action::GotoEffects => "effects, on the mixer",
            Action::NextBeat => "next beat",
            Action::PrevBeat => "previous beat",
            Action::NextBar => "next bar",
            Action::NextPattern => "next pattern",
            Action::PrevPattern => "previous pattern",
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
            Action::OpenBelow => "add an entry after",
            Action::OpenAbove => "add an entry before",
            Action::Open => "edit the entry's pattern",
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
            Action::TogglePatternMute => "mute / unmute in this pattern",
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
            Action::PlaySong => "play the song from here",
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

/// The rows of the main keyboard, bottom up, each starting at its left. `[` and `]` are left
/// out, as they change octave.
const ROWS: [&str; 4] = ["zxcvbnm,./", "asdfghjkl;'", "qwertyuiop", "1234567890-="];

/// Ways of laying notes out on the keyboard in insert mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoteLayout {
    /// The tracker's two-row piano: Z–M is the lower octave, Q–U the upper.
    #[default]
    Piano,
    /// Wicki–Hayden, as on a concertina: a whole tone to the right, a fourth up-left and a fifth
    /// up-right (each row sits a little left of the one below), so every chord of a kind has the
    /// same shape.
    Wicki,
    /// As on a guitar or LinnStrument: a semitone to the right and a fourth a row up.
    Fourths,
    /// Only the notes of the pads' scale (`:scale`), from its root, as in Push's "in key" mode:
    /// the next note of the scale to the right, and about a fourth in its steps a row up.
    Scale,
}

impl NoteLayout {
    pub const ALL: [NoteLayout; 4] = [
        NoteLayout::Piano,
        NoteLayout::Wicki,
        NoteLayout::Fourths,
        NoteLayout::Scale,
    ];

    pub fn name(self) -> &'static str {
        match self {
            NoteLayout::Piano => "piano",
            NoteLayout::Wicki => "wicki",
            NoteLayout::Fourths => "fourths",
            NoteLayout::Scale => "scale",
        }
    }

    /// Key → semitones above the keyboard octave's C. Only [`NoteLayout::Scale`] uses `scale`
    /// and its `root` (semitones above C).
    pub fn keys(self, scale: &Scale, root: u8) -> HashMap<char, u8> {
        let grid = |up: usize, across: usize, pitch: &dyn Fn(usize) -> u8| -> HashMap<char, u8> {
            ROWS.iter()
                .enumerate()
                .flat_map(|(row, keys)| {
                    keys.chars()
                        .enumerate()
                        .map(move |(col, c)| (c, row * up + col * across))
                })
                .map(|(c, n)| (c, pitch(n)))
                .collect()
        };
        let semitones = |n: usize| n as u8;
        match self {
            NoteLayout::Piano => PIANO.into_iter().collect(),
            NoteLayout::Wicki => grid(5, 2, &semitones),
            NoteLayout::Fourths => grid(5, 1, &semitones),
            NoteLayout::Scale => {
                // A fourth is 5 of 12 semitones: 3 steps of a seven-note scale, 2 of five or six
                // (rounding a half down, as blues' third step is a tritone).
                let steps = scale.steps;
                let up = (steps.len() * 5 + 5) / 12;
                grid(up, 1, &|degree| {
                    let octave = (degree / steps.len()) as u8;
                    root + steps[degree % steps.len()] + 12 * octave
                })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Keymap {
    /// Normal and select mode. Select mode uses the same keys; motions extend instead of move.
    pub normal: HashMap<Key, Binding>,
    pub insert: HashMap<Key, Action>,
    /// Insert mode and `r`: key → semitones above the keyboard octave's C, laid out as `layout`.
    pub piano: HashMap<char, u8>,
    pub layout: NoteLayout,
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
            (Ctrl(' '), PlaySong),
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
                (Char('s'), GotoSong),
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
                (Char('p'), NextPattern),
            ],
        };
        let previous = Menu {
            title: "previous",
            items: vec![
                (Char('b'), PrevBeat),
                (Char('m'), PrevBar),
                (Char('n'), PrevNote),
                (Char('p'), PrevPattern),
            ],
        };

        // Not Helix's `t` (select till a character), which has no use on a grid.
        let track = Menu {
            title: "track",
            items: vec![
                (Char('t'), SelectTracks),
                (Char('m'), ToggleMute),
                (Char('M'), TogglePatternMute),
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
                (Char('o'), OpenBelow),
                (Char('O'), OpenAbove),
                (Key::Enter, Open),
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
            piano: NoteLayout::Piano.keys(&SCALES[0], 0),
            layout: NoteLayout::Piano,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isomorphic_layouts_keep_their_intervals_everywhere() {
        let wicki = NoteLayout::Wicki.keys(&SCALES[0], 0);
        // z is C; right a tone, up-left (a) a fourth, up-right (s) a fifth.
        assert_eq!(
            [wicki[&'z'], wicki[&'x'], wicki[&'a'], wicki[&'s']],
            [0, 2, 5, 7]
        );
        // The same shape a row and two keys over: a major triad on F (a, d, then s's neighbour).
        assert_eq!([wicki[&'a'], wicki[&'d'], wicki[&'w']], [5, 9, 12]);
        assert_eq!(wicki[&'='], 37);

        let fourths = NoteLayout::Fourths.keys(&SCALES[0], 0);
        assert_eq!(
            [fourths[&'z'], fourths[&'x'], fourths[&'a'], fourths[&'q']],
            [0, 1, 5, 10]
        );
        // Octave keys stay octave keys.
        assert!(!wicki.contains_key(&'[') && !fourths.contains_key(&']'));
    }

    #[test]
    fn the_scale_layout_plays_only_the_scale() {
        // A minor: A B C D E F G, then up a row is three steps, a fourth.
        let minor = NoteLayout::Scale.keys(&SCALES[1], 9);
        let row: Vec<_> = "zxcvbnm,".chars().map(|c| minor[&c]).collect();
        assert_eq!(row, [9, 11, 12, 14, 16, 17, 19, 21]);
        assert_eq!(minor[&'a'], 14);
        // Pentatonic: up a row is two steps.
        let pentatonic = NoteLayout::Scale.keys(&SCALES[4], 0);
        assert_eq!(
            [pentatonic[&'z'], pentatonic[&'a'], pentatonic[&'q']],
            [0, 4, 9]
        );
    }
}
