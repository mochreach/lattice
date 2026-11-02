//! The fields of an open track: each step's note, its velocity, its two gestures, and a value in
//! each of the track's lock columns; and, left of the first track's, the conductor's two gesture
//! fields.
//! `h` `l` go field by field, `f` jumps to one, and typing, `^a` `^x`, `d`, `y` `p` and `&` work
//! on the field the cursor's in, on every step the selections reach.

use std::str::FromStr;

use lattice_core::{
    Change, Conductor, GESTURES, Gesture, Lock, MAX_BPM, MAX_SPEED, MAX_STEPS, MIN_BPM, ParamInfo,
    SongGesture, Step, TICKS, Track,
};

use crate::keymap::Key;
use crate::{Cursor, Editor, Effect, Mode, Page, changed, locks};

/// Which of a step's fields the cursor is in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StepField {
    #[default]
    Note,
    Velocity,
    /// One of the step's two gesture fields.
    Gesture(usize),
    /// A lock column, by its place among the track's.
    Lock(usize),
    /// One of the conductor's gesture fields, left of the first track's: it's the pattern's,
    /// not the track's, and has its own length.
    Conductor(usize),
}

/// What a field holds on a step, as its column shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Value {
    /// From 00 to 99, across the setting's range or a note's velocities; sliding there, or not.
    Number(u8, bool),
    /// `--`: the setting back to the mixer's.
    Reset,
    /// One of the conductor's.
    Conductor(SongGesture),
    /// One of a track's.
    Gesture(Gesture),
}

/// A gesture as it's typed into a field: the conductor's or a track's.
trait Typed: FromStr<Err = String> + Copy {
    /// Whether more could be typed after `text` to make another.
    fn could_go_on(text: &str) -> bool;
    /// What's written for a key typed.
    fn written(c: char) -> char;
    fn value(self) -> Value;
    /// What the status line says once it's in.
    fn said(self) -> Option<String>;
}

impl Typed for SongGesture {
    fn could_go_on(text: &str) -> bool {
        SongGesture::could_go_on(text)
    }

    fn written(c: char) -> char {
        c.to_ascii_uppercase()
    }

    fn value(self) -> Value {
        Value::Conductor(self)
    }

    fn said(self) -> Option<String> {
        None
    }
}

impl Typed for Gesture {
    fn could_go_on(text: &str) -> bool {
        Gesture::could_go_on(text)
    }

    /// Capitals, but `r`, which fades where `R` doesn't.
    fn written(c: char) -> char {
        match c {
            'r' => 'r',
            c => c.to_ascii_uppercase(),
        }
    }

    fn value(self) -> Value {
        Value::Gesture(self)
    }

    fn said(self) -> Option<String> {
        Some(self.describe())
    }
}

/// What's being typed into a field in insert mode: `~`, then the first digit; or a gesture so
/// far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Typing {
    slide: bool,
    digit: Option<u8>,
    gesture: String,
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
        StepField::Note | StepField::Conductor(_) => None,
        StepField::Gesture(g) => step.gestures.get(g).copied().flatten().map(Value::Gesture),
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
        StepField::Note | StepField::Conductor(_) => {}
        StepField::Gesture(g) => {
            let gesture = match value {
                Some(Value::Gesture(gesture)) => Some(gesture),
                None => None,
                Some(_) => return,
            };
            if let Some(field) = (track.steps.get_mut(at)).and_then(|s| s.gestures.get_mut(g)) {
                *field = gesture;
            }
        }
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
                Some(Value::Conductor(_) | Value::Gesture(_)) => return,
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

