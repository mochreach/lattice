//! Chords on the grid: a note that plays more notes with it, written by name with `:chord`.

use lattice_core::{Chord, Note, chord};

use crate::pads::Scale;

use crate::{Cursor, Editor, Effect, Key, TrackId, changed, octave_pitch};

/// A note entered whose key is still down: notes played before it comes up join its chord,
/// rather than going on down the track.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Chording {
    /// The keys held down, the first the root's.
    keys: Vec<Key>,
    /// The steps it went in on, and its pitch.
    steps: Vec<(TrackId, usize)>,
    root: u8,
}

/// The chord called `name` at `octave`: its root there, its notes close above, and a bass
/// after a slash just below the root. The root's the note's pitch, the rest its chord.
pub(crate) fn voiced(name: &str, octave: u8) -> Result<(u8, Chord), String> {
    let (root, intervals, bass) = chord::parse(name)?;
    let pitch = octave_pitch(octave, i16::from(root))
        .ok_or_else(|| format!("{name} is off the top at octave {octave}"))?;
    let mut offsets: Vec<i8> = intervals.iter().copied().filter(|&i| i != 0).collect();
    if let Some(bass) = bass.filter(|&b| b != root) {
        // Both are pitch classes, under 12.
        offsets.push(-(((root + 12 - bass) % 12) as i8));
    }
    let chord = Chord::new(&offsets).ok_or_else(|| format!("{name} has too many notes"))?;
    Ok((pitch, chord))
}

/// `note`'s chord an inversion up, its lowest note an octave higher, or down, its highest an
/// octave lower; as it is where that would leave MIDI's range. The root stays the note's pitch
/// wherever it goes.
fn inverted(note: Note, up: bool) -> Note {
    let mut pitches: Vec<u8> = note.chord.pitches(note.pitch).collect();
    let outer = match up {
        true => (pitches.iter().enumerate()).min_by_key(|&(_, p)| p),
        false => (pitches.iter().enumerate()).max_by_key(|&(_, p)| p),
    };
    let Some((i, &from)) = outer else {
        return note;
    };
    // An octave on, or further if a note's doubled there already, so none is lost.
    let octaves = (1..=10).map(|n| match up {
        true => i16::from(from) + 12 * n,
        false => i16::from(from) - 12 * n,
    });
    let Some(to) = octaves
        .filter_map(|p| u8::try_from(p).ok().filter(|&p| p <= 127))
        .find(|p| !pitches.contains(p))
    else {
        return note;
    };
    if let Some(p) = pitches.get_mut(i) {
        *p = to;
    }
    let Some((&root, rest)) = pitches.split_first() else {
        return note;
    };
    Note {
        pitch: root,
        chord: Chord::of(root, rest.iter().copied()),
        ..note
    }
}

/// What sounds `pitch` and its `chord` on `track` as they're entered: each of the notes.
pub(crate) fn previews(track: usize, pitch: u8, chord: Chord) -> impl Iterator<Item = Effect> {
    chord
        .pitches(pitch)
        .map(move |pitch| Effect::Preview { track, pitch })
}

/// The chord of `scale`, from `root`, on `pitch`: its third and fifth as the scale stacks
/// them. A note outside the scale, or where the scale has no third or fifth to give (a
/// pentatonic's gaps), has a major chord's, the one most often borrowed.
pub(crate) fn scale_chord(scale: &Scale, root: u8, pitch: u8) -> Chord {
    let steps = scale.steps;
    let degree = (pitch % 12 + 12 - root % 12) % 12;
    let at = steps.iter().position(|&s| s == degree);
    // The scale's note `n` above `pitch`, in semitones, if `pitch` is in the scale.
    let above = |n: usize| -> Option<i8> {
        let i = at? + n;
        let step = *steps.get(i % steps.len())?;
        Some((i16::from(step) + 12 * (i / steps.len()) as i16 - i16::from(degree)) as i8)
    };
    let third = above(2).filter(|t| (3..=4).contains(t)).unwrap_or(4);
    let fifth = above(4).filter(|f| (6..=8).contains(f)).unwrap_or(7);
    let notes = [third, fifth].into_iter();
    Chord::of(pitch, notes.filter_map(|n| pitch.checked_add_signed(n)))
}

