//! Objects, as Helix's match mode selects them: `mi` and `ma` then a key select the note,
//! phrase, beat, bar, group, chord or lock each cursor is in, and `mm` goes to the other end of
//! its note or lock.

use lattice_core::{LockColumn, Step};

use crate::fields::StepField;
use crate::{Cursor, Editor, Infobox, Selection};

/// Something on the grid `mi` or `ma` can select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Object {
    Note,
    Phrase,
    Beat,
    Bar,
    Group,
    Chord,
    Lock,
}

/// Each object by its key, with what inside and around select.
const OBJECTS: [(char, Object, &str, &str); 7] = [
    (
        'n',
        Object::Note,
        "note: the note and its held steps",
        "note and the rest after it",
    ),
    (
        'p',
        Object::Phrase,
        "phrase: notes up to a beat's rest",
        "phrase and the rest after it",
    ),
    (
        'b',
        Object::Beat,
        "beat, on the selected tracks",
        "beat, across every track",
    ),
    (
        'm',
        Object::Bar,
        "bar, on the selected tracks",
        "bar, across every track",
    ),
    (
        'g',
        Object::Group,
        "group's tracks, on the selected steps",
        "group's tracks, every step",
    ),
    (
        'c',
        Object::Chord,
        "chord: each note sounding on the row, on its first step",
        "chord: each note sounding on the row, whole",
    ),
    (
        'l',
        Object::Lock,
        "lock: where it holds, up to the setting's next",
        "lock and the setting's next lock",
    ),
];

/// A run of steps on one track, inclusive at both ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    pub first: usize,
    pub last: usize,
}

impl Span {
    fn contains(&self, step: usize) -> bool {
        (self.first..=self.last).contains(&step)
    }
}

/// The steps with notes on, in order.
fn note_starts(steps: &[Step]) -> Vec<usize> {
    (steps.iter().enumerate())
        .filter(|(_, s)| s.note.is_some())
        .map(|(i, _)| i)
        .collect()
}

/// Each note's steps: from where it starts, and inside, as long as it's held, but never into
/// the next note; around, on through the rest up to the next note or the end.
fn note_spans(steps: &[Step], around: bool) -> Vec<Span> {
    let starts = note_starts(steps);
    let last = steps.len().saturating_sub(1);
    (starts.iter().enumerate())
        .map(|(i, &first)| {
            let before_next = starts.get(i + 1).map_or(last, |&next| next - 1);
            let held = steps
                .get(first)
                .and_then(|s| s.note)
                .map_or(1, |n| n.length);
            let end = first + usize::from(held.max(1)) - 1;
            Span {
                first,
                last: if around {
                    before_next
                } else {
                    end.min(before_next)
                },
            }
        })
        .collect()
}

/// The notes in runs broken by `gap` steps or more without a note sounding: inside, from the
/// first note to the end of the last's held steps; around, on through the rest.
fn phrase_spans(steps: &[Step], gap: usize, around: bool) -> Vec<Span> {
    let mut phrases: Vec<Span> = vec![];
    for note in note_spans(steps, false) {
        match phrases.last_mut() {
            Some(phrase) if note.first.saturating_sub(phrase.last + 1) < gap => {
                phrase.last = note.last
            }
            _ => phrases.push(note),
        }
    }
    if around {
        let last = steps.len().saturating_sub(1);
        let nexts: Vec<usize> = phrases.iter().skip(1).map(|p| p.first - 1).collect();
        for (i, phrase) in phrases.iter_mut().enumerate() {
            phrase.last = nexts.get(i).copied().unwrap_or(last);
        }
    }
    phrases
}

/// The span of `spans` the step's in, or the one before it if it's between them: a step in a
/// rest belongs to the note or phrase before, as its around takes it in.
fn span_at(spans: &[Span], step: usize) -> Option<Span> {
    spans.iter().rev().find(|s| s.first <= step).copied()
}

/// Where each of `column`'s locks holds: inside, from its step up to the setting's next lock,
/// or the end; around, onto the next lock too.
fn lock_spans(steps: &[Step], column: LockColumn, around: bool) -> Vec<Span> {
    let locked: Vec<usize> = (steps.iter().enumerate())
        .filter(|(_, s)| s.locks.iter().any(|l| l.column() == column))
        .map(|(i, _)| i)
        .collect();
    let last = steps.len().saturating_sub(1);
    (locked.iter().enumerate())
        .map(|(i, &first)| Span {
            first,
            last: match locked.get(i + 1) {
                Some(&next) if around => next,
                Some(&next) => next - 1,
                None => last,
            },
        })
        .collect()
}

impl Editor {
    /// What follows `mi` or `ma`, for the infobox.
    pub(crate) fn objects_infobox(&self, around: bool) -> Infobox {
        Infobox {
            title: if around { "around" } else { "inside" }.into(),
            items: (OBJECTS.iter())
                .map(|&(key, _, inside, outside)| {
                    let what = if around { outside } else { inside };
                    (key.to_string(), what.to_string())
                })
                .collect(),
        }
    }

