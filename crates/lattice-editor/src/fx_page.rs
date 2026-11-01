//! The fx page: one strip's chain of effects, drawn like the pattern. Its lanes are columns side
//! by side, read top to bottom, and each row is an effect or a send that copies the sound into
//! another lane (into its own or one to its left, it's feedback, heard a block later). Each lane
//! ends in its `out`, the level it goes into the chain's output at.
//!
//! It shares the mixer's cursor for the strip and the effect whose settings are shown, so moving
//! onto an effect here shows its settings below, as on the mixer, and `tab` goes down into them:
//! nudging, `:set`, locking and recording locks all work as they do there. On the lanes, `h` `l`
//! change lane and `j` `k` row, `C-a` `C-x` nudge a send or out, `d` removes the row, or on an
//! out the whole lane, and `:fx-add` and `:fx-send` add rows after the cursor.

use lattice_core::{FxChain, FxLane, FxRow, MAX_LANES, MIN_DB, RowAt, is_feedback, max_send};

use crate::keymap::Action;
use crate::mixer::{PARAM_COLUMNS, Page, Row, Strip};
use crate::{Editor, Effect, Mode, PromptKind};

/// What a nudge moves a send or an out by, in dB.
const LEVEL_STEP: f32 = 0.5;
/// A new send's level into a lane that hasn't run yet, and back as feedback, in dB.
const FORWARD_DB: f32 = 0.0;
const FEEDBACK_DB: f32 = -6.0;

/// Where the fx page's cursor is on the lanes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FxCursor {
    pub lane: usize,
    /// The row in the lane; past its last, the lane's out.
    pub row: usize,
    /// On the lane's out, which it stays on when changing lane.
    pub out: bool,
}

impl FxCursor {
    pub fn at(self) -> RowAt {
        RowAt {
            lane: self.lane,
            row: self.row,
        }
    }
}

impl Editor {
    /// The chain the fx page shows: the mixer cursor's strip's.
    pub fn fx_strip(&self) -> Strip {
        self.strip(self.mixer_cursor().strip)
    }

    /// The fx page's cursor, kept within the lanes and rows there are now.
    pub fn fx_cursor(&self) -> FxCursor {
        let chain = self.chain(self.fx_strip());
        let lane = self.fx.lane.min(chain.lanes().len() - 1);
        let len = chain.lanes()[lane].rows.len();
        let out = self.fx.out || self.fx.row >= len;
        FxCursor {
            lane,
            row: if out { len } else { self.fx.row },
            out,
        }
    }

    /// Whether the fx page's cursor is down in the settings rather than on the lanes.
    pub fn in_fx_settings(&self) -> bool {
        matches!(self.mixer_cursor().row, Row::Param(_))
    }

    /// `gf`: opens the fx page, on the pattern cursor's track if coming from the pattern.
    pub(crate) fn goto_fx(&mut self) {
        if self.page == Page::Pattern {
            self.mixer.strip = self.cursor().track;
            self.fx = FxCursor::default();
        }
        self.page = Page::Fx;
        self.mode = Mode::Normal;
        self.mixer.row = Row::Chain;
        self.show_fx_row();
    }

    /// Moves the fx cursor, keeping it within the lanes, and shows the effect it's on.
    fn move_fx(&mut self, to: FxCursor) {
        self.fx = to;
        self.fx = self.fx_cursor();
        self.show_fx_row();
    }

    /// Shows the settings of the effect under the fx cursor, if it's on one.
    fn show_fx_row(&mut self) {
        let cursor = self.fx_cursor();
        let chain = self.chain(self.fx_strip());
        let row = chain.rows().find(|(at, _, _)| *at == cursor.at());
        if let Some((_, _, Some(effect))) = row {
            self.mixer.effect = effect;
            self.mixer.instrument = false;
        }
    }