impl Editor {
    /// Whether track `t` is a chord track.
    pub(crate) fn is_chord_track(&self, t: usize) -> bool {
        self.grid.tracks.get(t).is_some_and(|t| t.chords)
    }

    /// The scale's chord on `pitch`, from `:scale`.
    pub(crate) fn scale_chord(&self, pitch: u8) -> Chord {
        scale_chord(&self.scale(), self.root, pitch)
    }

    /// `tc`: the tracks the selections reach become chord tracks, or, if they all are, tracks
    /// of notes again. Their notes stay as they are either way.
    pub(crate) fn toggle_chords(&mut self) {
        let tracks = self.selected_tracks();
        let on = !tracks.iter().all(|&t| self.is_chord_track(t));
        self.checkpoint();
        let mut names = vec![];
        for &t in &tracks {
            if let Some(track) = self.grid.tracks.get_mut(t) {
                track.chords = on;
                names.push(track.name.clone());
            }
        }
        let names = names.join(", ");
        self.message = Some(match on {
            true => format!("{names}: chords, the piano keys playing the scale's"),
            false => format!("{names}: notes again"),
        });
    }

    /// Each of `tracks` whose every note is a chord becomes a chord track: a whole chord
    /// written or recorded onto an empty track makes it one. Not notes held together, which
    /// might be a chord or two notes of a tune.
    pub(crate) fn mark_chord_tracks(&mut self, tracks: &[usize]) {
        for &t in tracks {
            let Some(track) = self.grid.tracks.get_mut(t) else {
                continue;
            };
            let mut notes = track.steps.iter().filter_map(|s| s.note).peekable();
            if notes.peek().is_some() && notes.all(|n| !n.chord.is_empty()) {
                track.chords = true;
            }
        }
    }

    /// `V` and `A-V`: each selected chord voiced `by` inversions up, its lowest note raised an
    /// octave each time, or down, its highest lowered. A note on its own is left as it is.
    pub(crate) fn invert(&mut self, by: i32) -> Vec<Effect> {
        let any = (self.selections.iter())
            .flat_map(|s| self.columns(s.region()))
            .any(|(t, steps)| {
                (self.grid.tracks.get(t))
                    .and_then(|t| t.steps.get(steps))
                    .is_some_and(|s| {
                        s.iter()
                            .any(|s| s.note.is_some_and(|n| !n.chord.is_empty()))
                    })
            });
        if !any {
            self.message =
                Some("no chords here to invert: :chord writes one, tc makes chord tracks".into());
            return vec![];
        }
        let mut last = None;
        let mut effects = self.edit(|s| {
            let Some(note) = &mut s.note else {
                return;
            };
            if note.chord.is_empty() {
                return;
            }
            for _ in 0..by.unsigned_abs() {
                *note = inverted(*note, by > 0);
            }
            last = Some(*note);
        });
        if let Some(note) = last {
            effects.extend(previews(self.cursor().track, note.pitch, note.chord));
            self.message = lattice_core::chord::label(&note);
        }
        effects
    }

    /// `enter` on a chord track: the command line, ready to write the cursor's chord by name.
    pub(crate) fn name_chord(&mut self) {
        let track = self.cursor().track;
        if !self.is_chord_track(track) {
            return;
        }
        let name = (self.grid.tracks.get(track))
            .and_then(|t| t.steps.get(self.cursor().step))
            .and_then(|s| s.note)
            .and_then(|n| lattice_core::chord::label(&n))
            .unwrap_or_default();
        self.open_command(&format!("chord {name}"));
    }

    /// Whether `key` coming up will be heard, so it can be held: a MIDI note's always is.
    fn reports_release(&self, key: Key) -> bool {
        self.releases || matches!(key, Key::Note(_))
    }

