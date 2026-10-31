//! The `:` command line.
//!
//! Each command's [`Spec`] says what values it takes, and lines are parsed against it in one
//! place, [`parse`]: as they're typed, to point out mistakes early, and again when they run.

use std::fmt::Debug;
use std::ops::Range;
use std::path::PathBuf;

use lattice_core::{
    Condition, MAX_DB, MAX_STEPS, MAX_TRACKS, MIN_DB, Mix, Rule, Step, Track, Transform,
};

use crate::{Editor, Effect};

/// What a command takes after its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Param {
    Value(Value),
    /// An optional `every N [OFFSET]` or `pass K`, always last.
    Condition,
}

/// A value a command takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Value {
    /// The placeholder shown until it's typed.
    pub name: &'static str,
    /// What messages call it.
    pub what: &'static str,
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A whole number from `min` to `max`, optionally signed. A `max` of [`ANY`] is no limit
    /// worth mentioning.
    Int { min: i64, max: i64 },
    /// A number from `min` to `max` that may have a fractional part, such as a tempo of 122.5.
    Decimal { min: i64, max: i64 },
    /// The rest of the line, as free text.
    Text,
    /// The rest of the line, which may be empty.
    OptionalText,
}

/// The largest number a condition takes.
const ANY: i64 = u32::MAX as i64;

/// How far a rule can rotate the pattern: a whole track at most, either way.
const ROTATION: i64 = MAX_STEPS as i64;

const fn int(name: &'static str, what: &'static str, min: i64, max: i64) -> Param {
    Param::Value(Value {
        name,
        what,
        kind: Kind::Int { min, max },
    })
}

const fn decimal(name: &'static str, what: &'static str, min: i64, max: i64) -> Param {
    Param::Value(Value {
        name,
        what,
        kind: Kind::Decimal { min, max },
    })
}

const fn optional_text(name: &'static str, what: &'static str) -> Param {
    Param::Value(Value {
        name,
        what,
        kind: Kind::OptionalText,
    })
}

const fn text(name: &'static str, what: &'static str) -> Param {
    Param::Value(Value {
        name,
        what,
        kind: Kind::Text,
    })
}

/// The values in a condition.
const EVERY: Value = Value {
    name: "N",
    what: "every",
    kind: Kind::Int { min: 1, max: ANY },
};
const OFFSET: Value = Value {
    name: "OFFSET",
    what: "offset",
    kind: Kind::Int { min: 0, max: ANY },
};
const PASS: Value = Value {
    name: "K",
    what: "pass",
    kind: Kind::Int { min: 0, max: ANY },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub name: &'static str,
    pub alias: Option<&'static str>,
    pub params: &'static [Param],
    /// What the command does. [`Spec::description`] adds the range of its value.
    pub describe: &'static str,
}

const CONDITION: &str = "[every N [OFFSET] | pass K]";
/// Steps in a track added with `:track-add`.
const NEW_TRACK_STEPS: usize = 16;

use Param::{Condition as When, Value as V};