    /// `mi` or `ma` then `key`: selects the object each selection's cursor is in. A selection
    /// with none keeps as it was, and with none anywhere, the player's told why.
    pub(crate) fn select_object(&mut self, key: crate::Key, around: bool) {
        let crate::Key::Char(c) = key else {
            return;
        };
        let Some(&(_, object, _, _)) = OBJECTS.iter().find(|o| o.0 == c) else {
            self.message = Some(format!("no object on {c}: n p b m g c l"));
            self.key_failed = true;
            return;
        };
        let mut selections = vec![];
        let mut primary = 0;
        let mut found = false;
        for (i, &s) in self.selections.iter().enumerate() {
            if i == self.primary {
                primary = selections.len();
            }
            match self.object(s, object, around) {
                Some(new) if !new.is_empty() => {
                    found = true;
                    selections.extend(new);
                }
                _ => selections.push(s),
            }
        }
        if !found {
            self.message = Some(self.no_object(object));
            self.key_failed = true;
            return;
        }
        self.set_selections(selections, primary);
    }

    /// Why there's nothing to select.
    fn no_object(&self, object: Object) -> String {
        match object {
            Object::Note | Object::Phrase => "no note here, or before it on the track".into(),
            Object::Group => "this track isn't in a group".into(),
            Object::Chord => "nothing sounding on this row".into(),
            Object::Lock => "no lock here, or before it on the track".into(),
            Object::Beat | Object::Bar => "nothing to select".into(),
        }
    }

    /// The selections the object around `s`'s cursor makes: one, or a chord's several.
    fn object(&self, s: Selection, object: Object, around: bool) -> Option<Vec<Selection>> {
        let c = s.cursor;
        let region = s.region();
        let last_track = self.grid.tracks.len().saturating_sub(1);
        let on_track = |span: Span| {
            let from = Cursor {
                track: c.track,
                step: span.first,
            };
            let to = Cursor {
                track: c.track,
                step: span.last,
            };
            vec![self.clamped(from, to)]
        };
        let rows = |unit: usize| {
            let first = c.step / unit * unit;
            let (from, to) = if around {
                (0, last_track)
            } else {
                (region.first_track, region.last_track)
            };
            vec![self.clamped(
                Cursor {
                    track: from,
                    step: first,
                },
                Cursor {
                    track: to,
                    step: first + unit - 1,
                },
            )]
        };
        let steps = &self.grid.tracks.get(c.track)?.steps;
        Some(match object {
            Object::Note => on_track(span_at(&note_spans(steps, around), c.step)?),
            Object::Phrase => on_track(span_at(&phrase_spans(steps, self.beat(), around), c.step)?),
            Object::Beat => rows(self.beat()),
            Object::Bar => rows(self.bar()),
            Object::Group => {
                let group = self.grid.groups.get(self.grid.group_of(c.track)?)?;
                let tracks = group.tracks();
                let (first, last) = if around {
                    (0, usize::MAX)
                } else {
                    (region.first_step, region.last_step)
                };
                vec![self.clamped(
                    Cursor {
                        track: tracks.start,
                        step: first,
                    },
                    Cursor {
                        track: tracks.end.saturating_sub(1),
                        step: last,
                    },
                )]
            }
            Object::Chord => (0..self.grid.tracks.len())
                .filter_map(|t| {
                    let steps = &self.grid.tracks.get(t)?.steps;
                    let note = span_at(&note_spans(steps, false), c.step)?;
                    note.contains(c.step).then(|| {
                        let last = if around { note.last } else { note.first };
                        self.clamped(
                            Cursor {
                                track: t,
                                step: note.first,
                            },
                            Cursor {
                                track: t,
                                step: last,
                            },
                        )
                    })
                })
                .collect(),
            Object::Lock => {
                let column = self.lock_column_at(c)?;
                on_track(span_at(&lock_spans(steps, column, around), c.step)?)
            }
        })
    }

    /// The setting whose lock holds at `c`: the lock column the cursor's in, or else the
    /// setting locked nearest before it on its track.
    pub(crate) fn lock_column_at(&self, c: Cursor) -> Option<LockColumn> {
        let steps = &self.grid.tracks.get(c.track)?.steps;
        if c == self.cursor()
            && let StepField::Lock(i) = self.field()
        {
            return self.lock_columns(c.track).get(i).map(|l| l.column);
        }
        let upto = steps.get(..=c.step.min(steps.len().saturating_sub(1)))?;
        (upto.iter().rev())
            .find_map(|s| s.locks.iter().next())
            .map(|l| l.column())
    }

