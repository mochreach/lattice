//! The `:` command line.
//!
//! Each command's [`Spec`] says what values it takes, and lines are parsed against it in one
//! place, [`parse`]: as they're typed, to point out mistakes early, and again when they run.

use std::fmt::Debug;
use std::ops::Range;
use std::path::PathBuf;

use lattice_core::{
    Condition, GESTURES, Looping, MAX_CHOKE, MAX_DB, MAX_STEPS, MAX_TRACKS, MIN_DB, MIN_THRESHOLD,
    Mix, Rule, Step, Track, Transform,
};

use crate::{Direction, Editor, Effect, NoteLayout};

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
pub(crate) const NEW_TRACK_STEPS: usize = 16;

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
        describe: "track length, or the conductor's in it",
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
        name: "choke",
        alias: None,
        params: &[int("N", "choke group", 0, MAX_CHOKE as i64)],
        describe: "choke group: tracks in one track group with the same number cut each other off (0 for none)",
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
        name: "lock",
        alias: None,
        params: &[text("PARAM VALUE", "setting and value")],
        describe: "lock a setting on the selected steps; FROM..TO sweeps, ~V slides, -- puts back; alone, adds its column",
    },
    Spec {
        name: "unlock",
        alias: None,
        params: &[optional_text("[PARAM]", "setting")],
        describe: "clear a setting's locks from the selected steps, or all; along a track, its column too",
    },
    Spec {
        name: "macro",
        alias: None,
        params: &[text("NAME", "name")],
        describe: "add a macro to the track, a knob that turns several of its settings",
    },
    Spec {
        name: "map",
        alias: None,
        params: &[text("NAME [DEPTH]", "macro and depth")],
        describe: "have a macro turn the setting under the cursor, by DEPTH (-1 to 1, 0.5 if left out)",
    },
    Spec {
        name: "unmap",
        alias: None,
        params: &[text("NAME", "macro")],
        describe: "stop a macro turning the setting under the cursor",
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
        describe: "choose the instrument: synth, switched, monochord, krell, tuss, sampler, chop, kick, snare, hat, clap, tom, or midi for gear outside Lattice",
    },
    Spec {
        name: "sample",
        alias: None,
        params: &[optional_text("[PATH]", "path")],
        describe: "play a WAV file on a sampler; with no path, the last selection exported",
    },
    Spec {
        name: "chop",
        alias: None,
        params: &[optional_text("[PATH]", "path")],
        describe: "cut a WAV file into a piece a step, each step playing its own; with no path, the last selection exported",
    },
    Spec {
        name: "fx-add",
        alias: None,
        params: &[text("EFFECT", "effect")],
        describe: "add an effect (delay, reverb, filter, ensemble, phaser, drive, comp, pitch) after the one under the cursor",
    },
    Spec {
        name: "fx-send",
        alias: None,
        params: &[text("LANE [DB]", "lane and level")],
        describe: "fx page: send into a lane after the cursor (the next letter adds one); into this lane or one to its left, it's feedback",
    },
    Spec {
        name: "chain-save",
        alias: None,
        params: &[text("NAME", "name")],
        describe: "save the strip's chain of effects, to load on any strip of any song",
    },
    Spec {
        name: "chain-load",
        alias: None,
        params: &[text("NAME", "name")],
        describe: "put a saved chain on the strip, in place of its own; Lattice comes with shimmer",
    },
    Spec {
        name: "preset",
        alias: None,
        params: &[text("NAME", "name")],
        describe: "put a preset on the track: its instrument, effects, macros and face, in place of the track's own; Lattice's are in their engine's folder, as tuss/flim",
    },
    Spec {
        name: "preset-save",
        alias: None,
        params: &[text("NAME", "name")],
        describe: "save the track's instrument, effects and macros as a preset, giving it a face if it has none; mine/NAME saves it in a folder",
    },
    Spec {
        name: "face",
        alias: None,
        params: &[text("TITLE", "title")],
        describe: "title the track's face, as the instrument page shows it",
    },
    Spec {
        name: "face-art",
        alias: None,
        params: &[optional_text("[PATH]", "path")],
        describe: "draw the track's face from a text file, up to 8 frames of 8 rows of 31 characters, each after an empty line; none takes the art away",
    },
    Spec {
        name: "face-show",
        alias: None,
        params: &[optional_text("[SETTING]", "setting")],
        describe: "put one of the instrument's settings on the track's face, beside its macros; the one under the cursor if none's named",
    },
    Spec {
        name: "face-hide",
        alias: None,
        params: &[optional_text("[SETTING]", "setting")],
        describe: "take a setting off the track's face",
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
        describe: "steps to move after each note or paste; C-0 to C-9 set it too",
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
        name: "rec",
        alias: None,
        params: &[],
        describe: "record on or off: notes played while the song plays go in at the playhead",
    },
    Spec {
        name: "arm",
        alias: None,
        params: &[optional_text("[PASSES]", "number of passes")],
        describe: "arm this track to record the audio input a pass at a time, for PASSES passes or until stopped",
    },
    Spec {
        name: "disarm",
        alias: None,
        params: &[],
        describe: "disarm the track armed to record the audio input",
    },
    Spec {
        name: "arm!",
        alias: None,
        params: &[optional_text("[PASSES]", "number of passes")],
        describe: "arm this track to record a new loop over the one it plays",
    },
    Spec {
        name: "midi",
        alias: None,
        params: &[],
        describe: "the MIDI keyboards and controllers Lattice is listening to",
    },
    Spec {
        name: "port",
        alias: None,
        params: &[optional_text("[PORT]", "port's number or name, or none")],
        describe: "play the track on MIDI gear, by its port's number or any part of its name, or none for Lattice's own port; alone, lists the ports",
    },
    Spec {
        name: "clock",
        alias: None,
        params: &[optional_text(
            "[PORT]",
            "port's number or name, lattice, or none",
        )],
        describe: "send MIDI clock, start and stop to a port, or stop sending it there; none stops it everywhere, and alone, says where it goes",
    },
    Spec {
        name: "cc",
        alias: None,
        params: &[
            int("N", "controller", 0, 127),
            optional_text("[VALUE]", "0 to 127, or none"),
        ],
        describe: "send MIDI controller N at a value from 0 to 127 on the track, or none to stop sending it",
    },
    Spec {
        name: "midi-learn",
        alias: None,
        params: &[optional_text(
            "[TARGET]",
            "action, :command, macro N, this or none",
        )],
        describe: "bind the next MIDI control moved to an action, a :command, macro N, this setting (in this song), or none to unbind it; alone, stops",
    },
    Spec {
        name: "input",
        alias: None,
        params: &[optional_text("DEVICE", "device")],
        describe: "the device the looper records, or an app playing: a number from the list, or part of a name",
    },
    Spec {
        name: "output",
        alias: None,
        params: &[optional_text("DEVICE", "device")],
        describe: "the device you hear Lattice through: a number from the list, or part of a name",
    },
    Spec {
        name: "latency",
        alias: None,
        params: &[int("MS", "milliseconds", -500, 500)],
        describe: "milliseconds to add to the latency recording allows for (or take off)",
    },
    Spec {
        name: "count",
        alias: None,
        params: &[int("BARS", "bars", 0, 4)],
        describe: "bars counted in before recording (0 for none)",
    },
    Spec {
        name: "threshold",
        alias: None,
        params: &[optional_text("[DB]", "level")],
        describe: "this loop track waits for the input to reach this level, -60 to 0 dB, then records from where you come in (off for none)",
    },
    Spec {
        name: "overdub",
        alias: None,
        params: &[optional_text("[on|off]", "on or off")],
        describe: "whether the loop pedal overdubs this loop track pass after pass (on), or records one pass and plays it (off, as at first); alone, turns it over",
    },
    Spec {
        name: "layout",
        alias: None,
        params: &[text("LAYOUT", "layout")],
        describe: "how notes lie on the keyboard in insert mode: piano, wicki (Wicki-Hayden), fourths, or scale (only the notes of :scale)",
    },
    Spec {
        name: "vel",
        alias: None,
        params: &[int("VELOCITY", "velocity", 0, 99)],
        describe: "how hard notes are played and entered, 00 to 99 as the vel field shows it (also the numpad's / * -)",
    },
    Spec {
        name: "scale",
        alias: None,
        params: &[text("NAME [ROOT]", "scale")],
        describe: "the scale the pads play outside a group: major, minor, dorian, mixolydian, pentatonic, minor-pentatonic, blues, chromatic; and its root",
    },
    Spec {
        name: "keys",
        alias: None,
        params: &[],
        describe: "show the keys as they're pressed, for screencasts",
    },
    Spec {
        name: "help",
        alias: Some("h"),
        params: &[],
        describe: "every key and command (esc to go back)",
    },
    Spec {
        name: "manual",
        alias: Some("man"),
        params: &[optional_text("[TOPIC]", "topic")],
        describe: "everything explained, at the contents or at TOPIC; again for the next place (esc to go back)",
    },
    Spec {
        name: "tutor",
        alias: None,
        params: &[optional_text("[next|back|end]", "what to do")],
        describe: "start the tutor, or move through it",
    },
    Spec {
        name: "tutor!",
        alias: None,
        params: &[],
        describe: "start the tutor, discarding unsaved changes",
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
        name: "new",
        alias: Some("n"),
        params: &[optional_text("[PATH]", "path")],
        describe: "start an empty song, to save at PATH, or wherever :w says",
    },
    Spec {
        name: "new!",
        alias: Some("n!"),
        params: &[optional_text("[PATH]", "path")],
        describe: "start an empty song, discarding unsaved changes",
    },
    Spec {
        name: "open!",
        alias: Some("o!"),
        params: &[text("PATH", "path")],
        describe: "open a song, discarding unsaved changes",
    },
    Spec {
        name: "recover",
        alias: None,
        params: &[],
        describe: "bring back the unsaved changes kept when Lattice last stopped without saving",
    },
    Spec {
        name: "recover!",
        alias: None,
        params: &[],
        describe: "bring back the kept changes, discarding the unsaved changes there are now",
    },
    Spec {
        name: "pattern",
        alias: Some("pat"),
        params: &[text("NAME", "pattern")],
        describe: "go to a pattern, by name or number (also ]p [p); while playing, it's cued to follow the one playing",
    },
    Spec {
        name: "pattern-new",
        alias: Some("pn"),
        params: &[optional_text("[NAME]", "name")],
        describe: "add an empty pattern after this one, and go to it",
    },
    Spec {
        name: "pattern-dup",
        alias: Some("pd"),
        params: &[optional_text("[NAME]", "name")],
        describe: "copy this pattern, and go to the copy",
    },
    Spec {
        name: "pattern-name",
        alias: None,
        params: &[text("NAME", "name")],
        describe: "rename this pattern (on the song page, the cursor's)",
    },
    Spec {
        name: "pattern-mute",
        alias: Some("pm"),
        params: &[],
        describe: "mute or unmute in this pattern only (also tM)",
    },
    Spec {
        name: "pattern-remove",
        alias: None,
        params: &[],
        describe: "remove this pattern, and its places in the song",
    },
    Spec {
        name: "bars",
        alias: None,
        params: &[optional_text("[BARS | auto]", "bars")],
        describe: "how long this pattern (on the song page, the cursor's) plays, in bars; auto (the default) is as long as its longest track",
    },
    Spec {
        name: "meter",
        alias: None,
        params: &[optional_text("[BEATS [STEPS]]", "meter")],
        describe: "beats in a bar, and steps in a beat (3 for a waltz, 2 3 for 6/8, 2+2+3 for 7/8 in groups); patterns keep their steps and the tempo keeps their speed",
    },
    Spec {
        name: "pattern-meter",
        alias: None,
        params: &[optional_text("[BEATS|song]", "meter")],
        describe: "this pattern's own beats in a bar (5, or 2+2+3), in the song's steps a beat; song for the song's again",
    },
    Spec {
        name: "export",
        alias: None,
        params: &[optional_text("[PATH] [BARS]", "path")],
        describe: "render the song to a WAV file, by default as arranged",
    },
    Spec {
        name: "export-sel",
        alias: None,
        params: &[optional_text("[PATH]", "path")],
        describe: "render the selection to a sample in the song's samples folder",
    },
    Spec {
        name: "export-loop",
        alias: None,
        params: &[optional_text("[PATH]", "path")],
        describe: "render the selection to a sample exactly its length, its tail mixed into the start, to loop",
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

/// The names a value can take, for those that are one of a few.
fn choices(what: &str) -> Option<Vec<&'static str>> {
    match what {
        "effect" => Some(
            lattice_core::FxKind::ALL
                .into_iter()
                .map(lattice_core::Kind::name)
                .collect(),
        ),
        "instrument" => Some(
            lattice_core::InstKind::ALL
                .into_iter()
                .map(lattice_core::Kind::name)
                .collect(),
        ),
        _ => None,
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
            Kind::Text | Kind::OptionalText => {
                let a = if what.starts_with(['a', 'e', 'i', 'o', 'u']) {
                    "an"
                } else {
                    "a"
                };
                match choices(what) {
                    Some(names) => format!("expected {a} {what}: {}", names.join(", ")),
                    None => format!("expected {a} {what}"),
                }
            }
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
pub(crate) struct Match {
    pub spec: &'static Spec,
    pub matched: Vec<usize>,
}

/// Commands that fuzzy-match `query`, best first.
pub(crate) fn complete(query: &str) -> Vec<Match> {
    let mut scored: Vec<(i32, usize, Match)> = COMMANDS
        .iter()
        .enumerate()
        .filter_map(|(order, spec)| {
            let (mut score, matched) = fuzzy(query, spec.name)?;
            if spec.alias == Some(query) {
                score += 100;
            }
            Some((score, order, Match { spec, matched }))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, c)| c).collect()
}

/// The command the player most likely meant by `typed`, which names none: the best one with
/// its letters in order, as `tab` would have filled in (`fxadd`), or else one or two letters
/// off (`tmepo`, `revrese`).
pub(crate) fn nearest(typed: &str) -> Option<&'static str> {
    if let Some(best) = complete(typed).first() {
        return Some(best.spec.name);
    }
    let names = COMMANDS
        .iter()
        .flat_map(|c| [Some(c.name), c.alias])
        .flatten();
    let (distance, name) = names
        .map(|n| (edits(typed, n), n))
        .min_by_key(|&(d, _)| d)?;
    // Most of the word has to survive, or any short word would be near something.
    (distance <= 2 && distance * 2 < name.chars().count()).then_some(name)
}

/// How many letters have to be added, removed, changed or swapped to turn `a` into `b`.
fn edits(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    // Three rows of the table: two back, the last, and the one being filled.
    let mut two_back: Vec<usize> = vec![0; b.len() + 1];
    let mut last: Vec<usize> = (0..=b.len()).collect();
    for (i, &ca) in a.iter().enumerate() {
        let mut row = vec![i + 1; b.len() + 1];
        for (j, &cb) in b.iter().enumerate() {
            let change = last
                .get(j)
                .map_or(usize::MAX, |&d| d + usize::from(ca != cb));
            let remove = last.get(j + 1).map_or(usize::MAX, |&d| d + 1);
            let add = row.get(j).map_or(usize::MAX, |&d| d + 1);
            let mut best = change.min(remove).min(add);
            let swapped = i > 0 && j > 0 && a.get(i - 1) == Some(&cb) && b.get(j - 1) == Some(&ca);
            if swapped && let Some(&d) = two_back.get(j - 1) {
                best = best.min(d + 1);
            }
            if let Some(cell) = row.get_mut(j + 1) {
                *cell = best;
            }
        }
        two_back = std::mem::replace(&mut last, row);
    }
    last.last().copied().unwrap_or(a.len())
}

/// Whether `query`'s characters appear in order in `name`, and how well: matches at the start
/// and runs of consecutive characters score higher, and shorter names win ties.
pub(crate) fn fuzzy(query: &str, name: &str) -> Option<(i32, Vec<usize>)> {
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
    let (complete, partial) = match words.split_last() {
        Some((&last, init)) if !rest.ends_with(' ') => (init, Some(last)),
        _ => (words.as_slice(), None),
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
        todo.extend(values.iter().skip(complete.len()).map(|s| s.to_string()));
        if has_condition {
            todo.push(CONDITION.into());
        }
        if partial.is_some() && !todo.is_empty() {
            todo.remove(0);
        }
    } else if has_condition && let Some(extra) = complete.get(values.len()..) {
        match (extra, partial) {
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
    // The values were parsed to their spec, so these only fail if the spec and the code that
    // runs it disagree, which the tests would catch.

    /// Value `i` as a number, already checked against its range.
    fn int<T: TryFrom<i64>>(&self, i: usize) -> Result<T, String> {
        match self.args.get(i) {
            Some(Arg::Int(n)) => T::try_from(*n).map_err(|_| self.wrong(i, "out of range")),
            _ => Err(self.wrong(i, "not a whole number")),
        }
    }

    fn decimal(&self, i: usize) -> Result<f64, String> {
        match self.args.get(i) {
            Some(Arg::Decimal(n)) => Ok(*n),
            _ => Err(self.wrong(i, "not a decimal")),
        }
    }

    fn text(&self, i: usize) -> Result<&str, String> {
        match self.args.get(i) {
            Some(Arg::Text(text)) => Ok(text),
            _ => Err(self.wrong(i, "not text")),
        }
    }

    fn wrong(&self, i: usize, what: &str) -> String {
        format!("value {} of :{} is {what}", i + 1, self.spec.name)
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
        None if live => return Err(Problem::new(name.range, "unknown command")),
        None => {
            let message = match nearest(name.text) {
                Some(near) => format!("unknown command; did you mean :{near}?"),
                None => "unknown command".into(),
            };
            return Err(Problem::new(name.range, message));
        }
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
        // Checked against its range, so it fits.
        let whole = |n: i64| u32::try_from(n).unwrap_or(u32::MAX);
        let Some((word, typing)) = self.next() else {
            return Ok(Some(Condition::Always));
        };
        // The short forms rules are shown in: `/4`, `/4+1` and `@2`.
        if let Some(rest) = word.text.strip_prefix('/') {
            let (n, offset) = rest.split_once('+').unwrap_or((rest, "0"));
            return match (EVERY.int(n), OFFSET.int(offset)) {
                (Some(n), Some(offset)) => Ok(Some(Condition::Every {
                    n: whole(n),
                    offset: whole(offset),
                })),
                _ if typing && (n.is_empty() || offset.is_empty()) => Ok(None),
                _ => Err(Problem::new(word.range, "expected /N or /N+OFFSET")),
            };
        }
        if let Some(k) = word.text.strip_prefix('@') {
            return match PASS.int(k) {
                Some(k) => Ok(Some(Condition::Pass(whole(k)))),
                None if typing && k.is_empty() => Ok(None),
                None => Err(Problem::new(word.range, "expected @K")),
            };
        }
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
            _ => Err(Problem::new(word.range, "expected every, pass, /N or @K")),
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
        Ok(song_path(typed))
    }
}

/// A song's path as typed, `.lattice` added if it has no extension.
pub(crate) fn song_path(typed: &str) -> PathBuf {
    let path = PathBuf::from(typed);
    match path.extension() {
        Some(_) => path,
        None => path.with_extension("lattice"),
    }
}

/// Changes the mixer settings of every track the selections reach, as one undo step.
fn mix(editor: &mut Editor, change: impl Fn(&mut Mix)) -> Result<Vec<Effect>, String> {
    let tracks = mix_tracks(editor)?;
    editor.checkpoint();
    for &t in &tracks {
        if let Some(track) = editor.grid.tracks.get_mut(t) {
            change(&mut track.mix);
        }
    }
    Ok(tracks.into_iter().map(Effect::TrackChanged).collect())
}

/// The tracks a mixer setting goes to, all of them there.
fn mix_tracks(editor: &Editor) -> Result<Vec<usize>, String> {
    let tracks = editor.mix_targets()?;
    if tracks.iter().any(|&t| t >= editor.grid.tracks.len()) {
        return Err("no such track".into());
    }
    Ok(tracks)
}

/// `tracks` for a message: the track's name if there's one, or how many.
fn named(editor: &Editor, tracks: &[usize]) -> String {
    match tracks {
        [t] => (editor.grid.tracks.get(*t)).map_or_else(|| "1 track".into(), |t| t.name.clone()),
        tracks => format!("{} tracks", tracks.len()),
    }
}

/// Track `track`, for a command that changes it.
fn track_mut(editor: &mut Editor, track: usize) -> Result<&mut Track, String> {
    editor
        .grid
        .tracks
        .get_mut(track)
        .ok_or_else(|| "no such track".into())
}

pub(crate) fn run(editor: &mut Editor, line: &str) -> Result<Vec<Effect>, String> {
    let Some(command) = parse(line, false).map_err(|problem| problem.message)? else {
        return Ok(vec![]);
    };
    // The track under the page's cursor, for the commands that change one track.
    let track = editor.page_track().ok_or("move onto a track first");

    match command.spec.name {
        "quit" if editor.modified() => Err(UNSAVED_QUIT.into()),
        "quit" | "quit!" => Ok(vec![Effect::Quit]),
        "write" => Ok(vec![Effect::Save(save_path(editor, command.text(0)?)?)]),
        "write-quit" => Ok(vec![
            Effect::Save(save_path(editor, command.text(0)?)?),
            Effect::Quit,
        ]),
        "open" if editor.modified() => Err(UNSAVED_OPEN.into()),
        "open" | "open!" => Ok(vec![Effect::Open(command.text(0)?.into())]),
        "recover" if editor.modified() => {
            Err("unsaved changes: :w to save them, or :recover! to bring back the kept ones".into())
        }
        "recover" | "recover!" => Ok(vec![Effect::Recover]),
        "new" if editor.modified() => {
            Err("unsaved changes: :w to save them, or :new! to start anyway".into())
        }
        "new" | "new!" => editor.new_song(command.text(0)?),
        "pattern" => {
            let index = editor.find_pattern(command.text(0)?)?;
            Ok(editor.go_to_pattern(index))
        }
        "pattern-new" => editor.new_pattern(command.text(0)?, false),
        "pattern-dup" => editor.new_pattern(command.text(0)?, true),
        "pattern-name" => {
            editor.rename_pattern(command.text(0)?)?;
            Ok(vec![])
        }
        "pattern-remove" => editor.remove_pattern(),
        "pattern-mute" => {
            let tracks = mix_tracks(editor)?;
            // Toggled together, as mutes are.
            let on = (tracks.iter())
                .filter_map(|&t| editor.grid.tracks.get(t))
                .any(|t| !t.pattern_mute);
            editor.checkpoint();
            for &t in &tracks {
                if let Some(track) = editor.grid.tracks.get_mut(t) {
                    track.pattern_mute = on;
                }
            }
            let what = if on { "muted" } else { "playing again" };
            editor.message = Some(format!("{} {what} in this pattern", named(editor, &tracks)));
            Ok(tracks.into_iter().map(Effect::TrackChanged).collect())
        }
        "bars" => {
            editor.set_bars(command.text(0)?)?;
            Ok(vec![])
        }
        "meter" => editor.set_meter(command.text(0)?),
        "pattern-meter" => editor.set_pattern_meter(command.text(0)?),
        "export" => editor.export_song(command.text(0)?),
        "export-sel" => editor.export_selection(command.text(0)?, false),
        "export-loop" => editor.export_selection(command.text(0)?, true),
        "tutor" if command.text(0).is_ok_and(|t| !t.is_empty()) => {
            editor.tutor_command(command.text(0)?)
        }
        "tutor" if editor.modified() => {
            Err("unsaved changes: :w to save them, or :tutor! to start anyway".into())
        }
        "tutor" | "tutor!" => Ok(editor.start_tutor()),
        "help" => {
            editor.open_help();
            Ok(vec![])
        }
        "manual" => {
            editor.open_manual(command.text(0)?)?;
            Ok(vec![])
        }
        "bpm" => {
            let bpm = command.decimal(0)?;
            editor.bpm = bpm;
            editor.message = Some(format!("{bpm} bpm"));
            Ok(vec![Effect::Bpm(bpm)])
        }
        "len" if editor.in_conductor() => {
            let len: usize = command.int(0)?;
            editor.checkpoint();
            if let Some(conductor) = editor.conductor_mut() {
                conductor.steps.resize(len, [None; GESTURES]);
            }
            editor.clamp_selection();
            editor.message = Some(format!("the conductor's {len} steps long"));
            Ok(vec![])
        }
        "len" => {
            let len: usize = command.int(0)?;
            let track = track?;
            track_mut(editor, track)?;
            editor.checkpoint();
            track_mut(editor, track)?.steps.resize(len, Step::default());
            editor.clamp_selection();
            Ok(vec![Effect::TrackChanged(track)])
        }
        "step" => {
            editor.auto_step = command.int(0)?;
            Ok(vec![])
        }
        "oct" => {
            editor.octave = command.int(0)?;
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
        "arm" | "arm!" => {
            editor.arm(command.spec.name == "arm!", command.text(0)?)?;
            Ok(vec![])
        }
        "disarm" => {
            editor.disarm();
            Ok(vec![])
        }
        "midi" => {
            let mut bound = match editor.controls().len() {
                0 => String::new(),
                1 => "; 1 control bound".into(),
                n => format!("; {n} controls bound"),
            };
            match editor.knobs().len() {
                0 => {}
                1 => bound.push_str("; 1 knob on a setting"),
                n => bound.push_str(&format!("; {n} knobs on settings")),
            }
            let out = match editor.midi_outputs.as_slice() {
                [] => String::new(),
                names => format!("; out: {}", names.join(", ")),
            };
            editor.message = Some(match editor.midi_inputs.as_slice() {
                [] => format!(
                    "no MIDI keyboard or controller: plug one in and Lattice listens to it{bound}{out}"
                ),
                names => format!("midi in: {}{bound}{out}", names.join(", ")),
            });
            Ok(vec![])
        }
        "port" => editor.set_port(command.text(0).unwrap_or_default()),
        "clock" => editor.toggle_clock(command.text(0).unwrap_or_default()),
        "cc" => {
            let value = command.text(1).unwrap_or_default();
            editor.set_cc(command.int(0)?, value)
        }
        "midi-learn" => {
            let target = command.text(0).ok().map(str::to_string);
            editor.midi_learn(target.as_deref())?;
            Ok(vec![])
        }
        "input" => editor.choose_device(Direction::Input, command.text(0)?),
        "output" => editor.choose_device(Direction::Output, command.text(0)?),
        "latency" => {
            editor.latency = command.int(0)?;
            editor.message = Some(format!("latency adjusted by {} ms", editor.latency));
            Ok(vec![Effect::Latency(editor.latency)])
        }
        "count" => {
            editor.count_in = command.int(0)?;
            Ok(vec![])
        }
        "threshold" => {
            let change = match command.text(0)?.trim() {
                "" => None,
                "off" => Some(None),
                typed => match typed.trim_end_matches("dB").trim().parse::<f32>() {
                    Ok(db) if (MIN_THRESHOLD..=0.0).contains(&db) => Some(Some(db)),
                    _ => {
                        return Err(format!(
                            "the threshold goes from {MIN_THRESHOLD} to 0 dB, not {typed}"
                        ));
                    }
                },
            };
            editor.set_looping(change.map(|db| move |l: &mut Looping| l.threshold = db))?;
            Ok(vec![])
        }
        "overdub" => {
            let change = match command.text(0)?.trim() {
                // On its own, it turns it the other way.
                "" => Some(None),
                "on" => Some(Some(true)),
                "off" => Some(Some(false)),
                typed => return Err(format!("overdub on or off, not {typed}")),
            };
            editor.set_looping(
                change.map(|on| move |l: &mut Looping| l.overdub = on.unwrap_or(!l.overdub)),
            )?;
            Ok(vec![])
        }
        "layout" => {
            let name = command.text(0)?;
            let layout = NoteLayout::ALL
                .into_iter()
                .find(|l| l.name() == name)
                .ok_or_else(|| format!("no layout {name}: try piano, wicki, fourths or scale"))?;
            editor.lay_out_keys(layout);
            editor.message = Some(match layout {
                NoteLayout::Scale => format!("notes laid out in {}", editor.scale_name()),
                _ => format!("notes laid out as {name}"),
            });
            Ok(vec![])
        }
        "vel" => {
            // In the vel field's units, so a velocity reads the same everywhere.
            editor.velocity = crate::number_velocity(command.int(0)?);
            editor.message = Some(format!(
                "velocity {:02}",
                crate::velocity_number(editor.velocity)
            ));
            Ok(vec![])
        }
        "scale" => {
            editor.set_scale(command.text(0)?)?;
            Ok(vec![])
        }
        "rec" => {
            editor.toggle_record();
            Ok(vec![])
        }
        "keys" => {
            editor.show_keys = !editor.show_keys;
            editor.message = Some(format!(
                "keys {}",
                if editor.show_keys { "shown" } else { "hidden" }
            ));
            Ok(vec![])
        }
        "click" => {
            editor.click = !editor.click;
            Ok(vec![Effect::Click(editor.click)])
        }
        "name" => {
            let name = command.text(0)?.into();
            let track = track?;
            track_mut(editor, track)?;
            editor.checkpoint();
            track_mut(editor, track)?.name = name;
            Ok(vec![])
        }
        "level" => {
            let level = command.decimal(0)? as f32;
            match editor.bus_under_cursor() {
                Some(bus) => Ok(editor.set_bus_level(bus, level)),
                None => mix(editor, |mix| mix.level = level),
            }
        }
        "pan" => {
            let pan = command.int::<i32>(0)? as f32 / 100.0;
            mix(editor, |mix| mix.pan = pan)
        }
        "send" => {
            let send = command.decimal(0)? as f32;
            mix(editor, |mix| mix.send = send)
        }
        "choke" => {
            let choke = command.int(0)?;
            let tracks = mix_tracks(editor)?;
            editor.checkpoint();
            for &t in &tracks {
                if let Some(track) = editor.grid.tracks.get_mut(t) {
                    track.choke = choke;
                }
            }
            Ok(tracks.into_iter().map(Effect::TrackChanged).collect())
        }
        "mute" | "solo" => {
            let solo = command.spec.name == "solo";
            let flag = |mix: Mix| if solo { mix.solo } else { mix.mute };
            // Toggled together: if any selected track is off, they all go on.
            // As heard, the conductor's over the tracks' own, which take over again.
            let tracks = mix_tracks(editor)?;
            let on = (tracks.iter())
                .filter_map(|&t| editor.heard_mix(t))
                .any(|mix| !flag(mix));
            let unconducted: Vec<_> = tracks.iter().filter_map(|&t| editor.unconduct(t)).collect();
            let mut effects = mix(editor, |mix| {
                if solo {
                    mix.solo = on;
                } else {
                    mix.mute = on;
                }
            })?;
            effects.extend(unconducted);
            let what = match (solo, on) {
                (true, true) => "soloed",
                (true, false) => "unsoloed",
                (false, true) => "muted",
                (false, false) => "unmuted",
            };
            editor.message = Some(format!("{} {what}", named(editor, &tracks)));
            Ok(effects)
        }
        "set" => {
            let word = command.text(0)?;
            // A setting takes its choices by name, and its unit, as a lock does.
            let value = match editor.param_under_cursor() {
                Some(info) => crate::locks::value(&info, word)?,
                None => word
                    .parse()
                    .ok()
                    .filter(|v: &f32| v.is_finite())
                    .ok_or_else(|| format!("{word:?} isn't a number"))?,
            };
            editor.set_mixer_value(value)
        }
        "lock" => editor.lock(command.text(0)?),
        "unlock" => editor.unlock(command.text(0)?),
        "macro" => editor.add_macro(command.text(0)?),
        "map" => editor.map_macro(command.text(0)?),
        "unmap" => editor.unmap_macro(command.text(0)?),
        "fx-add" => editor.add_effect(command.text(0)?),
        "fx-send" => editor.add_send(command.text(0)?),
        "chain-save" => editor.save_chain(command.text(0)?),
        "chain-load" => editor.load_chain(command.text(0)?),
        "preset" => editor.load_preset(command.text(0)?),
        "preset-save" => editor.save_preset(command.text(0)?),
        "face" => editor.set_face(command.text(0)?),
        "face-art" => editor.face_art(command.text(0)?),
        "face-show" => editor.show_on_face(command.text(0)?, true),
        "face-hide" => editor.show_on_face(command.text(0)?, false),
        "inst" => editor.set_instrument(command.text(0)?),
        "sample" => editor.set_sample(command.text(0)?),
        "chop" => editor.set_chop(command.text(0)?),
        "group-name" => editor.rename_group(command.text(0)?),
        "track-add" => {
            let tracks = &editor.grid.tracks;
            if tracks.len() >= MAX_TRACKS {
                return Err(format!("at most {MAX_TRACKS} tracks"));
            }
            let mut name = command.text(0)?.to_string();
            if name.is_empty() {
                // The first `track N` not already taken.
                // Of one more than there are tracks, one's always free.
                name = (tracks.len() + 1..=2 * tracks.len() + 1)
                    .map(|n| format!("track {n}"))
                    .find(|name| tracks.iter().all(|t| t.name != *name))
                    .unwrap_or_default();
            }
            // After the track under the cursor, or at the end from a bus.
            let at = track.map_or(tracks.len(), |t| t + 1);
            editor.checkpoint();
            let track = Track {
                name,
                steps: vec![Step::default(); NEW_TRACK_STEPS],
                ..Track::default()
            };
            if !editor.insert_track(at, track) {
                return Err("can't add a track there".into());
            }
            editor.point_page_at(at);
            Ok(vec![Effect::TracksChanged])
        }
        "track-remove" => {
            if editor.grid.tracks.len() == 1 {
                return Err("can't remove the only track".into());
            }
            let track = track?;
            track_mut(editor, track)?;
            editor.checkpoint();
            if !editor.remove_track(track) {
                return Err("no such track".into());
            }
            let left = editor.grid.tracks.len().saturating_sub(1);
            editor.point_page_at(track.min(left));
            Ok(vec![Effect::TracksChanged])
        }
        "track-move" => {
            let by: isize = command.int(0)?;
            let track = track?;
            track_mut(editor, track)?;
            let to = track
                .saturating_add_signed(by)
                .min(editor.grid.tracks.len().saturating_sub(1));
            if to == track {
                return Ok(vec![]);
            }
            editor.checkpoint();
            if !editor.move_track(track, to) {
                return Err("no such track".into());
            }
            editor.point_page_at(to);
            Ok(vec![Effect::TracksChanged])
        }
        name @ ("rot" | "drift" | "rev" | "tr") => {
            let transform = match name {
                "rot" => Transform::Rotate(command.int(0)?),
                "drift" => Transform::Drift(command.int(0)?),
                "tr" => Transform::Transpose(command.int(0)?),
                _ => Transform::Reverse,
            };
            let track = track?;
            track_mut(editor, track)?;
            editor.checkpoint();
            track_mut(editor, track)?.rules.push(Rule {
                transform,
                when: command.when,
            });
            Ok(vec![Effect::TrackChanged(track)])
        }
        "pop" => {
            let track = track?;
            if track_mut(editor, track)?.rules.is_empty() {
                return Err("no rules to remove".into());
            }
            editor.checkpoint();
            track_mut(editor, track)?.rules.pop();
            Ok(vec![Effect::TrackChanged(track)])
        }
        "clear" => {
            let track = track?;
            track_mut(editor, track)?;
            editor.checkpoint();
            track_mut(editor, track)?.rules.clear();
            Ok(vec![Effect::TrackChanged(track)])
        }
        name => Err(format!(":{name} isn't handled")),
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
        assert_eq!(names("dr"), ["drift", "disarm", "midi-learn"]);
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
            "tr 5 /",
            "tr 5 /4",
            "tr 5 /4+",
            "tr 5 @",
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
        assert_eq!(
            flagged("tr 5 x"),
            flag("x", "expected every, pass, /N or @K")
        );
        assert_eq!(flagged("tr 5 every 0 "), flag("0", "every is 1 or more"));
        assert_eq!(flagged("rev pass x"), flag("x", "pass is a whole number"));
        assert_eq!(flagged("tr 5 /0"), flag("/0", "expected /N or /N+OFFSET"));
        assert_eq!(flagged("tr 5 /x"), flag("/x", "expected /N or /N+OFFSET"));
        assert_eq!(flagged("tr 5 @x"), flag("@x", "expected @K"));
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
            "expected every, pass, /N or @K"
        );
        assert_eq!(problem("tr 5 every").unwrap().message, "every is 1 or more");
        assert_eq!(problem("name ").unwrap().message, "expected a name");
        assert_eq!(
            problem("fx-add ").unwrap().message,
            "expected an effect: delay, reverb, filter, ensemble, phaser, drive, comp, pitch"
        );
        assert_eq!(
            problem("fo").unwrap().message,
            "unknown command; did you mean :follow?"
        );
        assert_eq!(problem("rev every 2 1"), None);
        assert_eq!(problem(""), None);
    }

    #[test]
    fn unknown_commands_suggest_the_nearest() {
        let message = |line| parse(line, false).err().unwrap().message;
        assert_eq!(
            message("fxadd delay"),
            "unknown command; did you mean :fx-add?"
        );
        assert_eq!(message("lyaout"), "unknown command; did you mean :layout?");
        assert_eq!(message("zz"), "unknown command");
        assert_eq!(edits("tmepo", "tempo"), 1);
        assert_eq!(edits("", "bpm"), 3);
        assert_eq!(edits("kitten", "sitting"), 3);
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
        assert_eq!(
            find("len").unwrap().description(),
            "track length, or the conductor's in it, 1 to 256"
        );
        assert_eq!(find("name").unwrap().description(), "rename the track");
        assert_eq!(find("rev").unwrap().description(), "rule: play backwards");
    }
}
