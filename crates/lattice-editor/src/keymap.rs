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

impl Key {
    /// A key as Helix writes it in its config: `a`, `C-a`, `A-;`, `esc`, `ret`, `space`,
    /// `minus`. The opposite of how it's shown.
    pub fn parse(name: &str) -> Option<Key> {
        let single = |s: &str| {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => match s {
                    "space" => Some(' '),
                    "minus" => Some('-'),
                    "backtick" => Some('`'),
                    _ => None,
                },
            }
        };
        if let Some(rest) = name.strip_prefix("C-") {
            return single(rest).map(|c| Key::Ctrl(c.to_ascii_lowercase()));
        }
        if let Some(rest) = name.strip_prefix("A-") {
            return single(rest).map(Key::Alt);
        }
        let key = match name {
            "esc" => Key::Esc,
            "ret" => Key::Enter,
            "backspace" => Key::Backspace,
            "del" => Key::Delete,
            "tab" => Key::Tab,
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "pageup" => Key::PageUp,
            "pagedown" => Key::PageDown,
            "home" => Key::Home,
            "end" => Key::End,
            _ => match name.strip_prefix("pad") {
                Some(rest) if !rest.is_empty() => match single(rest)? {
                    c @ '0'..='9' => Key::Pad(c as u8 - b'0'),
                    c => Key::PadOp(c),
                },
                _ => Key::Char(single(name)?),
            },
        };
        Some(key)
    }

    /// Keys typed in a row, as the tutor writes them: `gp:bpm 140<ret>`, with any key that isn't
    /// a character by its name in angle brackets, and `<lt>` for `<`.
    pub fn sequence(keys: &str) -> Result<Vec<Key>, String> {
        let mut out = vec![];
        let mut rest = keys;
        while let Some(c) = rest.chars().next() {
            rest = &rest[c.len_utf8()..];
            if c != '<' {
                out.push(Key::Char(c));
                continue;
            }
            let Some((name, after)) = rest.split_once('>') else {
                return Err("a < without its >: <lt> types a <".into());
            };
            let key = match name {
                "lt" => Some(Key::Char('<')),
                "gt" => Some(Key::Char('>')),
                _ => Key::parse(name),
            };
            out.push(key.ok_or_else(|| format!("no key called <{name}>"))?);
            rest = after;
        }
        Ok(out)
    }
}

impl fmt::Display for Key {
    /// Helix-style names: `g`, `C-a`, `A-;`, `esc`.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Key::Char(' ') => f.write_str("space"),
            // Named, as the tutor and help mark keys out between backticks.
            Key::Char('`') => f.write_str("backtick"),
            Key::Char(c) => write!(f, "{c}"),
            Key::Ctrl(' ') => f.write_str("C-space"),
            Key::Alt(' ') => f.write_str("A-space"),
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
    /// Turn the selected steps back to front; on a chop track, each piece plays backwards too.
    ReverseSteps,
    /// Repeat the first step, or with a count the first that many, through the selection.
    RepeatSteps,
    /// Move the selected steps down one, or up, the last coming round to the first.
    RotateStepsForward,
    RotateStepsBackward,
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
    /// On the mixer page, start locking; again, lock the settings changed since onto the
    /// selected steps, and put them back.
    LockSetting,
    /// Loop playback round the selected steps, or on the song page, the selected bars or
    /// entries; with only the cursor, round its beat or bar; on what's looping, stop.
    LoopSelection,
    ToggleSolo,
    AddTrack,
    /// Copy, then remove, every track the selections reach.
    RemoveTrack,
    YankTracks,
    PasteTracksAfter,
    PasteTracksBefore,
    MoveTrackLeft,
    MoveTrackRight,
    /// To the previous or next track, on its notes.
    PrevTrack,
    NextTrack,
    /// `f` then a key: to a field of the open track.
    GotoField,
    /// Fill the field evenly between the values at each selection's ends.
    Fill,
    /// Group the tracks the selections reach, or ungroup the group they're in.
    ToggleGroup,
    /// Fold or unfold the groups the selections reach.
    ToggleFold,
    /// Keep the tracks the selections reach open, showing their fields; again, close them.
    ToggleOpen,
    TogglePlay,
    /// Play the song from its start.
    PlaySong,
    /// Play the song from the entry playing this pattern, or the song page's cursor.
    PlayFromHere,
    /// Stop the song and every sound: held notes, echoes, reverb tails and loops.
    Hush,
    /// Play a fill while it's held, or until it's pressed again where the terminal doesn't
    /// report key releases.
    PlayFill,
    /// Arm or disarm recording at the playhead.
    ToggleRecord,
    ToggleView,
    Quit,
}

