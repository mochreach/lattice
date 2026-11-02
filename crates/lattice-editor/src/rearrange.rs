//! Rearranging the selected steps, on each track they reach, as Helix rearranges text: `~` turns
//! them back to front, `*` repeats the first through the rest, and `A-(` and `A-)` rotate them.
//! On a chop track, turning the steps round turns each piece round too, with the `<` gesture, so
//! the slice of sample they cover plays backwards.

use lattice_core::instrument::{InstKind, Instrument, chop};
use lattice_core::{Gesture, Step};

use crate::{Editor, Effect, changed};

impl Editor {
    /// Turns the selected steps back to front: a note that started `k` steps in now ends `k`
    /// steps from the end.
    pub(crate) fn reverse_steps(&mut self) -> Vec<Effect> {
        self.rearrange(reverse)
    }

    /// Repeats the first `unit` selected steps through the rest of the selection.
    pub(crate) fn repeat_steps(&mut self, unit: usize) -> Vec<Effect> {
        self.rearrange(|steps, _| {
            let unit = unit.clamp(1, steps.len().max(1));
            let Some((first, rest)) = steps.split_at_mut_checked(unit) else {
                return;
            };
            for (to, from) in rest.iter_mut().zip(first.iter().cycle()) {
                *to = *from;
            }
        })
    }

    /// Moves the selected steps `by` steps down, or up if it's negative, the ones off the end
    /// coming round to the start.
    pub(crate) fn rotate_steps(&mut self, by: isize) -> Vec<Effect> {
        self.rearrange(|steps, _| {
            if steps.is_empty() {
                return;
            }
            let by = by.rem_euclid(steps.len() as isize) as usize;
            steps.rotate_right(by);
        })
    }

    fn rearrange(&mut self, f: impl Fn(&mut [Step], &Instrument)) -> Vec<Effect> {
        self.checkpoint();
        let columns: Vec<_> = (self.selections.iter())
            .flat_map(|s| self.columns(s.region()))
            .collect();
        let mut tracks = vec![];
        for (t, steps) in columns {
            let Some(track) = self.grid.tracks.get_mut(t) else {
                continue;
            };
            let Some(steps) = track.steps.get_mut(steps) else {
                continue;
            };
            f(steps, &track.instrument);
            tracks.push(t);
        }
        changed(tracks)
    }
}

pub(crate) fn reverse(steps: &mut [Step], instrument: &Instrument) {
    let n = steps.len();
    let mut reversed = vec![Step::default(); n];
    // Steps with only locks first, so a note landing on one keeps them as well as its own.
    for (to, step) in reversed.iter_mut().rev().zip(steps.iter()) {
        if step.note.is_none() {
            *to = *step;
        }
    }
    for (i, step) in steps.iter().enumerate() {
        let Some(mut note) = step.note else {
            continue;
        };
        // Held past the selection, it's cut at its end, as what's selected is turned round alone.
        let span = usize::from(note.length).clamp(1, n - i);
        note.length = span as u16;
        let mut turned = Step {
            note: Some(note),
            ..*step
        };
        if instrument.kind == InstKind::Chop {
            turn_piece(&mut turned, span, instrument);
        }
        // In range: `span` is at most `n - i`.
        let Some(to) = reversed.get_mut(n - i - span) else {
            continue;
        };
        for lock in to.locks.iter() {
            if turned.locks.find(lock).is_none() {
                turned.locks.set(*lock);
            }
        }
        *to = turned;
    }
    steps.copy_from_slice(&reversed);
}