/// Every `:` command, in the order the completion list shows them before anything is typed.
pub const COMMANDS: &[Spec] = &[
    Spec {
        name: "tr",
        alias: None,
        params: &[int("SEMITONES", "semitones", -128, 127), When],
        describe: "rule: transpose",
    },
    Spec {
        name: "rot",
        alias: None,
        params: &[int("STEPS", "steps", -ROTATION, ROTATION), When],
        describe: "rule: rotate the pattern",
    },
    Spec {
        name: "drift",
        alias: None,
        params: &[int("STEPS", "steps", -ROTATION, ROTATION), When],
        describe: "rule: rotate further each time it applies",
    },
    Spec {
        name: "rev",
        alias: None,
        params: &[When],
        describe: "rule: play backwards",
    },
    Spec {
        name: "pop",
        alias: None,
        params: &[],
        describe: "remove the track's last rule",
    },
    Spec {
        name: "clear",
        alias: None,
        params: &[],
        describe: "remove all the track's rules",
    },
    Spec {
        name: "len",
        alias: None,
        params: &[int("STEPS", "length", 1, MAX_STEPS as i64)],
        describe: "track length",
    },
    Spec {
        name: "name",
        alias: None,
        params: &[text("NAME", "name")],
        describe: "rename the track",
    },
    Spec {
        name: "level",
        alias: None,
        params: &[decimal("DB", "level", MIN_DB as i64, MAX_DB as i64)],
        describe: "mixer: level in dB, of the selected tracks or strip",
    },
    Spec {
        name: "pan",
        alias: None,
        params: &[int("PAN", "pan", -100, 100)],
        describe: "mixer: pan, left to right",
    },
    Spec {
        name: "send",
        alias: None,
        params: &[decimal("DB", "send", MIN_DB as i64, MAX_DB as i64)],
        describe: "mixer: send to the send bus, in dB",
    },
    Spec {
        name: "mute",
        alias: None,
        params: &[],
        describe: "mixer: mute or unmute",
    },
    Spec {
        name: "solo",
        alias: None,
        params: &[],
        describe: "mixer: solo or unsolo",
    },
    Spec {
        name: "set",
        alias: None,
        params: &[text("VALUE", "value")],
        describe: "mixer: set the value under the cursor",
    },
    Spec {
        name: "inst",
        alias: None,
        params: &[text("INSTRUMENT", "instrument")],
        describe: "choose the instrument: synth, kick, snare, hat, clap or tom",
    },
    Spec {
        name: "fx-add",
        alias: None,
        params: &[text("EFFECT", "effect")],
        describe: "add an effect (delay, drive, comp) after the one under the cursor",
    },
    Spec {
        name: "track-add",
        alias: Some("ta"),
        params: &[optional_text("[NAME]", "name")],
        describe: "add a track after this one",
    },
    Spec {
        name: "track-remove",
        alias: Some("trm"),
        params: &[],
        describe: "remove this track",
    },
    Spec {
        name: "track-move",
        alias: Some("tmv"),
        params: &[int(
            "±TRACKS",
            "tracks to move",
            -(MAX_TRACKS as i64),
            MAX_TRACKS as i64,
        )],
        describe: "move this track left or right",
    },
    Spec {
        name: "group-name",
        alias: None,
        params: &[text("NAME", "name")],
        describe: "rename this track's group",
    },
    Spec {
        name: "bpm",
        alias: None,
        params: &[decimal("BPM", "bpm", 20, 400)],
        describe: "tempo",
    },
    Spec {
        name: "step",
        alias: None,
        params: &[int("STEPS", "auto-step", 0, 64)],
        describe: "steps to move after each note",
    },
    Spec {
        name: "oct",
        alias: None,
        params: &[int("OCTAVE", "octave", 0, 9)],
        describe: "keyboard octave",
    },
    Spec {
        name: "click",
        alias: None,
        params: &[],
        describe: "metronome on or off",
    },
    Spec {
        name: "follow",
        alias: None,
        params: &[],
        describe: "follow the playhead on or off",
    },
    Spec {
        name: "help",
        alias: Some("h"),
        params: &[],
        describe: "list the commands",
    },
    Spec {
        name: "write",
        alias: Some("w"),
        params: &[optional_text("[PATH]", "path")],
        describe: "save the song, optionally to a new file",
    },
    Spec {
        name: "open",
        alias: Some("o"),
        params: &[text("PATH", "path")],
        describe: "open a song, or start a new one there",
    },
    Spec {
        name: "open!",
        alias: Some("o!"),
        params: &[text("PATH", "path")],
        describe: "open a song, discarding unsaved changes",
    },
    Spec {
        name: "write-quit",
        alias: Some("wq"),
        params: &[optional_text("[PATH]", "path")],
        describe: "save the song and quit",
    },
    Spec {
        name: "quit",
        alias: Some("q"),
        params: &[],
        describe: "quit, unless there are unsaved changes",
    },
    Spec {
        name: "quit!",
        alias: Some("q!"),
        params: &[],
        describe: "quit, discarding unsaved changes",
    },
];

pub fn find(name: &str) -> Option<&'static Spec> {
    COMMANDS
        .iter()
        .find(|c| c.name == name || c.alias == Some(name))
}

impl Spec {
    /// The parameters as placeholders: `SEMITONES [every N [OFFSET] | pass K]`.
    pub fn signature(&self) -> String {
        let words: Vec<&str> = self
            .params
            .iter()
            .map(|p| match p {
                V(value) => value.name,
                When => CONDITION,
            })
            .collect();
        words.join(" ")
    }

    /// What the command does, with the range of its value if that's limited: `tempo, 20 to 400`.
    pub fn description(&self) -> String {
        let range = self.params.iter().find_map(|p| match p {
            V(Value {
                kind: Kind::Int { min, max } | Kind::Decimal { min, max },
                ..
            }) if *max != ANY => Some(format!(", {min} to {max}")),
            _ => None,
        });
        format!("{}{}", self.describe, range.unwrap_or_default())
    }
}

