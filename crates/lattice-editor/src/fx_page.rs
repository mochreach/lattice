//! The fx page: one strip's chain of effects, drawn like the pattern. Its lanes are columns side
//! by side, read top to bottom, and each row is an effect or a send that copies the sound into
//! another lane (into its own or one to its left, it's feedback, heard a block later). Under each
//! effect are its settings, a line each, and each lane ends in its `out`, the level it goes into
//! the chain's output at, drawn along the bottom.
//!
//! The cursor moves over the lines as over the pattern's steps: `j` `k` line by line, onto the
//! empty lines below a short lane too, `h` `l` from lane to lane on the same line, and `w` `b`
//! from effect to effect, past their settings. `C-a` `C-x` turn whatever the cursor's on, a
//! setting, a send or an out. Settings go through the mixer's cursor, which the page keeps on
//! the effect and setting under its own, so `:set`, locking and recorded locks work as there.
//! `d` removes an effect or a send, or on an out, the whole lane, and `:fx-add` and `:fx-send`
//! add rows after the cursor.

use lattice_core::{FxChain, FxLane, FxRow, MAX_LANES, MIN_DB, RowAt, is_feedback, max_send};

use crate::keymap::Action;
use crate::mixer::{Page, Row, Strip};
use crate::{Editor, Effect, Mode, PromptKind};

/// What a nudge moves a send or an out by, in dB.
const LEVEL_STEP: f32 = 0.5;
/// A new send's level into a lane that hasn't run yet, and back as feedback, in dB.
const FORWARD_DB: f32 = 0.0;
const FEEDBACK_DB: f32 = -6.0;

/// Where the fx page's cursor is: a lane, and a line down the page. The lines past a lane's
/// last are empty, down to the outs, on the line after the longest lane's last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FxCursor {
    pub lane: usize,
    pub line: usize,
    /// On the track's macros, left of the lanes, rather than a lane.
    pub macros: bool,
}

/// A line of the fx page, in one lane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FxLine {
    /// An effect's name: the row, and the effect's index in the chain.
    Fx {
        row: usize,
        effect: usize,
    },
    /// One of an effect's settings, by its place in the effect's list.
    Param {
        row: usize,
        effect: usize,
        param: usize,
    },
    Send {
        row: usize,
        to: usize,
        level: f32,
    },
    /// Below the lane's last row.
    Empty,
    Out,
    /// A macro, by its index on the track, in the macros' column.
    Macro {
        index: usize,
    },
    /// A setting the macro turns, by its place in the macro's targets.
    Target {
        index: usize,
        target: usize,
    },
}

impl FxLine {
    /// The row it's part of.
    pub fn row(self) -> Option<usize> {
        match self {
            FxLine::Fx { row, .. } | FxLine::Param { row, .. } | FxLine::Send { row, .. } => {
                Some(row)
            }
            _ => None,
        }
    }

    /// Whether `w` and `b` stop on it: an effect's name, a send, a macro, or the out.
    fn is_stop(self) -> bool {
        !matches!(
            self,
            FxLine::Param { .. } | FxLine::Empty | FxLine::Target { .. }
        )
    }
}

/// Each lane's lines, top to bottom, down to its last row's.
pub fn fx_lines(chain: &FxChain) -> Vec<Vec<FxLine>> {
    let mut lanes = vec![vec![]; chain.lanes().len()];
    for (at, row, effect) in chain.rows() {
        let Some(lines) = lanes.get_mut(at.lane) else {
            continue;
        };
        match (row, effect) {
            (FxRow::Send { to, level }, _) => lines.push(FxLine::Send {
                row: at.row,
                to,
                level,
            }),
            (FxRow::Fx, Some(effect)) => {
                let Some(fx) = chain.get(effect) else {
                    continue;
                };
                lines.push(FxLine::Fx {
                    row: at.row,
                    effect,
                });
                let params = fx.params().len();
                lines.extend((0..params).map(|param| FxLine::Param {
                    row: at.row,
                    effect,
                    param,
                }));
            }
            (FxRow::Fx, None) => {}
        }
    }
    lanes
}

impl Editor {
    /// The chain the fx page shows: the mixer cursor's strip's.
    pub fn fx_strip(&self) -> Strip {
        self.strip(self.mixer_cursor().strip)
    }

