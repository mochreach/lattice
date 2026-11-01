//! The instrument page: every track's instrument side by side, a column each, read top to
//! bottom like the fx page's lanes. A column starts with the instrument's name, then has its
//! settings a line each, the track's macros, and last its chain of effects.
//!
//! `h` `l` move from track to track on the same line, `j` `k` down and up a column, and `w` `b`
//! from part to part. `C-a` `C-x` turn the setting or macro under the cursor, and `c` sets it,
//! or on the name, changes the instrument. Settings go through the mixer's cursor, which the
//! page keeps on the track and setting under its own, so `:set`, `:map`, locking and recorded
//! locks work as there. `enter` on the chain, or `gf` anywhere, opens the track's fx page.

use crate::keymap::Action;
use crate::mixer::{Page, Row};
use crate::{Editor, Effect, Mode, PromptKind};

/// Where the instrument page's cursor is: a track's column, and a line down it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InstCursor {
    pub track: usize,
    pub line: usize,
}

/// A line of an instrument page's column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstLine {
    /// The instrument's name.
    Kind,
    /// One of its settings, by its place in the instrument's list.
    Param(usize),
    /// One of the track's macros, by its index.
    Macro(usize),
    /// The track's chain of effects.
    Chain,
}

impl InstLine {
    /// Whether `w` and `b` stop on it: the first line of each part.
    fn starts_part(self) -> bool {
        matches!(
            self,
            InstLine::Kind | InstLine::Param(0) | InstLine::Macro(0) | InstLine::Chain
        )
    }
}

impl Editor {
    /// Track `t`'s column on the instrument page, top to bottom.
    pub fn inst_lines(&self, t: usize) -> Vec<InstLine> {
        let track = &self.grid.tracks[t];
        let params = track.instrument.params().len();
        let mut lines = vec![InstLine::Kind];
        lines.extend((0..params).map(InstLine::Param));
        lines.extend((0..track.macros.len()).map(InstLine::Macro));
        lines.push(InstLine::Chain);
        lines
    }

    /// The instrument page's cursor, kept within the tracks and the column it's in.
    pub fn inst_cursor(&self) -> InstCursor {
        let track = self.inst.track.min(self.grid.tracks.len() - 1);
        let line = self.inst.line.min(self.inst_lines(track).len() - 1);
        InstCursor { track, line }
    }

    pub fn inst_line(&self) -> InstLine {
        let cursor = self.inst_cursor();
        self.inst_lines(cursor.track)[cursor.line]
    }

    /// `gi`: the instrument page, on the pattern cursor's track, or the mixer's or fx page's.
    pub(crate) fn goto_instrument(&mut self) {
        let track = match self.page {
            Page::Mixer | Page::Fx => match self.fx_strip() {
                crate::Strip::Track(t) => t,
                _ => self.inst_cursor().track,
            },
            Page::Instrument => self.inst_cursor().track,
            _ => self.cursor().track,
        };
        if track != self.inst.track || self.page != Page::Instrument {
            // Onto its first setting.
            self.inst = InstCursor { track, line: 1 };
        }
        self.page = Page::Instrument;
        self.mode = Mode::Normal;
        self.move_inst(self.inst);
    }

    /// Moves the instrument page's cursor, and the mixer's onto the track and setting under it.
    fn move_inst(&mut self, to: InstCursor) {
        self.inst = to;
        self.inst = self.inst_cursor();
        self.mixer.strip = self.inst.track;
        self.mixer.instrument = true;
        self.mixer.row = match self.inst_line() {
            InstLine::Param(p) => Row::Param(p),
            _ => Row::Chain,
        };
    }

    /// The `n`th line from the cursor's that starts a part, forward or back.
    fn inst_part(&self, n: usize, forward: bool) -> usize {
        let cursor = self.inst_cursor();
        let lines = self.inst_lines(cursor.track);
        let mut line = cursor.line;
        for _ in 0..n {
            let next = if forward {
                (line + 1..lines.len()).find(|&l| lines[l].starts_part())
            } else {
                (0..line).rev().find(|&l| lines[l].starts_part())
            };
            match next {
                Some(l) => line = l,
                None => break,
            }
        }
        line
    }