    /// Handles a key's action on the fx page. `None` for the actions the pages share.
    pub(crate) fn fx_act(&mut self, action: Action, n: usize) -> Option<Vec<Effect>> {
        if self.in_fx_settings() {
            return self.fx_settings_act(action, n);
        }
        let cursor = self.fx_cursor();
        let lanes = self.chain(self.fx_strip()).lanes().len();
        let by = n as i32;
        match action {
            Action::Left => self.move_fx(FxCursor {
                lane: cursor.lane.saturating_sub(n),
                ..cursor
            }),
            Action::Right => self.move_fx(FxCursor {
                lane: (cursor.lane + n).min(lanes - 1),
                ..cursor
            }),
            Action::Up => self.move_fx(FxCursor {
                row: cursor.row.saturating_sub(n),
                out: false,
                ..cursor
            }),
            Action::Down if cursor.out => self.enter_fx_settings(),
            Action::Down => self.move_fx(FxCursor {
                row: cursor.row + n,
                out: false,
                ..cursor
            }),
            Action::FirstStep => self.move_fx(FxCursor {
                row: 0,
                out: false,
                ..cursor
            }),
            Action::LastStep => self.move_fx(FxCursor {
                out: true,
                ..cursor
            }),
            Action::ToggleView => self.enter_fx_settings(),
            Action::NoteUp | Action::Longer => return Some(self.fx_nudge(by)),
            Action::NoteDown | Action::Shorter => return Some(self.fx_nudge(-by)),
            Action::Delete | Action::Clear => return Some(self.fx_delete()),
            Action::Change => {
                let line = match self.fx_value() {
                    Some(_) => "set ",
                    None => "fx-add ",
                };
                self.open_prompt(PromptKind::Command);
                if let Mode::Command(prompt) = &mut self.mode {
                    prompt.line = line.into();
                    prompt.typed = prompt.line.clone();
                }
            }
            // The strip, locking and the rest work as on the mixer.
            _ => {
                let effects = self.mixer_act(action, n);
                self.mixer.row = Row::Chain;
                return effects;
            }
        }
        Some(vec![])
    }

    /// Down into the settings of the effect last on, if there is one.
    fn enter_fx_settings(&mut self) {
        match self.shown() {
            Some(shown) if shown.slot.is_some() => self.mixer.row = Row::Param(0),
            _ => self.message = Some("no effect to show the settings of".into()),
        }
    }

    /// In the settings, keys work as on the mixer, but up from the top row and `tab` go back
    /// to the lanes.
    fn fx_settings_act(&mut self, action: Action, n: usize) -> Option<Vec<Effect>> {
        match (action, self.mixer_cursor().row) {
            (Action::ToggleView, _) => self.mixer.row = Row::Chain,
            (Action::Up, Row::Param(p)) if p < PARAM_COLUMNS => self.mixer.row = Row::Chain,
            (Action::FirstStep, _) => self.mixer.row = Row::Chain,
            _ => {
                let effects = self.mixer_act(action, n);
                // The mixer's rows above the settings aren't on this page.
                if !self.in_fx_settings() {
                    self.mixer.row = Row::Chain;
                }
                return effects;
            }
        }
        Some(vec![])
    }

    /// The send's or out's level under the fx cursor, and the range it can take, in dB.
    pub(crate) fn fx_value(&self) -> Option<(f32, f32, f32)> {
        let cursor = self.fx_cursor();
        let chain = self.chain(self.fx_strip());
        if cursor.out {
            let out = chain.lanes()[cursor.lane].out;
            return Some((out, MIN_DB, lattice_core::MAX_DB));
        }
        match chain.row(cursor.at())? {
            FxRow::Send { to, level } => Some((level, MIN_DB, max_send(cursor.lane, to))),
            FxRow::Fx => None,
        }
    }

    /// Sets the send's or out's level under the fx cursor, in dB.
    pub(crate) fn set_fx_value(&mut self, value: f32) -> Result<Vec<Effect>, String> {
        let Some((old, min, max)) = self.fx_value() else {
            return Err("nothing to set here: move onto a send, an out or a setting".into());
        };
        if !(min..=max).contains(&value) {
            return Err(format!("this goes from {min} to {max}"));
        }
        if value == old {
            return Ok(vec![]);
        }
        self.checkpoint();
        let cursor = self.fx_cursor();
        let chain = self.chain_mut(self.fx_strip());
        if cursor.out {
            chain.set_out(cursor.lane, value);
        } else {
            chain.set_send_level(cursor.at(), value);
        }
        Ok(vec![Effect::MixerChanged])
    }

