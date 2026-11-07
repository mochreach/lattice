//! Suggestions on the command line: the commands matching what's typed, then, once a command's
//! named, the values that fit the word being typed, such as the effects after `:fx-add`, the
//! track's settings after `:lock` or the files after `:open`. They're matched as command names
//! are, by their letters in order, and `tab` fills them in one after another.

use std::path::PathBuf;

use lattice_core::{CHAINS, FxKind, InstKind, Kind, MAX_LANES, MAX_STEPS};

use crate::Editor;
use crate::command::{self, fuzzy};
use crate::keymap::NoteLayout;
use crate::locks::{suggest_choices, suggest_settings};
use crate::pads::SCALES;

/// Most entries of a folder looked through for files.
const MAX_FILES: usize = 1000;

/// A suggestion for the word being typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// What `tab` puts in place of the word.
    pub text: String,
    /// Where the letters typed are in `text`, by byte.
    pub matched: Vec<usize>,
    /// A command's parameters, shown faintly after it.
    pub signature: String,
    /// What it is or does.
    pub describe: String,
    /// Not a value but a reminder that any will do, `[N]`, shown last; `tab` passes it by.
    pub any: bool,
}

/// Where the word being typed at the end of `line` starts.
pub fn word_start(line: &str) -> usize {
    line.rfind(' ').map_or(0, |i| i + 1)
}

impl Editor {
    /// Suggestions for the word being typed at the end of `line`, best first.
    pub(crate) fn suggest(&self, line: &str) -> Vec<Completion> {
        let start = word_start(line);
        let typed = &line[start..];
        if start == 0 {
            return (command::complete(typed).into_iter())
                .map(|c| Completion {
                    text: c.spec.name.into(),
                    matched: c.matched,
                    signature: c.spec.signature(),
                    any: false,
                    describe: c.spec.description(),
                })
                .collect();
        }
        let mut words = line[..start].split_whitespace();
        let Some(spec) = words.next().and_then(command::find) else {
            return vec![];
        };
        let before: Vec<&str> = words.collect();
        match files(spec.name) {
            Some(extension) if before.is_empty() => paths(typed, extension),
            _ => {
                let mut found = ranked(typed, self.choices(spec.name, &before));
                if let Some((text, describe)) = any_number(spec.name, &before) {
                    found.push(Completion {
                        text: text.into(),
                        matched: vec![],
                        signature: String::new(),
                        describe,
                        any: true,
                    });
                }
                found
            }
        }
    }

    /// The values that fit command `name`'s next word, after the words `before`, each with what
    /// it is.
    fn choices(&self, name: &str, before: &[&str]) -> Vec<(String, String)> {
        let kinds = |names: Vec<(&str, &[lattice_core::ParamInfo])>| {
            (names.into_iter())
                .map(|(name, params)| {
                    let mut params: Vec<&str> = params.iter().map(|p| p.name).collect();
                    // A MIDI track's controllers, by the range rather than one by one.
                    if name == "midi" {
                        params.truncate(2);
                        params.push("cc0…cc127");
                    }
                    (name.to_string(), params.join(" "))
                })
                .collect()
        };
        let words = |words: &[&str]| {
            words
                .iter()
                .map(|w| (w.to_string(), String::new()))
                .collect()
        };
        let Some(track) = self.grid.tracks.get(self.cursor().track) else {
            return vec![];
        };
        match (name, before.len()) {
            ("fx-add", 0) => kinds(FxKind::ALL.iter().map(|k| (k.name(), k.params())).collect()),
            ("inst", 0) => kinds(
                InstKind::ALL
                    .iter()
                    .map(|k| (k.name(), k.params()))
                    .collect(),
            ),
            ("chain-load", 0) => (self.chains.iter())
                .map(|name| {
                    let whose = if CHAINS.contains(&name.as_str()) {
                        "Lattice's"
                    } else {
                        "saved"
                    };
                    (name.clone(), whose.into())
                })
                .collect(),
            ("preset", 0) => (self.presets.iter())
                .map(|name| {
                    let whose = match lattice_core::preset(name) {
                        Some(preset) => {
                            format!("{} · {}", preset.face.title, preset.instrument.kind.name())
                        }
                        None => "saved".into(),
                    };
                    (name.clone(), whose)
                })
                .collect(),
            ("face-show" | "face-hide", 0) => (track.instrument.params().iter())
                .zip(&track.instrument.values)
                .map(|(p, &value)| (p.name.to_string(), p.format(value)))
                .collect(),
            ("port", 0) => (self.midi_outputs.iter().enumerate())
                .map(|(i, name)| (name.clone(), format!("port {}", i + 1)))
                .chain([("none".into(), "Lattice's own port".into())])
                .collect(),
            ("clock", 0) => (self.midi_outputs.iter().enumerate())
                .map(|(i, name)| {
                    let on = (self.clock_ports.iter()).any(|p| p.eq_ignore_ascii_case(name));
                    let what = if on { "clock on" } else { "port" };
                    (name.clone(), format!("{what} {}", i + 1))
                })
                .chain([
                    ("lattice".into(), "Lattice's own port".into()),
                    ("none".into(), "no clock".into()),
                ])
                .collect(),
            ("lock" | "unlock", 0) => suggest_settings(track),
            ("lock", 1) => match before.first() {
                Some(setting) => words(&suggest_choices(track, setting)),
                None => vec![],
            },
            ("map" | "unmap", 0) => match self.macro_track() {
                Ok(t) => (self.grid.tracks.get(t).into_iter())
                    .flat_map(|track| track.macros.iter().enumerate())
                    .map(|(i, m)| {
                        let n = m.targets.len();
                        let s = if n == 1 { "" } else { "s" };
                        (
                            m.name.clone(),
                            format!("macro {}, turns {n} setting{s}", i + 1),
                        )
                    })
                    .collect(),
                Err(_) => vec![],
            },
            ("fx-send", 0) => {
                let lanes = self.chain(self.fx_strip()).lanes().len();
                (0..(lanes + 1).min(MAX_LANES))
                    .map(|k| {
                        let letter = lattice_core::FxLane::letter(k).to_string();
                        let what = if k == lanes { "a new lane" } else { "" };
                        (letter, what.into())
                    })
                    .collect()
            }
            ("layout", 0) => words(&NoteLayout::ALL.map(NoteLayout::name)),
            ("scale", 0) => words(&SCALES.map(|s| s.name)),
            ("tutor", 0) => words(&["next", "back", "end"]),
            ("bars", 0) => words(&["auto"]),
            ("pattern", 0) => (self.arrangement.patterns.iter().enumerate())
                .map(|(i, p)| (p.name.clone(), format!("pattern {}", i + 1)))
                .collect(),
            ("tr" | "rot" | "drift" | "rev", _) => self.rule_choices(name, before),
            ("input", 0) => devices(&self.inputs),
            ("output", 0) => devices(&self.outputs),
            _ => vec![],
        }
    }
}