    /// `pitch`, entered on `key` at `cursors`, starts a chord while its key's held, if the
    /// key's coming up will be heard.
    pub(crate) fn start_chord(&mut self, key: Key, pitch: u8, cursors: &[Cursor]) {
        self.chording = self.reports_release(key).then(|| Chording {
            keys: vec![key],
            steps: (cursors.iter())
                .filter_map(|c| Some((*self.ids.get(c.track)?, c.step)))
                .collect(),
            root: pitch,
        });
    }

    /// `pitch`, played on `key` while the last note entered is still held, joins its chord,
    /// in the same undo; `None` if nothing's held.
    pub(crate) fn join_chord(&mut self, key: Key, pitch: u8) -> Option<Vec<Effect>> {
        if !self.reports_release(key) {
            return None;
        }
        let chording = self.chording.as_mut()?;
        chording.keys.push(key);
        let root = chording.root;
        let mut tracks = vec![];
        for &(id, step) in &chording.steps {
            let Some(t) = self.ids.iter().position(|&i| i == id) else {
                continue;
            };
            // Changed since some other way, it's left as it is.
            let note = (self.grid.tracks.get_mut(t))
                .and_then(|track| track.steps.get_mut(step))
                .and_then(|s| s.note.as_mut())
                .filter(|n| n.pitch == root);
            if let Some(note) = note {
                note.chord = note.chord.with(root, pitch);
                tracks.push(t);
            }
        }
        let Some(&track) = tracks.first() else {
            // Its steps have all changed since: it goes in as a note of its own.
            self.chording = None;
            return None;
        };
        let mut effects = changed(tracks);
        effects.push(Effect::Preview { track, pitch });
        Some(effects)
    }

    /// `key`'s come up: once all a chord's keys have, the next note goes on down the track.
    pub(crate) fn release_chord(&mut self, key: Key) {
        if let Some(chording) = &mut self.chording {
            chording.keys.retain(|&k| k != key);
            if chording.keys.is_empty() {
                self.chording = None;
            }
        }
    }

