//! The fields of an open track: each step's note, its velocity, and a value in each of the
//! track's lock columns. `h` `l` go field by field, `f` jumps to one, and typing, `^a` `^x`, `d`,
//! `y` `p` and `&` work on the field the cursor's in, on every step the selections reach.

use lattice_core::{Change, Lock, ParamInfo, Step, Track};

use crate::keymap::Key;
use crate::{Editor, Effect, Mode, Page, changed, locks};

/// Which of a step's fields the cursor is in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StepField {
    #[default]
    Note,
    Velocity,
    /// A lock column, by its place among the track's.
    Lock(usize),
}

/// What a field holds on a step, as its column shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Value {
    /// From 00 to 99, across the setting's range or a note's velocities; sliding there, or not.
    Number(u8, bool),
    /// `--`: the setting back to the mixer's.
    Reset,
}

/// What's being typed into a field in insert mode: `~`, then the first digit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Typing {
    slide: bool,
    digit: Option<u8>,
}

/// A velocity, 1 to 127, as 00 to 99.
fn velocity_number(velocity: u8) -> u8 {
    (u32::from(velocity) * 99).div_ceil(127) as u8
}

/// 00 to 99 as a velocity: 99 is the loudest, 127, and 00 the quietest that sounds.
fn number_velocity(number: u8) -> u8 {
    ((u32::from(number.min(99)) * 127 + 49) / 99).max(1) as u8
}

/// A setting's value as 00 to 99, along its curve.
fn number(info: &ParamInfo, value: f32) -> u8 {
    (info.fraction(value).clamp(0.0, 1.0) * 99.0).round() as u8
}

/// What `field` holds on `step` of `track`. `None` for nothing there.
fn read(track: &Track, step: &Step, field: StepField) -> Option<Value> {
    match field {
        StepField::Note => None,
        StepField::Velocity => step
            .note
            .map(|n| Value::Number(velocity_number(n.velocity), false)),
        StepField::Lock(i) => {
            let column = *track.lock_columns().get(i)?;
            let lock = step.locks.iter().find(|l| l.column() == column)?;
            let info = locks::column_info(track, column)?;
            Some(match lock.change {
                Change::Reset => Value::Reset,
                change => Value::Number(number(info, lock.value), change == Change::Slide),
            })
        }
    }
}

/// Writes `value` into `field` on the step `at` of track `t`, or clears it. A velocity only goes
/// on a note, and clearing one puts it back to `default`.
fn write(track: &mut Track, at: usize, field: StepField, value: Option<Value>, default: u8) {
    match field {
        StepField::Note => {}
        StepField::Velocity => {
            let Some(note) = track.steps.get_mut(at).and_then(|s| s.note.as_mut()) else {
                return;
            };
            note.velocity = match value {
                Some(Value::Number(n, _)) => number_velocity(n),
                _ => default,
            };
        }
        StepField::Lock(i) => {
            let Some(&column) = track.lock_columns().get(i) else {
                return;
            };
            let Some(info) = locks::column_info(track, column) else {
                return;
            };
            let lock = match value {
                None => None,
                Some(Value::Reset) => Some(column.lock(info.min, Change::Reset)),
                Some(Value::Number(n, slide)) => {
                    let change = if slide { Change::Slide } else { Change::Set };
                    Some(column.lock(info.at(f32::from(n.min(99)) / 99.0), change))
                }
            };
            // The column's the track's already, as it was found among them.
            track.add_column(column);
            let Some(step) = track.steps.get_mut(at) else {
                return;
            };
            match lock {
                Some(lock) => {
                    step.locks.set(lock);
                }
                None => step.locks.retain(|l: &mut Lock| l.column() != column),
            }
        }
    }
}

impl Editor {
    /// The fields of track `t`, in order: just its notes if its group's folded.
    fn fields_of(&self, t: usize) -> Vec<StepField> {
        let folded = (self.grid.group_of(t))
            .and_then(|g| self.grid.groups.get(g))
            .is_some_and(|g| g.folded);
        if folded {
            return vec![StepField::Note];
        }
        let columns = self.lock_columns(t).len();
        [StepField::Note, StepField::Velocity]
            .into_iter()
            .chain((0..columns).map(StepField::Lock))
            .collect()
    }