impl Editor {
    /// What fits a rule's next word: its amount, as an interval or a number of steps, then when
    /// it applies, `every N [OFFSET]` or `pass K`.
    fn rule_choices(&self, name: &str, before: &[&str]) -> Vec<(String, String)> {
        let beat = self.grid.steps_per_beat as i64;
        let steps = |n: i64, s: &str| match n.abs() {
            1 => format!("a step {s}"),
            n if n == beat => format!("a beat {s}"),
            n if n == 2 * beat => format!("two beats {s}"),
            n => format!("{n} steps {s}"),
        };
        let amount: Vec<(String, String)> = match name {
            "tr" => [
                (12, "an octave up"),
                (-12, "an octave down"),
                (7, "a fifth up"),
                (-5, "a fourth down, to the fifth below"),
                (5, "a fourth up"),
                (-7, "a fifth down"),
                (3, "a minor third up"),
                (4, "a major third up"),
                (2, "a tone up"),
                (-2, "a tone down"),
                (1, "a semitone up"),
                (-1, "a semitone down"),
            ]
            .map(|(n, what)| (n.to_string(), what.to_string()))
            .to_vec(),
            "rot" => [1, -1, beat, -beat, 2, -2, 2 * beat]
                .map(|n| {
                    (
                        n.to_string(),
                        steps(n, if n > 0 { "later" } else { "earlier" }),
                    )
                })
                .to_vec(),
            "drift" => [1, -1, beat, -beat]
                .map(|n| {
                    let way = if n > 0 { "later" } else { "earlier" };
                    (n.to_string(), steps(n, &format!("{way} each time")))
                })
                .to_vec(),
            _ => vec![],
        };
        let values = usize::from(name != "rev");
        let Some(when) = before.get(values..) else {
            return amount;
        };
        let pairs = |pairs: Vec<(u32, String)>| {
            (pairs.into_iter())
                .map(|(n, what)| (n.to_string(), what))
                .collect()
        };
        match when {
            [] => vec![
                (
                    "every".into(),
                    "every N passes, from pass OFFSET (0 if left out)".into(),
                ),
                ("pass".into(), "on pass K only, counting from 0".into()),
            ],
            ["every"] => pairs(
                [2, 3, 4, 8]
                    .map(|n| {
                        let what = match n {
                            2 => "every other pass".into(),
                            n => format!("every {} pass", ordinal(n)),
                        };
                        (n, what)
                    })
                    .to_vec(),
            ),
            ["every", n] => match n.parse::<u32>() {
                Ok(n @ 1..=16) => pairs(
                    (0..n)
                        .map(|k| (k, format!("the {} pass of every {n}", ordinal(k + 1))))
                        .collect(),
                ),
                _ => vec![],
            },
            ["pass"] => pairs(
                (0..4)
                    .map(|k| (k, format!("the {} pass only", ordinal(k + 1))))
                    .collect(),
            ),
            _ => vec![],
        }
    }
}