impl Value {
    /// What's wanted, for when it's missing or wrong: `bpm is 20 to 400`.
    pub fn expected(&self) -> String {
        let what = self.what;
        match self.kind {
            Kind::Int { min: 0, max: ANY } => format!("{what} is a whole number"),
            Kind::Int { min, max: ANY } => format!("{what} is {min} or more"),
            Kind::Int { min, max } | Kind::Decimal { min, max } => {
                format!("{what} is {min} to {max}")
            }
            Kind::Text | Kind::OptionalText => format!("expected a {what}"),
        }
    }

    /// The number in `word`, if it is one and in range.
    fn int(&self, word: &str) -> Option<i64> {
        let Kind::Int { min, max } = self.kind else {
            return None;
        };
        let digits = word.strip_prefix(['+', '-']).unwrap_or(word);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        word.trim_start_matches('+')
            .parse()
            .ok()
            .filter(|n| (min..=max).contains(n))
    }

    /// The number in `word`, if it is one and in range, fractional part and all.
    fn decimal(&self, word: &str) -> Option<f64> {
        let Kind::Decimal { min, max } = self.kind else {
            return None;
        };
        let unsigned = word.strip_prefix(['+', '-']).unwrap_or(word);
        let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
        let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        if whole.is_empty() || !digits(whole) || !digits(fraction) {
            return None;
        }
        word.trim_start_matches('+')
            .parse()
            .ok()
            .filter(|n| (min as f64..=max as f64).contains(n))
    }

    /// Whether typing more could make `word` valid: a lone `-`, `4` on the way to `40`, or
    /// `122.` on the way to `122.5`.
    fn could_become(&self, word: &str) -> bool {
        let (min, max) = match self.kind {
            Kind::Int { min, max } => (min, max),
            Kind::Decimal { min, max } => {
                let (whole, fraction) = word.split_once('.').unwrap_or((word, ""));
                if !fraction.bytes().all(|b| b.is_ascii_digit()) {
                    return false;
                }
                let as_int = Value {
                    kind: Kind::Int { min, max },
                    ..*self
                };
                return as_int.int(whole).is_some() || as_int.could_become(whole);
            }
            Kind::Text | Kind::OptionalText => return true,
        };
        let (negative, digits) = match word.strip_prefix('-') {
            Some(digits) => (true, digits),
            None => (false, word.strip_prefix('+').unwrap_or(word)),
        };
        if !digits.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        let typed: i128 = digits.parse().unwrap_or(0);
        let (min, max) = (i128::from(min), i128::from(max));
        let limit = min.abs().max(max.abs());
        // With `extra` more digits, the number lies somewhere in `low..=high`.
        (0..20)
            .skip(usize::from(digits.is_empty()))
            .map(|extra| 10_i128.pow(extra))
            .take_while(|scale| typed * scale <= limit)
            .any(|scale| {
                let (low, high) = (typed * scale, typed * scale + scale - 1);
                let (low, high) = if negative { (-high, -low) } else { (low, high) };
                low <= max && high >= min
            })
    }
}

/// A command matching what's been typed, with the positions of the matched characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub spec: &'static Spec,
    pub matched: Vec<usize>,
}

/// Commands that fuzzy-match `query`, best first.
pub fn complete(query: &str) -> Vec<Completion> {
    let mut scored: Vec<(i32, usize, Completion)> = COMMANDS
        .iter()
        .enumerate()
        .filter_map(|(order, spec)| {
            let (mut score, matched) = fuzzy(query, spec.name)?;
            if spec.alias == Some(query) {
                score += 100;
            }
            Some((score, order, Completion { spec, matched }))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, c)| c).collect()
}

/// Whether `query`'s characters appear in order in `name`, and how well: matches at the start
/// and runs of consecutive characters score higher, and shorter names win ties.
fn fuzzy(query: &str, name: &str) -> Option<(i32, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, vec![]));
    }
    let mut matched = Vec::new();
    let mut score = 0;
    let mut chars = name.char_indices();
    for q in query.chars() {
        let (i, _) = chars.find(|&(_, c)| c.eq_ignore_ascii_case(&q))?;
        score += match matched.last() {
            _ if i == 0 => 10,
            Some(&last) if last + 1 == i => 5,
            _ => 1,
        };
        matched.push(i);
    }
    Some((score * 10 - name.len() as i32, matched))
}

