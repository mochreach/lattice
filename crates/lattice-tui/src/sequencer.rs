//! Keeps the engine's pattern length, conductor and cue in line with the editor's, and tells the
//! editor when the engine moves from one pattern to the next, or the conductor does something.

use lattice_core::{Conductor, Lane};
use lattice_editor::{Editor, Next};
use lattice_engine::{Command, Conducted, Cue, Handle, Section};

#[derive(Default)]
pub struct Sequencer {
    /// The engine's boundaries and pattern changes, as far as the editor's been told.
    boundaries: u64,
    swaps: u64,
    /// The cue sent, with the lanes and length it carried, so it's sent again if they change.
    cued: Option<(Next, Vec<Lane>, u64)>,
    /// The pattern length sent.
    length: Option<u64>,
    /// The playing pattern's conductor sent.
    conductor: Option<Conductor>,
    /// The song's mutes sent, now and next.
    mutes: Option<(u32, u32)>,
    playing: bool,
}

impl Sequencer {
    /// The engine's pattern changes the editor has seen, for [`Command::SetTrack`].
    pub fn swaps(&self) -> u64 {
        self.swaps
    }

    /// Call every frame, before handling keys.
    pub fn sync(&mut self, editor: &mut Editor, handle: &mut Handle) {
        let status = handle.status();
        let playing = status.playing();
        if self.playing && !playing {
            // Stopping clears the engine's counters and its cue.
            editor.stopped();
            self.boundaries = 0;
            self.swaps = 0;
            self.cued = None;
            self.mutes = None;
            self.conductor = None;
        }
        self.playing = playing;
        let (boundaries, swaps) = (status.boundaries(), status.swaps());
        if boundaries > self.boundaries {
            // Patterns last longer than a frame, so there's never more than one to catch up on.
            self.boundaries = boundaries;
            let changed_to = match self.cued.take() {
                Some((Next::Pattern(p), ..)) if swaps > self.swaps => Some(p),
                _ => None,
            };
            self.swaps = swaps;
            editor.boundary(changed_to);
            if changed_to.is_some() {
                // The engine has the new pattern's conductor, from its cue.
                self.conductor = None;
            }
        }
        while let Some(conducted) = handle.conducted() {
            match conducted {
                Conducted::Pattern(p) => editor.conducted_pattern(p),
                Conducted::Mute { track, muted } => editor.conducted_mix(track, Some(muted), None),
                Conducted::Solo { track, soloed } => {
                    editor.conducted_mix(track, None, Some(soloed));
                }
            }
        }

        let length = editor.pattern_steps();
        if self.length != Some(length) && handle.send(Command::SetLength(length)) {
            self.length = Some(length);
        }
        let current = editor.arrangement.current;
        if let Some(conductor) = editor.arrangement.conductor(current)
            && self.conductor.as_ref() != Some(conductor)
        {
            let command = Command::SetConductor {
                conductor: Box::new(conductor.clone()),
                swaps: self.swaps,
            };
            if handle.send(command) {
                self.conductor = Some(conductor.clone());
            }
        }
        let mutes = editor.song_mutes();
        if self.mutes != Some(mutes) {
            let (now, next) = mutes;
            if handle.send(Command::SetMutes { now, next }) {
                self.mutes = Some(mutes);
            }
        }
        let next = editor.next();
        let (next, lanes, steps) = match next {
            Next::Pattern(p) => match (
                editor.arrangement.lanes(&editor.grid, p),
                editor.arrangement.steps(&editor.grid, p),
            ) {
                (Some(lanes), Some(steps)) => (next, lanes, steps),
                // There's no such pattern to go on to, so the song stops.
                _ => (Next::Stop, vec![], 0),
            },
            Next::Loop | Next::Stop => (next, vec![], 0),
        };
        let wanted = (next, lanes, steps);
        if self.cued.as_ref() != Some(&wanted) {
            let cue = match next {
                Next::Loop => Cue::Loop,
                Next::Stop => Cue::Stop,
                Next::Pattern(p) => Cue::Pattern(Box::new(Section {
                    lanes: wanted.1.clone(),
                    steps,
                    conductor: editor.arrangement.conductor(p).cloned().unwrap_or_default(),
                    // Picked by hand, it takes over at the next bar; in the song, when the
                    // pattern before it ends.
                    at_bar: editor.song_position.is_none(),
                })),
            };
            if handle.send(Command::Cue(cue)) {
                self.cued = Some(wanted);
            }
        }
    }
}
