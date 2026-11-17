//! Two hands on one keyboard. In the duet layout (`:layout duet`) each of the left seven
//! columns' keys plays a whole chord in the pads' scale, and the rest the scale itself, for a
//! tune over them. `:hand left` and `:hand right` give a hand the selection's tracks to play
//! wherever the cursor is, so chords can go to a pad while a lead screams over the top. A track
//! holds one note on a step, so a chord's notes are dealt out across the hand's tracks, or,
//! without them, across the cursors'.

use lattice_core::Note;

use crate::keymap::{Hand, hand};
use crate::{Cursor, Editor, Effect, Key, NoteLayout, TrackId, changed};

impl Editor {
    /// `:hand left|right|off`: the selection's tracks become the hand's, or with `off`, both
    /// hands go back to playing the cursor's track. Either hand lays the keys out as a duet.
    pub(crate) fn set_hand(&mut self, which: &str) -> Result<(), String> {
        let hand = match which.trim() {
            "left" => Hand::Left,
            "right" => Hand::Right,
            "off" | "" => {
                self.hands = Default::default();
                self.voices.clear();
                self.message = Some("both hands play the cursor's track".into());
                return Ok(());
            }
            typed => return Err(format!("left, right or off, not {typed}")),
        };
        let tracks = self.selected_tracks();
        let ids: Vec<TrackId> = (tracks.iter())
            .filter_map(|&t| self.ids.get(t).copied())
            .collect();
        if ids.is_empty() {
            return Err("no such track".into());
        }
        let names: Vec<String> = (tracks.iter())
            .filter_map(|&t| self.grid.tracks.get(t))
            .map(|t| t.name.clone())
            .collect();
        self.voices.retain(|(_, id)| !ids.contains(id));
        if let Some(slot) = self.hands.get_mut(hand as usize) {
            *slot = ids;
        }
        if self.keymap.layout != NoteLayout::Duet {
            self.lay_out_keys(NoteLayout::Duet);
        }
        self.message = Some(format!("{} hand plays {}", hand.name(), names.join(", ")));
        Ok(())
    }

    /// The tracks the hand that plays `key` has of its own, in the duet layout: none if it
    /// plays the cursor's.
    fn hand_tracks(&self, key: Key) -> Vec<usize> {
        if self.keymap.layout != NoteLayout::Duet {
            return vec![];
        }
        let c = match (self.keymap.duet_key(key), key) {
            (Some((c, _)), _) | (None, Key::Char(c)) => c,
            _ => return vec![],
        };
        let Some(ids) = hand(c).and_then(|h| self.hands.get(h as usize)) else {
            return vec![];
        };
        // A track removed since takes its place with it.
        (ids.iter())
            .filter_map(|id| self.ids.iter().position(|i| i == id))
            .collect()
    }

    /// The notes `key` plays at the keyboard octave: its chord under the left hand in the duet
    /// layout, voiced by Shift or Alt, or its one note.
    pub(crate) fn key_pitches(&self, key: Key) -> Vec<u8> {
        if let Some(chord) = self.keymap.chord(key) {
            return chord.iter().filter_map(|&n| self.octave_pitch(n)).collect();
        }
        self.piano_pitch(key).into_iter().collect()
    }

    /// Plays what `key` plays with a hand that has tracks of its own, if it does: each note on
    /// a track none of the hand's other keys is holding, or, all held, the one held longest; a
    /// note with no track left isn't played. It's recorded when recording, and otherwise only
    /// heard, wherever the cursor is.
    pub(crate) fn play_with_hand(&mut self, key: Key) -> Option<Vec<Effect>> {
        let tracks = self.hand_tracks(key);
        if tracks.is_empty() {
            return None;
        }
        // Pressed again without its release, it lets go of what it held.
        self.voices.retain(|&(k, _)| k != key);
        let mut effects = vec![];
        for pitch in self.key_pitches(key) {
            let Some(track) = self.take_voice(key, &tracks) else {
                break;
            };
            effects.extend(
                (self.record_note(key, &[track], pitch, 0))
                    .unwrap_or_else(|| vec![Effect::Preview { track, pitch }]),
            );
        }
        Some(effects)
    }