/// Faint placeholders for what's still to type after the command name, shown after the cursor.
/// `None` while the name itself is being typed.
pub fn hint(line: &str) -> Option<String> {
    let (name, rest) = line.split_once(' ')?;
    let spec = find(name)?;
    let words: Vec<&str> = rest.split_whitespace().collect();
    // The word under the cursor, if one is half-typed.
    let (complete, partial) = if rest.is_empty() || rest.ends_with(' ') {
        (&words[..], None)
    } else {
        (&words[..words.len() - 1], words.last().copied())
    };

    let values: Vec<&str> = spec
        .params
        .iter()
        .filter_map(|p| match p {
            V(value) => Some(value.name),
            When => None,
        })
        .collect();
    let has_condition = spec.params.contains(&When);

    let mut todo: Vec<String> = Vec::new();
    let mut suffix = String::new();
    if complete.len() < values.len() {
        todo.extend(values[complete.len()..].iter().map(|s| s.to_string()));
        if has_condition {
            todo.push(CONDITION.into());
        }
        if partial.is_some() {
            todo.remove(0);
        }
    } else if has_condition {
        match (&complete[values.len()..], partial) {
            ([], None) => todo.push(CONDITION.into()),
            ([], Some(p)) => {
                if let Some(rest) = "every".strip_prefix(p) {
                    suffix = rest.into();
                    todo.extend(["N".into(), "[OFFSET]".into()]);
                } else if let Some(rest) = "pass".strip_prefix(p) {
                    suffix = rest.into();
                    todo.push("K".into());
                }
            }
            (["every"], None) => todo.extend(["N".into(), "[OFFSET]".into()]),
            (["every"], Some(_)) | (["every", _], None) => todo.push("[OFFSET]".into()),
            (["pass"], None) => todo.push("K".into()),
            _ => {}
        }
    }

    let mut hint = suffix;
    for word in todo {
        // Separate from a half-typed word, but not after a space the user typed.
        if !hint.is_empty() || partial.is_some() {
            hint.push(' ');
        }
        hint.push_str(&word);
    }
    (!hint.is_empty()).then_some(hint)
}

/// Something wrong on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The offending characters' positions in the line. Empty, at the end, for something missing.
    pub range: Range<usize>,
    pub message: String,
}

impl Problem {
    fn new(range: Range<usize>, message: impl Into<String>) -> Self {
        Self {
            range,
            message: message.into(),
        }
    }
}

/// What's wrong with `line` so far, as it's typed. Words that typing more could make valid,
/// and values not typed yet, aren't problems until enter.
pub fn problem(line: &str) -> Option<Problem> {
    parse(line, true).err()
}

/// A line parsed against its command's [`Spec`], ready to run.
struct Command {
    spec: &'static Spec,
    /// One per [`Param::Value`], in order.
    args: Vec<Arg>,
    when: Condition,
}

enum Arg {
    Int(i64),
    Decimal(f64),
    Text(String),
}

impl Command {
    /// Value `i` as a number, already checked against its range.
    fn int<T: TryFrom<i64>>(&self, i: usize) -> T
    where
        T::Error: Debug,
    {
        match &self.args[i] {
            Arg::Int(n) => T::try_from(*n).expect("checked against its range"),
            _ => unreachable!("value {i} of {} isn't a whole number", self.spec.name),
        }
    }

    fn decimal(&self, i: usize) -> f64 {
        match &self.args[i] {
            Arg::Decimal(n) => *n,
            _ => unreachable!("value {i} of {} isn't a decimal", self.spec.name),
        }
    }

    fn text(&self, i: usize) -> &str {
        match &self.args[i] {
            Arg::Text(text) => text,
            _ => unreachable!("value {i} of {} is a number", self.spec.name),
        }
    }
}

/// Parses `line` against its command's parameters. While `live` the line is still being typed:
/// `Ok(None)` means nothing is wrong so far. Otherwise it's only `Ok(None)` if the line is empty.
fn parse(line: &str, live: bool) -> Result<Option<Command>, Problem> {
    let mut words = Words::new(line, live);
    let Some((name, typing)) = words.next() else {
        return Ok(None);
    };
    let spec = match find(name.text) {
        Some(spec) => spec,
        None if typing && !complete(name.text).is_empty() => return Ok(None),
        None => return Err(Problem::new(name.range, "unknown command")),
    };
    let mut args = Vec::new();
    let mut when = Condition::Always;
    for param in spec.params {
        match param {
            V(value) => match words.value(value)? {
                Some(arg) => args.push(arg),
                None => return Ok(None),
            },
            When => match words.condition()? {
                Some(condition) => when = condition,
                None => return Ok(None),
            },
        }
    }
    if let Some((extra, _)) = words.next() {
        return Err(Problem::new(extra.range, "too many arguments"));
    }
    Ok(Some(Command { spec, args, when }))
}

