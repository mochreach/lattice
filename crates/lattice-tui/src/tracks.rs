//! Keeps the engine's tracks, and each one's instrument, in line with the editor's as tracks are
//! added, removed and moved, and as instruments are chosen and set.

use lattice_core::{Grid, Instrument};
use lattice_editor::{Editor, TrackId};
use lattice_engine::{Command, Handle, Processor, Target, TrackPlayer, instrument};

pub struct Tracks {
    sample_rate: f32,
    /// The engine's tracks, in order, and the instrument each has, as far as the commands sent
    /// so far go.
    engine: Vec<(TrackId, Instrument)>,
}

impl Tracks {
    /// The editor's tracks are playing, with the instruments from [`Tracks::instruments`].
    pub fn new(editor: &Editor, sample_rate: f32) -> Self {
        Self {
            sample_rate,
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
            .map(|track| instrument::build(&track.instrument, sample_rate))
            .collect()
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
                    let player = TrackPlayer::new(
                        track.clone(),
                        instrument::build(&track.instrument, self.sample_rate),
                        self.sample_rate,
                    );
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
            if sent.kind != wanted.kind {
                let instrument = instrument::build(wanted, self.sample_rate);
                if !send(Command::SetInstrument { index, instrument }) {
                    return false;
                }
                *sent = wanted.clone();
                continue;
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
        Editor::new(crate::demo::song(), 120.0)
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
        for key in "gmglhhjjjjjjjAgptytp".chars() {
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