    /// One of `tracks` for a note of `key`'s: a free one, or the one another key has held
    /// longest.
    fn take_voice(&mut self, key: Key, tracks: &[usize]) -> Option<usize> {
        let id_of = |t: usize| self.ids.get(t).copied();
        let free = (tracks.iter().copied())
            .find(|&t| !self.voices.iter().any(|&(_, v)| Some(v) == id_of(t)));
        let oldest = || {
            (self.voices.iter())
                .filter(|&&(k, _)| k != key)
                .find_map(|&(_, v)| tracks.iter().copied().find(|&t| id_of(t) == Some(v)))
        };
        let track = free.or_else(oldest)?;
        let id = id_of(track)?;
        self.voices.retain(|&(_, v)| v != id);
        self.voices.push((key, id));
        Some(track)
    }

    /// A chord key in insert mode, with no hand of its own: its notes dealt out to the cursors'
    /// tracks, one each, the root to the primary's, and entered on their steps, or at the
    /// playhead when recording. Notes beyond the cursors are only heard, on the primary's track;
    /// with one cursor, that's all but the root.
    pub(crate) fn insert_chord(&mut self, key: Key, chord: &[u8]) -> Vec<Effect> {
        let primary = self.cursor().track;
        let mut cursors: Vec<Cursor> = self.selections.iter().map(|s| s.cursor).collect();
        cursors.sort_by_key(|c| (c.track != primary, c.track));
        cursors.dedup_by_key(|c| c.track);
        let dealt: Vec<(Cursor, u8)> = cursors.into_iter().zip(chord.iter().copied()).collect();
        let mut effects = vec![];
        if self.recording() {
            for &(c, pitch) in &dealt {
                // Recording, a note always goes in somewhere, so there's always an answer.
                effects.extend(
                    self.record_note(key, &[c.track], pitch, 0)
                        .unwrap_or_default(),
                );
            }
        } else {
            self.checkpoint();
            let velocity = self.touch.unwrap_or(self.velocity);
            for &(c, pitch) in &dealt {
                if let Some(step) =
                    (self.grid.tracks.get_mut(c.track)).and_then(|t| t.steps.get_mut(c.step))
                {
                    step.note = Some(Note {
                        pitch,
                        velocity,
                        length: 1,
                    });
                }
            }
            self.advance();
            effects.extend(changed(dealt.iter().map(|(c, _)| c.track)));
            effects.extend((dealt.iter()).map(|&(c, pitch)| Effect::Preview {
                track: c.track,
                pitch,
            }));
        }
        effects.extend(
            (chord.iter().skip(dealt.len())).map(|&pitch| Effect::Preview {
                track: primary,
                pitch,
            }),
        );
        effects
    }

