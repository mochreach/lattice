//! The instrument page: every track's instrument side by side, a column each, read top to
//! bottom like the fx page's lanes. A column starts with the instrument's name, then has its
//! settings a line each, the track's macros, and last its chain of effects.
//!
//! `i` plays the cursor's track on the piano keys, as insert mode does on the pattern, but
//! without entering the notes, unless record's armed and the song's playing; the arrows still
//! move and `C-a` `C-x` still turn, so a sound can be played and shaped at once. `esc` stops.
//!
//! A track with a preset's face shows that instead: its title, its macros, the settings it puts
//! forward and its chain. `enter` on the title opens all its settings, and again goes back to
//! the face.
//!
//! `h` `l` move from track to track on the same line, `j` `k` down and up a column, and `w` `b`
//! from part to part. `C-a` `C-x` turn the setting or macro under the cursor, and `c` sets it,
//! or on the name, changes the instrument. Settings go through the mixer's cursor, which the
//! page keeps on the track and setting under its own, so `:set`, `:map`, locking and recorded
//! locks work as there. `enter` on the chain, or `gf` anywhere, opens the track's fx page.

use crate::keymap::{Action, Binding};
use crate::mixer::{Page, Row};
use crate::{Editor, Effect, Key, Mode, PromptKind};

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

impl Editor {
    /// Track `t`'s column on the instrument page, top to bottom.
    pub fn inst_lines(&self, t: usize) -> Vec<InstLine> {
        let Some(track) = self.grid.tracks.get(t) else {
            return vec![];
        };
        let macros = (0..track.macros.len()).map(InstLine::Macro);
        let mut lines = vec![InstLine::Kind];
        match &track.face {
            Some(face) if self.shows_face(t) => {
                lines.extend(macros);
                lines.extend(face.controls.iter().map(|&c| InstLine::Param(c.into())));
            }
            _ => {
                lines.extend((0..track.instrument.params().len()).map(InstLine::Param));
                lines.extend(macros);
            }
        }
        lines.push(InstLine::Chain);
        lines
    }

    /// Whether track `t`'s column shows its face, rather than all its settings.
    pub fn shows_face(&self, t: usize) -> bool {
        let opened = self
            .ids
            .get(t)
            .is_some_and(|id| self.inst_opened.contains(id));
        self.grid
            .tracks
            .get(t)
            .is_some_and(|track| track.face.is_some())
            && !opened
    }

    /// `enter` on the title: opens all of a face's settings, or goes back to the face.
    fn toggle_face(&mut self, t: usize) {
        let (Some(track), Some(&id)) = (self.grid.tracks.get(t), self.ids.get(t)) else {
            return;
        };
        if track.face.is_none() {
            self.message = Some("no face: :preset or :face gives it one".into());
            return;
        }
        if !self.inst_opened.remove(&id) {
            self.inst_opened.insert(id);
        }
    }

    /// The instrument page's cursor, kept within the tracks and the column it's in.
    pub fn inst_cursor(&self) -> InstCursor {
        let track = self
            .inst
            .track
            .min(self.grid.tracks.len().saturating_sub(1));
        let line = (self.inst.line).min(self.inst_lines(track).len().saturating_sub(1));
        InstCursor { track, line }
    }

