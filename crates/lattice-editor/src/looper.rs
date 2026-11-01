//! The looper, on the editor's side: `:arm` arms a track to record the audio input, and while
//! record's on and the song plays, the frontend records it a pass at a time, each pass becoming
//! a sound file in the song's samples folder that the track plays, chopped a piece a step. Each
//! pass overdubs the last, and each can be undone.

use std::path::{Path, PathBuf};

use lattice_core::InstKind;

use crate::{Editor, Effect, TrackId};

/// The track armed to record, and whether its next pass starts afresh rather than overdubbing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Armed {
    pub track: TrackId,
    pub fresh: bool,
}

impl Editor {
    /// `:arm`, or `:arm!` to record afresh: arms the cursor's track, in place of any other.
    /// Arming it again only says so, rather than disarming it, which would be easy to miss.
    pub(crate) fn arm(&mut self, fresh: bool) {
        let t = self.cursor().track;
        let track = self.ids[t];
        let name = &self.grid.tracks[t].name;
        self.armed = Some(Armed { track, fresh });
        let how = if self.overdubbing(t).is_some() {
            "overdubbing its loop"
        } else {
            "a new loop"
        };
        self.message = Some(format!(
            "{name} armed, {how}: record on (R) and play to record a pass"
        ));
    }

    /// `:disarm`.
    pub(crate) fn disarm(&mut self) {
        self.message = Some(match self.armed() {
            Some(t) => format!("{} disarmed", self.grid.tracks[t].name),
            None => "nothing's armed".into(),
        });
        self.armed = None;
    }

    /// The armed track's index, if it's still there.
    pub fn armed(&self) -> Option<usize> {
        let armed = self.armed?;
        self.ids.iter().position(|&id| id == armed.track)
    }

    /// The sound file a pass recorded into track `t` is mixed onto: the one it plays, unless
    /// it's recording afresh, plays none, or has no notes left to play it with, when what's
    /// recorded over would be silent.
    pub fn overdubbing(&self, t: usize) -> Option<&Path> {
        let fresh = self
            .armed
            .is_some_and(|a| a.fresh && Some(t) == self.armed());
        let track = self.grid.tracks.get(t)?;
        let playing = track.steps.iter().any(|s| s.note.is_some());
        let instrument = &track.instrument;
        (!fresh && playing && instrument.kind == InstKind::Chop)
            .then_some(instrument.file.as_deref())
            .flatten()
    }

    /// Where to save the next pass: a new file in the song's samples folder.
    pub fn take_file(&self) -> PathBuf {
        self.sample_file("", "take")
    }

    /// A pass has been recorded for track `t` into `path`: the track plays it, cut into a piece
    /// a step if it wasn't playing a loop already, as one undo.
    pub fn recorded(&mut self, t: usize, path: PathBuf) -> Vec<Effect> {
        if t >= self.grid.tracks.len() {
            return vec![];
        }
        self.checkpoint();
        let overdub = self.overdubbing(t).is_some();
        if overdub {
            self.grid.tracks[t].instrument.file = Some(path);
        } else {
            self.chop_track(t, path);
        }
        if let Some(armed) = &mut self.armed {
            armed.fresh = false;
        }
        let name = &self.grid.tracks[t].name;
        self.message = Some(match overdub {
            true => format!("{name}: pass overdubbed"),
            false => format!("{name}: pass recorded"),
        });
        vec![Effect::TrackChanged(t), Effect::MixerChanged]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;
    use lattice_core::{Grid, Step, Track, instrument::chop};

    fn editor() -> Editor {
        let track = |name: &str| Track {
            name: name.into(),
            steps: vec![Step::default(); 8],
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

    fn run(e: &mut Editor, line: &str) {
        e.handle(Key::Char(':'));
        for c in line.chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
    }

    #[test]
    fn passes_record_a_loop_then_overdub_it() {
        let mut e = editor();
        run(&mut e, "arm");
        assert_eq!(e.armed(), Some(0));
        assert_eq!(e.overdubbing(0), None, "nothing to overdub yet");

        e.recorded(0, "/s/take-1.wav".into());
        let track = &e.grid.tracks[0];
        assert_eq!(track.instrument.kind, InstKind::Chop);
        assert_eq!(track.steps[3].note.unwrap().pitch, chop::pitch(3));
        assert_eq!(e.overdubbing(0), Some(Path::new("/s/take-1.wav")));

        // Played over, the next pass overdubs, keeping the steps as they are.
        e.grid.tracks[0].steps[1] = Step::default();
        e.recorded(0, "/s/take-2.wav".into());
        let track = &e.grid.tracks[0];
        assert_eq!(
            track.instrument.file.as_deref(),
            Some(Path::new("/s/take-2.wav"))
        );
        assert_eq!(track.steps[1], Step::default());

        // Each pass undoes on its own.
        e.handle(Key::Char('u'));
        let file = e.grid.tracks[0].instrument.file.clone();
        assert_eq!(file.as_deref(), Some(Path::new("/s/take-1.wav")));
    }

    #[test]
    fn a_loop_with_its_steps_deleted_records_afresh() {
        let mut e = editor();
        run(&mut e, "arm");
        e.recorded(0, "/s/take-1.wav".into());
        e.grid.tracks[0].steps.fill(Step::default());
        assert_eq!(e.overdubbing(0), None, "nothing plays it to overdub");
        e.recorded(0, "/s/take-2.wav".into());
        assert_eq!(
            e.grid.tracks[0].steps[3].note.unwrap().pitch,
            chop::pitch(3)
        );
    }

    #[test]
    fn arming_afresh_records_over_the_loop_once() {
        let mut e = editor();
        e.recorded(0, "/s/take-1.wav".into());
        run(&mut e, "arm!");
        assert_eq!(e.overdubbing(0), None);
        e.recorded(0, "/s/take-2.wav".into());
        assert_eq!(e.overdubbing(0), Some(Path::new("/s/take-2.wav")));
    }

    #[test]
    fn arm_follows_its_track_until_disarmed() {
        let mut e = editor();
        run(&mut e, "arm");
        run(&mut e, "track-move 1");
        assert_eq!(e.armed(), Some(1));
        run(&mut e, "arm");
        assert_eq!(e.armed(), Some(1), "arming again leaves it armed");
        run(&mut e, "disarm");
        assert_eq!(e.armed(), None);
        run(&mut e, "arm");
        run(&mut e, "track-remove");
        assert_eq!(e.armed(), None, "gone with its track");
    }
}
