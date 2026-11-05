//! A MIDI track's instrument: rather than sounding, it sends its notes and settings out over
//! MIDI, each stamped with when it's heard, for the frontend to send then. So gear plays in time
//! with Lattice's own instruments, after the audio's latency as they are.
//!
//! A note goes out on the track's channel; a lock or a setting on a controller goes out as that
//! controller, and on the bend as pitch bend; a `P12` gesture as program change 12. A controller
//! at -1 isn't sent at all.

use std::sync::atomic::{AtomicU64, Ordering};

use lattice_core::InstKind;
use lattice_core::instrument::midi::{BEND, CC, CHANNEL};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::processor::{Event, EventKind, ParamInfo, Processor};

/// When the block being rendered is heard, in nanoseconds from whenever the frontend counts
/// from; 0 when nothing's said. The engine sets it for each block (see
/// [`crate::Engine::set_heard_at`]), so a MIDI track can stamp its messages without knowing
/// the time itself.
pub static BLOCK_AT: AtomicU64 = AtomicU64::new(0);

/// Most messages waiting to go out from one track: far more than a block's worth.
const CAPACITY: usize = 4096;

/// A message to send, and when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sent {
    /// When it's heard, as [`BLOCK_AT`] counts; 0 for at once.
    pub at: u64,
    bytes: [u8; 3],
    len: u8,
}

impl Sent {
    /// `bytes`, the first three of them, to send `at`.
    pub(crate) fn new(at: u64, bytes: &[u8]) -> Self {
        Self {
            at,
            ..Self::now(bytes)
        }
    }

    fn now(bytes: &[u8]) -> Self {
        let mut sent = Sent {
            at: 0,
            bytes: [0; 3],
            len: 0,
        };
        for (to, &from) in sent.bytes.iter_mut().zip(bytes) {
            *to = from;
            sent.len += 1;
        }
        sent
    }

    /// The message as it goes down the wire.
    pub fn bytes(&self) -> &[u8] {
        self.bytes
            .get(..usize::from(self.len))
            .unwrap_or(&self.bytes)
    }
}

/// The messages from one MIDI track, or the clock, for the frontend to send.
pub type Messages = Consumer<Sent>;

/// A queue for the engine's MIDI clock: its end for [`crate::Command::SetClockOut`], and the
/// frontend's.
pub fn clock_queue() -> (Producer<Sent>, Messages) {
    RingBuffer::new(CAPACITY)
}

/// A MIDI track's instrument.
pub struct MidiOut {
    /// `None` where nothing's listening, as when a song's rendered to a file.
    out: Option<Producer<Sent>>,
    sample_rate: f32,
    /// Its settings, by id.
    values: [f32; CC as usize + 128],
    /// The channel the notes sounding went out on, 0 to 15.
    channel: u8,
    /// How many of each pitch are sounding, so each gets its note off.
    sounding: [u8; 128],
    /// Muted, or another's soloed: notes don't go out.
    muted: bool,
    /// Its settings haven't gone out yet.
    fresh: bool,
}

impl MidiOut {
    /// A MIDI track's instrument, starting at `values`, and the messages it sends. Allocates, so
    /// build it off the audio thread.
    pub fn new(sample_rate: f32, values: &[f32]) -> (Self, Messages) {
        let (producer, consumer) = RingBuffer::new(CAPACITY);
        let mut out = Self::silent(sample_rate, values);
        out.out = Some(producer);
        (out, consumer)
    }

    /// One that sends nothing, for rendering to a file.
    pub fn silent(sample_rate: f32, values: &[f32]) -> Self {
        let params = InstKind::Midi.params();
        let mut all = [-1.0; CC as usize + 128];
        for ((to, info), &value) in all.iter_mut().zip(params).zip(values) {
            *to = info.clamp(value);
        }
        // Any not given are at their defaults.
        for (to, info) in all.iter_mut().zip(params).skip(values.len()) {
            *to = info.default;
        }
        let channel = channel_of(all.first().copied().unwrap_or(1.0));
        Self {
            out: None,
            sample_rate,
            values: all,
            channel,
            sounding: [0; 128],
            muted: false,
            fresh: true,
        }
    }