impl Action {
    /// Every action, in the order they're declared, for finding them by name.
    pub const ALL: &[Action] = &[
        Action::Left,
        Action::Right,
        Action::Up,
        Action::Down,
        Action::HalfPageUp,
        Action::HalfPageDown,
        Action::PageUp,
        Action::PageDown,
        Action::FirstStep,
        Action::LastStep,
        Action::FirstTrack,
        Action::LastTrack,
        Action::GotoMixer,
        Action::GotoPattern,
        Action::GotoSong,
        Action::GotoInstrument,
        Action::GotoEffects,
        Action::NextBeat,
        Action::PrevBeat,
        Action::NextBar,
        Action::NextPattern,
        Action::PrevPattern,
        Action::PrevBar,
        Action::NextNote,
        Action::PrevNote,
        Action::SelectRows,
        Action::SelectAll,
        Action::SelectTracks,
        Action::Collapse,
        Action::Flip,
        Action::SelectMode,
        Action::CopyDown,
        Action::CopyUp,
        Action::ReverseSteps,
        Action::RepeatSteps,
        Action::RotateStepsForward,
        Action::RotateStepsBackward,
        Action::SplitRows,
        Action::MergeAll,
        Action::KeepPrimary,
        Action::RemovePrimary,
        Action::RotateForward,
        Action::RotateBackward,
        Action::SelectNotes,
        Action::KeepNotes,
        Action::RemoveNotes,
        Action::Insert,
        Action::Append,
        Action::OpenBelow,
        Action::OpenAbove,
        Action::Open,
        Action::Change,
        Action::CommandLine,
        Action::Escape,
        Action::Delete,
        Action::Clear,
        Action::ClearAdvance,
        Action::Yank,
        Action::Paste,
        Action::PasteInsert,
        Action::Replace,
        Action::Undo,
        Action::Redo,
        Action::NoteUp,
        Action::NoteDown,
        Action::NoteOctaveUp,
        Action::NoteOctaveDown,
        Action::Longer,
        Action::Shorter,
        Action::KeyboardOctaveUp,
        Action::KeyboardOctaveDown,
        Action::ToggleMute,
        Action::TogglePatternMute,
        Action::LockSetting,
        Action::LoopSelection,
        Action::ToggleSolo,
        Action::AddTrack,
        Action::RemoveTrack,
        Action::YankTracks,
        Action::PasteTracksAfter,
        Action::PasteTracksBefore,
        Action::MoveTrackLeft,
        Action::MoveTrackRight,
        Action::PrevTrack,
        Action::NextTrack,
        Action::GotoField,
        Action::Fill,
        Action::ToggleGroup,
        Action::ToggleFold,
        Action::ToggleOpen,
        Action::TogglePlay,
        Action::PlaySong,
        Action::PlayFromHere,
        Action::Hush,
        Action::PlayFill,
        Action::ToggleRecord,
        Action::ToggleView,
        Action::Quit,
    ];

    /// What a config file calls it: the variant in snake case, as Helix names its commands
    /// (`goto_mixer`, `toggle_pattern_mute`).
    pub fn name(self) -> String {
        let mut name = String::new();
        for (i, c) in format!("{self:?}").chars().enumerate() {
            if c.is_ascii_uppercase() && i > 0 {
                name.push('_');
            }
            name.push(c.to_ascii_lowercase());
        }
        name
    }

    pub fn named(name: &str) -> Option<Action> {
        Action::ALL.iter().copied().find(|a| a.name() == name)
    }