    /// The fx page's lines, lane by lane, and the line the outs are on, below the longest lane
    /// and the macros.
    pub fn fx_page_lines(&self) -> (Vec<Vec<FxLine>>, usize) {
        let lanes = fx_lines(self.chain(self.fx_strip()));
        let outs = (lanes.iter().map(Vec::len))
            .chain([self.macro_lines().len()])
            .max()
            .unwrap_or(0);
        (lanes, outs)
    }

    /// The macros' column: each of the fx page's track's macros, then the settings it turns.
    pub fn macro_lines(&self) -> Vec<FxLine> {
        let Strip::Track(t) = self.fx_strip() else {
            return vec![];
        };
        let Some(track) = self.grid.tracks.get(t) else {
            return vec![];
        };
        let mut lines = vec![];
        for (index, m) in track.macros.iter().enumerate() {
            lines.push(FxLine::Macro { index });
            lines.extend((0..m.targets.len()).map(|target| FxLine::Target { index, target }));
        }
        lines
    }

    /// The fx page's cursor, kept within the lanes and lines there are now.
    pub fn fx_cursor(&self) -> FxCursor {
        let (lanes, outs) = self.fx_page_lines();
        FxCursor {
            lane: self.fx.lane.min(lanes.len().saturating_sub(1)),
            line: self.fx.line.min(outs),
            macros: self.fx.macros && matches!(self.fx_strip(), Strip::Track(_)),
        }
    }

    /// What's under the fx page's cursor.
    pub fn fx_line(&self) -> FxLine {
        let cursor = self.fx_cursor();
        if cursor.macros {
            return (self.macro_lines().get(cursor.line).copied()).unwrap_or(FxLine::Empty);
        }
        let (lanes, outs) = self.fx_page_lines();
        match (lanes.get(cursor.lane)).and_then(|lane| lane.get(cursor.line)) {
            Some(&line) => line,
            None if cursor.line == outs => FxLine::Out,
            None => FxLine::Empty,
        }
    }

    /// Puts the fx page's cursor on macro `index`.
    pub(crate) fn goto_macro(&mut self, index: usize) {
        let line = (self.macro_lines().iter())
            .position(|&l| l == FxLine::Macro { index })
            .unwrap_or(0);
        self.move_fx(FxCursor {
            line,
            macros: true,
            ..self.fx_cursor()
        });
    }

    /// Whether the fx page's cursor is on one of an effect's settings, which the mixer's
    /// cursor is then on too.
    pub(crate) fn on_fx_param(&self) -> bool {
        matches!(self.mixer_cursor().row, Row::Param(_))
    }

    /// `gf`: opens the fx page, on the pattern cursor's track if coming from the pattern.
    pub(crate) fn goto_fx(&mut self) {
        match self.page {
            Page::Pattern => {
                self.mixer.strip = self.cursor().track;
                self.fx = FxCursor::default();
            }
            // The mixer's cursor is on the instrument page's track already.
            Page::Instrument => self.fx = FxCursor::default(),
            _ => {}
        }
        self.page = Page::Fx;
        self.mode = Mode::Normal;
        self.show_fx_line();
    }

    /// Moves the fx cursor, keeping it within the lanes, and puts the mixer's on the effect and
    /// setting it's on.
    fn move_fx(&mut self, to: FxCursor) {
        self.fx = to;
        self.fx = self.fx_cursor();
        self.show_fx_line();
    }

    fn show_fx_line(&mut self) {
        self.mixer.row = Row::Chain;
        match self.fx_line() {
            FxLine::Fx { effect, .. } => {
                self.mixer.effect = effect;
                self.mixer.instrument = false;
            }
            FxLine::Param { effect, param, .. } => {
                self.mixer.effect = effect;
                self.mixer.instrument = false;
                self.mixer.row = Row::Param(param);
            }
            _ => {}
        }
    }