/// A track's gesture `by` further on: more or fewer hits, 24ths, steps or times as fast, within
/// what each can be; a gesture with no amount as it is.
fn nudge_gesture(gesture: Gesture, by: i32) -> Gesture {
    let within =
        |n: u8, low: u8, high: u8| (i32::from(n) + by).clamp(low.into(), high.into()) as u8;
    let step = (MAX_STEPS - 1) as u8;
    match gesture {
        Gesture::Retrigger(n) => Gesture::Retrigger(within(n, 2, 9)),
        Gesture::Fade(n) => Gesture::Fade(within(n, 2, 9)),
        Gesture::Delay(n) => Gesture::Delay(within(n, 1, TICKS - 1)),
        Gesture::Cut(n) => Gesture::Cut(within(n, 1, TICKS - 1)),
        Gesture::Jump(n) => Gesture::Jump(within(n, 0, step)),
        Gesture::Wait(n) => Gesture::Wait(within(n, 1, 99)),
        Gesture::Faster(n) => Gesture::Faster(within(n, 1, MAX_SPEED)),
        Gesture::Slower(n) => Gesture::Slower(within(n, 1, MAX_SPEED)),
        other => other,
    }
}

/// A conductor's gesture `by` further on: a tempo by so many BPM; anything else as it is.
fn nudge_song_gesture(gesture: SongGesture, by: i32) -> SongGesture {
    let bpm = |bpm: u16| (i32::from(bpm) + by).clamp(i32::from(MIN_BPM), i32::from(MAX_BPM)) as u16;
    match gesture {
        SongGesture::Tempo(b) => SongGesture::Tempo(bpm(b)),
        SongGesture::Slide(b) => SongGesture::Slide(bpm(b)),
        other => other,
    }
}

impl Editor {
    /// The fields of track `t`, in order: just its notes if its group's folded.
    fn fields_of(&self, t: usize) -> Vec<StepField> {
        let folded = (self.grid.group_of(t))
            .and_then(|g| self.grid.groups.get(g))
            .is_some_and(|g| g.folded);
        let conductor = (0..GESTURES).map(StepField::Conductor).filter(|_| t == 0);
        if folded {
            return conductor.chain([StepField::Note]).collect();
        }
        let columns = self.lock_columns(t).len();
        (conductor.chain([StepField::Note, StepField::Velocity]))
            .chain((0..GESTURES).map(StepField::Gesture))
            .chain((0..columns).map(StepField::Lock))
            .collect()
    }

    /// Whether the cursor's in the conductor's fields.
    pub fn in_conductor(&self) -> bool {
        matches!(self.field(), StepField::Conductor(_))
    }

    /// The pattern being edited's conductor.
    pub fn conductor(&self) -> Option<&Conductor> {
        self.arrangement.conductor(self.arrangement.current)
    }

    /// How many steps the conductor has: with none of its own, as many as the pattern.
    pub fn conductor_len(&self) -> usize {
        match self.conductor() {
            Some(c) if !c.steps.is_empty() => c.steps.len(),
            _ => self.pattern_steps() as usize,
        }
    }

    /// The conductor, to change: given steps of its own, if it hasn't any, as many as the
    /// pattern has.
    pub(crate) fn conductor_mut(&mut self) -> Option<&mut Conductor> {
        let len = self.conductor_len();
        let current = self.arrangement.current;
        let conductor = &mut self.arrangement.patterns.get_mut(current)?.conductor;
        if conductor.steps.is_empty() {
            *conductor = Conductor::new(len);
        }
        Some(conductor)
    }

    /// What `field` holds on step `at` of track `t`, or of the conductor.
    fn read_field(&self, t: usize, at: usize, field: StepField) -> Option<Value> {
        if let StepField::Conductor(g) = field {
            return self.conductor()?.get(at, g).map(Value::Conductor);
        }
        let track = self.grid.tracks.get(t)?;
        read(track, track.steps.get(at)?, field)
    }

    /// Writes `value` into `field` on step `at` of track `t`, or of the conductor; or clears it.
    fn write_field(&mut self, t: usize, at: usize, field: StepField, value: Option<Value>) {
        if let StepField::Conductor(g) = field {
            let gesture = match value {
                Some(Value::Conductor(gesture)) => Some(gesture),
                None => None,
                Some(_) => return,
            };
            let Some(step) = (self.conductor_mut())
                .and_then(|c| c.steps.get_mut(at))
                .and_then(|step| step.get_mut(g))
            else {
                return;
            };
            *step = gesture;
            return;
        }
        let default = self.velocity;
        if let Some(track) = self.grid.tracks.get_mut(t) {
            write(track, at, field, value, default);
        }
    }

