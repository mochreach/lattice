//! Keeps the engine's pattern length, conductor and cue in line with the editor's, and tells the
//! editor when the engine moves from one pattern to the next, or the conductor does something.

use lattice_core::{Conductor, Lane, Meter};
use lattice_editor::{Editor, Next};
use lattice_engine::{Command, Conducted, Cue, Handle, Section};

#[derive(Default)]
pub struct Sequencer {
    /// The engine's boundaries and pattern changes, as far as the editor's been told.
    boundaries: u64,
    swaps: u64,
    /// The cue sent, with all it carried, so it's sent again if any of it changes.
    cued: Option<Cued>,
    /// The pattern length sent.
    length: Option<u64>,
    /// The playing pattern's conductor sent.
    conductor: Option<Conductor>,
    /// The steps the playing pattern loops round, as sent.
    looped: Option<Option<(u64, u64)>>,
    /// The song's mutes sent, now and next.
    mutes: Option<(u32, u32)>,
    playing: bool,
}

/// What a cue carried.
#[derive(PartialEq)]
struct Cued {
    next: Next,
    lanes: Vec<Lane>,
    steps: u64,
    meter: Meter,
    /// The next pattern's conductor, whose tempos `:meter` rescales.
    conductor: Option<Conductor>,
    /// Whether it takes over at the next bar, as one picked by hand does.
    at_bar: bool,
    /// The steps it loops round.
    looped: Option<(u64, u64)>,
}

impl Sequencer {
    /// The engine's pattern changes the editor has seen, for [`Command::SetTrack`].
    pub fn swaps(&self) -> u64 {
        self.swaps
    }

    /// Sends the pattern's length and conductor if the engine doesn't have them. Also called
    /// just before playing, as starting the song can change pattern on the same key, and
    /// otherwise its first step would play to the last pattern's conductor.
    pub fn send_pattern(&mut self, editor: &Editor, handle: &mut Handle) {
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
        let looped = editor.step_loop(current);
        if self.looped != Some(looped) {
            let command = Command::SetLoop {
                range: looped,
                swaps: self.swaps,
            };
            if handle.send(command) {
                self.looped = Some(looped);
            }
        }
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
                Some(Cued {
                    next: Next::Pattern(p),
                    ..
                }) if swaps > self.swaps => Some(p),
                _ => None,
            };
            self.swaps = swaps;
            editor.boundary(changed_to);
            if changed_to.is_some() {
                // The engine has the new pattern's conductor and loop, from its cue.
                self.conductor = None;
                self.looped = None;
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

        self.send_pattern(editor, handle);
        let mutes = editor.song_mutes();
        if self.mutes != Some(mutes) {
            let (now, next) = mutes;
            if handle.send(Command::SetMutes { now, next }) {
                self.mutes = Some(mutes);
            }
        }
        let next = editor.next();
        let meter = |p| editor.arrangement.meter(&editor.grid, p);
        let (next, lanes, steps, meter) = match next {
            Next::Pattern(p) => match (
                editor.arrangement.lanes(&editor.grid, p),
                editor.arrangement.steps(&editor.grid, p),
            ) {
                (Some(lanes), Some(steps)) => (next, lanes, steps, meter(p)),
                // There's no such pattern to go on to, so the song stops.
                _ => (Next::Stop, vec![], 0, editor.grid.meter()),
            },
            Next::Loop | Next::Stop => (next, vec![], 0, editor.grid.meter()),
        };
        let (conductor, looped) = match next {
            Next::Pattern(p) => (editor.arrangement.conductor(p), editor.step_loop(p)),
            Next::Loop | Next::Stop => (None, None),
        };
        // Picked by hand, it takes over at the next bar; in the song, when the pattern before
        // it ends.
        let at_bar = editor.song_position.is_none();
        let same = self.cued.as_ref().is_some_and(|c| {
            (
                c.next,
                &c.lanes,
                c.steps,
                &c.meter,
                c.conductor.as_ref(),
                c.at_bar,
                c.looped,
            ) == (next, &lanes, steps, &meter, conductor, at_bar, looped)
        });
        if !same {
            let wanted = Cued {
                next,
                lanes,
                steps,
                meter,
                conductor: conductor.cloned(),
                at_bar,
                looped,
            };
            let cue = match next {
                Next::Loop => Cue::Loop,
                Next::Stop => Cue::Stop,
                Next::Pattern(_) => Cue::Pattern(Box::new(Section {
                    lanes: wanted.lanes.clone(),
                    steps,
                    beats_per_bar: wanted.meter.beats,
                    groups: wanted.meter.group_starts(),
                    conductor: wanted.conductor.clone().unwrap_or_default(),
                    at_bar,
                    looped,
                })),
            };
            if handle.send(Command::Cue(cue)) {
                self.cued = Some(wanted);
            }
        }
    }
}
