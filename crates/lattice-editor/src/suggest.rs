//! Suggestions on the command line: the commands matching what's typed, then, once a command's
//! named, the values that fit the word being typed, such as the effects after `:fx-add`, the
//! track's settings after `:lock` or the files after `:open`. They're matched as command names
//! are, by their letters in order, and `tab` fills them in one after another.

use std::path::PathBuf;

use lattice_core::{FxKind, InstKind, Kind, MAX_LANES, PRESETS};

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
            _ => ranked(typed, self.choices(spec.name, &before)),
        }
    }

    /// The values that fit command `name`'s next word, after the words `before`, each with what
    /// it is.
    fn choices(&self, name: &str, before: &[&str]) -> Vec<(String, String)> {
        let kinds = |names: Vec<(&str, &[lattice_core::ParamInfo])>| {
            (names.into_iter())
                .map(|(name, params)| {
                    let params: Vec<&str> = params.iter().map(|p| p.name).collect();
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
        let track = &self.grid.tracks[self.cursor().track];
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
                    let whose = if PRESETS.contains(&name.as_str()) {
                        "Lattice's"
                    } else {
                        "saved"
                    };
                    (name.clone(), whose.into())
                })
                .collect(),
            ("lock" | "unlock", 0) => suggest_settings(track),
            ("lock", 1) => words(&suggest_choices(track, before[0])),
            ("map" | "unmap", 0) => match self.macro_track() {
                Ok(t) => (self.grid.tracks[t].macros.iter().enumerate())
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
            ("input", 0) => devices(&self.inputs),
            ("output", 0) => devices(&self.outputs),
            _ => vec![],
        }
    }
}

/// The commands that take a file first, and the kind of file, if only one kind will do.
fn files(command: &str) -> Option<Option<&'static str>> {
    match command {
        "open" | "open!" | "new" | "new!" | "write" | "write-quit" => Some(Some("lattice")),
        "sample" | "chop" | "export" | "export-sel" | "export-loop" => Some(Some("wav")),
        _ => None,
    }
}

fn devices(devices: &crate::Devices) -> Vec<(String, String)> {
    (devices.found.iter().enumerate())
        .map(|(i, d)| (d.name.clone(), format!("device {}", i + 1)))
        .collect()
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
    let (folder, name) = typed.split_at(typed.rfind('/').map_or(0, |i| i + 1));
    let path = match folder.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME").map(|home| PathBuf::from(home).join(rest)),
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