    /// The field the cursor's in, on the pattern page: its note unless it's moved to another.
    pub fn field(&self) -> StepField {
        let fields = self.fields_of(self.cursor().track);
        match self.field {
            field if self.page == Page::Pattern && fields.contains(&field) => field,
            _ => StepField::Note,
        }
    }

    /// The field edits go to, if not the notes: only while every selection is on the cursor's
    /// track. Across tracks, edits take whole steps.
    pub(crate) fn on_field(&self) -> Option<StepField> {
        let track = self.cursor().track;
        let one_track = (self.selections.iter())
            .all(|s| s.region().first_track == track && s.region().last_track == track);
        Some(self.field()).filter(|&f| f != StepField::Note && one_track)
    }

    /// `h` `l`: `n` fields left or right through the open track, then on into the next track's,
    /// as each opens when the cursor gets there.
    pub(crate) fn step_field(&mut self, right: bool, n: usize) {
        for _ in 0..n {
            let track = self.cursor().track;
            let fields = self.fields_of(track);
            let at = fields.iter().position(|&f| f == self.field()).unwrap_or(0);
            let next = if right {
                at.checked_add(1)
            } else {
                at.checked_sub(1)
            };
            if let Some(&field) = next.and_then(|i| fields.get(i)) {
                self.field = field;
                continue;
            }
            let by = if right { 1 } else { -1 };
            self.motion(|e, c| e.offset(c, by, 0));
            let now = self.cursor().track;
            if now != track {
                self.field = match right {
                    true => StepField::Note,
                    false => self.fields_of(now).last().copied().unwrap_or_default(),
                };
            }
        }
    }

    /// `f` then a key: to the note (`n`), the velocity (`v`), or a lock column by the letter its
    /// header underlines.
    pub(crate) fn goto_field(&mut self, key: Key) {
        let Key::Char(c) = key else {
            return;
        };
        let t = self.cursor().track;
        let field = match c {
            'n' => Some(StepField::Note),
            'v' => Some(StepField::Velocity),
            c => (self.lock_columns(t).iter())
                .position(|label| label.key == c)
                .map(StepField::Lock),
        };
        match field.filter(|f| self.fields_of(t).contains(f)) {
            Some(field) => self.field = field,
            None => self.message = Some(format!("{c} isn't a field on this track")),
        }
    }

    /// Plays a key typed into a field in insert mode: two digits set a value and move on, `~`
    /// first makes it slide, and `-` puts the setting back. `None` for a key that isn't one.
    pub(crate) fn type_field(&mut self, key: Key, field: StepField) -> Option<Vec<Effect>> {
        let Key::Char(c) = key else {
            self.typing = Typing::default();
            return None;
        };
        let value = match c {
            '~' if matches!(field, StepField::Lock(_)) => {
                self.typing.slide = true;
                self.message = Some("~ slides there: type the value".into());
                return Some(vec![]);
            }
            '-' if matches!(field, StepField::Lock(_)) => Value::Reset,
            '0'..='9' => {
                let digit = c as u8 - b'0';
                match self.typing.digit {
                    None => {
                        self.typing.digit = Some(digit);
                        let slide = if self.typing.slide { "~" } else { "" };
                        self.message = Some(format!("{slide}{digit}_"));
                        return Some(vec![]);
                    }
                    Some(first) => Value::Number(first * 10 + digit, self.typing.slide),
                }
            }
            _ => return Some(vec![]),
        };
        self.typing = Typing::default();
        self.checkpoint();
        let cursors: Vec<_> = self.selections.iter().map(|s| s.cursor).collect();
        let default = self.velocity;
        for c in &cursors {
            if let Some(track) = self.grid.tracks.get_mut(c.track) {
                write(track, c.step, field, Some(value), default);
            }
        }
        self.advance();
        Some(changed(cursors.iter().map(|c| c.track)))
    }

    /// Changes `field`'s value on every step the selections reach that has one.
    fn map_field(&mut self, field: StepField, f: impl Fn(Value) -> Option<Value>) -> Vec<Effect> {
        self.checkpoint();
        let columns: Vec<_> = (self.selections.iter())
            .flat_map(|s| self.columns(s.region()))
            .collect();
        let default = self.velocity;
        for (t, steps) in &columns {
            let Some(track) = self.grid.tracks.get_mut(*t) else {
                continue;
            };
            for at in steps.clone() {
                let Some(step) = track.steps.get(at) else {
                    continue;
                };
                if let Some(value) = read(track, step, field) {
                    write(track, at, field, f(value), default);
                }
            }
        }
        changed(columns.into_iter().map(|(t, _)| t))
    }

