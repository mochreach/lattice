//! The `:` command line.

use lattice_core::{Condition, MAX_STEPS, Rule, Step, Transform};

use crate::{Editor, Effect};

pub const HELP: &str = "q · bpm N · len N · step N · oct N · click · follow · name NAME · \
rot N | drift N | rev | tr N  [every N [OFFSET] | pass K] · pop · clear";

pub(crate) fn run(editor: &mut Editor, line: &str) -> Result<Vec<Effect>, String> {
    let mut words = line.split_whitespace();
    let Some(command) = words.next() else {
        return Ok(vec![]);
    };
    let args: Vec<&str> = words.collect();
    let track = editor.cursor.track;

    match command {
        "q" | "quit" => Ok(vec![Effect::Quit]),
        "help" | "h" => Err(HELP.into()),
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
        _ => Err(format!("unknown command: {command}  (:help)")),
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
