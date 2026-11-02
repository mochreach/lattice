//! Live recording. With record armed (`R` or `:rec`) and the song playing, the notes played in
//! insert mode, piano keys and pads alike, go in where the playhead is, to the nearest step,
//! rather than at the cursor. Each pass through the track is one undo, so a bad take comes out
//! whole. When the frontend reports key releases, a note is as long as it was held. When record is
//! armed, play starts with a count-in (`:count`), and a note played just before the song starts
//! goes on its first step.

use lattice_core::Note;

use crate::{Editor, Effect, Key, Mode, Page, changed};

/// Where playback is, as the frontend last saw it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transport {
    /// Per track index: the step playing and which pass through the track it's on, or `None`
    /// if it hasn't played yet.
    pub positions: Vec<Option<(usize, u32)>>,
    /// How far into those steps playback is, from 0 to 1.
    pub into_step: f64,
    /// Steps of the count-in still to come, counting the one sounding; 0 once the song starts.
    pub count_in: usize,
}

/// A recorded note whose key is still down, to be given its length when it comes up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Held {
    key: Key,
    track: usize,
    step: usize,
    pitch: u8,
    /// Where it was played, in steps since the track started.
    from: f64,
    /// Repeated on every step that's a multiple of this, while held; 0 for not repeated.
    every: usize,
}

impl Transport {
    /// Where `track` is, in steps since it started, to a fraction of a step; negative while
    /// counting in.
    pub(crate) fn at(&self, track: usize, len: usize) -> Option<f64> {
        if self.count_in > 0 {
            return Some(self.into_step - self.count_in as f64);
        }
        let (step, pass) = (*self.positions.get(track)?)?;
        Some((u64::from(pass) * len as u64 + step as u64) as f64 + self.into_step)
    }
}

impl Editor {
    /// Tells the editor where playback is, or that it's stopped (`None`), before each key.
    pub fn set_transport(&mut self, transport: Option<Transport>) {
        if transport.is_none() {
            self.take = None;
            self.held.clear();
            self.live_lock = None;
        }
        self.transport = transport;
    }

    /// Whether the notes played now are recorded at the playhead.
    pub fn recording(&self) -> bool {
        self.record && self.mode == Mode::Insert && self.transport.is_some()
    }

    /// Arms or disarms recording. Arming starts insert mode, ready to play.
    pub(crate) fn toggle_record(&mut self) {
        self.record = !self.record;
        self.held.clear();
        self.take = None;
        self.live_lock = None;
        if self.record && self.page == Page::Mixer {
            self.message =
                Some("record armed: settings changed while the song plays are locked".into());
        } else if self.record && self.page == Page::Instrument {
            self.start_playing();
            self.message = Some(
                "record armed: notes played and settings turned while the song plays are recorded"
                    .into(),
            );
        } else if self.record {
            if self.mode != Mode::Insert {
                self.insert_at(false);
            }
            self.message = Some("record armed: play notes while the song plays".into());
        } else {
            self.message = Some("record off".into());
        }
    }

    /// Enters `pitch`, played on `key`, at the playhead of each of `tracks`, rounded to the
    /// nearest step. `None` if not recording, so the note goes in at the cursor as usual.
    pub(crate) fn record_note(
        &mut self,
        key: Key,
        tracks: &[usize],
        pitch: u8,
        every: usize,
    ) -> Option<Vec<Effect>> {
        if !self.recording() {
            return None;
        }
        let transport = self.transport.as_ref()?;
        let mut notes = vec![];
        for &track in tracks {
            let len = self.track_len(track);
            // Too early in the count-in for the first step, it's only heard.
            if let Some(from) = transport.at(track, len).filter(|&f| len > 0 && f >= -0.5) {
                let at = from.round().max(0.0) as u64;
                notes.push((track, (at % len as u64) as usize, at / len as u64, from));
            }
        }
        let Some(&(_, _, pass, from)) = notes.first() else {
            return Some(vec![Effect::Preview {
                track: tracks[0],
                pitch,
            }]);
        };
        // Each pass through the primary track is one take, undone together.
        if self.take != Some(pass) {
            self.checkpoint();
            self.take = Some(pass);
        }
        for &(track, step, _, from) in &notes {
            self.grid.tracks[track].steps[step].note = Some(Note {
                pitch,
                velocity: self.velocity,
                length: 1,
            });
            self.held.retain(|h| h.key != key || h.track != track);
            self.held.push(Held {
                key,
                track,
                step,
                pitch,
                from,
                every,
            });
        }
        let mut effects = changed(notes.iter().map(|n| n.0));
        // Rounded back onto a step that's already played, it's sounded now; rounded on to the
        // next, the sequencer plays it there, in time.
        if from.round() <= from {
            effects.push(Effect::Preview {
                track: notes[0].0,
                pitch,
            });
        }
        Some(effects)
    }

