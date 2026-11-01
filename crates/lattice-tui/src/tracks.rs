//! Keeps the engine's tracks, and each one's instrument, in line with the editor's as tracks are
//! added, removed and moved, and as instruments are chosen and set.

use lattice_core::{Grid, Instrument};
use lattice_editor::{Editor, TrackId};
use lattice_engine::{Command, Handle, Processor, Target, TrackPlayer, instrument};

use crate::samples;

pub struct Tracks {
    sample_rate: f32,
    /// A sampler's file that couldn't be read, to tell the user.
    problem: Option<String>,
    /// The engine's tracks, in order, and the instrument each has, as far as the commands sent
    /// so far go.
    engine: Vec<(TrackId, Instrument)>,
}

impl Tracks {
    /// The editor's tracks are playing, with the instruments from [`Tracks::instruments`].
    pub fn new(editor: &Editor, sample_rate: f32) -> Self {
        let problem = editor
            .grid
            .tracks
            .iter()
            .find_map(|t| samples::for_instrument(&t.instrument).err());
        Self {
            sample_rate,
            problem,
            engine: editor
                .track_ids()
                .iter()
                .zip(&editor.grid.tracks)
                .map(|(&id, track)| (id, track.instrument.clone()))
                .collect(),
        }
    }

    /// An instrument for each of the grid's tracks, for the engine to start with.
    pub fn instruments(grid: &Grid, sample_rate: f32) -> Vec<Box<dyn Processor>> {
        grid.tracks
            .iter()
            .map(|track| build(&track.instrument, sample_rate).0)
            .collect()
    }

    /// A sampler's file that couldn't be read since the last call.
    pub fn take_problem(&mut self) -> Option<String> {
        self.problem.take()
    }

    /// Sends the engine the commands that bring its tracks in line with the editor's: removing
    /// those that have gone, moving those that have moved and adding new ones, then swapping or
    /// setting instruments, built here off the audio thread. Returns false if the command queue
    /// filled up; the rest is sent on the next sync.
    pub fn sync(&mut self, editor: &Editor, handle: &mut Handle) -> bool {
        self.sync_with(editor, |command| handle.send(command))
    }

    fn sync_with(&mut self, editor: &Editor, mut send: impl FnMut(Command) -> bool) -> bool {
        let wanted = editor.track_ids();
        // From the back, so the indices still to visit don't shift.
        for index in (0..self.engine.len()).rev() {
            if !wanted.contains(&self.engine[index].0) {
                if !send(Command::RemoveTrack(index)) {
                    return false;
                }
                self.engine.remove(index);
            }
        }
        for (index, (&id, track)) in wanted.iter().zip(&editor.grid.tracks).enumerate() {
            if self.engine.get(index).map(|e| e.0) == Some(id) {
                continue;
            }
            // Everything before `index` already matches, so a moved track is further on.
            match self.engine.iter().position(|e| e.0 == id) {
                Some(from) => {
                    if !send(Command::MoveTrack { from, to: index }) {
                        return false;
                    }
                    let moved = self.engine.remove(from);
                    self.engine.insert(index, moved);
                }
                None => {
                    let (instrument, problem) = build(&track.instrument, self.sample_rate);
                    self.problem = problem.or(self.problem.take());
                    let player = TrackPlayer::new(track.clone(), instrument, self.sample_rate);
                    if !send(Command::InsertTrack { index, player }) {
                        return false;
                    }
                    self.engine.insert(index, (id, track.instrument.clone()));
                }
            }
        }
        for (index, track) in editor.grid.tracks.iter().enumerate() {
            let sent = &mut self.engine[index].1;
            let wanted = &track.instrument;
            if sent.kind != wanted.kind || sent.file != wanted.file {
                let (instrument, problem) = build(wanted, self.sample_rate);
                self.problem = problem.or(self.problem.take());
                if !send(Command::SetInstrument { index, instrument }) {
                    return false;
                }
                // The engine takes it to be at its defaults, to go back to after a step's lock,
                // so send it the settings that differ.
                *sent = Instrument::new(wanted.kind);
                sent.file = wanted.file.clone();
            }
            for (id, &value) in wanted.values.iter().enumerate() {
                if sent.values[id] != value {
                    let target = Target::Instrument(index);
                    if !send(Command::SetParam {
                        target,
                        id: id as u32,
                        value,
                    }) {
                        return false;
                    }
                    sent.values[id] = value;
                }
            }
        }
        true
    }
}

