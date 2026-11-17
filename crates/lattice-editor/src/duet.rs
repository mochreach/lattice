//! Two hands on one keyboard. In the duet layout (`:layout duet`) the left five columns are a
//! Tonnetz, for chords, and the rest the pads' scale, for a tune over them. `:hand left` and
//! `:hand right` give a hand the selection's tracks to play wherever the cursor is, so chords
//! can go to a pad while a lead screams over the top. A hand with several tracks deals the
//! notes of a chord out across them, one each, as a track holds one note on a step.

use crate::keymap::{Hand, hand};
use crate::{Editor, Effect, Key, NoteLayout, TrackId};

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
        let Key::Char(c) = key else {
            return vec![];
        };
        if self.keymap.layout != NoteLayout::Duet {
            return vec![];
        }
        let Some(ids) = hand(c).and_then(|h| self.hands.get(h as usize)) else {
            return vec![];
        };
        // A track removed since takes its place with it.
        (ids.iter())
            .filter_map(|id| self.ids.iter().position(|i| i == id))
            .collect()
    }

    /// Plays `pitch`, on `key`, with a hand that has tracks of its own, if it does: on one no
    /// other of the hand's keys is holding, or, all held, the one held longest. It's recorded
    /// when recording, and otherwise only heard, wherever the cursor is.
    pub(crate) fn play_with_hand(&mut self, key: Key, pitch: u8) -> Option<Vec<Effect>> {
        let tracks = self.hand_tracks(key);
        let id_of = |t: usize| self.ids.get(t).copied();
        let free = tracks.iter().copied().find(|&t| {
            let id = id_of(t);
            !self.voices.iter().any(|&(k, v)| k != key && Some(v) == id)
        });
        let oldest = || {
            (self.voices.iter())
                .find_map(|&(_, v)| tracks.iter().copied().find(|&t| id_of(t) == Some(v)))
        };
        let track = free.or_else(oldest).or(tracks.first().copied())?;
        let id = id_of(track)?;
        self.voices.retain(|&(k, v)| k != key && v != id);
        self.voices.push((key, id));
        if let Some(effects) = self.record_note(key, &[track], pitch, 0) {
            return Some(effects);
        }
        Some(vec![Effect::Preview { track, pitch }])
    }

    /// A key's come up, so the hand's track it was holding is free for the next note.
    pub(crate) fn release_voice(&mut self, key: Key) {
        self.voices.retain(|&(k, _)| k != key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Transport;
    use crate::{Mode, pads::SCALES};
    use lattice_core::{Grid, Step, Track};

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

    #[test]
    fn every_three_keys_together_on_the_left_are_a_triad() {
        let keys = NoteLayout::Duet.keys(&SCALES[0], 0);
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
        // The darker side below, the brighter above, and the odd chords in straight lines.
        assert_eq!(chord("gfv"), [0, 5, 8], "F minor");
        assert_eq!(chord("fr4"), [0, 4, 8], "augmented, up-left");
        assert_eq!(chord("zse4"), [2, 5, 8, 11], "diminished, up-right");
        // Three major thirds are an octave, so the number row is the bottom row again.
        for (low, high) in "zxcvb".chars().zip("12345".chars()) {
            assert_eq!(keys[&low], keys[&high]);
        }
        // Neighbours are never a semitone, a whole tone or a tritone apart.
        for (c, n) in [
            ('f', 'd'),
            ('f', 'r'),
            ('f', 't'),
            ('f', 'c'),
            ('f', 'v'),
            ('f', 'g'),
        ] {
            let gap = (keys[&c] - keys[&n])
                .rem_euclid(12)
                .min((keys[&n] - keys[&c]).rem_euclid(12));
            assert!(gap >= 3, "{c}{n}: {gap}");
        }
    }

    #[test]
    fn the_right_hand_plays_the_scale_an_octave_up_from_j() {
        let minor = SCALES.iter().find(|s| s.name == "minor").unwrap();
        let keys = NoteLayout::Duet.keys(minor, 9);
        let row: Vec<_> = "hjkl;'".chars().map(|c| keys[&c]).collect();
        // G# isn't in A minor: the leading note below is G.
        assert_eq!(row, [19, 21, 23, 24, 26, 28]);
        // A row up is a fourth, three steps; down, three steps under.
        assert_eq!([keys[&'u'], keys[&'m']], [26, 16]);
        assert_eq!(keys[&'f'], 9, "the left's tonic, an octave under");
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

        // Wherever the cursor is, a chord goes to a pad each and the tune to the lead.
        keys(&mut e, "thi");
        let chord = keys(&mut e, "fdr");
        assert_eq!(heard(&chord), [(0, 60), (1, 67), (2, 64)]);
        assert_eq!(heard(&keys(&mut e, "j")), [(3, 72)]);
        assert!(
            e.grid
                .tracks
                .iter()
                .all(|t| t.steps.iter().all(|s| s.note.is_none())),
            "only heard, stopped"
        );
        // Let go of the root, and the next note takes its track.
        e.release(Key::Char('f'));
        assert_eq!(heard(&keys(&mut e, "t")), [(0, 69)]);

        // Recording, the chord goes in on one step across the pads.
        for k in "fdrtj".chars() {
            e.release(Key::Char(k));
        }
        e.record = true;
        e.set_transport(Some(Transport {
            positions: vec![Some((4, 0)); 4],
            into_step: 0.1,
            count_in: 0,
        }));
        assert_eq!(e.mode, Mode::Insert);
        keys(&mut e, "gftk");
        let at = |t: usize| e.grid.tracks[t].steps[4].note.map(|n| n.pitch);
        assert_eq!(
            [at(0), at(1), at(2), at(3)],
            [Some(65), Some(60), Some(69), Some(74)]
        );

        // Off, the keys play the cursor's track again, pad3.
        e.run_command("hand off");
        e.set_transport(None);
        assert_eq!(heard(&keys(&mut e, "d")), [(2, 67)]);
    }
}
