//! The interface every instrument and effect implements: built-in engines, built-in effects and,
//! later, hosted CLAP plugins.
//!
//! It mirrors CLAP's processing model: a processor renders a whole block at a time, and receives
//! a list of events stamped with their sample offset within that block. Parameters are described
//! by [`ParamInfo`] and changed by [`EventKind::Param`] events, in their own units, as in CLAP.

use std::sync::Arc;

pub use crate::param::ParamInfo;
use crate::sampler::Sample;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    /// Sample offset within the block.
    pub offset: u32,
    pub kind: EventKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventKind {
    NoteOn {
        pitch: u8,
        velocity: u8,
    },
    NoteOff {
        pitch: u8,
    },
    /// Cut off every sounding note quickly, as CLAP's note choke does: another drum in the
    /// track's choke group has hit.
    Choke,
    /// Set a parameter, by its [`ParamInfo::id`], to `value` in the parameter's own units.
    Param {
        id: u32,
        value: f32,
    },
    /// How the next note-on plays, from a step's gestures: for that note alone.
    Shape(Shape),
    /// A `P12` gesture: change to program 12. Only MIDI out does anything with it.
    Program(u8),
}

/// How a note plays, beyond its pitch and velocity: what a step's `<`, `O` and `G` gestures
/// ask of it. Instruments that can't do something leave it out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Shape {
    /// Played the other way round from the instrument's direction, by one playing a file.
    pub reverse: bool,
    /// Started this many sixteenths into its sound file, or its piece.
    pub offset: u8,
    /// Slid from the last note's pitch over this many frames, by a synth or string.
    pub glide: u32,
}

impl Shape {
    /// Its sixteenths in, as a share of the way.
    pub fn along(self) -> f64 {
        f64::from(self.offset) / f64::from(lattice_core::gesture::OFFSETS)
    }
}

pub trait Processor: Send {
    /// Processes `left.len()` frames in place, applying `events` (sorted by offset) at their
    /// exact sample offsets. On entry `left` and `right` hold the input, which is silence for an
    /// instrument; on return they hold the output. Called on the audio thread, so it must be
    /// real-time safe: no allocation, no locks, no I/O.
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]);

    /// The parameters [`EventKind::Param`] events can set.
    fn params(&self) -> &[ParamInfo] {
        &[]
    }

    /// The track's muted, or another soloed, from the next block on. Only MIDI out, which
    /// isn't heard through the mixer, needs to know.
    fn mute(&mut self, _muted: bool) {}

    /// For an instrument that plays a sound file: swaps `sample` for the one it's playing,
    /// carrying on from the new one. Returns false, leaving it alone, if it plays none.
    fn swap_sample(&mut self, _sample: &mut Option<Arc<Sample>>) -> bool {
        false
    }
}

impl std::fmt::Debug for dyn Processor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Processor").finish_non_exhaustive()
    }
}

pub enum Segment<'a> {
    /// Render these frames of the left and right channels.
    Render(&'a mut [f32], &'a mut [f32]),
    /// Apply this event before rendering on.
    Event(&'a Event),
}

/// Walks a block in order, handing over the stretches of `left` and `right` between events and
/// each event at its offset. Saves every instrument writing the same splitting loop. The block is
/// as long as the shorter channel; an event past its end is applied at the end.
pub fn segments(
    events: &[Event],
    left: &mut [f32],
    right: &mut [f32],
    mut f: impl FnMut(Segment<'_>),
) {
    let len = left.len().min(right.len());
    let (mut left, mut right) = (frames(left, len), frames(right, len));
    let mut pos = 0;
    for event in events {
        let at = (event.offset as usize).clamp(pos, len);
        // `at` is between `pos` and `len`, so both channels, `len - pos` long, split there.
        // (Taken, so the rest can be put back; were a split ever out of range, the rest of the
        // block would go unrendered rather than panic.)
        if let (Some((l, left_rest)), Some((r, right_rest))) = (
            std::mem::take(&mut left).split_at_mut_checked(at - pos),
            std::mem::take(&mut right).split_at_mut_checked(at - pos),
        ) {
            if !l.is_empty() {
                f(Segment::Render(l, r));
            }
            (left, right, pos) = (left_rest, right_rest, at);
        }
        f(Segment::Event(event));
    }
    if !left.is_empty() {
        f(Segment::Render(left, right));
    }
}

/// The first `len` frames of `buffer`, or all of it if it's shorter. The engine's buffers each
/// hold a whole block, so for a block's length that's its frames; were one ever shorter, what
/// works on it zipped with the others stops at its end rather than panicking.
pub(crate) fn frames(buffer: &mut [f32], len: usize) -> &mut [f32] {
    // Clamped to its length, so the split is in range.
    let len = len.min(buffer.len());
    buffer.split_at_mut(len).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_interleave_audio_and_events() {
        let on = |offset| Event {
            offset,
            kind: EventKind::NoteOn {
                pitch: 60,
                velocity: 100,
            },
        };
        let events = [on(0), on(10), on(10), on(40)];
        let (mut left, mut right) = ([0.0; 32], [0.0; 32]);
        let mut seen = Vec::new();
        let mut pos = 0;
        segments(&events, &mut left, &mut right, |s| {
            seen.push(match s {
                Segment::Render(l, r) => {
                    assert_eq!(l.len(), r.len());
                    pos += l.len();
                    format!("{}..{}", pos - l.len(), pos)
                }
                Segment::Event(e) => format!("@{}", e.offset),
            })
        });
        // An event past the end of the block is applied at the end.
        assert_eq!(seen, ["@0", "0..10", "@10", "@10", "10..32", "@40"]);
    }

    #[test]
    fn segments_stop_at_the_shorter_channel() {
        let (mut left, mut right) = ([0.0; 32], [0.0; 20]);
        let mut lengths = Vec::new();
        segments(&[], &mut left, &mut right, |s| {
            if let Segment::Render(l, r) = s {
                lengths.push((l.len(), r.len()));
            }
        });
        assert_eq!(lengths, [(20, 20)]);
    }
}
