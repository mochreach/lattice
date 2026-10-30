//! A plain stereo feedback delay, with a lowpass in the feedback path so repeats darken as they
//! fade.

use crate::param::{ParamInfo, Smoothed};
use crate::processor::{Event, EventKind, Processor, Segment, segments};

/// The longest delay time, in seconds.
const MAX_TIME: f32 = 2.0;

pub const TIME: u32 = 0;
pub const FEEDBACK: u32 = 1;
pub const TONE: u32 = 2;
pub const MIX: u32 = 3;

const PARAMS: [ParamInfo; 4] = [
    ParamInfo {
        id: TIME,
        name: "time",
        min: 0.01,
        max: MAX_TIME,
        default: 0.375,
        unit: "s",
    },
    ParamInfo {
        id: FEEDBACK,
        name: "feedback",
        min: 0.0,
        max: 0.95,
        default: 0.45,
        unit: "",
    },
    ParamInfo {
        id: TONE,
        name: "tone",
        min: 200.0,
        max: 16_000.0,
        default: 4_000.0,
        unit: "Hz",
    },
    ParamInfo {
        id: MIX,
        name: "mix",
        min: 0.0,
        max: 1.0,
        default: 1.0,
        unit: "",
    },
];

pub struct Delay {
    sample_rate: f32,
    left: Vec<f32>,
    right: Vec<f32>,
    write: usize,
    /// In samples.
    time: Smoothed,
    feedback: Smoothed,
    mix: Smoothed,
    /// One-pole lowpass coefficient, and its state per channel.
    tone: f32,
    low: [f32; 2],
}

impl Delay {
    /// Allocates the delay line, so build it off the audio thread.
    pub fn new(sample_rate: f32) -> Self {
        let len = (MAX_TIME * sample_rate) as usize + 2;
        let default = |id: u32| PARAMS[id as usize].default;
        let mut delay = Self {
            sample_rate,
            left: vec![0.0; len],
            right: vec![0.0; len],
            write: 0,
            time: Smoothed::new(default(TIME) * sample_rate, sample_rate),
            feedback: Smoothed::new(default(FEEDBACK), sample_rate),
            mix: Smoothed::new(default(MIX), sample_rate),
            tone: 0.0,
            low: [0.0; 2],
        };
        delay.set_tone(default(TONE));
        delay
    }

    fn set_tone(&mut self, hz: f32) {
        self.tone = 1.0 - (-std::f32::consts::TAU * hz / self.sample_rate).exp();
    }

    fn set(&mut self, id: u32, value: f32) {
        let Some(info) = PARAMS.get(id as usize) else {
            return;
        };
        let value = info.clamp(value);
        match id {
            TIME => self.time.set(value * self.sample_rate),
            FEEDBACK => self.feedback.set(value),
            TONE => self.set_tone(value),
            _ => self.mix.set(value),
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = self.left.len();
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let time = self.time.tick();
            let feedback = self.feedback.tick();
            let mix = self.mix.tick();

            // Read `time` samples behind the write head, between two samples.
            let back = self.write as f32 + len as f32 - time;
            let i = back as usize;
            let frac = back - i as f32;
            let (a, b) = (i % len, (i + 1) % len);
            let delayed = [
                self.left[a] + (self.left[b] - self.left[a]) * frac,
                self.right[a] + (self.right[b] - self.right[a]) * frac,
            ];
            for (low, d) in self.low.iter_mut().zip(delayed) {
                *low += self.tone * (d - *low);
            }

            self.left[self.write] = *l + self.low[0] * feedback;
            self.right[self.write] = *r + self.low[1] * feedback;
            self.write = (self.write + 1) % len;

            *l += (self.low[0] - *l) * mix;
            *r += (self.low[1] - *r) * mix;
        }
    }
}

impl Processor for Delay {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left.len(), |segment| match segment {
            Segment::Render(range) => self.render(&mut left[range.clone()], &mut right[range]),
            Segment::Event(event) => {
                if let EventKind::Param { id, value } = event.kind {
                    self.set(id, value);
                }
            }
        });
    }

    fn params(&self) -> &[ParamInfo] {
        &PARAMS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(id: u32, value: f32) -> Event {
        Event {
            offset: 0,
            kind: EventKind::Param { id, value },
        }
    }

    /// An impulse through the delay at 1 kHz, as the left channel.
    fn impulse(events: &[Event], len: usize) -> Vec<f32> {
        let mut delay = Delay::new(1_000.0);
        let mut left = vec![0.0; len];
        let mut right = vec![0.0; len];
        left[0] = 1.0;
        delay.process(events, &mut left, &mut right);
        assert!(right.iter().all(|&x| x == 0.0), "channels stay apart");
        left
    }

    #[test]
    fn repeats_arrive_at_the_delay_time_and_fade() {
        let events = [param(TIME, 0.1), param(TONE, 16_000.0)];
        // Set before any audio, so time and feedback start gliding from their defaults: let
        // them settle first.
        let mut delay = Delay::new(1_000.0);
        let (mut l, mut r) = (vec![0.0; 100], vec![0.0; 100]);
        delay.process(&events, &mut l, &mut r);

        let mut left = vec![0.0; 400];
        let mut right = vec![0.0; 400];
        left[0] = 1.0;
        delay.process(&[], &mut left, &mut right);
        let peak = |at: usize| {
            left[at - 5..at + 5]
                .iter()
                .fold(0.0f32, |m, x| m.max(x.abs()))
        };
        assert_eq!(left[0], 0.0, "wet only");
        assert!(left[1..90].iter().all(|x| x.abs() < 1e-3));
        let (first, second) = (peak(100), peak(200));
        assert!(first > 0.3, "{first}");
        assert!(second < first && second > 0.05, "{first} then {second}");
    }

    #[test]
    fn mix_at_zero_passes_the_input_through() {
        let dry = impulse(&[param(MIX, 0.0)], 2_000);
        // Starting from fully wet, the dry signal glides in over 20 samples.
        assert!((dry[0] - 0.05).abs() < 1e-6, "{}", dry[0]);
        assert_eq!(dry[1_500], 0.0);
    }

    #[test]
    fn out_of_range_values_are_clamped_and_unknown_ids_ignored() {
        let out = impulse(&[param(FEEDBACK, 50.0), param(99, 1.0)], 10_000);
        assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    }
}