/// For a word that takes any number in a range, what to say so under the suggestions: rules'
/// amounts, how often they apply and on which pass.
fn any_number(name: &str, before: &[&str]) -> Option<(&'static str, String)> {
    let steps = MAX_STEPS;
    let values = usize::from(name != "rev");
    if !matches!(name, "tr" | "rot" | "drift" | "rev") {
        return None;
    }
    let Some(when) = before.get(values..) else {
        return Some(match name {
            "tr" => ("[N]", "N semitones up, or down if negative, to 127".into()),
            "rot" => (
                "[N]",
                format!("N steps later, or earlier if negative, to {steps}"),
            ),
            _ => (
                "[N]",
                format!("N steps later each time, or earlier if negative, to {steps}"),
            ),
        });
    };
    match when {
        ["every"] => Some(("[N]", "every Nth pass".into())),
        ["every", n] => {
            let n: u32 = n.parse().ok()?;
            Some((
                "[OFFSET]",
                format!("from pass OFFSET, 0 to {}", n.saturating_sub(1)),
            ))
        }
        ["pass"] => Some(("[K]", "pass K only, counting from 0".into())),
        _ => None,
    }
}

/// `1st`, `2nd`, `3rd`, `4th`, `11th`, `21st`.
pub(crate) fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// The commands that take a file first, and the kind of file, if only one kind will do.
fn files(command: &str) -> Option<Option<&'static str>> {
    match command {
        "open" | "open!" | "new" | "new!" | "write" | "write-quit" => Some(Some("lattice")),
        "sample" | "chop" | "export" | "export-sel" | "export-loop" => Some(Some("wav")),
        "face-art" => Some(Some("txt")),
        _ => None,
    }
}

fn devices(devices: &crate::Devices) -> Vec<(String, String)> {
    let mut found: Vec<_> = (devices.found.iter().enumerate())
        .map(|(i, d)| {
            let in_use = if devices.chosen.as_ref() == Some(&d.id) {
                ", in use"
            } else {
                ""
            };
            (d, (d.name.clone(), format!("device {}{in_use}", i + 1)))
        })
        .collect();
    // Apps first, where they're seen: they're listed after the devices, past the ten shown.
    found.sort_by_key(|(d, _)| !d.id.starts_with(crate::APP));
    found.into_iter().map(|(_, choice)| choice).collect()
}

/// The `choices` that match `typed`, best first, and in their own order otherwise.
fn ranked(typed: &str, choices: Vec<(String, String)>) -> Vec<Completion> {
    let mut found: Vec<(i32, Completion)> = (choices.into_iter())
        .filter_map(|(text, describe)| {
            let (score, matched) = fuzzy(typed, &text)?;
            Some((
                score,
                Completion {
                    text,
                    matched,
                    signature: String::new(),
                    any: false,
                    describe,
                },
            ))
        })
        .collect();
    // Stable, so ties keep their order.
    found.sort_by_key(|(score, _)| -score);
    starting(found.into_iter().map(|(_, c)| c).collect(), typed, 0)
}

/// The files and folders matching `typed`, a path typed so far: in the folder it names, those
/// whose names match what's after its last `/`, folders first. Files are only those ending in
/// `extension`, if it's given; hidden ones only once a `.` is typed.
fn paths(typed: &str, extension: Option<&str>) -> Vec<Completion> {
    // Just after a `/` is always a char boundary, so it always splits.
    let after = typed.rfind('/').map_or(0, |i| i + 1);
    let (folder, name) = typed.split_at_checked(after).unwrap_or(("", typed));
    let path = match folder.strip_prefix("~/") {
        Some(rest) => crate::home().map(|home| home.join(rest)),
        None if folder.is_empty() => Some(PathBuf::from(".")),
        None => Some(PathBuf::from(folder)),
    };
    let Some(entries) = path.and_then(|path| std::fs::read_dir(path).ok()) else {
        return vec![];
    };
    let mut found: Vec<(bool, i32, Completion)> = (entries.flatten().take(MAX_FILES))
        .filter_map(|entry| {
            let file = entry.file_name().into_string().ok()?;
            if file.starts_with('.') && !name.starts_with('.') {
                return None;
            }
            let is_folder = entry.path().is_dir();
            if !is_folder
                && extension
                    .is_some_and(|ext| !file.rsplit_once('.').is_some_and(|(_, e)| e == ext))
            {
                return None;
            }
            let (score, matched) = fuzzy(name, &file)?;
            let slash = if is_folder { "/" } else { "" };
            Some((
                is_folder,
                score,
                Completion {
                    text: format!("{folder}{file}{slash}"),
                    matched: matched.iter().map(|m| m + folder.len()).collect(),
                    signature: String::new(),
                    any: false,
                    describe: String::new(),
                },
            ))
        })
        .collect();
    found.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)).then(a.2.text.cmp(&b.2.text)));
    let found = found.into_iter().map(|(_, _, c)| c).collect();
    starting(found, name, folder.len())
}

/// Only those of `found` that start with `typed`, `skip` bytes in, if any do: a value's first
/// letters narrow the list to it, and its letters anywhere only find it when nothing starts so.
fn starting(found: Vec<Completion>, typed: &str, skip: usize) -> Vec<Completion> {
    let starts = |c: &Completion| {
        (c.text.get(skip..)).is_some_and(|t| t.to_lowercase().starts_with(&typed.to_lowercase()))
    };
    if found.iter().any(starts) {
        found.into_iter().filter(starts).collect()
    } else {
        found
    }
}