    /// A key has come up: the notes it's previewing end, and the notes it recorded last as long
    /// as it was held.
    pub fn release(&mut self, key: Key) -> Vec<Effect> {
        // The numpad's `0` coming up ends note repeat.
        let repeats = key == Key::Pad(0);
        if repeats {
            self.repeat = false;
        }
        let mut effects = self.release_previews(key, repeats);
        effects.extend(self.release_recorded(key));
        effects
    }

    fn release_recorded(&mut self, key: Key) -> Vec<Effect> {
        let Some(transport) = &self.transport else {
            return vec![];
        };
        let mut tracks = vec![];
        let velocity = self.velocity;
        for h in self.held.iter().filter(|h| h.key == key) {
            let len = self.track_len(h.track);
            let Some(to) = transport.at(h.track, len) else {
                continue;
            };
            // Repeated: a hit on each step of the repeat's grid it was held through.
            if h.every > 0 {
                let first = h.from.ceil().max(0.0) as u64;
                let steps = &mut self.grid.tracks[h.track].steps;
                for at in (first..=to.floor().max(0.0) as u64).filter(|a| a % h.every as u64 == 0) {
                    steps[(at % len as u64) as usize].note = Some(Note {
                        pitch: h.pitch,
                        velocity,
                        length: 1,
                    });
                    tracks.push(h.track);
                }
                continue;
            }
            let length = (to - h.from).round().clamp(1.0, len as f64) as u16;
            if let Some(note) = &mut self.grid.tracks[h.track].steps[h.step].note
                && note.pitch == h.pitch
                && note.length != length
            {
                note.length = length;
                tracks.push(h.track);
            }
        }
        self.held.retain(|h| h.key != key);
        changed(tracks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{Grid, Step, Track};

    fn editor() -> Editor {
        let track = |len| Track {
            face: None,
            pattern_mute: false,
            name: "t".into(),
            steps: vec![Step::default(); len],
            mix: Default::default(),
            instrument: Default::default(),
            effects: Default::default(),
            choke: 0,
            macros: vec![],
            rules: vec![],
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![track(16), track(8)],
            groups: vec![],
        };
        Editor::new(grid, 120.0)
    }

    /// Playback on step `step` of every track (pass 0), `into` the way through it.
    fn at(e: &mut Editor, step: usize, pass: u32, into: f64) {
        e.set_transport(Some(Transport {
            positions: vec![Some((step, pass)); 2],
            into_step: into,
            count_in: 0,
        }));
    }

    fn pitches(e: &Editor, track: usize) -> Vec<(usize, u8, u16)> {
        e.grid.tracks[track]
            .steps
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.note.map(|n| (i, n.pitch, n.length)))
            .collect()
    }

    #[test]
    fn notes_go_in_at_the_nearest_step_while_playing() {
        let mut e = editor();
        e.handle(Key::Char('R'));
        assert!(e.record);
        assert_eq!(e.mode, Mode::Insert);

        // Stopped, notes go in at the cursor as ever.
        e.handle(Key::Char('z'));
        assert_eq!(pitches(&e, 0), [(0, 60, 1)]);
        assert_eq!(e.cursor().step, 1);

        // Playing, early in step 5: sounded now, entered there, and the cursor stays put.
        at(&mut e, 5, 0, 0.2);
        let effects = e.handle(Key::Char('x'));
        assert!(effects.contains(&Effect::Preview {
            track: 0,
            pitch: 62
        }));
        // Late in step 7: on to step 8, which the sequencer will play.
        at(&mut e, 7, 0, 0.7);
        let effects = e.handle(Key::Char('c'));
        assert_eq!(effects, [Effect::TrackChanged(0)]);
        // Late in the last step: round to the top of the track.
        at(&mut e, 15, 0, 0.9);
        e.handle(Key::Char('v'));
        assert_eq!(pitches(&e, 0), [(0, 65, 1), (5, 62, 1), (8, 64, 1)]);
        assert_eq!(e.cursor().step, 1);

        // Disarmed, it's ordinary insert mode again.
        e.handle(Key::Char('R'));
        e.handle(Key::Char('z'));
        assert_eq!(e.grid.tracks[0].steps[1].note.unwrap().pitch, 60);
    }

    #[test]
    fn held_keys_set_the_length_and_each_pass_is_one_undo() {
        let mut e = editor();
        e.releases = true;
        e.handle(Key::Char('R'));
        at(&mut e, 2, 0, 0.1);
        e.handle(Key::Char('z'));
        at(&mut e, 4, 0, 0.0);
        e.handle(Key::Char('x'));
        at(&mut e, 5, 0, 0.3);
        assert_eq!(
            e.release(Key::Char('z')),
            [
                Effect::Release {
                    track: 0,
                    pitch: 60
                },
                Effect::TrackChanged(0)
            ]
        );
        assert!(e.release(Key::Char('z')).is_empty(), "already up");
        at(&mut e, 4, 0, 0.6);
        e.release(Key::Char('x'));
        assert_eq!(pitches(&e, 0), [(2, 60, 3), (4, 62, 1)]);

        // The next pass is a new take; undo takes it out and leaves the first.
        at(&mut e, 1, 1, 0.0);
        e.handle(Key::Char('c'));
        e.handle(Key::Char('v'));
        assert_eq!(pitches(&e, 0), [(1, 65, 1), (2, 60, 3), (4, 62, 1)]);
        e.handle(Key::Esc);
        e.handle(Key::Char('u'));
        assert_eq!(pitches(&e, 0), [(2, 60, 3), (4, 62, 1)]);
        // Normal mode doesn't record, and `u` once more takes out the first take.
        assert!(!e.recording());
        e.handle(Key::Char('u'));
        assert_eq!(pitches(&e, 0), []);
    }

    #[test]
    fn a_shorter_track_records_on_its_own_pass() {
        let mut e = editor();
        e.handle(Key::Char('l'));
        e.handle(Key::Char(':'));
        for c in "rec".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        assert!(e.record);
        // Track 1 is 8 steps long, so it's on step 2 of its second pass while track 0 is on 10.
        e.set_transport(Some(Transport {
            positions: vec![Some((10, 0)), Some((2, 1))],
            into_step: 0.0,
            count_in: 0,
        }));
        e.handle(Key::Char('z'));
        assert_eq!(pitches(&e, 0), []);
        assert_eq!(pitches(&e, 1), [(2, 60, 1)]);

        // Stopping ends the take.
        e.set_transport(None);
        assert!(e.take.is_none());
    }

    #[test]
    fn a_note_just_before_the_count_in_ends_goes_on_the_first_step() {
        let mut e = editor();
        e.handle(Key::Char('R'));
        let counting = |left, into| Transport {
            positions: vec![None, None],
            into_step: into,
            count_in: left,
        };
        // Early in the count-in: heard, not recorded, and not entered at the cursor either.
        e.set_transport(Some(counting(9, 0.5)));
        assert_eq!(
            e.handle(Key::Char('z')),
            [Effect::Preview {
                track: 0,
                pitch: 60
            }]
        );
        assert_eq!(pitches(&e, 0), []);
        // Half a step early: on step 0, played there by the sequencer, and held into the song.
        e.set_transport(Some(counting(1, 0.7)));
        assert_eq!(e.handle(Key::Char('x')), [Effect::TrackChanged(0)]);
        at(&mut e, 1, 0, 0.8);
        e.release(Key::Char('x'));
        assert_eq!(pitches(&e, 0), [(0, 62, 2)]);
    }
}