    /// One-line description, for the infobox and help.
    pub fn describe(self) -> &'static str {
        match self {
            Action::Left => "previous field, or track",
            Action::Right => "next field, or track",
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
            Action::GotoInstrument => "instrument page: every track's instrument, side by side",
            Action::GotoEffects => "fx page: the track's effects, in lanes",
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
            Action::ReverseSteps => "reverse selected steps",
            Action::RepeatSteps => "repeat first step (or count) through selection",
            Action::RotateStepsForward => "rotate selected steps down",
            Action::RotateStepsBackward => "rotate selected steps up",
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
            Action::LockSetting => "mixer: L, change settings, L locks them on the selected steps",
            Action::LoopSelection => "loop playback round the selection, or stop",
            Action::ToggleSolo => "solo / unsolo",
            Action::AddTrack => "add a track after this one",
            Action::RemoveTrack => "remove tracks (and copy)",
            Action::YankTracks => "copy tracks",
            Action::PasteTracksAfter => "paste tracks after",
            Action::PasteTracksBefore => "paste tracks before",
            Action::MoveTrackLeft => "move track left",
            Action::MoveTrackRight => "move track right",
            Action::PrevTrack => "previous track, on its notes",
            Action::NextTrack => "next track, on its notes",
            Action::GotoField => {
                "to a field: n note, v velocity, g G gestures, a lock column's letter, C the conductor"
            }
            Action::Fill => "fill the field evenly between the selection's ends",
            Action::ToggleGroup => "group / ungroup tracks",
            Action::ToggleFold => "fold / unfold group",
            Action::ToggleOpen => "keep tracks open / close them",
            Action::TogglePlay => "play / stop",
            Action::PlaySong => "play the song from the start",
            Action::PlayFromHere => "play the song from this pattern",
            Action::Hush => "stop everything: the song, every note, echo and tail",
            Action::PlayFill => "hold for a fill: steps marked F play, and F- don't",
            Action::ToggleRecord => "record on / off",
            Action::ToggleView => "written or playing pattern",
            Action::Quit => "quit",
        }
    }
}

/// A prefix key's menu, shown in the infobox while it waits for the next key.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub title: String,
    /// In display order. Never menus themselves.
    pub items: Vec<(Key, Binding)>,
}