    fn send(&mut self, at: u64, bytes: &[u8]) {
        if let Some(out) = &mut self.out {
            // Full, the frontend's stopped reading: there's no one to hear it.
            let _ = out.push(Sent {
                at,
                ..Sent::now(bytes)
            });
        }
    }

    /// Ends every note sounding.
    fn release(&mut self, at: u64) {
        let status = 0x80 | self.channel;
        for pitch in 0..128u8 {
            let count = self
                .sounding
                .get_mut(usize::from(pitch))
                .map_or(0, std::mem::take);
            for _ in 0..count {
                self.send(at, &[status, pitch, 0]);
            }
        }
    }

    /// Sends setting `id` as it is now: a controller, unless it's at -1, or the bend.
    fn send_setting(&mut self, at: u64, id: u32) {
        let Some(&value) = self.values.get(id as usize) else {
            return;
        };
        let status = self.channel;
        match id {
            BEND => {
                let bend = ((value.clamp(-1.0, 1.0) + 1.0) * 8192.0)
                    .round()
                    .min(16383.0) as u16;
                self.send(at, &[0xe0 | status, (bend & 0x7f) as u8, (bend >> 7) as u8]);
            }
            id if id >= CC && value >= 0.0 => {
                let number = (id - CC) as u8;
                self.send(at, &[0xb0 | status, number, value.round().min(127.0) as u8]);
            }
            _ => {}
        }
    }

    fn event(&mut self, at: u64, kind: EventKind) {
        match kind {
            EventKind::NoteOn { pitch, velocity } if !self.muted && pitch < 128 => {
                if let Some(count) = self.sounding.get_mut(usize::from(pitch)) {
                    *count = count.saturating_add(1);
                }
                self.send(at, &[0x90 | self.channel, pitch, velocity.clamp(1, 127)]);
            }
            EventKind::NoteOn { .. } | EventKind::Shape(_) => {}
            EventKind::NoteOff { pitch } => {
                let Some(count) = self.sounding.get_mut(usize::from(pitch)) else {
                    return;
                };
                if *count > 0 {
                    *count -= 1;
                    self.send(at, &[0x80 | self.channel, pitch, 0]);
                }
            }
            EventKind::Choke => self.release(at),
            EventKind::Program(program) => {
                self.send(at, &[0xc0 | self.channel, program.min(127)]);
            }
            EventKind::Param { id, value } => {
                let Some(info) = InstKind::Midi.params().get(id as usize) else {
                    return;
                };
                let value = info.clamp(value);
                let Some(old) = self.values.get_mut(id as usize) else {
                    return;
                };
                if *old == value {
                    return;
                }
                *old = value;
                if id == CHANNEL {
                    // Notes started on the old channel end there.
                    self.release(at);
                    self.channel = channel_of(value);
                    return;
                }
                self.send_setting(at, id);
            }
        }
    }
}

/// Channel `value`, 1 to 16 as it's shown, as it's sent: 0 to 15.
fn channel_of(value: f32) -> u8 {
    (value.round().clamp(1.0, 16.0) as u8) - 1
}