    /// The `n`th line from the cursor's, in its lane, that `w` (or going back, `b`) stops on.
    fn fx_stop(&self, n: usize, forward: bool) -> usize {
        let cursor = self.fx_cursor();
        let (lanes, outs) = self.fx_page_lines();
        let macros = self.macro_lines();
        let lane: &[FxLine] = match cursor.macros {
            true => &macros,
            false => lanes.get(cursor.lane).map_or(&[], Vec::as_slice),
        };
        let stop = |line: usize| lane.get(line).is_none_or(|l| l.is_stop());
        let mut line = cursor.line;
        for _ in 0..n {
            line = if forward {
                // Past the lane's last row, onto its out.
                match (line + 1..lane.len()).find(|&l| stop(l)) {
                    Some(l) => l,
                    None => outs,
                }
            } else {
                let last = line.min(lane.len());
                (0..last).rev().find(|&l| stop(l)).unwrap_or(0)
            };
        }
        line
    }

    /// Handles a key's action on the fx page. `None` for the actions the pages share.
    pub(crate) fn fx_act(&mut self, action: Action, n: usize) -> Option<Vec<Effect>> {
        let cursor = self.fx_cursor();
        let lanes = self.chain(self.fx_strip()).lanes().len();
        let by = n as i32;
        let line = |line| FxCursor { line, ..cursor };
        let track = matches!(self.fx_strip(), Strip::Track(_));
        match action {
            // The macros are left of lane A, on a track, onto the last of them if the cursor's
            // below it.
            Action::Left if !cursor.macros && track && cursor.lane < n => {
                let last = self.macro_lines().len().saturating_sub(1);
                self.move_fx(FxCursor {
                    line: cursor.line.min(last),
                    macros: true,
                    ..cursor
                })
            }
            Action::Left => self.move_fx(FxCursor {
                lane: cursor.lane.saturating_sub(n),
                ..cursor
            }),
            Action::Right if cursor.macros => self.move_fx(FxCursor {
                lane: n.saturating_sub(1),
                macros: false,
                ..cursor
            }),
            Action::Right => self.move_fx(FxCursor {
                lane: (cursor.lane + n).min(lanes.saturating_sub(1)),
                ..cursor
            }),
            Action::Up => self.move_fx(line(cursor.line.saturating_sub(n))),
            Action::Down => self.move_fx(line(cursor.line + n)),
            Action::NextNote => self.move_fx(line(self.fx_stop(n, true))),
            Action::PrevNote => self.move_fx(line(self.fx_stop(n, false))),
            Action::FirstStep => self.move_fx(line(0)),
            Action::LastStep => self.move_fx(line(usize::MAX)),
            Action::ToggleView => {}
            Action::NoteUp | Action::Longer | Action::NoteDown | Action::Shorter => {
                let up = matches!(action, Action::NoteUp | Action::Longer);
                if self.on_fx_param() && !cursor.macros {
                    // The mixer nudges settings.
                    return self.mixer_act(action, n);
                }
                return Some(self.fx_nudge(if up { by } else { -by }));
            }
            Action::Delete | Action::Clear => return Some(self.fx_delete()),
            Action::Change => {
                let line = match self.fx_line() {
                    FxLine::Param { .. }
                    | FxLine::Send { .. }
                    | FxLine::Out
                    | FxLine::Macro { .. }
                    | FxLine::Target { .. } => "set ",
                    _ => "fx-add ",
                };
                self.prompt_with(line);
            }
            // A new effect after the cursor's, as `o` opens a step below it on the grid.
            Action::OpenBelow => self.prompt_with("fx-add "),
            // The strip, locking and the rest work as on the mixer.
            _ => {
                let effects = self.mixer_act(action, n);
                self.show_fx_line();
                return effects;
            }
        }
        Some(vec![])
    }

    /// Opens the command line with `line` typed, for the rest to be added.
    fn prompt_with(&mut self, line: &str) {
        self.open_prompt(PromptKind::Command);
        if let Mode::Command(prompt) = &mut self.mode {
            prompt.line = line.into();
            prompt.typed = prompt.line.clone();
        }
    }