#[derive(Debug, Clone)]
struct Word<'a> {
    text: &'a str,
    /// Character positions in the line.
    range: Range<usize>,
}

/// The words of a line, read off one value at a time. While `live`, a missing value or a last
/// word that could still become valid gives `Ok(None)`, to stop checking without a problem.
struct Words<'a> {
    words: Vec<Word<'a>>,
    next: usize,
    live: bool,
    /// Whether the line ends in a space, so its last word is finished.
    finished: bool,
    /// The line's length in characters.
    end: usize,
}

impl<'a> Words<'a> {
    fn new(line: &'a str, live: bool) -> Self {
        let mut words = Vec::new();
        let mut start = None;
        let end = line.chars().count();
        // A space past the end finishes the last word.
        let chars = line.char_indices().chain([(line.len(), ' ')]).enumerate();
        for (i, (byte, c)) in chars {
            match (start, c.is_whitespace()) {
                (None, false) => start = Some((i, byte)),
                (Some((from, from_byte)), true) => {
                    words.push(Word {
                        text: &line[from_byte..byte],
                        range: from..i,
                    });
                    start = None;
                }
                _ => {}
            }
        }
        Self {
            words,
            next: 0,
            live,
            finished: line.ends_with(char::is_whitespace),
            end,
        }
    }

    /// The next word, and whether it's still being typed.
    fn next(&mut self) -> Option<(Word<'a>, bool)> {
        let word = self.words.get(self.next)?.clone();
        self.next += 1;
        let typing = self.live && !self.finished && self.next == self.words.len();
        Some((word, typing))
    }

    fn missing<T>(&self, value: &Value) -> Result<Option<T>, Problem> {
        if self.live {
            Ok(None)
        } else {
            Err(Problem::new(self.end..self.end, value.expected()))
        }
    }

    fn value(&mut self, value: &Value) -> Result<Option<Arg>, Problem> {
        Ok(match value.kind {
            Kind::Int { .. } => self.int(value)?.map(Arg::Int),
            Kind::Decimal { .. } => self.decimal(value)?.map(Arg::Decimal),
            Kind::Text | Kind::OptionalText => self.text(value)?.map(Arg::Text),
        })
    }

    fn int(&mut self, value: &Value) -> Result<Option<i64>, Problem> {
        let Some((word, typing)) = self.next() else {
            return self.missing(value);
        };
        match value.int(word.text) {
            Some(n) => Ok(Some(n)),
            None if typing && value.could_become(word.text) => Ok(None),
            None => Err(Problem::new(word.range, value.expected())),
        }
    }

    fn decimal(&mut self, value: &Value) -> Result<Option<f64>, Problem> {
        let Some((word, typing)) = self.next() else {
            return self.missing(value);
        };
        match value.decimal(word.text) {
            Some(n) => Ok(Some(n)),
            None if typing && value.could_become(word.text) => Ok(None),
            None => Err(Problem::new(word.range, value.expected())),
        }
    }

    /// The rest of the line, with single spaces between words.
    fn text(&mut self, value: &Value) -> Result<Option<String>, Problem> {
        let rest: Vec<&str> = std::iter::from_fn(|| self.next())
            .map(|(word, _)| word.text)
            .collect();
        if rest.is_empty() && value.kind != Kind::OptionalText {
            return self.missing(value);
        }
        Ok(Some(rest.join(" ")))
    }

    /// An optional `every N [OFFSET]` or `pass K`.
    fn condition(&mut self) -> Result<Option<Condition>, Problem> {
        let whole = |n: i64| u32::try_from(n).expect("checked against its range");
        let Some((word, typing)) = self.next() else {
            return Ok(Some(Condition::Always));
        };
        match word.text {
            "every" => {
                let Some(n) = self.int(&EVERY)? else {
                    return Ok(None);
                };
                let offset = if self.next < self.words.len() {
                    match self.int(&OFFSET)? {
                        Some(offset) => offset,
                        None => return Ok(None),
                    }
                } else {
                    0
                };
                Ok(Some(Condition::Every {
                    n: whole(n),
                    offset: whole(offset),
                }))
            }
            "pass" => Ok(self.int(&PASS)?.map(|k| Condition::Pass(whole(k)))),
            partial if typing && ["every", "pass"].iter().any(|k| k.starts_with(partial)) => {
                Ok(None)
            }
            _ => Err(Problem::new(word.range, "expected every or pass")),
        }
    }
}