    fn fx_nudge(&mut self, by: i32) -> Vec<Effect> {
        let Some((old, min, max)) = self.fx_value() else {
            return vec![];
        };
        let new = (old + by as f32 * LEVEL_STEP).clamp(min, max);
        self.set_fx_value(new).unwrap_or_default()
    }

    /// Removes the row under the fx cursor: an effect or a send, or on an out, the lane.
    fn fx_delete(&mut self) -> Vec<Effect> {
        let cursor = self.fx_cursor();
        let strip = self.fx_strip();
        let chain = self.chain(strip);
        if cursor.out {
            if cursor.lane == 0 {
                self.message = Some("lane A takes the chain's input, so it stays".into());
                return vec![];
            }
            self.checkpoint();
            let removed = self.chain_mut(strip).remove_lane(cursor.lane);
            let mut effects = vec![Effect::MixerChanged];
            for index in removed {
                effects.extend(self.chain_changed(strip, index, false));
            }
            effects.dedup();
            self.message = Some(format!("lane {} removed", FxLane::letter(cursor.lane)));
            self.move_fx(cursor);
            return effects;
        }
        let effect = chain
            .rows()
            .find(|(at, _, _)| *at == cursor.at())
            .map(|(_, _, effect)| effect);
        let effects = match effect {
            None => {
                self.message = Some("nothing here to remove".into());
                return vec![];
            }
            Some(Some(index)) => {
                self.checkpoint();
                self.chain_mut(strip).remove(index);
                self.chain_changed(strip, index, false)
            }
            Some(None) => {
                self.checkpoint();
                self.chain_mut(strip).remove_send(cursor.at());
                vec![Effect::MixerChanged]
            }
        };
        self.move_fx(cursor);
        effects
    }

    /// Where a row added on the fx page goes: after the cursor's, or at the end of the lane
    /// from its out.
    pub(crate) fn fx_insert_at(&self) -> RowAt {
        let cursor = self.fx_cursor();
        let row = if cursor.out {
            cursor.row
        } else {
            cursor.row + 1
        };
        RowAt {
            lane: cursor.lane,
            row,
        }
    }

    /// Moves the fx cursor onto the row at `at`, after adding it.
    pub(crate) fn fx_onto(&mut self, at: RowAt) {
        self.move_fx(FxCursor {
            lane: at.lane,
            row: at.row,
            out: false,
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
        let chain: &mut FxChain = self.chain_mut(strip);
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
        // Onto B's out, then a delay at the end of B, and a send back into A.
        keys(&mut e, "l:fx-add delay\n:fx-send a\n");
        assert_eq!(e.fx_cursor().at(), RowAt { lane: 1, row: 1 });
        assert_eq!(rows(&e)[1][1], FxRow::Send { to: 0, level: -6.0 });
        assert_eq!(e.shown().unwrap().slot, Some(1), "the delay's still shown");
        // Feedback stops at 0 dB.
        keys(&mut e, "20A");
        assert_eq!(rows(&e)[1][1], FxRow::Send { to: 0, level: 0.0 });
        assert!(keys(&mut e, ":set 3\n").is_empty());

        // Its out, and a lock on the delay, which goes with the lane.
        keys(&mut e, "jX");
        assert_eq!(e.grid.tracks[0].effects.lanes()[1].out, -0.5);
        let lock = Lock {
            fx: Some(1),
            param: 0,
            value: 0.5,
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
    fn the_settings_are_below_the_lanes() {
        let mut e = editor();
        keys(&mut e, "gf:fx-add delay\n\t");
        assert!(e.in_fx_settings());
        let before = e.grid.tracks[0].effects[0].values[0];
        keys(&mut e, "A");
        assert_ne!(e.grid.tracks[0].effects[0].values[0], before);
        keys(&mut e, "k");
        assert!(
            !e.in_fx_settings(),
            "up from the top row, back to the lanes"
        );
        assert!(e.add_send("E").is_err());
    }
}