    /// The send's or out's level under the fx cursor, and the range it can take, in dB; or the
    /// macro's value, or the depth it turns a setting by.
    pub(crate) fn fx_value(&self) -> Option<(f32, f32, f32)> {
        let lane = self.fx_cursor().lane;
        let macro_at = |index: usize| match self.fx_strip() {
            Strip::Track(t) => self.grid.tracks.get(t)?.macros.get(index),
            _ => None,
        };
        match self.fx_line() {
            FxLine::Macro { index } => Some((macro_at(index)?.value, 0.0, 1.0)),
            FxLine::Target { index, target } => {
                Some((macro_at(index)?.targets.get(target)?.depth, -1.0, 1.0))
            }
            FxLine::Out => {
                let out = self.chain(self.fx_strip()).lanes().get(lane)?.out;
                Some((out, MIN_DB, lattice_core::MAX_DB))
            }
            FxLine::Send { to, level, .. } => Some((level, MIN_DB, max_send(lane, to))),
            _ => None,
        }
    }

    /// Sets the send's or out's level under the fx cursor, in dB.
    pub(crate) fn set_fx_value(&mut self, value: f32) -> Result<Vec<Effect>, String> {
        let Some((old, min, max)) = self.fx_value() else {
            return Err("nothing to set here: move onto a setting, a send or an out".into());
        };
        if !(min..=max).contains(&value) {
            return Err(format!("this goes from {min} to {max}"));
        }
        if value == old {
            return Ok(vec![]);
        }
        let line = self.fx_line();
        if let Strip::Track(t) = self.fx_strip() {
            match line {
                FxLine::Macro { index } => return Ok(self.set_macro(t, index, value)),
                FxLine::Target { index, target } => {
                    return Ok(self.set_depth(t, index, target, value));
                }
                _ => {}
            }
        }
        self.checkpoint();
        let lane = self.fx_cursor().lane;
        let chain = self.chain_mut_of(self.fx_strip()).ok_or("no such strip")?;
        match line {
            FxLine::Send { row, .. } => chain.set_send_level(RowAt { lane, row }, value),
            _ => chain.set_out(lane, value),
        }
        Ok(vec![Effect::MixerChanged])
    }

    fn fx_nudge(&mut self, by: i32) -> Vec<Effect> {
        let Some((old, min, max)) = self.fx_value() else {
            return vec![];
        };
        let step = match self.fx_line() {
            FxLine::Macro { .. } | FxLine::Target { .. } => crate::macros::NUDGE,
            _ => LEVEL_STEP,
        };
        let new = (old + by as f32 * step).clamp(min, max);
        self.set_fx_value(new).unwrap_or_default()
    }

    /// Removes the row under the fx cursor: an effect or a send, or on an out, the lane.
    fn fx_delete(&mut self) -> Vec<Effect> {
        let cursor = self.fx_cursor();
        let strip = self.fx_strip();
        let effects = match self.fx_line() {
            FxLine::Out if cursor.lane == 0 => {
                self.message = Some("lane A takes the chain's input, so it stays".into());
                return vec![];
            }
            FxLine::Out => {
                self.checkpoint();
                let removed =
                    (self.chain_mut_of(strip)).map_or(vec![], |c| c.remove_lane(cursor.lane));
                let mut effects = vec![Effect::MixerChanged];
                for index in removed {
                    effects.extend(self.chain_changed(strip, index, false));
                }
                effects.dedup();
                self.message = Some(format!("lane {} removed", FxLane::letter(cursor.lane)));
                effects
            }
            FxLine::Fx { effect, .. } => {
                if self.chain(strip).get(effect).is_none() {
                    self.message = Some("no such effect to remove".into());
                    return vec![];
                }
                self.checkpoint();
                if let Some(chain) = self.chain_mut_of(strip) {
                    chain.remove(effect);
                }
                self.chain_changed(strip, effect, false)
            }
            FxLine::Send { row, .. } => {
                let at = RowAt {
                    lane: cursor.lane,
                    row,
                };
                if !matches!(self.chain(strip).row(at), Some(FxRow::Send { .. })) {
                    self.message = Some("no such send to remove".into());
                    return vec![];
                }
                self.checkpoint();
                if let Some(chain) = self.chain_mut_of(strip) {
                    chain.remove_send(at);
                }
                vec![Effect::MixerChanged]
            }
            FxLine::Param { .. } => {
                self.message = Some("d on the effect's name removes it".into());
                return vec![];
            }
            FxLine::Macro { index } => match strip {
                Strip::Track(t) => self.remove_macro(t, index),
                _ => vec![],
            },
            FxLine::Target { index, target } => match strip {
                Strip::Track(t) => self.remove_target(t, index, target),
                _ => vec![],
            },
            FxLine::Empty => {
                self.message = Some("nothing here to remove".into());
                return vec![];
            }
        };
        self.move_fx(cursor);
        effects
    }