    /// `:chord NAME`: every selected step plays the chord, voiced at its track's octave,
    /// keeping a note's velocity and length if it had one.
    pub(crate) fn set_chord(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        let columns: Vec<_> = (self.selections.iter())
            .flat_map(|s| self.columns(s.region()))
            .collect();
        let voicings = (columns.iter())
            .map(|(t, _)| voiced(name, self.octave_of(*t)))
            .collect::<Result<Vec<_>, _>>()?;
        self.checkpoint();
        let mut last = None;
        for ((t, steps), (pitch, chord)) in columns.iter().zip(voicings) {
            let Some(steps) =
                (self.grid.tracks.get_mut(*t)).and_then(|track| track.steps.get_mut(steps.clone()))
            else {
                continue;
            };
            for step in steps {
                let note = Note {
                    pitch,
                    velocity: step.note.map_or(self.velocity, |n| n.velocity),
                    length: step.note.map_or(1, |n| n.length),
                    chord,
                };
                step.note = Some(note);
                last = Some(note);
            }
        }
        let tracks: Vec<usize> = columns.iter().map(|(t, _)| *t).collect();
        self.mark_chord_tracks(&tracks);
        let mut effects = changed(tracks);
        if let Some(note) = last {
            effects.extend(previews(self.cursor().track, note.pitch, note.chord));
            self.message = chord::label(&note);
        }
        Ok(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{editor, keys, notes};
    use crate::{Key, Mode};

    #[test]
    fn a_chord_is_voiced_above_its_root_with_a_slash_bass_below() {
        let (pitch, chord) = voiced("Am7", 4).unwrap();
        assert_eq!(pitch, 69);
        assert_eq!(chord.offsets(), [3, 7, 10]);
        let (pitch, chord) = voiced("C/E", 4).unwrap();
        assert_eq!(pitch, 60);
        assert_eq!(chord.offsets(), [4, 7, -8]);
        assert!(voiced("Cnope", 4).is_err());
    }

    fn label_at(e: &Editor, track: usize, step: usize) -> Option<String> {
        (e.grid.tracks[track].steps[step].note).and_then(|n| chord::label(&n))
    }

    #[test]
    fn the_scale_s_chords_stack_its_thirds() {
        let major = crate::pads::SCALES[0];
        let name = |root, pitch| {
            let chord = scale_chord(&major, root, pitch);
            chord::name(&chord.pitches(pitch).collect::<Vec<_>>())
        };
        assert_eq!(name(0, 62).as_deref(), Some("Dm"));
        assert_eq!(name(0, 71).as_deref(), Some("Bdim"));
        assert_eq!(name(0, 67).as_deref(), Some("G"));
        // Outside the scale, a major chord, as most borrowed chords are.
        assert_eq!(name(0, 66).as_deref(), Some("F#"));
        // In G major, F# is the scale's.
        assert_eq!(name(7, 66).as_deref(), Some("F#dim"));
    }

    #[test]
    fn a_chord_track_enters_the_scale_s_chords_and_inverts_them() {
        let mut e = editor();
        keys(&mut e, "tc");
        assert!(e.grid.tracks[0].chords);
        assert!(!e.grid.tracks[1].chords);
        keys(&mut e, "izcb");
        e.handle(Key::Esc);
        let names: Vec<_> = (0..3).map(|s| label_at(&e, 0, s)).collect();
        assert_eq!(
            names,
            [Some("C".into()), Some("Em".into()), Some("G".into())]
        );
        // A MIDI keyboard's notes go in as played.
        keys(&mut e, "i");
        e.play_note(50, 90);
        e.stop_note(50);
        e.handle(Key::Esc);
        assert_eq!(
            e.grid.tracks[0].steps[3].note.unwrap().chord,
            Chord::default()
        );

        // Inverted up, then down again.
        keys(&mut e, "gg");
        keys(&mut e, "V");
        assert_eq!(label_at(&e, 0, 0).as_deref(), Some("C/E"));
        assert_eq!(e.message.as_deref(), Some("C/E"));
        keys(&mut e, "V");
        assert_eq!(label_at(&e, 0, 0).as_deref(), Some("C/G"));
        e.handle(Key::Alt('V'));
        e.handle(Key::Alt('V'));
        let note = e.grid.tracks[0].steps[0].note.unwrap();
        assert_eq!(
            note.chord.pitches(note.pitch).collect::<Vec<_>>(),
            [60, 64, 67]
        );
        // A note on its own has nothing to invert.
        keys(&mut e, "3jV");
        assert!(e.message.as_deref().unwrap().starts_with("no chords here"));

        // Enter offers the cursor's chord to rename.
        keys(&mut e, "gg");
        e.handle(Key::Enter);
        let Mode::Command(prompt) = &e.mode else {
            panic!("no prompt");
        };
        assert_eq!(prompt.line, "chord C");

        // Back to a track of notes, they stay chords.
        e.handle(Key::Esc);
        keys(&mut e, "tc");
        assert!(!e.grid.tracks[0].chords);
        assert_eq!(label_at(&e, 0, 0).as_deref(), Some("C"));
    }

    #[test]
    fn a_chord_on_an_empty_track_makes_it_a_chord_track() {
        let mut e = editor();
        keys(&mut e, "tl");
        e.run_command("chord F");
        assert!(e.grid.tracks[1].chords);
        // Not where it joins notes of its own.
        keys(&mut e, "th");
        notes(&mut e, "z");
        keys(&mut e, "j");
        e.run_command("chord F");
        assert!(!e.grid.tracks[0].chords);
    }

    #[test]
    fn chord_writes_a_chord_on_every_selected_step() {
        let mut e = editor();
        keys(&mut e, "vjj");
        let effects = e.run_command("chord Dm7");
        let track = &e.grid.tracks[0];
        for step in &track.steps[..3] {
            let note = step.note.unwrap();
            assert_eq!(chord::label(&note).as_deref(), Some("Dm7"));
        }
        assert!(track.steps[3].note.is_none());
        assert_eq!(e.message.as_deref(), Some("Dm7"));
        let previewed = (effects.iter())
            .filter(|e| matches!(e, Effect::Preview { .. }))
            .count();
        assert_eq!(previewed, 4);
        assert!(e.run_command("chord Q").is_empty());
        assert!(e.message.unwrap().starts_with("Q has no root"));
    }
}