/// Plays a chop step's piece the other way, with a `<` gesture or without one, and starts it
/// from the piece it now reaches first, so a note held over `span` pieces covers the same ones.
fn turn_piece(step: &mut Step, span: usize, instrument: &Instrument) {
    let Some(note) = &mut step.note else {
        return;
    };
    let fields = &mut step.gestures;
    let turned = match fields.iter_mut().find(|g| **g == Some(Gesture::Reverse)) {
        Some(field) => {
            *field = None;
            false
        }
        None => {
            // With no room for it, it plays as it did.
            let Some(free) = fields.iter_mut().find(|g| g.is_none()) else {
                return;
            };
            *free = Some(Gesture::Reverse);
            true
        }
    };
    let reverse = (instrument.get(chop::DIRECTION) >= 0.5) != turned;
    let pieces = instrument.get(chop::PIECES).round().max(1.0) as usize;
    let piece = chop::piece(note.pitch, pieces) as isize;
    let along = span as isize - 1;
    let piece = piece + if reverse { along } else { -along };
    note.pitch = chop::pitch(piece.rem_euclid(pieces as isize) as usize);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{Lock, Note};

    fn note(pitch: u8, length: u16) -> Step {
        Step::with_note(Note {
            pitch,
            velocity: 100,
            length,
        })
    }

    fn pitches(steps: &[Step]) -> Vec<Option<(u8, u16)>> {
        steps
            .iter()
            .map(|s| s.note.map(|n| (n.pitch, n.length)))
            .collect()
    }

    #[test]
    fn reversing_mirrors_where_notes_start_and_end() {
        let mut steps = vec![note(60, 1), Step::default(), note(62, 2), Step::default()];
        steps.push(note(64, 3));
        reverse(&mut steps, &Instrument::default());
        // The held note was cut at the selection's end.
        assert_eq!(
            pitches(&steps),
            [Some((64, 1)), Some((62, 2)), None, None, Some((60, 1))]
        );
    }

    #[test]
    fn reversing_a_chop_track_turns_each_piece_round() {
        let mut chopper = Instrument::new(InstKind::Chop);
        chopper.values[chop::PIECES as usize] = 8.0;
        let mut steps: Vec<Step> = (0..4).map(|i| note(chop::pitch(i), 1)).collect();
        steps[1] = note(chop::pitch(1), 2);
        steps[2] = Step::default();
        reverse(&mut steps, &chopper);
        // Piece 1 held over pieces 1 and 2 now starts from the end of piece 2, going back.
        assert_eq!(
            pitches(&steps),
            [
                Some((chop::pitch(3), 1)),
                Some((chop::pitch(2), 2)),
                None,
                Some((chop::pitch(0), 1)),
            ]
        );
        let reversed = |s: &Step| s.gestures().any(|g| g == Gesture::Reverse);
        assert!(steps.iter().filter(|s| s.note.is_some()).all(reversed));

        // Twice round, it's as it was, with no gestures left.
        reverse(&mut steps, &chopper);
        assert_eq!(pitches(&steps)[1], Some((chop::pitch(1), 2)));
        assert!(steps.iter().all(|s| s.gestures().next().is_none()));

        // A piece already turned plays forwards, keeping its other gestures, and one with no
        // room for a `<` plays as it did.
        let mut steps: Vec<Step> = (0..3).map(|i| note(chop::pitch(i), 1)).collect();
        steps[0].gestures = [Some(Gesture::Reverse), Some(Gesture::Accent)];
        steps[2].gestures = [Some(Gesture::Accent), Some(Gesture::Retrigger(2))];
        reverse(&mut steps, &chopper);
        assert_eq!(steps[2].gestures, [None, Some(Gesture::Accent)]);
        assert_eq!(steps[1].gestures, [Some(Gesture::Reverse), None]);
        assert_eq!(
            steps[0].gestures,
            [Some(Gesture::Accent), Some(Gesture::Retrigger(2))]
        );
    }

    #[test]
    fn a_step_with_only_locks_keeps_them_under_a_note() {
        let lock = Lock {
            fx: None,
            param: 1,
            value: 0.5,
            ..Default::default()
        };
        let mut steps = vec![note(60, 2), Step::default()];
        steps[1].locks.set(lock);
        reverse(&mut steps, &Instrument::default());
        assert_eq!(steps[0].note.unwrap().pitch, 60);
        assert_eq!(steps[0].locks.find(&lock), Some(&lock));
    }
}