    /// A key's come up, so the hand's track it was holding is free for the next note.
    pub(crate) fn release_voice(&mut self, key: Key) {
        self.voices.retain(|&(k, _)| k != key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mode;
    use crate::keymap::chords;
    use crate::pads::{SCALES, Scale};
    use crate::record::Transport;
    use lattice_core::{Grid, Step, Track};
    use std::collections::HashMap;

    fn editor() -> Editor {
        let track = |name: &str| Track {
            face: None,
            pattern_mute: false,
            columns: vec![],
            name: name.into(),
            steps: vec![Step::default(); 16],
            mix: Default::default(),
            instrument: Default::default(),
            effects: Default::default(),
            choke: 0,
            macros: vec![],
            knobs: vec![],
            rules: vec![],
            looper: None,
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            beats_per_bar: 4,
            grouping: vec![],
            tracks: ["pad1", "pad2", "pad3", "lead"].map(track).to_vec(),
            groups: vec![],
        };
        Editor::new(grid, 120.0)
    }

    fn keys(e: &mut Editor, s: &str) -> Vec<Effect> {
        s.chars().flat_map(|c| e.handle(Key::Char(c))).collect()
    }

    /// The tracks and pitches `effects` sound.
    fn heard(effects: &[Effect]) -> Vec<(usize, u8)> {
        (effects.iter())
            .filter_map(|e| match *e {
                Effect::Preview { track, pitch } | Effect::Hold { track, pitch, .. } => {
                    Some((track, pitch))
                }
                _ => None,
            })
            .collect()
    }

    fn named(name: &str) -> &'static Scale {
        SCALES.iter().find(|s| s.name == name).unwrap()
    }

    #[test]
    fn every_three_keys_together_in_the_tonnetz_are_a_triad() {
        let keys = NoteLayout::Tonnetz.keys(named("major"), 0);
        let chord = |k: &str| {
            let mut notes: Vec<i16> = k.chars().map(|c| keys[&c]).collect();
            notes.sort_unstable();
            notes
        };
        // C major's six chords, close together in the octave from C, the tonic under F.
        assert_eq!(chord("fdr"), [0, 4, 7], "C");
        assert_eq!(chord("frt"), [0, 4, 9], "A minor");
        assert_eq!(chord("gft"), [0, 5, 9], "F");
        assert_eq!(chord("dse"), [2, 7, 11], "G");
        assert_eq!(chord("der"), [4, 7, 11], "E minor");
        assert_eq!(chord("asz"), [2, 5, 9], "D minor");
        assert_eq!(chord("gfv"), [0, 5, 8], "F minor");
        // The odd chords lie in straight lines.
        assert_eq!(chord("fr4"), [0, 4, 8], "augmented, up-left");
        assert_eq!(chord("zse4"), [2, 5, 8, 11], "diminished, up-right");
        // Three major thirds are an octave, so the number row is the bottom row again.
        for (low, high) in "zxcvb".chars().zip("12345".chars()) {
            assert_eq!(keys[&low], keys[&high]);
        }
        // Across the whole keyboard, neighbours are never a semitone, tone or tritone apart.
        for (c, n) in [
            ('f', 'd'),
            ('f', 'r'),
            ('f', 't'),
            ('f', 'c'),
            ('f', 'v'),
            ('j', 'k'),
        ] {
            let gap = (keys[&c] - keys[&n]).rem_euclid(12);
            assert!((3..=9).contains(&gap), "{c}{n}: {gap}");
        }
    }

    /// A chord key's notes, as pitch classes from C, the bass last.
    fn chord_of(chords: &HashMap<char, Vec<i16>>, c: char) -> Vec<i16> {
        chords[&c].iter().map(|n| n.rem_euclid(12)).collect()
    }

    #[test]
    fn each_column_on_the_left_is_a_chord_of_the_scale() {
        let chords = chords(named("major"), 0);
        let row =
            |keys: &str| -> Vec<Vec<i16>> { keys.chars().map(|c| chord_of(&chords, c)).collect() };
        // C Dm Em F G Am B°, one to seven, each with its bass.
        assert_eq!(
            row("asdfghj"),
            [
                vec![0, 4, 7, 0],
                vec![2, 5, 9, 2],
                vec![4, 7, 11, 4],
                vec![5, 9, 0, 5],
                vec![7, 11, 2, 7],
                vec![9, 0, 4, 9],
                vec![11, 2, 5, 11]
            ]
        );
        // Above, the other quality: Cm, then D, E, A and B major from outside the key.
        assert_eq!(
            row("qwertyu"),
            [
                vec![0, 3, 7, 0],
                vec![2, 6, 9, 2],
                vec![4, 8, 11, 4],
                vec![5, 8, 0, 5],
                vec![7, 10, 2, 7],
                vec![9, 1, 4, 9],
                vec![11, 3, 6, 11]
            ]
        );
        // Above that, the scale's sevenths: Cmaj7, G7, and B half-diminished.
        assert_eq!(chord_of(&chords, '1'), [0, 4, 7, 11, 0], "Cmaj7");
        assert_eq!(chord_of(&chords, '5'), [7, 11, 2, 5, 7], "G7");
        assert_eq!(chord_of(&chords, '7'), [11, 2, 5, 9, 11], "Bø7");
        // Below, the third in the bass: C/E, G/B.
        assert_eq!(chord_of(&chords, 'z'), [0, 4, 7, 4]);
        assert_eq!(chord_of(&chords, 'b'), [7, 11, 2, 11]);
        // Close together in the octave from the root, with the bass an octave under.
        assert_eq!(chords[&'g'], [7, 11, 2, -5]);
        // The root goes first, as the one note a lone cursor takes.
        assert_eq!(NoteLayout::Duet.keys(named("major"), 0)[&'g'], 7);
    }

    #[test]
    fn a_minor_key_has_the_major_fifth_above_its_own() {
        // In A: Am B° C Dm Em F G, with E major above Em.
        let chords = chords(named("minor"), 9);
        let roots: Vec<_> = "asdfghj".chars().map(|c| chord_of(&chords, c)[0]).collect();
        assert_eq!(roots, [9, 11, 0, 2, 4, 5, 7]);
        assert_eq!(chord_of(&chords, 't'), [4, 8, 11, 4], "E major");
    }

    #[test]
    fn harmonic_minor_brings_its_augmented_and_diminished_chords() {
        // In A: Am B° C+ Dm E F G#°.
        let chords = chords(named("harmonic-minor"), 9);
        assert_eq!(chord_of(&chords, 's'), [11, 2, 5, 11], "B diminished");
        assert_eq!(chord_of(&chords, 'd'), [0, 4, 8, 0], "C augmented");
        assert_eq!(chord_of(&chords, 'g'), [4, 8, 11, 4], "E major");
        assert_eq!(chord_of(&chords, 'j'), [8, 11, 2, 8], "G# diminished");
        assert_eq!(chord_of(&chords, '5'), [4, 8, 11, 2, 4], "E7");
        // Above B°, B major, leading to E.
        assert_eq!(chord_of(&chords, 'w'), [11, 3, 6, 11]);
    }

    #[test]
    fn scales_without_seven_notes_still_make_seven_chords() {
        // C pentatonic has no F or B, so their chords are added; D gets a suspended fourth.
        let pentatonic = chords(named("pentatonic"), 0);
        assert_eq!(chord_of(&pentatonic, 's'), [2, 7, 9, 2], "Dsus4");
        assert_eq!(chord_of(&pentatonic, 'f'), [5, 9, 0, 5], "F");
        // Every key of every scale has a chord of three notes at least, and a bass.
        for scale in SCALES {
            let keys = chords(&scale, 0);
            assert_eq!(keys.len(), 28, "{}", scale.name);
            assert!(keys.values().all(|c| c.len() >= 4), "{}", scale.name);
        }
    }

    #[test]
    fn shift_and_alt_voice_a_chord_an_inversion_up_and_down() {
        let mut e = editor();
        e.releases = true;
        e.run_command("layout duet");
        keys(&mut e, "tltli");
        // C as laid out, C E G over a C bass; with Shift the C goes up to the top, E G C;
        // with Alt the G comes down under, G C E. The root goes in, wherever it went.
        assert_eq!(
            heard(&keys(&mut e, "a")),
            [(2, 60), (2, 64), (2, 67), (2, 48)]
        );
        assert_eq!(
            heard(&keys(&mut e, "A")),
            [(2, 72), (2, 64), (2, 67), (2, 48)]
        );
        assert_eq!(e.grid.tracks[2].steps[1].note.map(|n| n.pitch), Some(72));
        assert_eq!(
            heard(&e.handle(Key::Alt('a'))),
            [(2, 60), (2, 64), (2, 55), (2, 48)]
        );
        // With Shift, the number row's symbols are its keys, and `R` and `!` are chords here,
        // not record and the loop pedal.
        assert_eq!(heard(&keys(&mut e, "%")).first(), Some(&(2, 67)), "G7");
        assert_eq!(heard(&keys(&mut e, "R")).first(), Some(&(2, 65)), "Fm");
        assert!(!e.record);

        // A hand's tracks are freed when the key comes up, though Shift came up first.
        e.handle(Key::Esc);
        keys(&mut e, "vthth");
        e.run_command("hand left");
        keys(&mut e, "i");
        assert_eq!(
            heard(&keys(&mut e, "D")),
            [(0, 76), (1, 67), (2, 71)],
            "G B E"
        );
        e.release(Key::Char('d'));
        assert!(e.voices.is_empty());
    }

    #[test]
    fn the_right_hand_plays_on_while_the_left_holds_shift_or_alt() {
        let mut e = editor();
        e.releases = true;
        e.run_command("layout duet");
        keys(&mut e, "tltli");
        // With Shift held for the chord, K is k and { is [: notes, not the octave.
        keys(&mut e, "A");
        assert_eq!(heard(&keys(&mut e, "K")), [(2, 72)]);
        assert_eq!(
            heard(&keys(&mut e, "{")),
            [(2, 83)],
            "B, an octave and a seventh up"
        );
        assert_eq!(e.octave, 4);
        assert_eq!(heard(&e.handle(Key::Alt('l'))), [(2, 74)]);

        // A hand's track is freed when k comes up, though it went down as K.
        e.handle(Key::Esc);
        e.run_command("hand right");
        keys(&mut e, "i");
        assert_eq!(heard(&keys(&mut e, "K")), [(2, 72)]);
        e.release(Key::Char('k'));
        assert!(e.voices.is_empty());
    }

    #[test]
    fn the_right_hand_plays_the_scale_an_octave_up_from_k() {
        let keys = NoteLayout::Duet.keys(named("minor"), 9);
        let row: Vec<_> = "kl;'".chars().map(|c| keys[&c]).collect();
        assert_eq!(row, [21, 23, 24, 26]);
        // A row up is a fourth, three steps; down, three steps under.
        assert_eq!([keys[&'i'], keys[&',']], [26, 16]);
    }

    #[test]
    fn a_chord_key_with_no_hand_deals_its_notes_out_to_the_cursors() {
        let mut e = editor();
        e.releases = true;
        e.run_command("layout duet");
        // One cursor, on pad3: the whole chord's heard, and its root goes in.
        keys(&mut e, "tltli");
        let chord = keys(&mut e, "a");
        assert_eq!(heard(&chord), [(2, 60), (2, 64), (2, 67), (2, 48)]);
        assert_eq!(e.grid.tracks[2].steps[0].note.map(|n| n.pitch), Some(60));
        assert_eq!(e.cursor().step, 1, "moved on once, for the chord");
        // The right hand plays the cursor's track, a note at a time.
        assert_eq!(heard(&keys(&mut e, "k")), [(2, 72)]);

        // Three cursors, one on each pad: a note each, the bass heard on the primary's.
        e.handle(Key::Esc);
        e.selections = (0..3)
            .map(|track| crate::Selection::point(Cursor { track, step: 0 }))
            .collect();
        e.primary = 0;
        let primary = e.cursor().track;
        keys(&mut e, "i");
        let chord = keys(&mut e, "g");
        let at = |e: &Editor, t: usize| e.grid.tracks[t].steps[0].note.map(|n| n.pitch);
        let mut placed: Vec<_> = (0..3).filter_map(|t| at(&e, t)).collect();
        placed.sort_unstable();
        assert_eq!(placed, [62, 67, 71], "G's root, third and fifth, D G B");
        assert!(heard(&chord).contains(&(primary, 55)), "the bass, heard");
    }

    #[test]
    fn the_instrument_page_plays_a_whole_chord_on_one_key() {
        let mut e = editor();
        e.run_command("layout duet");
        keys(&mut e, "gii");
        let before = e.grid.tracks.clone();
        assert_eq!(
            heard(&keys(&mut e, "a")),
            [(0, 60), (0, 64), (0, 67), (0, 48)]
        );
        assert_eq!(heard(&keys(&mut e, "k")), [(0, 72)]);
        assert_eq!(e.grid.tracks, before, "heard, not entered");
    }

    #[test]
    fn hands_play_their_own_tracks_and_spread_chords_over_them() {
        let mut e = editor();
        e.releases = true;
        // The pads for the left hand, the lead for the right.
        keys(&mut e, "vtltl");
        e.run_command("hand left");
        assert_eq!(
            e.message.as_deref(),
            Some("left hand plays pad1, pad2, pad3")
        );
        assert_eq!(e.keymap.layout, NoteLayout::Duet);
        e.handle(Key::Esc);
        keys(&mut e, "tl");
        e.run_command("hand right");
        assert_eq!(e.message.as_deref(), Some("right hand plays lead"));

        // Wherever the cursor is, a chord's notes go a pad each, the bass left out for want
        // of a fourth, and the tune to the lead.
        keys(&mut e, "thi");
        assert_eq!(heard(&keys(&mut e, "a")), [(0, 60), (1, 64), (2, 67)]);
        assert_eq!(heard(&keys(&mut e, "k")), [(3, 72)]);
        assert!(
            (e.grid.tracks.iter()).all(|t| t.steps.iter().all(|s| s.note.is_none())),
            "only heard, stopped"
        );
        // The next chord, the old one still held, takes the pads it held longest.
        assert_eq!(heard(&keys(&mut e, "g")), [(0, 67), (1, 71), (2, 62)]);

        // Recording, a chord goes in on one step across the pads.
        for k in "agk".chars() {
            e.release(Key::Char(k));
        }
        e.record = true;
        e.set_transport(Some(Transport {
            positions: vec![Some((4, 0)); 4],
            into_step: 0.1,
            count_in: 0,
        }));
        assert_eq!(e.mode, Mode::Insert);
        keys(&mut e, "fl");
        let at = |t: usize| e.grid.tracks[t].steps[4].note.map(|n| n.pitch);
        assert_eq!(
            [at(0), at(1), at(2), at(3)],
            [Some(65), Some(69), Some(60), Some(74)],
            "F A C, and the lead's D"
        );

        // Off, the keys play the cursor's track again, pad3.
        e.run_command("hand off");
        e.set_transport(None);
        assert_eq!(heard(&keys(&mut e, "k")), [(2, 72)]);
    }
}