    /// Where a row added on the fx page goes: after the cursor's row, or at the end of the lane
    /// from below it.
    pub(crate) fn fx_insert_at(&self) -> RowAt {
        let lane = self.fx_cursor().lane;
        let row = match self.fx_line().row() {
            Some(row) => row + 1,
            // The cursor's lane is one there is, as `fx_cursor` keeps it so.
            None => (self.chain(self.fx_strip()).lanes().get(lane)).map_or(0, |l| l.rows.len()),
        };
        RowAt { lane, row }
    }

    /// Moves the fx cursor onto the row at `at`, after adding it.
    pub(crate) fn fx_onto(&mut self, at: RowAt) {
        let (lanes, _) = self.fx_page_lines();
        let line = (lanes.get(at.lane))
            .and_then(|lane| lane.iter().position(|l| l.row() == Some(at.row)))
            .unwrap_or(0);
        self.move_fx(FxCursor {
            lane: at.lane,
            line,
            macros: false,
        });
    }

    /// `:fx-send LANE [DB]`: a send into lane `LANE` (a letter) after the fx cursor. The next
    /// letter after the last lane adds a lane.
    pub(crate) fn add_send(&mut self, args: &str) -> Result<Vec<Effect>, String> {
        if self.page != Page::Fx {
            return Err("fx-send works on the fx page (gf)".into());
        }
        let mut words = args.split_whitespace();
        let lane = words.next().unwrap_or("");
        let strip = self.fx_strip();
        let lanes = self.chain(strip).lanes().len();
        let to = match lane.to_ascii_uppercase().as_bytes() {
            &[c @ b'A'..=b'Z'] => usize::from(c - b'A'),
            _ => {
                return Err(format!(
                    "which lane? a letter, A to {}",
                    lane_letters(lanes)
                ));
            }
        };
        if to > lanes || to >= MAX_LANES {
            return Err(format!("which lane? A to {}", lane_letters(lanes)));
        }
        let at = self.fx_insert_at();
        let level = match words.next() {
            Some(word) => word
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("{word:?} isn't a level in dB"))?,
            None if is_feedback(at.lane, to) => FEEDBACK_DB,
            None => FORWARD_DB,
        };
        let most = max_send(at.lane, to);
        if !(MIN_DB..=most).contains(&level) {
            return Err(format!("this send goes from {MIN_DB} to {most} dB"));
        }
        self.checkpoint();
        let chain = self.chain_mut_of(strip).ok_or("no such strip")?;
        if to == lanes {
            chain.add_lane();
        }
        chain.insert_send(at, to, level)?;
        let (from, into) = (FxLane::letter(at.lane), FxLane::letter(to));
        self.message = Some(if is_feedback(at.lane, to) {
            format!("{from} feeds back into {into} at {level} dB")
        } else {
            format!("{from} sends into {into} at {level} dB")
        });
        self.fx_onto(at);
        Ok(vec![Effect::MixerChanged])
    }
}