    /// `^a` `^x` on a field: each value up or down by `by`, out of 99.
    pub(crate) fn nudge_field(&mut self, field: StepField, by: i32) -> Vec<Effect> {
        self.map_field(field, |value| {
            Some(match value {
                Value::Number(n, slide) => {
                    Value::Number((i32::from(n) + by).clamp(0, 99) as u8, slide)
                }
                Value::Reset => Value::Reset,
            })
        })
    }

    /// `d` on a field: clears its values, a velocity back to the one notes go in at.
    pub(crate) fn clear_field(&mut self, field: StepField) -> Vec<Effect> {
        self.map_field(field, |_| None)
    }

    /// `y` on a field: copies its values, step by step, from each selection.
    pub(crate) fn yank_field(&mut self, field: StepField) {
        let mut clips = vec![];
        for s in &self.selections {
            let region = s.region();
            let Some(track) = self.grid.tracks.get(region.first_track) else {
                continue;
            };
            let values = (region.first_step..=region.last_step)
                .filter_map(|at| track.steps.get(at))
                .map(|step| read(track, step, field))
                .collect();
            clips.push(values);
        }
        let steps = clips.iter().map(Vec::len).sum::<usize>();
        self.field_clipboard = clips;
        self.message = Some(format!("copied {steps} values"));
    }

    /// `p` on a field: writes the values copied from a field down from each selection's first
    /// step, `n` times, as `p` does steps.
    pub(crate) fn paste_field(&mut self, field: StepField, n: usize) -> Vec<Effect> {
        if self.field_clipboard.iter().all(Vec::is_empty) {
            self.message = Some("no values copied: y in a field copies them".into());
            return vec![];
        }
        self.checkpoint();
        let default = self.velocity;
        let mut tracks = vec![];
        for (i, s) in self.selections.clone().iter().enumerate() {
            let region = s.region();
            let Some(clip) = (self.field_clipboard.get(i))
                .or(self.field_clipboard.last())
                .cloned()
            else {
                continue;
            };
            let Some(track) = self.grid.tracks.get_mut(region.first_track) else {
                continue;
            };
            let len = track.steps.len();
            let values = clip.iter().cycle().take(clip.len() * n);
            for (at, &value) in (region.first_step..len).zip(values) {
                write(track, at, field, value, default);
            }
            tracks.push(region.first_track);
        }
        changed(tracks)
    }

    /// `&` on a field: fills each selection evenly between the values at its ends, which both
    /// need one. Velocities go only on notes.
    pub(crate) fn fill_field(&mut self) -> Vec<Effect> {
        let Some(field) = self.on_field() else {
            self.message = Some("& fills a field: go to one with l or f".into());
            return vec![];
        };
        let mut fills = vec![];
        for s in &self.selections {
            let region = s.region();
            let Some(track) = self.grid.tracks.get(region.first_track) else {
                continue;
            };
            let end = |at: usize| match track.steps.get(at).and_then(|s| read(track, s, field)) {
                Some(Value::Number(n, _)) => Some(n),
                _ => None,
            };
            let (Some(from), Some(to)) = (end(region.first_step), end(region.last_step)) else {
                self.message = Some("& needs a value at each end of the selection".into());
                return vec![];
            };
            fills.push((region, from, to));
        }
        self.checkpoint();
        let default = self.velocity;
        for &(region, from, to) in &fills {
            let Some(track) = self.grid.tracks.get_mut(region.first_track) else {
                continue;
            };
            let span = (region.last_step - region.first_step).max(1) as f32;
            for at in region.first_step..=region.last_step {
                let along = (at - region.first_step) as f32 / span;
                let n = f32::from(from) + (f32::from(to) - f32::from(from)) * along;
                write(
                    track,
                    at,
                    field,
                    Some(Value::Number(n.round() as u8, false)),
                    default,
                );
            }
        }
        changed(fills.iter().map(|(r, ..)| r.first_track))
    }

