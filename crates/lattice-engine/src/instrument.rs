//! The interface every sound source implements: built-in engines and, later, hosted CLAP plugins.
//!
//! It mirrors CLAP's processing model: an instrument renders a whole block at a time, and
//! receives a list of events stamped with their sample offset within that block.

use std::ops::Range;

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
    /// Set an instrument parameter; `value` is normalised to 0–1.
    Param {
        id: u32,
        value: f32,
    },
}

pub trait Instrument: Send {
    /// Renders `left.len()` frames, overwriting `left` and `right`, applying `events` (sorted by
    /// offset) at their exact sample offsets. Called on the audio thread, so it must be real-time
    /// safe: no allocation, no locks, no I/O.
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]);
}

pub enum Segment<'a> {
    /// Render these frames.
    Render(Range<usize>),
    /// Apply this event before rendering on.
    Event(&'a Event),
}

/// Walks a block of `len` frames in order, yielding the stretches of audio between events and
/// each event at its offset. Saves every instrument writing the same splitting loop.
pub fn segments(events: &[Event], len: usize, mut f: impl FnMut(Segment<'_>)) {
    let mut pos = 0;
    for event in events {
        let at = (event.offset as usize).min(len);
        if at > pos {
            f(Segment::Render(pos..at));
            pos = at;
        }
        f(Segment::Event(event));
    }
    if pos < len {
        f(Segment::Render(pos..len));
    }
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
        let mut seen = Vec::new();
        segments(&events, 32, |s| {
            seen.push(match s {
                Segment::Render(r) => format!("{}..{}", r.start, r.end),
                Segment::Event(e) => format!("@{}", e.offset),
            })
        });
        // An event past the end of the block is applied at the end.
        assert_eq!(seen, ["@0", "0..10", "@10", "@10", "10..32", "@40"]);
    }
}