/// The letters a send can go to: the lanes, and the next for a new one.
fn lane_letters(lanes: usize) -> char {
    FxLane::letter(lanes.min(MAX_LANES - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;
    use lattice_core::{Grid, Lock, Step, Track};

    fn editor() -> Editor {
        let track = Track {
            name: "a".into(),
            steps: vec![Step::default(); 16],
            ..Track::default()
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            beats_per_bar: 4,
            grouping: vec![],
            tracks: vec![track],
            groups: vec![],
        };
        Editor::new(grid, 120.0)
    }

    fn keys(e: &mut Editor, s: &str) -> Vec<Effect> {
        let key = |c| match c {
            'A' => Key::Ctrl('a'),
            'X' => Key::Ctrl('x'),
            '\n' => Key::Enter,
            '\t' => Key::Tab,
            c => Key::Char(c),
        };
        s.chars().flat_map(|c| e.handle(key(c))).collect()
    }

    fn rows(e: &Editor) -> Vec<Vec<FxRow>> {
        let chain = &e.grid.tracks[0].effects;
        chain.lanes().iter().map(|l| l.rows.clone()).collect()
    }

    #[test]
    fn branching_into_lanes_and_back() {
        let mut e = editor();
        keys(&mut e, "gf:fx-add filter\n");
        assert_eq!(e.page, Page::Fx);
        keys(&mut e, ":fx-send B\n");
        assert_eq!(
            rows(&e),
            [vec![FxRow::Fx, FxRow::Send { to: 1, level: 0.0 }], vec![]],
            "a new lane, sent into at unity"
        );
        // Across onto B's empty line, a delay at the top of B, and a send back into A.
        keys(&mut e, "l:fx-add delay\n:fx-send a\n");
        let send = FxLine::Send {
            row: 1,
            to: 0,
            level: -6.0,
        };
        assert_eq!(e.fx_line(), send);
        assert_eq!(e.shown().unwrap().slot, Some(1), "the delay's still shown");
        // Feedback stops at 0 dB.
        keys(&mut e, "20A");
        assert_eq!(rows(&e)[1][1], FxRow::Send { to: 0, level: 0.0 });
        assert!(keys(&mut e, ":set 3\n").is_empty());

        // Its out, and a lock on the delay, which goes with the lane.
        keys(&mut e, "geX");
        assert_eq!(e.fx_line(), FxLine::Out);
        assert_eq!(e.grid.tracks[0].effects.lanes()[1].out, -0.5);
        let lock = Lock {
            fx: Some(1),
            param: 0,
            value: 0.5,
            ..Default::default()
        };
        e.grid.tracks[0].steps[2].locks.set(lock);
        let effects = keys(&mut e, "d");
        assert!(effects.contains(&Effect::TrackChanged(0)), "{effects:?}");
        assert_eq!(rows(&e), [vec![FxRow::Fx]], "the send into B went too");
        assert!(e.grid.tracks[0].steps[2].locks.is_empty());
        assert!(e.grid.tracks[0].effects.is_plain());
        assert_eq!(e.fx_cursor().lane, 0);

        keys(&mut e, "u");
        assert_eq!(e.grid.tracks[0].effects.lanes().len(), 2, "undone");
    }

    #[test]
    fn o_adds_an_effect_after_the_cursors() {
        let mut e = editor();
        keys(&mut e, "gf:fx-add delay\n:fx-add reverb\nbo");
        assert!(matches!(&e.mode, Mode::Command(p) if p.line == "fx-add "));
        keys(&mut e, "filter\n");
        let kinds: Vec<_> = e.grid.tracks[0].effects.iter().map(|f| f.kind).collect();
        use lattice_core::FxKind;
        assert_eq!(kinds, [FxKind::Delay, FxKind::Filter, FxKind::Reverb]);
    }

    #[test]
    fn a_key_with_nothing_to_act_on_says_so() {
        let mut e = editor();
        keys(&mut e, "gfy");
        assert_eq!(e.message.as_deref(), Some("y does nothing on the fx page"));
    }

    #[test]
    fn settings_are_lines_under_their_effect() {
        let mut e = editor();
        keys(&mut e, "gf:fx-add delay\n:fx-add reverb\n");
        assert_eq!(e.fx_line(), FxLine::Fx { row: 1, effect: 1 });
        // b goes back to the delay's name, past its settings.
        keys(&mut e, "b");
        assert_eq!(e.fx_line(), FxLine::Fx { row: 0, effect: 0 });
        keys(&mut e, "j");
        assert_eq!(
            e.fx_line(),
            FxLine::Param {
                row: 0,
                effect: 0,
                param: 0
            }
        );
        let before = e.grid.tracks[0].effects[0].values[0];
        keys(&mut e, "A");
        assert_ne!(
            e.grid.tracks[0].effects[0].values[0], before,
            "turned as passed"
        );
        // w skips the rest of the delay's settings, onto the reverb, then the out.
        keys(&mut e, "w");
        assert_eq!(e.fx_line(), FxLine::Fx { row: 1, effect: 1 });
        keys(&mut e, "w");
        assert_eq!(e.fx_line(), FxLine::Out);
        keys(&mut e, "kd");
        assert_eq!(
            e.grid.tracks[0].effects.len(),
            2,
            "d on a setting doesn't remove"
        );
        assert!(e.add_send("E").is_err());
    }
}