    /// Whether insert mode's keys go to a field, rather than playing notes.
    pub(crate) fn typing_field(&self) -> bool {
        self.mode == Mode::Insert && self.on_field().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Key;
    use lattice_core::{Grid, Track};

    fn editor() -> Editor {
        let track = Track {
            name: "lead".into(),
            steps: vec![Step::note(60); 8],
            ..Track::default()
        };
        let mut e = Editor::new(
            Grid {
                name: "fields".into(),
                steps_per_beat: 4,
                tracks: vec![track.clone(), track],
                groups: vec![],
            },
            120.0,
        );
        crate::command::run(&mut e, "lock cutoff").unwrap();
        e
    }

    fn keys(e: &mut Editor, keys: &str) {
        for c in keys.chars() {
            e.handle(match c {
                '\x1b' => Key::Esc,
                c => Key::Char(c),
            });
        }
    }

    fn column(e: &Editor, t: usize) -> Vec<Option<Value>> {
        let track = &e.grid.tracks[t];
        track
            .steps
            .iter()
            .map(|s| read(track, s, StepField::Lock(0)))
            .collect()
    }

    #[test]
    fn h_and_l_go_field_by_field_then_on_to_the_next_track() {
        let mut e = editor();
        keys(&mut e, "l");
        assert_eq!((e.cursor().track, e.field()), (0, StepField::Velocity));
        keys(&mut e, "l");
        assert_eq!((e.cursor().track, e.field()), (0, StepField::Lock(0)));
        keys(&mut e, "l");
        assert_eq!((e.cursor().track, e.field()), (1, StepField::Note));
        keys(&mut e, "h");
        assert_eq!((e.cursor().track, e.field()), (0, StepField::Lock(0)));
        // th and tl land on the notes; f goes straight to a field.
        keys(&mut e, "tl");
        assert_eq!((e.cursor().track, e.field()), (1, StepField::Note));
        keys(&mut e, "th");
        assert_eq!((e.cursor().track, e.field()), (0, StepField::Note));
        keys(&mut e, "fc");
        assert_eq!(e.field(), StepField::Lock(0));
        keys(&mut e, "fv");
        assert_eq!(e.field(), StepField::Velocity);
        keys(&mut e, "fx");
        assert!(e.message.as_ref().unwrap().contains("isn't a field"));
    }

    #[test]
    fn typing_sets_values_slides_and_resets() {
        let mut e = editor();
        keys(&mut e, "fci20~90-\x1b");
        assert_eq!(
            column(&e, 0)[..4],
            [
                Some(Value::Number(20, false)),
                Some(Value::Number(90, true)),
                Some(Value::Reset),
                None
            ]
        );
        // On the cutoff's curve: 99 is the top of its range.
        keys(&mut e, "ggi99\x1b");
        let lock = e.grid.tracks[0].steps[0]
            .locks
            .iter()
            .next()
            .copied()
            .unwrap();
        assert_eq!(lock.value, 16_000.0);
        // Velocity, 00 to 99.
        keys(&mut e, "ggfvi50\x1b");
        assert_eq!(e.grid.tracks[0].steps[0].note.unwrap().velocity, 64);
        assert!(e.grid.tracks[1].steps.iter().all(|s| s.locks.is_empty()));
    }

    #[test]
    fn nudge_clear_copy_paste_and_fill_work_on_the_field() {
        let mut e = editor();
        keys(&mut e, "fci10\x1bjjjjjjji50\x1bgg");
        // ^a on every value the selection reaches.
        keys(&mut e, "tt");
        e.handle(Key::Ctrl('a'));
        assert_eq!(column(&e, 0)[0], Some(Value::Number(11, false)));
        assert_eq!(column(&e, 0)[7], Some(Value::Number(51, false)));
        assert_eq!(column(&e, 0)[3], None, "none where there wasn't one");
        // & fills between the ends.
        keys(&mut e, "&");
        let filled: Vec<_> = column(&e, 0).into_iter().flatten().collect();
        assert_eq!(filled.len(), 8);
        assert_eq!(filled[4], Value::Number(34, false));
        assert!(
            e.grid.tracks[0].steps.iter().all(|s| s.note.is_some()),
            "notes stay"
        );
        // y and p copy values into another step's field.
        keys(&mut e, "\x1bggyjjjp");
        assert_eq!(column(&e, 0)[3], Some(Value::Number(11, false)));
        // d clears the field, leaving the note.
        keys(&mut e, "d");
        assert_eq!(column(&e, 0)[3], None);
        assert!(e.grid.tracks[0].steps[3].note.is_some());
        // Across tracks, edits take whole steps.
        keys(&mut e, "fnggvtld");
        assert!(e.grid.tracks[0].steps[0].note.is_none());
    }
}