    /// Handles a key's action on the instrument page. `None` for the actions the pages share.
    pub(crate) fn inst_act(&mut self, action: Action, n: usize) -> Option<Vec<Effect>> {
        let cursor = self.inst_cursor();
        let tracks = self.grid.tracks.len();
        let line = |line| InstCursor { line, ..cursor };
        match action {
            Action::Left => self.move_inst(InstCursor {
                track: cursor.track.saturating_sub(n),
                ..cursor
            }),
            Action::Right => self.move_inst(InstCursor {
                track: (cursor.track + n).min(tracks - 1),
                ..cursor
            }),
            Action::Up => self.move_inst(line(cursor.line.saturating_sub(n))),
            Action::Down => self.move_inst(line(cursor.line + n)),
            Action::NextNote => self.move_inst(line(self.inst_part(n, true))),
            Action::PrevNote => self.move_inst(line(self.inst_part(n, false))),
            Action::FirstStep => self.move_inst(line(0)),
            Action::LastStep => self.move_inst(line(usize::MAX)),
            Action::ToggleView => {}
            Action::NoteUp | Action::Longer | Action::NoteDown | Action::Shorter => {
                let up = matches!(action, Action::NoteUp | Action::Longer);
                let by = if up { n as i32 } else { -(n as i32) };
                return Some(match self.inst_line() {
                    // The mixer nudges settings.
                    InstLine::Param(_) => self.mixer_act(action, n)?,
                    InstLine::Macro(m) => {
                        let value = self.grid.tracks[cursor.track].macros[m].value;
                        let step = crate::macros::NUDGE * by as f32;
                        self.set_macro(cursor.track, m, value + step)
                    }
                    InstLine::Kind => {
                        self.message = Some("c changes the instrument".into());
                        vec![]
                    }
                    InstLine::Chain => {
                        self.message = Some("enter opens the effects (gf)".into());
                        vec![]
                    }
                });
            }
            Action::Change => {
                let line = match self.inst_line() {
                    InstLine::Kind => "inst ",
                    InstLine::Param(_) | InstLine::Macro(_) => "set ",
                    InstLine::Chain => "fx-add ",
                };
                self.open_prompt(PromptKind::Command);
                if let Mode::Command(prompt) = &mut self.mode {
                    prompt.line = line.into();
                }
            }
            Action::Open if self.inst_line() == InstLine::Chain => self.goto_fx(),
            Action::Open => {}
            Action::Delete | Action::Clear => {
                return Some(match self.inst_line() {
                    InstLine::Macro(m) => self.remove_macro(cursor.track, m),
                    _ => {
                        self.message = Some("nothing here to remove".into());
                        vec![]
                    }
                });
            }
            // Locking, muting and the rest work as on the mixer.
            _ => {
                let effects = self.mixer_act(action, n);
                self.move_inst(self.inst);
                return effects;
            }
        }
        Some(vec![])
    }

    /// The macro's value under the instrument page's cursor, for `:set`.
    pub(crate) fn inst_macro(&self) -> Option<(usize, usize)> {
        match self.inst_line() {
            InstLine::Macro(m) if self.page == Page::Instrument => {
                Some((self.inst_cursor().track, m))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;
    use lattice_core::{Grid, InstKind, Step, Track};

    fn editor() -> Editor {
        let track = |name: &str| Track {
            name: name.into(),
            steps: vec![Step::default(); 16],
            ..Track::default()
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![track("a"), track("b")],
            groups: vec![],
        };
        Editor::new(grid, 120.0)
    }

    fn keys(e: &mut Editor, s: &str) -> Vec<Effect> {
        let key = |c| match c {
            'A' => Key::Ctrl('a'),
            'X' => Key::Ctrl('x'),
            '\n' => Key::Enter,
            c => Key::Char(c),
        };
        s.chars().flat_map(|c| e.handle(key(c))).collect()
    }

    #[test]
    fn every_instrument_side_by_side() {
        let mut e = editor();
        // From the pattern cursor's track, onto its first setting.
        keys(&mut e, "lgi");
        assert_eq!(e.page, Page::Instrument);
        assert_eq!(e.inst_cursor(), InstCursor { track: 1, line: 1 });
        assert_eq!(e.inst_line(), InstLine::Param(0));
        assert_eq!(
            e.shown().unwrap().slot,
            None,
            "the mixer's on the instrument"
        );

        // Turned as on the mixer, on this track only.
        keys(&mut e, "jA");
        let values = |e: &Editor, t: usize| e.grid.tracks[t].instrument.values.clone();
        assert_ne!(values(&e, 1), values(&e, 0));
        let changed = values(&e, 1);
        keys(&mut e, "hX");
        assert_ne!(values(&e, 0), changed);
        assert_eq!(e.inst_cursor().line, 2, "across on the same line");

        // c on the name changes the instrument.
        keys(&mut e, "gg");
        assert_eq!(e.inst_line(), InstLine::Kind);
        keys(&mut e, "c");
        keys(&mut e, "kick\n");
        assert_eq!(e.grid.tracks[0].instrument.kind, InstKind::Kick);

        // A macro, under the settings, turned and set.
        keys(&mut e, "j:map bloom\n");
        keys(&mut e, "w");
        assert_eq!(e.inst_line(), InstLine::Macro(0));
        keys(&mut e, "5A");
        assert!((e.grid.tracks[0].macros[0].value - 0.05).abs() < 1e-6);
        keys(&mut e, ":set 0.5\n");
        assert_eq!(e.grid.tracks[0].macros[0].value, 0.5);

        // The chain last, and enter opens it.
        keys(&mut e, "w");
        assert_eq!(e.inst_line(), InstLine::Chain);
        keys(&mut e, "\n");
        assert_eq!(e.page, Page::Fx);
        keys(&mut e, "gi");
        assert_eq!(e.inst_cursor().track, 0, "back on the same track");
    }
}