    /// The line under the instrument page's cursor; `None` only if there's no track.
    pub fn inst_line(&self) -> Option<InstLine> {
        let cursor = self.inst_cursor();
        self.inst_lines(cursor.track).get(cursor.line).copied()
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
            Some(InstLine::Param(p)) => Row::Param(p),
            _ => Row::Chain,
        };
    }

    /// The track notes played live go to: the instrument page's, or the pattern cursor's.
    pub(crate) fn played_track(&self) -> usize {
        match self.page {
            Page::Instrument => self.inst_cursor().track,
            _ => self.cursor().track,
        }
    }

    /// Starts playing the cursor's track on the piano keys.
    pub(crate) fn start_playing(&mut self) {
        self.mode = Mode::Insert;
        let Some(track) = self.grid.tracks.get(self.inst_cursor().track) else {
            return;
        };
        let name = &track.name;
        self.message = Some(format!("playing {name} on the keyboard; esc stops"));
    }

    /// A key while playing on the instrument page: a piano key plays the cursor's track, or
    /// records it at the playhead; the rest move and turn, with `C-a` `C-x` as in normal mode.
    pub(crate) fn play_key(&mut self, key: Key) -> Vec<Effect> {
        let track = self.inst_cursor().track;
        if let Some(pitch) = self.piano_pitch(key) {
            return (self.record_note(key, &[track], pitch, 0))
                .unwrap_or_else(|| vec![Effect::Preview { track, pitch }]);
        }
        let action = self.keymap.insert.get(&key).copied().or_else(|| {
            match (key, self.keymap.normal.get(&key)) {
                (Key::Ctrl(_), Some(&Binding::Action(action))) => Some(action),
                _ => None,
            }
        });
        match action {
            // Nothing to clear: no notes went in.
            Some(Action::ClearAdvance) | None => vec![],
            Some(action) => self.act(action, 1),
        }
    }

    /// The `n`th line from the cursor's that starts a part, forward or back.
    fn inst_part(&self, n: usize, forward: bool) -> usize {
        let cursor = self.inst_cursor();
        let lines = self.inst_lines(cursor.track);
        // A part starts where the kind of line changes.
        let starts = |l: usize| match l.checked_sub(1) {
            Some(before) => {
                lines.get(l).map(std::mem::discriminant)
                    != lines.get(before).map(std::mem::discriminant)
            }
            None => true,
        };
        let mut line = cursor.line;
        for _ in 0..n {
            let next = if forward {
                (line + 1..lines.len()).find(|&l| starts(l))
            } else {
                (0..line).rev().find(|&l| starts(l))
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
                track: (cursor.track + n).min(tracks.saturating_sub(1)),
                ..cursor
            }),
            Action::Up => self.move_inst(line(cursor.line.saturating_sub(n))),
            Action::Down => self.move_inst(line(cursor.line + n)),
            Action::NextNote => self.move_inst(line(self.inst_part(n, true))),
            Action::PrevNote => self.move_inst(line(self.inst_part(n, false))),
            Action::FirstStep => self.move_inst(line(0)),
            Action::LastStep => self.move_inst(line(usize::MAX)),
            Action::ToggleView => {}
            Action::Insert | Action::Append => self.start_playing(),
            Action::NoteUp | Action::Longer | Action::NoteDown | Action::Shorter => {
                let up = matches!(action, Action::NoteUp | Action::Longer);
                let by = if up { n as i32 } else { -(n as i32) };
                return Some(match self.inst_line() {
                    // The mixer nudges settings.
                    Some(InstLine::Param(_)) => self.mixer_act(action, n)?,
                    Some(InstLine::Macro(m)) => {
                        let Some(value) = (self.grid.tracks.get(cursor.track))
                            .and_then(|track| track.macros.get(m))
                            .map(|m| m.value)
                        else {
                            return Some(vec![]);
                        };
                        let step = crate::macros::NUDGE * by as f32;
                        self.set_macro(cursor.track, m, value + step)
                    }
                    Some(InstLine::Kind) => {
                        self.message = Some("c changes the instrument".into());
                        vec![]
                    }
                    Some(InstLine::Chain) => {
                        self.message = Some("enter opens the effects (gf)".into());
                        vec![]
                    }
                    None => vec![],
                });
            }
            Action::Change => {
                let Some(line) = self.inst_line() else {
                    return Some(vec![]);
                };
                let line = match line {
                    InstLine::Kind => "inst ",
                    InstLine::Param(_) | InstLine::Macro(_) => "set ",
                    InstLine::Chain => "fx-add ",
                };
                self.open_prompt(PromptKind::Command);
                if let Mode::Command(prompt) = &mut self.mode {
                    prompt.line = line.into();
                }
            }
            Action::Open => match self.inst_line() {
                Some(InstLine::Chain) => self.goto_fx(),
                Some(InstLine::Kind) => self.toggle_face(cursor.track),
                _ => {}
            },
            Action::Delete | Action::Clear => {
                return Some(match self.inst_line() {
                    Some(InstLine::Macro(m)) => self.remove_macro(cursor.track, m),
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
            Some(InstLine::Macro(m)) if self.page == Page::Instrument => {
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
        assert_eq!(e.inst_line(), Some(InstLine::Param(0)));
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
        assert_eq!(e.inst_line(), Some(InstLine::Kind));
        keys(&mut e, "c");
        keys(&mut e, "kick\n");
        assert_eq!(e.grid.tracks[0].instrument.kind, InstKind::Kick);

        // A macro, under the settings, turned and set.
        keys(&mut e, "j:map bloom\n");
        keys(&mut e, "w");
        assert_eq!(e.inst_line(), Some(InstLine::Macro(0)));
        keys(&mut e, "5A");
        assert!((e.grid.tracks[0].macros[0].value - 0.05).abs() < 1e-6);
        keys(&mut e, ":set 0.5\n");
        assert_eq!(e.grid.tracks[0].macros[0].value, 0.5);

        // The chain last, and enter opens it.
        keys(&mut e, "w");
        assert_eq!(e.inst_line(), Some(InstLine::Chain));
        keys(&mut e, "\n");
        assert_eq!(e.page, Page::Fx);
        keys(&mut e, "gi");
        assert_eq!(e.inst_cursor().track, 0, "back on the same track");
    }

    #[test]
    fn i_plays_the_track_on_the_keyboard() {
        let mut e = editor();
        keys(&mut e, "gilj");
        let before = e.grid.tracks.clone();
        keys(&mut e, "i");
        assert_eq!(e.mode, Mode::Insert);
        let pitch = (u16::from(e.octave) + 1) * 12;
        let played = keys(&mut e, "z");
        assert_eq!(
            played,
            [Effect::Preview {
                track: 1,
                pitch: pitch as u8
            }]
        );
        assert_eq!(e.grid.tracks, before, "heard, not entered");

        // The arrows move and C-a turns, while the letters stay notes.
        e.handle(Key::Left);
        assert_eq!(e.inst_cursor().track, 0);
        let value = e.grid.tracks[0].instrument.values[1];
        keys(&mut e, "A");
        assert_ne!(e.grid.tracks[0].instrument.values[1], value);
        assert!(matches!(
            &keys(&mut e, "z")[..],
            [Effect::Preview { track: 0, .. }]
        ));
        e.handle(Key::Backspace);
        assert_eq!(e.grid.tracks[0].steps, before[0].steps, "nothing to clear");
        e.handle(Key::Esc);
        assert_eq!(e.mode, Mode::Normal);
        assert_eq!(e.page, Page::Instrument);

        // Armed while the song plays, the notes go in at the playhead.
        keys(&mut e, "R");
        assert_eq!(e.mode, Mode::Insert, "arming starts playing");
        e.set_transport(Some(crate::record::Transport {
            positions: vec![Some((4, 0)); 2],
            into_step: 0.0,
            count_in: 0,
        }));
        keys(&mut e, "z");
        assert!(e.grid.tracks[0].steps[4].note.is_some());
    }

    #[test]
    fn a_face_shows_what_it_puts_forward_until_opened() {
        let mut e = editor();
        keys(&mut e, "gi:preset rubber\n");
        assert!(e.shows_face(0));
        let face = [
            InstLine::Kind,
            InstLine::Macro(0),
            InstLine::Macro(1),
            InstLine::Param(2),
            InstLine::Chain,
        ];
        assert_eq!(e.inst_lines(0), face);
        assert_eq!(
            e.inst_line(),
            Some(InstLine::Macro(0)),
            "onto the first macro"
        );
        keys(&mut e, "w");
        assert_eq!(
            e.inst_line(),
            Some(InstLine::Param(2)),
            "w stops on the settings"
        );
        keys(&mut e, "A");
        let decay = e.grid.tracks[0].instrument.values[2];
        assert!(decay > 0.25, "turned as any setting");

        // enter on the title opens them all, and again goes back.
        keys(&mut e, "gg\n");
        assert!(!e.shows_face(0));
        assert_eq!(e.inst_lines(0).len(), 1 + 9 + 2 + 1);
        keys(&mut e, "\n");
        assert_eq!(e.inst_lines(0), face);
        keys(&mut e, "l\n");
        assert!(e.message.as_deref().unwrap().contains("no face"));
    }
}
