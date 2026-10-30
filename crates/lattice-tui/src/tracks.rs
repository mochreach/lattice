//! Keeps the engine's tracks, and each one's instrument, in line with the editor's as tracks are
//! added, removed and moved.

use std::collections::HashMap;

use lattice_editor::{Editor, TrackId};
use lattice_engine::{Command, Handle, Patch, Synth, TrackPlayer};

pub struct Tracks {
    sample_rate: f32,
    /// Each track's patch, kept after the track is removed in case undo brings it back.
    patches: HashMap<TrackId, Patch>,
    /// New tracks take these in turn.
    palette: Vec<Patch>,
    next: usize,
    /// The engine's tracks, in order, as far as the commands sent so far go.
    engine: Vec<TrackId>,
}

impl Tracks {
    /// The editor's tracks are playing with `patches` in turn, as from [`Tracks::patches`].
    pub fn new(editor: &Editor, patches: Vec<Patch>, sample_rate: f32) -> Self {
        let ids = editor.track_ids();
        assert!(!patches.is_empty());
        Self {
            sample_rate,
            patches: ids
                .iter()
                .copied()
                .zip(patches.iter().copied().cycle())
                .collect(),
            palette: patches,
            next: ids.len(),
            engine: ids.to_vec(),
        }
    }

    /// Sends the engine the commands that bring its tracks in line with the editor's: removing
    /// those that have gone, moving those that have moved and adding new ones, built here off the
    /// audio thread. Returns false if the command queue filled up; the rest is sent on the next sync.
    pub fn sync(&mut self, editor: &Editor, handle: &mut Handle) -> bool {
        self.sync_with(editor, |command| handle.send(command))
    }

    fn sync_with(&mut self, editor: &Editor, mut send: impl FnMut(Command) -> bool) -> bool {
        let wanted = editor.track_ids();
        // From the back, so the indices still to visit don't shift.
        for index in (0..self.engine.len()).rev() {
            if !wanted.contains(&self.engine[index]) {
                if !send(Command::RemoveTrack(index)) {
                    return false;
                }
                self.engine.remove(index);
            }
        }
        for (index, &id) in wanted.iter().enumerate() {
            if self.engine.get(index) == Some(&id) {
                continue;
            }
            // Everything before `index` already matches, so a moved track is further on.
            let command = match self.engine.iter().position(|&e| e == id) {
                Some(from) => Command::MoveTrack { from, to: index },
                None => Command::InsertTrack {
                    index,
                    player: TrackPlayer::new(
                        editor.grid.tracks[index].clone(),
                        Box::new(Synth::new(self.patch(editor, id), self.sample_rate)),
                        self.sample_rate,
                    ),
                },
            };
            let moved = matches!(command, Command::MoveTrack { .. });
            if !send(command) {
                return false;
            }
            if moved {
                self.engine.retain(|&e| e != id);
            }
            self.engine.insert(index, id);
        }
        true
    }

    /// One of `palette` for each of `tracks` tracks, in turn: what a song starts with.
    pub fn patches(palette: &[Patch], tracks: usize) -> Vec<Patch> {
        palette.iter().copied().cycle().take(tracks).collect()
    }

    /// New tracks take the patches in turn from the first again, so an opened song's tracks
    /// sound as they would if Lattice had started with it.
    pub fn restart_palette(&mut self) {
        self.next = 0;
    }

    /// The track's patch: a pasted track's is the one it was copied from, and a new track takes
    /// the next from the palette.
    fn patch(&mut self, editor: &Editor, id: TrackId) -> Patch {
        let copied =
            std::iter::successors(editor.copied_from(id), |&from| editor.copied_from(from))
                .find_map(|from| self.patches.get(&from).copied());
        *self.patches.entry(id).or_insert_with(|| {
            if let Some(patch) = copied {
                return patch;
            }
            self.next += 1;
            self.palette[(self.next - 1) % self.palette.len()]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_editor::Key;

    fn command(editor: &mut Editor, line: &str) {
        for c in format!(":{line}").chars() {
            editor.handle(Key::Char(c));
        }
        editor.handle(Key::Enter);
    }

    /// Syncs, applying the commands to a list standing in for the engine's tracks.
    fn sync(tracks: &mut Tracks, editor: &Editor, engine: &mut Vec<TrackId>) {
        let wanted = editor.track_ids();
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
                other => panic!("unexpected {other:?}"),
            }
            true
        });
        assert!(synced);
        assert_eq!(engine, wanted);
    }

    #[test]
    fn engine_follows_the_editor_through_undo_and_redo() {
        let (grid, patches) = crate::demo::song();
        let mut editor = Editor::new(grid, 120.0);
        let mut tracks = Tracks::new(&editor, patches, 48_000.0);
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
            sync(&mut tracks, &editor, &mut engine);
        }
        assert_eq!(engine.len(), 3);
        for key in ['u', 'U'] {
            for _ in script {
                editor.handle(Key::Char(key));
                sync(&mut tracks, &editor, &mut engine);
            }
        }
        assert_eq!(
            tracks.patches.len(),
            6,
            "one patch for each track ever made"
        );
    }

    #[test]
    fn pasted_tracks_keep_the_patch_they_were_copied_from() {
        let (grid, patches) = crate::demo::song();
        let mut editor = Editor::new(grid, 120.0);
        let mut tracks = Tracks::new(&editor, patches, 48_000.0);
        let mut engine = editor.track_ids().to_vec();
        for key in "glty".chars() {
            editor.handle(Key::Char(key));
        }
        // Paste the last track twice, the second time copying the first copy.
        for key in "tptyghtP".chars() {
            editor.handle(Key::Char(key));
            sync(&mut tracks, &editor, &mut engine);
        }
        let ids = editor.track_ids();
        let last = ids[ids.len() - 2];
        let patch = |id| format!("{:?}", tracks.patches[&id]);
        assert_eq!(patch(ids[0]), patch(last));
        assert_eq!(patch(ids[ids.len() - 1]), patch(last));
    }
}
