//! The `:` command line.

use lattice_core::{Condition, MAX_STEPS, Rule, Step, Transform};

use crate::{Editor, Effect};

/// What a command takes after its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Param {
    /// A value, shown by this placeholder until it's typed.
    Value(&'static str),
    /// An optional `every N [OFFSET]` or `pass K`, always last.
    Condition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub name: &'static str,
    pub alias: Option<&'static str>,
    pub params: &'static [Param],
    pub describe: &'static str,
}

const CONDITION: &str = "[every N [OFFSET] | pass K]";

use Param::{Condition as When, Value};

/// Every `:` command, in the order the completion list shows them before anything is typed.
pub const COMMANDS: &[Spec] = &[
    Spec {
        name: "tr",
        alias: None,
        params: &[Value("SEMITONES"), When],
        describe: "rule: transpose",
    },
    Spec {
        name: "rot",
        alias: None,
        params: &[Value("STEPS"), When],
        describe: "rule: rotate the pattern",
    },
    Spec {
        name: "drift",
        alias: None,
        params: &[Value("STEPS"), When],
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
        params: &[Value("STEPS")],
        describe: "track length, 1 to 256",
    },
    Spec {
        name: "name",
        alias: None,
        params: &[Value("NAME")],
        describe: "rename the track",
    },
    Spec {
        name: "bpm",
        alias: None,
        params: &[Value("BPM")],
        describe: "tempo, 20 to 400",
    },
    Spec {
        name: "step",
        alias: None,
        params: &[Value("STEPS")],
        describe: "steps to move after each note, 0 to 64",
    },
    Spec {
        name: "oct",
        alias: None,
        params: &[Value("OCTAVE")],
        describe: "keyboard octave, 0 to 9",
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
        name: "quit",
        alias: Some("q"),
        params: &[],
        describe: "quit",
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
                Value(name) => name,
                When => CONDITION,
            })
            .collect();
        words.join(" ")
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
            Value(name) => Some(*name),
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

pub(crate) fn run(editor: &mut Editor, line: &str) -> Result<Vec<Effect>, String> {
    let mut words = line.split_whitespace();
    let Some(command) = words.next() else {
        return Ok(vec![]);
    };
    let args: Vec<&str> = words.collect();
    let track = editor.cursor.track;
    let spec = find(command).ok_or_else(|| format!("unknown command: {command}"))?;

    match spec.name {
        "quit" => Ok(vec![Effect::Quit]),
        "help" => {
            let names: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
            Err(names.join(" · "))
        }
        "bpm" => {
            let bpm: f64 = number(&args, 20.0, 400.0)?;
            editor.bpm = bpm;
            Ok(vec![Effect::Bpm(bpm)])
        }
        "len" => {
            let len: usize = number(&args, 1, MAX_STEPS)?;
            editor.checkpoint();
            editor.grid.tracks[track].steps.resize(len, Step::default());
            editor.clamp_selection();
            Ok(vec![Effect::TrackChanged(track)])
        }
        "step" => {
            editor.auto_step = number(&args, 0, 64)?;
            Ok(vec![])
        }
        "oct" => {
            editor.octave = number(&args, 0, 9)?;
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
            let name = args.join(" ");
            if name.is_empty() {
                return Err("name what?".into());
            }
            editor.grid.tracks[track].name = name;
            Ok(vec![])
        }
        "rot" | "drift" | "rev" | "tr" => {
            let (transform, rest) = transform(command, &args)?;
            let when = condition(rest)?;
            editor.checkpoint();
            editor.grid.tracks[track]
                .rules
                .push(Rule { transform, when });
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

fn transform<'a>(command: &str, args: &'a [&'a str]) -> Result<(Transform, &'a [&'a str]), String> {
    if command == "rev" {
        return Ok((Transform::Reverse, args));
    }
    let (amount, rest) = args
        .split_first()
        .ok_or_else(|| format!("{command} needs an amount"))?;
    let amount: i32 = amount
        .trim_start_matches('+')
        .parse()
        .map_err(|_| format!("not a number: {amount}"))?;
    let transform = match command {
        "rot" => Transform::Rotate(amount),
        "drift" => Transform::Drift(amount),
        _ => Transform::Transpose(
            i8::try_from(amount).map_err(|_| "transpose by -128 to 127".to_string())?,
        ),
    };
    Ok((transform, rest))
}

fn condition(args: &[&str]) -> Result<Condition, String> {
    let parse = |s: &str| s.parse::<u32>().map_err(|_| format!("not a number: {s}"));
    match args {
        [] => Ok(Condition::Always),
        ["every", n] => Ok(Condition::Every {
            n: parse(n)?.max(1),
            offset: 0,
        }),
        ["every", n, offset] => Ok(Condition::Every {
            n: parse(n)?.max(1),
            offset: parse(offset)?,
        }),
        ["pass", k] => Ok(Condition::Pass(parse(k)?)),
        _ => Err("conditions: every N [OFFSET] · pass K".into()),
    }
}

fn number<T>(args: &[&str], min: T, max: T) -> Result<T, String>
where
    T: std::str::FromStr + PartialOrd + std::fmt::Display + Copy,
{
    let [arg] = args else {
        return Err(format!("expected a number from {min} to {max}"));
    };
    match arg.parse::<T>() {
        Ok(n) if n >= min && n <= max => Ok(n),
        _ => Err(format!("expected a number from {min} to {max}")),
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
        assert_eq!(names("q"), ["quit"]);
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
        assert_eq!(hint("nope "), None);
    }
}