impl Processor for MidiOut {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        // It makes no sound of its own.
        left.fill(0.0);
        right.fill(0.0);
        if self.out.is_none() {
            return;
        }
        let block = BLOCK_AT.load(Ordering::Relaxed);
        if self.fresh {
            self.fresh = false;
            for id in BEND..CC + 128 {
                // The bend's centre needn't be said.
                if id != BEND || self.values.get(id as usize).is_some_and(|&v| v != 0.0) {
                    self.send_setting(block, id);
                }
            }
        }
        let nanos_per_frame = 1e9 / f64::from(self.sample_rate.max(1.0));
        for event in events {
            let at = block + (f64::from(event.offset) * nanos_per_frame) as u64;
            self.event(at, event.kind);
        }
    }

    fn params(&self) -> &[ParamInfo] {
        InstKind::Midi.params()
    }

    fn mute(&mut self, muted: bool) {
        if muted && !self.muted {
            self.release(BLOCK_AT.load(Ordering::Relaxed));
        }
        self.muted = muted;
    }
}

impl Drop for MidiOut {
    /// Replaced or removed, it lets go of its notes, so none hang on.
    fn drop(&mut self) {
        self.release(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::instrument::midi::cc;

    fn ev(offset: u32, kind: EventKind) -> Event {
        Event { offset, kind }
    }

    fn sent(messages: &mut Messages) -> Vec<Vec<u8>> {
        std::iter::from_fn(|| messages.pop().ok())
            .map(|s| s.bytes().to_vec())
            .collect()
    }

    #[test]
    fn sends_notes_settings_and_programs_on_its_channel() {
        let mut values = vec![-1.0; 130];
        values[0] = 3.0;
        values[1] = 0.0;
        values[cc(74) as usize] = 40.0;
        let (mut out, mut messages) = MidiOut::new(48_000.0, &values);
        let (mut l, mut r) = ([0.0; 64], [0.0; 64]);
        let events = [
            ev(0, EventKind::Program(12)),
            ev(
                0,
                EventKind::NoteOn {
                    pitch: 60,
                    velocity: 100,
                },
            ),
            ev(
                10,
                EventKind::Param {
                    id: cc(1),
                    value: 64.0,
                },
            ),
            ev(
                20,
                EventKind::Param {
                    id: cc(1),
                    value: 64.0,
                },
            ),
            ev(
                30,
                EventKind::Param {
                    id: BEND,
                    value: 1.0,
                },
            ),
            ev(40, EventKind::NoteOff { pitch: 60 }),
            ev(50, EventKind::NoteOff { pitch: 60 }),
        ];
        out.process(&events, &mut l, &mut r);
        assert_eq!(
            sent(&mut messages),
            [
                vec![0xb2, 74, 40],
                vec![0xc2, 12],
                vec![0x92, 60, 100],
                vec![0xb2, 1, 64],
                vec![0xe2, 0x7f, 0x7f],
                vec![0x82, 60, 0],
            ],
            "its settings first; each change once; a note ended once"
        );
    }

    #[test]
    fn lets_go_of_its_notes_muted_moved_or_dropped() {
        let (mut out, mut messages) = MidiOut::new(48_000.0, &[1.0]);
        let (mut l, mut r) = ([0.0; 8], [0.0; 8]);
        let on = |pitch| {
            ev(
                0,
                EventKind::NoteOn {
                    pitch,
                    velocity: 90,
                },
            )
        };
        out.process(&[on(60), on(64)], &mut l, &mut r);
        out.process(
            &[ev(
                0,
                EventKind::Param {
                    id: CHANNEL,
                    value: 2.0,
                },
            )],
            &mut l,
            &mut r,
        );
        assert_eq!(
            sent(&mut messages),
            [
                vec![0x90, 60, 90],
                vec![0x90, 64, 90],
                vec![0x80, 60, 0],
                vec![0x80, 64, 0],
            ]
        );
        out.process(&[on(62)], &mut l, &mut r);
        out.mute(true);
        out.process(&[on(65)], &mut l, &mut r);
        assert_eq!(sent(&mut messages), [vec![0x91, 62, 90], vec![0x81, 62, 0]]);
        out.mute(false);
        out.process(&[on(67)], &mut l, &mut r);
        drop(out);
        assert_eq!(sent(&mut messages), [vec![0x91, 67, 90], vec![0x81, 67, 0]]);
    }
}