impl Menu {
    fn new(title: &str, items: &[(Key, Action)]) -> Self {
        Self {
            title: title.into(),
            items: (items.iter())
                .map(|&(key, action)| (key, Binding::Action(action)))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Binding {
    Action(Action),
    /// A command line, without its `:`, run as though typed.
    Command(String),
    Menu(Menu),
}

impl Binding {
    /// What it does, for the infobox and help.
    pub fn describe(&self) -> String {
        match self {
            Binding::Action(action) => action.describe().into(),
            Binding::Command(line) => format!(":{line}"),
            Binding::Menu(menu) => format!("{}…", menu.title),
        }
    }

    /// What a config file binds a key to: an action's name, a command line after a `:`, or
    /// `none` for nothing.
    fn parse(target: &str) -> Result<Option<Binding>, String> {
        if target == "none" {
            return Ok(None);
        }
        if let Some(line) = target.strip_prefix(':') {
            let name = line.split_whitespace().next().unwrap_or_default();
            crate::command::find(name).ok_or_else(|| match crate::command::nearest(name) {
                Some(near) => format!("no command :{name}; did you mean :{near}?"),
                None => format!("no command :{name}"),
            })?;
            return Ok(Some(Binding::Command(line.trim().into())));
        }
        match Action::named(target) {
            Some(action) => Ok(Some(Binding::Action(action))),
            None => Err(format!("no action called {target:?}")),
        }
    }
}

/// The two-row piano layout used by most trackers: Z–M is the lower octave, Q–U the upper,
/// with the B below on `\` (left of Z on a UK keyboard), and on up from I to `]`.
const PIANO: [(char, i16); 38] = [
    ('\\', -1),
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
    ('i', 24),
    ('9', 25),
    ('o', 26),
    ('0', 27),
    ('p', 28),
    ('[', 29),
    ('=', 30),
    (']', 31),
];

/// The rows of the main keyboard, bottom up, each starting at its left.
const ROWS: [&str; 4] = ["zxcvbnm,./", "asdfghjkl;'", "qwertyuiop[]", "1234567890-="];

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
    pub fn keys(self, scale: &Scale, root: u8) -> HashMap<char, i16> {
        let grid = |up: usize, across: usize, pitch: &dyn Fn(usize) -> i16| -> HashMap<char, i16> {
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
        let semitones = |n: usize| n as i16;
        match self {
            NoteLayout::Piano => PIANO.into_iter().collect(),
            NoteLayout::Wicki => grid(5, 2, &semitones),
            NoteLayout::Fourths => grid(5, 1, &semitones),
            NoteLayout::Scale => {
                // A fourth is 5 of 12 semitones: 3 steps of a seven-note scale, 2 of five or six
                // (rounding a half down, as blues' third step is a tritone).
                let steps = scale.steps;
                // Every scale has steps, but one without would have nothing to lay out.
                if steps.is_empty() {
                    return grid(5, 1, &semitones);
                }
                let up = (steps.len() * 5 + 5) / 12;
                grid(up, 1, &|degree| {
                    let octave = (degree / steps.len()) as i16;
                    // In range: the remainder is less than the length.
                    let step = steps.get(degree % steps.len()).copied().unwrap_or_default();
                    i16::from(root) + i16::from(step) + 12 * octave
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
    pub piano: HashMap<char, i16>,
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
            (Alt(' '), PlayFromHere),
            (Ctrl('k'), Hush),
            (Char('`'), PlayFill),
            (Char('{'), KeyboardOctaveDown),
            (Char('}'), KeyboardOctaveUp),
            (Tab, ToggleView),
            (Char('R'), ToggleRecord),
            (Ctrl('c'), Quit),
        ];

        let goto = Menu::new(
            "goto",
            &[
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
        );

        let next = Menu::new(
            "next",
            &[
                (Char('b'), NextBeat),
                (Char('m'), NextBar),
                (Char('n'), NextNote),
                (Char('p'), NextPattern),
            ],
        );
        let previous = Menu::new(
            "previous",
            &[
                (Char('b'), PrevBeat),
                (Char('m'), PrevBar),
                (Char('n'), PrevNote),
                (Char('p'), PrevPattern),
            ],
        );

        // Not Helix's `t` (select till a character), which has no use on a grid.
        let track = Menu::new(
            "track",
            &[
                (Char('t'), SelectTracks),
                (Char('m'), ToggleMute),
                (Char('M'), TogglePatternMute),
                (Char('s'), ToggleSolo),
                (Char('a'), AddTrack),
                (Char('d'), RemoveTrack),
                (Char('y'), YankTracks),
                (Char('p'), PasteTracksAfter),
                (Char('P'), PasteTracksBefore),
                (Char('h'), PrevTrack),
                (Char('l'), NextTrack),
                (Char('H'), MoveTrackLeft),
                (Char('L'), MoveTrackRight),
                (Char('g'), ToggleGroup),
                (Char('z'), ToggleFold),
                (Char('e'), ToggleOpen),
            ],
        );

        let normal = common
            .into_iter()
            .chain([
                (Char('h'), Action::Left),
                (Char('l'), Action::Right),
                (Char('f'), GotoField),
                (Char('&'), Fill),
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
                (Alt(')'), RotateStepsForward),
                (Alt('('), RotateStepsBackward),
                (Char('~'), ReverseSteps),
                (Char('*'), RepeatSteps),
                (Char('s'), SelectNotes),
                (Char('K'), KeepNotes),
                (Char('L'), LockSetting),
                (Char('|'), LoopSelection),
                (Char('m'), ToggleMute),
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
            .chain([(Backspace, ClearAdvance), (Key::Delete, ClearAdvance)])
            .collect();

        Self {
            normal,
            insert,
            piano: NoteLayout::Piano.keys(&SCALES[0], 0),
            layout: NoteLayout::Piano,
        }
    }
}

impl Keymap {
    /// Binds `key` in normal and select mode, or within the menu `prefix` opens (making it one
    /// if it isn't), to `target`: an action's name, a `:command`, or `none` to unbind it.
    pub fn bind_normal(
        &mut self,
        prefix: Option<Key>,
        key: Key,
        target: &str,
    ) -> Result<(), String> {
        let binding = Binding::parse(target)?;
        let Some(prefix) = prefix else {
            match binding {
                Some(binding) => self.normal.insert(key, binding),
                None => self.normal.remove(&key),
            };
            return Ok(());
        };
        let menu = self.normal.entry(prefix).or_insert(Binding::Menu(Menu {
            title: prefix.to_string(),
            items: vec![],
        }));
        if !matches!(menu, Binding::Menu(_)) {
            *menu = Binding::Menu(Menu {
                title: prefix.to_string(),
                items: vec![],
            });
        }
        let Binding::Menu(menu) = menu else {
            return Ok(());
        };
        let at = menu.items.iter().position(|(k, _)| *k == key);
        match (binding, at) {
            (Some(binding), Some(i)) => {
                if let Some(item) = menu.items.get_mut(i) {
                    item.1 = binding;
                }
            }
            (Some(binding), None) => menu.items.push((key, binding)),
            (None, Some(i)) => {
                menu.items.remove(i);
            }
            (None, None) => {}
        }
        Ok(())
    }

    /// Binds `key` in insert mode to an action, or `none` to unbind it. Insert mode's other keys
    /// play notes, so commands aren't bound there.
    pub fn bind_insert(&mut self, key: Key, target: &str) -> Result<(), String> {
        match Binding::parse(target)? {
            Some(Binding::Action(action)) => {
                self.insert.insert(key, action);
            }
            None => {
                self.insert.remove(&key);
            }
            Some(_) => return Err("insert mode keys take actions, not commands".into()),
        }
        Ok(())
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
        assert!(!wicki.contains_key(&'{') && !fourths.contains_key(&'}'));
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

    #[test]
    fn every_bound_action_has_a_name() {
        let keymap = Keymap::default();
        let mut bound: Vec<Action> = keymap.insert.values().copied().collect();
        for binding in keymap.normal.values() {
            match binding {
                Binding::Action(action) => bound.push(*action),
                Binding::Menu(menu) => {
                    bound.extend(menu.items.iter().filter_map(|(_, b)| match b {
                        Binding::Action(action) => Some(*action),
                        _ => None,
                    }))
                }
                Binding::Command(_) => {}
            }
        }
        for action in bound {
            assert!(Action::ALL.contains(&action), "{action:?} missing from ALL");
            assert_eq!(Action::named(&action.name()), Some(action));
        }
        assert_eq!(Action::GotoMixer.name(), "goto_mixer");
        assert_eq!(
            Action::named("toggle_pattern_mute"),
            Some(Action::TogglePatternMute)
        );
    }

    #[test]
    fn sequences_name_keys_in_brackets() {
        assert_eq!(
            Key::sequence("g:a b<ret><C-a><lt><esc>"),
            Ok(vec![
                Key::Char('g'),
                Key::Char(':'),
                Key::Char('a'),
                Key::Char(' '),
                Key::Char('b'),
                Key::Enter,
                Key::Ctrl('a'),
                Key::Char('<'),
                Key::Esc,
            ])
        );
        assert!(Key::sequence("<nope>").is_err());
        assert!(Key::sequence("a<ret").is_err());
    }

    #[test]
    fn keys_parse_as_helix_writes_them() {
        let keys = [
            ("a", Key::Char('a')),
            ("C-a", Key::Ctrl('a')),
            ("C-space", Key::Ctrl(' ')),
            ("A-x", Key::Alt('x')),
            ("space", Key::Char(' ')),
            ("minus", Key::Char('-')),
            ("ret", Key::Enter),
            ("esc", Key::Esc),
            ("pageup", Key::PageUp),
            ("pad7", Key::Pad(7)),
        ];
        for (name, key) in keys {
            assert_eq!(Key::parse(name), Some(key), "{name}");
        }
        assert_eq!(Key::parse("hyper-x"), None);
        assert_eq!(Key::parse(""), None);
    }

    #[test]
    fn binds_and_unbinds() {
        let mut keymap = Keymap::default();
        keymap.bind_normal(None, Key::Ctrl('s'), ":w").unwrap();
        assert_eq!(keymap.normal[&Key::Ctrl('s')], Binding::Command("w".into()));
        keymap
            .bind_normal(Some(Key::Char('g')), Key::Char('e'), "none")
            .unwrap();
        let Binding::Menu(goto) = &keymap.normal[&Key::Char('g')] else {
            panic!()
        };
        assert!(goto.items.iter().all(|(k, _)| *k != Key::Char('e')));
        assert!(keymap.bind_normal(None, Key::Char('Q'), "fly").is_err());
        assert!(keymap.bind_insert(Key::Ctrl('s'), ":w").is_err());
    }
}