    /// What's changed by editing `field` on these tracks: nothing the engine plays from the
    /// tracks for the conductor's, which the sequencer sends itself.
    fn field_changed(field: StepField, tracks: impl IntoIterator<Item = usize>) -> Vec<Effect> {
        match field {
            StepField::Conductor(_) => vec![],
            _ => changed(tracks),
        }
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
                // The conductor's as long as it is, not the track.
                self.clamp_selection();
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

    /// `f` then a key: to the note (`n`), the velocity (`v`), the gestures (`g` `G`), a lock
    /// column by the letter its header underlines, or the conductor (`C`), from any track.
    pub(crate) fn goto_field(&mut self, key: Key) {
        let Key::Char(c) = key else {
            return;
        };
        if c == 'C' {
            self.motion(|_, c| Cursor { track: 0, ..c });
            self.field = StepField::Conductor(0);
            self.clamp_selection();
            return;
        }
        let t = self.cursor().track;
        let field = match c {
            'n' => Some(StepField::Note),
            'v' => Some(StepField::Velocity),
            'g' => Some(StepField::Gesture(0)),
            'G' => Some(StepField::Gesture(1)),
            c => (self.lock_columns(t).iter())
                .position(|label| label.key == c)
                .map(StepField::Lock),
        };
        match field.filter(|f| self.fields_of(t).contains(f)) {
            Some(field) => {
                self.field = field;
                self.clamp_selection();
            }
            None if c == 'g' || c == 'G' => {
                self.message = Some("this track's folded into its group: unfold it with tz".into());
            }
            None => self.message = Some(format!("{c} isn't a field on this track")),
        }
    }

    /// Plays a key typed into a field in insert mode: two digits set a value and move on, `~`
    /// first makes it slide, and `-` puts the setting back. `None` for a key that isn't one.
    pub(crate) fn type_field(&mut self, key: Key, field: StepField) -> Option<Vec<Effect>> {
        match field {
            StepField::Conductor(_) => return self.type_gesture::<SongGesture>(key, field),
            StepField::Gesture(_) => return self.type_gesture::<Gesture>(key, field),
            _ => {}
        }
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
        Some(self.put_field(field, value))
    }

    /// Writes `value` into `field` at every cursor, and moves on.
    fn put_field(&mut self, field: StepField, value: Value) -> Vec<Effect> {
        self.checkpoint();
        let cursors: Vec<_> = self.selections.iter().map(|s| s.cursor).collect();
        for c in &cursors {
            self.write_field(c.track, c.step, field, Some(value));
        }
        self.advance();
        Self::field_changed(field, cursors.iter().map(|c| c.track))
    }

    /// Plays a key typed into a gesture field, the conductor's or a track's: a gesture's letter,
    /// then its number, going in once it can't go on (`T140`, `R3`), or with enter (`P3`, `D2`);
    /// backspace takes a key back.
    fn type_gesture<G: Typed>(&mut self, key: Key, field: StepField) -> Option<Vec<Effect>> {
        let c = match key {
            Key::Char(c) => c,
            Key::Enter if !self.typing.gesture.is_empty() => {
                let text = std::mem::take(&mut self.typing.gesture);
                return Some(self.put_gesture(field, text.parse::<G>()));
            }
            // Leaving insert mode puts in what's been typed, if it's a gesture.
            Key::Esc => {
                let text = std::mem::take(&mut self.typing.gesture);
                let mut effects = match text.parse::<G>() {
                    Ok(gesture) => self.put_gesture(field, Ok(gesture)),
                    Err(_) => vec![],
                };
                self.typing = Typing::default();
                if let Some(&action) = self.keymap.insert.get(&key) {
                    effects.extend(self.act(action, 1));
                }
                return Some(effects);
            }
            Key::Backspace if !self.typing.gesture.is_empty() => {
                self.typing.gesture.pop();
                self.message = Some(format!("{}_", self.typing.gesture));
                return Some(vec![]);
            }
            _ => {
                self.typing = Typing::default();
                return None;
            }
        };
        let before = std::mem::take(&mut self.typing.gesture);
        let mut text = before.clone();
        text.push(G::written(c));
        // A gesture that could have gone on, `P3`, ends as another begins.
        if !G::could_go_on(&text)
            && text.parse::<G>().is_err()
            && let Ok(gesture) = before.parse::<G>()
        {
            let mut effects = self.put_gesture(field, Ok(gesture));
            effects.extend(self.type_gesture::<G>(key, field).unwrap_or_default());
            return Some(effects);
        }
        if G::could_go_on(&text) {
            self.message = Some(format!("{text}_  (enter if that's all)"));
            self.typing.gesture = text;
            return Some(vec![]);
        }
        Some(self.put_gesture(field, text.parse::<G>()))
    }

    /// Puts a gesture typed into `field` at every cursor, saying what it does; or says why it
    /// isn't one.
    fn put_gesture<G: Typed>(
        &mut self,
        field: StepField,
        gesture: Result<G, String>,
    ) -> Vec<Effect> {
        match gesture {
            Ok(gesture) => {
                let effects = self.put_field(field, gesture.value());
                self.message = gesture.said();
                effects
            }
            Err(e) => {
                self.message = Some(e);
                vec![]
            }
        }
    }

    /// Changes `field`'s value on every step the selections reach that has one.
    fn map_field(&mut self, field: StepField, f: impl Fn(Value) -> Option<Value>) -> Vec<Effect> {
        self.checkpoint();
        let columns: Vec<_> = (self.selections.iter())
            .flat_map(|s| self.columns(s.region()))
            .collect();
        for (t, steps) in &columns {
            for at in steps.clone() {
                if let Some(value) = self.read_field(*t, at, field) {
                    self.write_field(*t, at, field, f(value));
                }
            }
        }
        Self::field_changed(field, columns.into_iter().map(|(t, _)| t))
    }

    /// `^a` `^x` on a field: each value up or down by `by`, out of 99.
    pub(crate) fn nudge_field(&mut self, field: StepField, by: i32) -> Vec<Effect> {
        self.map_field(field, |value| {
            Some(match value {
                Value::Number(n, slide) => {
                    Value::Number((i32::from(n) + by).clamp(0, 99) as u8, slide)
                }
                Value::Reset => Value::Reset,
                Value::Conductor(gesture) => Value::Conductor(nudge_song_gesture(gesture, by)),
                Value::Gesture(gesture) => Value::Gesture(nudge_gesture(gesture, by)),
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
            let t = region.first_track;
            let len = self.track_len(t);
            let values = (region.first_step..=region.last_step.min(len.saturating_sub(1)))
                .filter(|&at| at < len)
                .map(|at| self.read_field(t, at, field))
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
        let mut tracks = vec![];
        for (i, s) in self.selections.clone().iter().enumerate() {
            let region = s.region();
            let Some(clip) = (self.field_clipboard.get(i))
                .or(self.field_clipboard.last())
                .cloned()
            else {
                continue;
            };
            let t = region.first_track;
            let len = self.track_len(t);
            let values = clip.iter().cycle().take(clip.len() * n);
            for (at, &value) in (region.first_step..len).zip(values) {
                self.write_field(t, at, field, value);
            }
            tracks.push(t);
        }
        Self::field_changed(field, tracks)
    }

    /// `&` on a field: fills each selection evenly between the values at its ends, which both
    /// need one. Velocities go only on notes.
    pub(crate) fn fill_field(&mut self) -> Vec<Effect> {
        let Some(field) = self.on_field() else {
            self.message = Some("& fills a field: go to one with l or f".into());
            return vec![];
        };
        if let StepField::Gesture(_) = field {
            self.message = Some("& fills values and tempos: y and p copy gestures".into());
            return vec![];
        }
        // Values, or tempos in the conductor, from one end to the other.
        let number = |value| match value {
            Some(Value::Number(n, _)) => Some(f32::from(n)),
            Some(Value::Conductor(SongGesture::Tempo(bpm) | SongGesture::Slide(bpm))) => {
                Some(f32::from(bpm))
            }
            _ => None,
        };
        let mut fills = vec![];
        for s in &self.selections {
            let region = s.region();
            let end = |at| number(self.read_field(region.first_track, at, field));
            let (Some(from), Some(to)) = (end(region.first_step), end(region.last_step)) else {
                self.message = Some(
                    match field {
                        StepField::Conductor(_) => "& needs a tempo at each end of the selection",
                        _ => "& needs a value at each end of the selection",
                    }
                    .into(),
                );
                return vec![];
            };
            fills.push((region, from, to));
        }
        self.checkpoint();
        for &(region, from, to) in &fills {
            let span = (region.last_step - region.first_step).max(1) as f32;
            // The ends stay as they are, a slide or not.
            for at in region.first_step + 1..region.last_step {
                let along = (at - region.first_step) as f32 / span;
                let n = (from + (to - from) * along).round();
                let value = match field {
                    StepField::Conductor(_) => Value::Conductor(SongGesture::Tempo(n as u16)),
                    _ => Value::Number(n as u8, false),
                };
                self.write_field(region.first_track, at, field, Some(value));
            }
        }
        Self::field_changed(field, fills.iter().map(|(r, ..)| r.first_track))
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

    fn gestures(e: &Editor, field: usize) -> Vec<Option<String>> {
        let c = e.conductor().unwrap();
        (0..c.steps.len())
            .map(|at| c.get(at, field).map(|g| g.to_string()))
            .collect()
    }

    #[test]
    fn the_conductor_is_left_of_the_first_track_with_its_own_length() {
        let mut e = editor();
        keys(&mut e, "h");
        assert_eq!((e.cursor().track, e.field()), (0, StepField::Conductor(1)));
        keys(&mut e, "h");
        assert_eq!(e.field(), StepField::Conductor(0));
        keys(&mut e, "h");
        assert_eq!(e.field(), StepField::Conductor(0), "nothing further left");
        // With none of its own, it's as long as the pattern, a bar; :len gives it its own.
        assert_eq!(e.conductor_len(), 16);
        crate::command::run(&mut e, "len 32").unwrap();
        assert_eq!(e.conductor_len(), 32);
        assert_eq!(e.grid.tracks[0].steps.len(), 8, "the track's as it was");
        keys(&mut e, "ge");
        assert_eq!(e.cursor().step, 31);
        // Back on the track, the cursor's kept to its steps.
        keys(&mut e, "ll");
        assert_eq!((e.field(), e.cursor().step), (StepField::Note, 7));
        // fC goes to it from any track.
        keys(&mut e, "tlfC");
        assert_eq!((e.cursor().track, e.field()), (0, StepField::Conductor(0)));
    }

    #[test]
    fn gestures_are_typed_in_and_edited_like_values() {
        let mut e = editor();
        // Each goes in once it's whole, or with enter; a bad one says why.
        keys(&mut e, "fCit140~96xp3");
        e.handle(Key::Enter);
        keys(&mut e, "t10");
        e.handle(Key::Enter);
        assert!(e.message.as_ref().unwrap().contains("20 to 400"));
        // Leaving insert mode puts in a gesture that's whole so far.
        keys(&mut e, "p1\x1bggjjjj");
        assert_eq!(gestures(&e, 0)[4], Some("P1".into()));
        keys(&mut e, "d");
        assert_eq!(
            gestures(&e, 0)[..5],
            [
                Some("T140".into()),
                Some("~96".into()),
                Some("X".into()),
                Some("P3".into()),
                None
            ]
        );
        assert!(e.grid.tracks[0].steps.iter().all(|s| s.locks.is_empty()));
        // ^a nudges tempos, d clears, y and p copy.
        keys(&mut e, "gg");
        e.handle(Key::Ctrl('a'));
        assert_eq!(gestures(&e, 0)[0], Some("T141".into()));
        keys(&mut e, "yjjjjjp");
        assert_eq!(gestures(&e, 0)[5], Some("T141".into()));
        keys(&mut e, "ggjjd");
        assert_eq!(gestures(&e, 0)[2], None);
        // & fills tempos between the ends.
        keys(&mut e, "ggit100\x1bggvjjjjj&");
        let filled = ["T100", "T108", "T116", "T125", "T133", "T141"];
        assert_eq!(gestures(&e, 0)[..6], filled.map(|g| Some(g.to_string())));
        // Undo puts it back.
        keys(&mut e, "u");
        assert_eq!(gestures(&e, 0)[1], Some("~96".into()));
    }

    fn track_gestures(e: &Editor, field: usize) -> Vec<Option<String>> {
        (e.grid.tracks[0].steps.iter())
            .map(|s| s.gestures[field].map(|g| g.to_string()))
            .collect()
    }

    #[test]
    fn a_tracks_gestures_are_typed_in_and_edited() {
        let mut e = editor();
        // Each goes in once it's whole, or with enter; `r` stays small, as it fades.
        keys(&mut e, "fgiR3r4d2");
        e.handle(Key::Enter);
        assert_eq!(e.message.as_deref(), Some("D2: delay, 2/24 of a step late"));
        keys(&mut e, "j1*2!\x1b");
        let typed = ["R3", "r4", "D2", "J1", "*2", "!", "", ""];
        let typed: Vec<_> = typed.map(|g| (!g.is_empty()).then(|| g.to_string())).into();
        assert_eq!(track_gestures(&e, 0), typed);
        assert_eq!(
            e.grid.tracks[0].steps[0].note.unwrap().pitch,
            60,
            "notes stay"
        );
        // ^a and ^x change the amount, within what it can be.
        keys(&mut e, "gg");
        e.handle(Key::Ctrl('a'));
        keys(&mut e, "jjj");
        e.handle(Key::Ctrl('x'));
        e.handle(Key::Ctrl('x'));
        let nudged = track_gestures(&e, 0);
        assert_eq!(nudged[0].as_deref(), Some("R4"));
        assert_eq!(nudged[3].as_deref(), Some("J0"), "no step before 0");
        // The second field's its own; y p copy, d clears, & doesn't fill them.
        keys(&mut e, "ggfGiX6\x1bgg");
        assert_eq!(track_gestures(&e, 1)[0], Some("X6".into()));
        keys(&mut e, "yjjjjjjjp");
        assert_eq!(track_gestures(&e, 1)[7], Some("X6".into()));
        keys(&mut e, "d");
        assert_eq!(track_gestures(&e, 1)[7], None);
        keys(&mut e, "ggvj&");
        assert!(e.message.as_ref().unwrap().contains("copy gestures"));
        // A bad one says why.
        keys(&mut e, "\x1bggfgiR1");
        assert!(e.message.as_ref().unwrap().contains("2 to 9"));
        // Leaving insert mode puts in a gesture typed so far, and the engine hears of it.
        keys(&mut e, "\x1bggfgiD2");
        assert_eq!(e.handle(Key::Esc), [Effect::TrackChanged(0)]);
        assert_eq!(
            (track_gestures(&e, 0)[0].as_deref(), &e.mode),
            (Some("D2"), &Mode::Normal)
        );
    }

    #[test]
    fn h_and_l_go_field_by_field_then_on_to_the_next_track() {
        let mut e = editor();
        keys(&mut e, "l");
        assert_eq!((e.cursor().track, e.field()), (0, StepField::Velocity));
        keys(&mut e, "l");
        assert_eq!((e.cursor().track, e.field()), (0, StepField::Gesture(0)));
        keys(&mut e, "ll");
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
        keys(&mut e, "fG");
        assert_eq!(e.field(), StepField::Gesture(1));
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
