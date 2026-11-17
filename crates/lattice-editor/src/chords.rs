//! Chords on the grid: a note that plays more notes with it, written by name with `:chord`.

use lattice_core::{Chord, Note, chord};

use crate::{Editor, Effect, changed, octave_pitch};

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

/// What sounds `note` on `track` as it's entered: each of a chord's notes.
pub(crate) fn previews(track: usize, note: Note) -> impl Iterator<Item = Effect> {
    (note.chord.pitches(note.pitch)).map(move |pitch| Effect::Preview { track, pitch })
}

impl Editor {
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
        let mut effects = changed(columns.iter().map(|(t, _)| *t));
        if let Some(note) = last {
            effects.extend(previews(self.cursor().track, note));
            self.message = chord::label(&note);
        }
        Ok(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{editor, keys};

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