/// The processor for `instrument`, and why its file couldn't be read if it plays one that
/// can't (it's silent until it can).
fn build(instrument: &Instrument, sample_rate: f32) -> (Box<dyn Processor>, Option<String>) {
    let (sample, problem) = match samples::for_instrument(instrument) {
        Ok(sample) => (sample, None),
        Err(problem) => (None, Some(problem)),
    };
    (instrument::build(instrument, sample_rate, sample), problem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::InstKind;
    use lattice_editor::Key;

    fn command(editor: &mut Editor, line: &str) {
        for c in format!(":{line}").chars() {
            editor.handle(Key::Char(c));
        }
        editor.handle(Key::Enter);
    }

    /// Syncs, applying the commands to a list standing in for the engine's tracks, and returns
    /// the instrument commands sent.
    fn sync(tracks: &mut Tracks, editor: &Editor, engine: &mut Vec<TrackId>) -> Vec<String> {
        let wanted = editor.track_ids();
        let mut instruments = vec![];
        let synced = tracks.sync_with(editor, |command| {
            match command {
                Command::InsertTrack { index, .. } => engine.insert(index, wanted[index]),
                Command::RemoveTrack(index) => {
                    engine.remove(index);
                }
                Command::MoveTrack { from, to } => {
                    let id = engine.remove(from);
                    engine.insert(to, id);
                }
                Command::SetInstrument { index, .. } => {
                    instruments.push(format!("instrument {index}"));
                }
                Command::SetParam {
                    target: Target::Instrument(index),
                    id,
                    value,
                } => instruments.push(format!("{index}: {id} = {value}")),
                other => panic!("unexpected {other:?}"),
            }
            true
        });
        assert!(synced);
        assert_eq!(engine, wanted);
        instruments
    }

    fn editor() -> Editor {
        Editor::new(crate::demo::song().grid, 120.0)
    }

    #[test]
    fn engine_follows_the_editor_through_undo_and_redo() {
        let mut editor = editor();
        let mut tracks = Tracks::new(&editor, 48_000.0);
        let mut engine = editor.track_ids().to_vec();
        let script = [
            "ta",
            "ta",
            "trm",
            "tmv -2",
            "tmv +3",
            "track-add",
            "trm",
            "trm",
        ];
        for line in script {
            command(&mut editor, line);
            assert!(sync(&mut tracks, &editor, &mut engine).is_empty());
        }
        for key in ['u', 'U'] {
            for _ in script {
                editor.handle(Key::Char(key));
                sync(&mut tracks, &editor, &mut engine);
            }
        }
        let kinds: Vec<_> = tracks.engine.iter().map(|e| e.1.kind).collect();
        let wanted: Vec<_> = editor
            .grid
            .tracks
            .iter()
            .map(|t| t.instrument.kind)
            .collect();
        assert_eq!(kinds, wanted, "instruments moved with their tracks");
    }

    #[test]
    fn instruments_are_swapped_or_set() {
        let mut editor = editor();
        let mut tracks = Tracks::new(&editor, 48_000.0);
        let mut engine = editor.track_ids().to_vec();
        let last = editor.grid.tracks.len() - 1;
        for key in "gl".chars() {
            editor.handle(Key::Char(key));
        }
        command(&mut editor, "inst kick");
        assert_eq!(
            sync(&mut tracks, &editor, &mut engine),
            [format!("instrument {last}")]
        );
        assert_eq!(tracks.engine[last].1.kind, InstKind::Kick);

        // Pasted tracks bring their instrument, settings and all.
        for key in "gijllAgptytp".chars() {
            let key = if key == 'A' {
                Key::Ctrl('a')
            } else {
                Key::Char(key)
            };
            editor.handle(key);
        }
        assert_eq!(
            sync(&mut tracks, &editor, &mut engine),
            [format!("{last}: 2 = 0.51")]
        );
        assert_eq!(
            tracks.engine[last + 1].1,
            editor.grid.tracks[last].instrument
        );
    }
}