    /// `mm`: each cursor to the other end of the note it's in, or in a lock column, of where
    /// the lock holds: from its first step to its last, and from anywhere else back to its first.
    pub(crate) fn match_end(&mut self) {
        let lock_field = matches!(self.on_field(), Some(StepField::Lock(_)));
        let span = |e: &Self, c: Cursor| -> Option<Span> {
            let steps = &e.grid.tracks.get(c.track)?.steps;
            let spans = match lock_field {
                true => lock_spans(steps, e.lock_column_at(c)?, false),
                false => note_spans(steps, false),
            };
            span_at(&spans, c.step).filter(|s| s.contains(c.step))
        };
        if !(self.selections.iter()).any(|s| span(self, s.cursor).is_some()) {
            let what = if lock_field { "lock" } else { "note" };
            self.message = Some(format!("no {what} here to go to the end of"));
            self.key_failed = true;
            return;
        }
        self.motion(|e, c| match span(e, c) {
            Some(span) if c.step == span.first => Cursor {
                step: span.last,
                ..c
            },
            Some(span) => Cursor {
                step: span.first,
                ..c
            },
            None => c,
        });
    }
}

#[cfg(test)]
mod tests {
    use lattice_core::{Group, Note};

    use crate::Key;
    use crate::tests::{editor, keys};

    fn note(e: &mut crate::Editor, track: usize, step: usize, length: u16) {
        e.grid.tracks[track].steps[step].note = Some(Note {
            pitch: 60,
            velocity: 100,
            length,
        });
    }

    /// Each selection as (first track, last track, first step, last step).
    fn selected(e: &crate::Editor) -> Vec<(usize, usize, usize, usize)> {
        (e.selections().iter())
            .map(|s| {
                let r = s.region();
                (r.first_track, r.last_track, r.first_step, r.last_step)
            })
            .collect()
    }

    #[test]
    fn a_note_inside_is_its_held_steps_and_around_takes_the_rest() {
        let mut e = editor();
        note(&mut e, 0, 2, 3);
        note(&mut e, 0, 8, 1);
        keys(&mut e, "3jmin");
        assert_eq!(selected(&e), [(0, 0, 2, 4)]);
        keys(&mut e, ";man");
        assert_eq!(selected(&e), [(0, 0, 2, 7)]);
        // In the rest after it, still the note before.
        keys(&mut e, ";gg6jmin");
        assert_eq!(selected(&e), [(0, 0, 2, 4)]);
        // Before any note there's nothing, and the selection stays.
        keys(&mut e, ";ggjmin");
        assert_eq!(selected(&e), [(0, 0, 1, 1)]);
        assert!(e.message.is_some());
    }

    #[test]
    fn a_phrase_breaks_at_a_rest_of_a_beat() {
        let mut e = editor();
        // A beat is four steps: 0 and 2 are a phrase, a rest of three, then 6; 6 to 11 is a
        // rest of four, a beat, so 12 starts another.
        for step in [0, 2, 6, 12] {
            note(&mut e, 0, step, 1);
        }
        keys(&mut e, "2jmip");
        assert_eq!(selected(&e), [(0, 0, 0, 6)]);
        keys(&mut e, "map");
        assert_eq!(selected(&e), [(0, 0, 0, 11)]);
    }

    #[test]
    fn beats_and_bars_go_across_every_track_around() {
        let mut e = editor();
        keys(&mut e, "5jmib");
        assert_eq!(selected(&e), [(0, 0, 4, 7)]);
        keys(&mut e, "mab");
        assert_eq!(selected(&e), [(0, 1, 4, 7)]);
        keys(&mut e, ";gg9jmam");
        // The second track is only 8 long.
        assert_eq!(selected(&e), [(0, 1, 0, 15)]);
    }

    #[test]
    fn a_group_inside_keeps_to_the_rows_and_around_takes_them_all() {
        let mut e = editor();
        e.grid.groups.push(Group::new("kit", 0..2));
        keys(&mut e, "2jv1jmig");
        assert_eq!(selected(&e), [(0, 1, 2, 3)]);
        keys(&mut e, "mag");
        assert_eq!(selected(&e), [(0, 1, 0, 15)]);
    }

    #[test]
    fn a_chord_selects_each_note_sounding_on_the_row() {
        let mut e = editor();
        note(&mut e, 0, 0, 4);
        note(&mut e, 1, 2, 2);
        keys(&mut e, "2jmic");
        assert_eq!(selected(&e), [(0, 0, 0, 0), (1, 1, 2, 2)]);
        keys(&mut e, "mac");
        assert_eq!(selected(&e), [(0, 0, 0, 3), (1, 1, 2, 3)]);
    }

    #[test]
    fn mm_goes_between_a_notes_ends() {
        let mut e = editor();
        note(&mut e, 0, 2, 4);
        keys(&mut e, "2jmm");
        assert_eq!(e.cursor().step, 5);
        keys(&mut e, "kmm");
        assert_eq!(e.cursor().step, 2);
        // In select mode it stretches the selection.
        keys(&mut e, "vmm");
        assert_eq!(selected(&e), [(0, 0, 2, 5)]);
        e.handle(Key::Esc);
        keys(&mut e, ";gg12jmm");
        assert!(e.message.is_some(), "no note at 12");
        assert_eq!(e.cursor().step, 12);
    }
}