const UNSAVED_QUIT: &str = "unsaved changes: :w to save them, or :q! to quit anyway";
const UNSAVED_OPEN: &str = "unsaved changes: :w to save them, or :o! to open anyway";

/// The path typed, or else the song's own file.
fn save_path(editor: &Editor, typed: &str) -> Result<PathBuf, String> {
    if typed.is_empty() {
        editor
            .path
            .clone()
            .ok_or_else(|| "no file name yet: :w PATH".into())
    } else {
        Ok(typed.into())
    }
}

/// Changes the mixer settings of every track the selections reach, as one undo step.
fn mix(editor: &mut Editor, change: impl Fn(&mut Mix)) -> Result<Vec<Effect>, String> {
    let tracks = editor.mix_targets()?;
    editor.checkpoint();
    for &t in &tracks {
        change(&mut editor.grid.tracks[t].mix);
    }
    Ok(tracks.into_iter().map(Effect::TrackChanged).collect())
}

pub(crate) fn run(editor: &mut Editor, line: &str) -> Result<Vec<Effect>, String> {
    let Some(command) = parse(line, false).map_err(|problem| problem.message)? else {
        return Ok(vec![]);
    };
    let track = editor.cursor().track;

    match command.spec.name {
        "quit" if editor.modified() => Err(UNSAVED_QUIT.into()),
        "quit" | "quit!" => Ok(vec![Effect::Quit]),
        "write" => Ok(vec![Effect::Save(save_path(editor, command.text(0))?)]),
        "write-quit" => Ok(vec![
            Effect::Save(save_path(editor, command.text(0))?),
            Effect::Quit,
        ]),
        "open" if editor.modified() => Err(UNSAVED_OPEN.into()),
        "open" | "open!" => Ok(vec![Effect::Open(command.text(0).into())]),
        "help" => {
            let names: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
            Err(names.join(" · "))
        }
        "bpm" => {
            let bpm = command.decimal(0);
            editor.bpm = bpm;
            Ok(vec![Effect::Bpm(bpm)])
        }
        "len" => {
            let len: usize = command.int(0);
            editor.checkpoint();
            editor.grid.tracks[track].steps.resize(len, Step::default());
            editor.clamp_selection();
            Ok(vec![Effect::TrackChanged(track)])
        }
        "step" => {
            editor.auto_step = command.int(0);
            Ok(vec![])
        }
        "oct" => {
            editor.octave = command.int(0);
            Ok(vec![])
        }
        "follow" => {
            editor.follow = !editor.follow;
            editor.message = Some(format!(
                "follow {}",
                if editor.follow { "on" } else { "off" }
            ));
            Ok(vec![])
        }
        "click" => {
            editor.click = !editor.click;
            Ok(vec![Effect::Click(editor.click)])
        }
        "name" => {
            editor.grid.tracks[track].name = command.text(0).into();
            Ok(vec![])
        }
        "level" => {
            let level = command.decimal(0) as f32;
            match editor.bus_under_cursor() {
                Some(bus) => Ok(editor.set_bus_level(bus, level)),
                None => mix(editor, |mix| mix.level = level),
            }
        }
        "pan" => {
            let pan = command.int::<i32>(0) as f32 / 100.0;
            mix(editor, |mix| mix.pan = pan)
        }
        "send" => {
            let send = command.decimal(0) as f32;
            mix(editor, |mix| mix.send = send)
        }
        "mute" | "solo" => {
            let solo = command.spec.name == "solo";
            let flag = |mix: Mix| if solo { mix.solo } else { mix.mute };
            // Toggled together: if any selected track is off, they all go on.
            let on = (editor.mix_targets()?.iter()).any(|&t| !flag(editor.grid.tracks[t].mix));
            mix(editor, |mix| {
                if solo {
                    mix.solo = on;
                } else {
                    mix.mute = on;
                }
            })
        }
        "set" => {
            let word = command.text(0);
            let value: f32 = word
                .parse()
                .ok()
                .filter(|v: &f32| v.is_finite())
                .ok_or_else(|| format!("{word:?} isn't a number"))?;
            editor.set_mixer_value(value)
        }
        "fx-add" => editor.add_effect(command.text(0)),
        "inst" => editor.set_instrument(command.text(0)),
        "group-name" => editor.rename_group(command.text(0)),
        "track-add" => {
            let tracks = &editor.grid.tracks;
            if tracks.len() >= MAX_TRACKS {
                return Err(format!("at most {MAX_TRACKS} tracks"));
            }
            let mut name = command.text(0).to_string();
            if name.is_empty() {
                // The first `track N` not already taken.
                name = (tracks.len() + 1..)
                    .map(|n| format!("track {n}"))
                    .find(|name| tracks.iter().all(|t| t.name != *name))
                    .unwrap();
            }
            editor.checkpoint();
            let track = Track {
                name,
                steps: vec![Step::default(); NEW_TRACK_STEPS],
                mix: Default::default(),
                instrument: Default::default(),
                effects: vec![],
                rules: vec![],
            };
            editor.insert_track(editor.cursor().track + 1, track);
            editor.select_point(editor.cursor().track + 1, editor.cursor().step);
            Ok(vec![Effect::TracksChanged])
        }
        "track-remove" => {
            if editor.grid.tracks.len() == 1 {
                return Err("can't remove the only track".into());
            }
            editor.checkpoint();
            editor.remove_track(track);
            editor.select_point(track, editor.cursor().step);
            Ok(vec![Effect::TracksChanged])
        }
        "track-move" => {
            let by: isize = command.int(0);
            let to = track
                .saturating_add_signed(by)
                .min(editor.grid.tracks.len() - 1);
            if to == track {
                return Ok(vec![]);
            }
            editor.checkpoint();
            editor.move_track(track, to);
            editor.select_point(to, editor.cursor().step);
            Ok(vec![Effect::TracksChanged])
        }
        name @ ("rot" | "drift" | "rev" | "tr") => {
            let transform = match name {
                "rot" => Transform::Rotate(command.int(0)),
                "drift" => Transform::Drift(command.int(0)),
                "tr" => Transform::Transpose(command.int(0)),
                _ => Transform::Reverse,
            };
            editor.checkpoint();
            editor.grid.tracks[track].rules.push(Rule {
                transform,
                when: command.when,
            });
            Ok(vec![Effect::TrackChanged(track)])
        }
        "pop" => {
            editor.checkpoint();
            editor.grid.tracks[track]
                .rules
                .pop()
                .ok_or("no rules to remove")?;
            Ok(vec![Effect::TrackChanged(track)])
        }
        "clear" => {
            editor.checkpoint();
            editor.grid.tracks[track].rules.clear();
            Ok(vec![Effect::TrackChanged(track)])
        }
        _ => unreachable!("every command in COMMANDS is handled"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(query: &str) -> Vec<&'static str> {
        complete(query).iter().map(|c| c.spec.name).collect()
    }

    #[test]
    fn fuzzy_completion() {
        assert_eq!(names("").len(), COMMANDS.len());
        assert_eq!(names("")[..2], ["tr", "rot"], "table order before typing");
        assert_eq!(names("tr")[0], "tr");
        assert_eq!(names("q")[..2], ["quit", "quit!"]);
        assert_eq!(names("wq")[0], "write-quit", "aliases match exactly");
        assert_eq!(names("dr"), ["drift"]);
        assert!(names("fl").starts_with(&["follow"]));
        assert_eq!(names("zz"), Vec::<&str>::new());
        assert_eq!(complete("rv")[0].spec.name, "rev");
        assert_eq!(complete("rv")[0].matched, [0, 2]);
    }

    #[test]
    fn hints_shrink_as_you_type() {
        assert_eq!(hint("tr"), None, "still typing the name");
        assert_eq!(
            hint("tr ").as_deref(),
            Some("SEMITONES [every N [OFFSET] | pass K]")
        );
        assert_eq!(
            hint("tr 5").as_deref(),
            Some(" [every N [OFFSET] | pass K]")
        );
        assert_eq!(
            hint("tr 5 ").as_deref(),
            Some("[every N [OFFSET] | pass K]")
        );
        assert_eq!(hint("tr 5 ev").as_deref(), Some("ery N [OFFSET]"));
        assert_eq!(hint("tr 5 every ").as_deref(), Some("N [OFFSET]"));
        assert_eq!(hint("tr 5 every 2").as_deref(), Some(" [OFFSET]"));
        assert_eq!(hint("tr 5 every 2 1"), None);
        assert_eq!(hint("rev p").as_deref(), Some("ass K"));
        assert_eq!(hint("bpm ").as_deref(), Some("BPM"));
        assert_eq!(hint("bpm 9"), None);
        assert_eq!(hint("pop "), None);
        assert_eq!(hint("w ").as_deref(), Some("[PATH]"));
        assert_eq!(hint("ta ").as_deref(), Some("[NAME]"));
        assert_eq!(hint("nope "), None);
    }

    /// The problem in `line` as it's typed, as the offending text and the message.
    fn flagged(line: &str) -> Option<(String, String)> {
        problem(line).map(|p| {
            let word = line
                .chars()
                .skip(p.range.start)
                .take(p.range.len())
                .collect();
            (word, p.message)
        })
    }

    fn flag(word: &str, message: &str) -> Option<(String, String)> {
        Some((word.into(), message.into()))
    }

    #[test]
    fn partial_words_that_could_become_valid_are_not_flagged() {
        for line in [
            "",
            "fo",
            "rv",
            "bpm",
            "bpm ",
            "bpm 4",
            "bpm 40",
            "bpm +",
            "tr -",
            "tr -12",
            "tr +5",
            "tr 5 ev",
            "tr 5 every",
            "tr 5 every ",
            "tr 5 every 0",
            "rev every 2 1",
            "rev p",
            "len 25",
            "name",
            "name My  lead",
            "oct 9 ",
        ] {
            assert_eq!(flagged(line), None, "{line:?}");
        }
    }

    #[test]
    fn mistakes_are_flagged_once_they_cannot_become_valid() {
        assert_eq!(flagged("bpm 1000"), flag("1000", "bpm is 20 to 400"));
        assert_eq!(flagged("bpm 4 "), flag("4", "bpm is 20 to 400"));
        assert_eq!(flagged("bpm -"), flag("-", "bpm is 20 to 400"));
        assert_eq!(flagged("bpm 1x"), flag("1x", "bpm is 20 to 400"));
        assert_eq!(flagged("oct 10"), flag("10", "octave is 0 to 9"));
        assert_eq!(flagged("tr x"), flag("x", "semitones is -128 to 127"));
        assert_eq!(flagged("tr -129"), flag("-129", "semitones is -128 to 127"));
        assert_eq!(flagged("zz"), flag("zz", "unknown command"));
        assert_eq!(flagged("fo "), flag("fo", "unknown command"));
        assert_eq!(flagged("tr 5 x"), flag("x", "expected every or pass"));
        assert_eq!(flagged("tr 5 every 0 "), flag("0", "every is 1 or more"));
        assert_eq!(flagged("rev pass x"), flag("x", "pass is a whole number"));
        assert_eq!(flagged("pop now"), flag("now", "too many arguments"));
        assert_eq!(flagged("bpm 120 5"), flag("5", "too many arguments"));
        assert_eq!(flagged("rev every 2 1 3"), flag("3", "too many arguments"));
        assert_eq!(
            problem(" bpm  5000").unwrap().range,
            6..10,
            "ranges count characters in the line"
        );
    }

    #[test]
    fn missing_values_are_problems_on_enter() {
        let problem = |line| parse(line, false).err();
        assert_eq!(problem("bpm"), Some(Problem::new(3..3, "bpm is 20 to 400")));
        assert_eq!(problem("bpm 4").unwrap().message, "bpm is 20 to 400");
        assert_eq!(
            problem("tr 5 ev").unwrap().message,
            "expected every or pass"
        );
        assert_eq!(problem("tr 5 every").unwrap().message, "every is 1 or more");
        assert_eq!(problem("name ").unwrap().message, "expected a name");
        assert_eq!(problem("fo").unwrap().message, "unknown command");
        assert_eq!(problem("rev every 2 1"), None);
        assert_eq!(problem(""), None);
    }

    #[test]
    fn descriptions_show_ranges() {
        assert_eq!(find("bpm").unwrap().description(), "tempo, 20 to 400");
    }

    #[test]
    fn tempo_can_be_fractional() {
        assert_eq!(problem("bpm 122.5"), None);
        assert_eq!(problem("bpm 122."), None, "on the way to 122.5");
        assert_eq!(
            problem("bpm 122.x"),
            Some(Problem::new(4..9, "bpm is 20 to 400"))
        );
        assert_eq!(
            problem("bpm 500.5 "),
            Some(Problem::new(4..9, "bpm is 20 to 400"))
        );
        assert_eq!(find("len").unwrap().description(), "track length, 1 to 256");
        assert_eq!(find("name").unwrap().description(), "rename the track");
        assert_eq!(find("rev").unwrap().description(), "rule: play backwards");
    }
}
