//! The looper, on the editor's side: `:arm` arms a track to record the audio input, and while
//! record's on and the song plays, the frontend records it a pass at a time, each pass becoming
//! a sound file in the song's samples folder that the track plays, chopped a piece a step. Each
//! pass overdubs the last, and each can be undone.

use std::path::{Path, PathBuf};

use lattice_core::InstKind;

use crate::{Editor, Effect, TrackId};

/// The track armed to record, whether its next pass starts afresh rather than overdubbing, and
/// how many passes are left to record, if it's not to keep going.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Armed {
    pub track: TrackId,
    pub fresh: bool,
    pub passes: Option<u32>,
}

/// The most passes `:arm` counts.
const MAX_PASSES: u32 = 99;

impl Editor {
    /// `:arm [PASSES]`, or `:arm!` to record afresh: arms the cursor's track, in place of any
    /// other, to record `passes` passes and disarm, or to keep going if none are typed. Arming it
    /// again only says so, rather than disarming it, which would be easy to miss.
    pub(crate) fn arm(&mut self, fresh: bool, passes: &str) -> Result<(), String> {
        let passes = match passes.trim() {
            "" => None,
            typed => match typed.parse() {
                Ok(n @ 1..=MAX_PASSES) => Some(n),
                _ => return Err(format!("passes go from 1 to {MAX_PASSES}, not {typed}")),
            },
        };
        let t = self.cursor().track;
        let (Some(&track), Some(to_arm)) = (self.ids.get(t), self.grid.tracks.get(t)) else {
            return Err("no such track".into());
        };
        let name = to_arm.name.clone();
        self.armed = Some(Armed {
            track,
            fresh,
            passes,
        });
        let how = if self.overdubbing(t).is_some() {
            "overdubbing its loop"
        } else {
            "a new loop"
        };
        let until = match passes {
            Some(1) => " for one pass".into(),
            Some(n) => format!(" for {n} passes"),
            None => String::new(),
        };
        self.message = Some(format!(
            "{name} armed, {how}{until}: record on (R) and play to record"
        ));
        Ok(())
    }

    /// `:disarm`.
    pub(crate) fn disarm(&mut self) {
        let armed = self.armed().and_then(|t| self.grid.tracks.get(t));
        self.message = Some(match armed {
            Some(track) => format!("{} disarmed", track.name),
            None => "nothing's armed".into(),
        });
        self.armed = None;
    }

    /// How long a step lasts, in seconds, at `bpm`.
    pub fn step_seconds(&self, bpm: f64) -> f64 {
        60.0 / (bpm.max(1.0) * f64::from(self.grid.steps_per_beat.max(1)))
    }

    /// The slowest the song could play a pass at: the slower of its own tempo and `playing`
    /// (the conductor's now), or any tempo a conductor sets or slides to.
    pub fn slowest_tempo(&self, playing: f64) -> f64 {
        (self.arrangement.patterns.iter())
            .flat_map(|p| &p.conductor.steps)
            .flatten()
            .filter_map(|g| g.and_then(|g| g.tempo()))
            .map(f64::from)
            .fold(self.bpm.min(playing), f64::min)
    }

    /// How many more passes the armed track's to record, if it's not to keep going.
    pub fn passes_left(&self) -> Option<u32> {
        self.armed.and_then(|a| a.passes)
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
        let Some(track) = self.grid.tracks.get(t) else {
            self.message = Some("the track recorded into has gone".into());
            return vec![];
        };
        let name = track.name.clone();
        self.checkpoint();
        let overdub = self.overdubbing(t).is_some();
        if overdub {
            if let Some(track) = self.grid.tracks.get_mut(t) {
                track.instrument.file = Some(path);
            }
        } else {
            self.chop_track(t, path);
        }
        let mut done = false;
        if let Some(armed) = &mut self.armed {
            armed.fresh = false;
            if let Some(passes) = &mut armed.passes {
                *passes = passes.saturating_sub(1);
                done = *passes == 0;
            }
        }
        if done {
            self.armed = None;
        }
        let pass = match overdub {
            true => "pass overdubbed",
            false => "pass recorded",
        };
        self.message = Some(match done {
            true => format!("{name}: {pass}, and disarmed"),
            false => format!("{name}: {pass}"),
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
            beats_per_bar: 4,
            grouping: vec![],
            tracks: vec![track("a"), track("b")],
            groups: vec![],
        };
        Editor::new(grid, 120.0)
    }

    #[test]
    fn a_pass_is_given_room_for_the_slowest_tempo_it_could_take() {
        let mut e = editor();
        assert_eq!(e.slowest_tempo(120.0), 120.0);
        assert_eq!(e.slowest_tempo(90.0), 90.0, "the conductor's slowed it");
        let current = e.arrangement.current;
        e.arrangement.patterns[current].conductor = lattice_core::Conductor::new(4);
        e.arrangement.patterns[current].conductor.steps[2][0] =
            Some(lattice_core::SongGesture::Slide(60));
        assert_eq!(e.slowest_tempo(120.0), 60.0, "and will slide to 60");
        assert_eq!(e.step_seconds(60.0), 0.25);
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
    fn a_count_of_passes_disarms_after_them() {
        let mut e = editor();
        run(&mut e, "arm 2");
        assert_eq!(e.passes_left(), Some(2));
        e.recorded(0, "/s/take-1.wav".into());
        assert_eq!(e.passes_left(), Some(1));
        e.recorded(0, "/s/take-2.wav".into());
        assert_eq!(e.armed(), None);
        assert_eq!(
            e.message.as_deref(),
            Some("a: pass overdubbed, and disarmed")
        );
        run(&mut e, "arm 0");
        assert_eq!(e.armed(), None, "{:?}", e.message);
        run(&mut e, "arm");
        assert_eq!(e.passes_left(), None, "keeps going");
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
