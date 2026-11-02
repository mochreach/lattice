//! A plain stereo feedback delay, with a lowpass in the feedback path so repeats darken as they
//! fade.

use lattice_core::FxKind;

use crate::param::{ParamInfo, Smoothed};
use crate::processor::{Event, EventKind, Processor, Segment, segments};

pub use lattice_core::fx::delay::{FEEDBACK, MAX_TIME, MIX, TIME, TONE};

const PARAMS: &[ParamInfo] = FxKind::Delay.params();

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
        let mut delay = Self {
            sample_rate,
            left: vec![0.0; len],
            right: vec![0.0; len],
            write: 0,
            time: Smoothed::new(0.0, sample_rate),
            feedback: Smoothed::new(0.0, sample_rate),
            mix: Smoothed::new(0.0, sample_rate),
            tone: 0.0,
            low: [0.0; 2],
        };
        for info in PARAMS {
            delay.set(info.id, info.default);
        }
        delay.settle();
        delay
    }

    /// Starts at `values` (one per parameter, by id) rather than gliding to them.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let mut delay = Self::new(sample_rate);
        for (id, &value) in values.iter().enumerate() {
            delay.set(id as u32, value);
        }
        delay.settle();
        delay
    }

    fn settle(&mut self) {
        for s in [&mut self.time, &mut self.feedback, &mut self.mix] {
            s.reset(s.target());
        }
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

    #[expect(
        clippy::indexing_slicing,
        reason = "the lines are the same length, never empty, and every index is wrapped to it"
    )]
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
            let [low_l, low_r] = self.low;

            self.left[self.write] = *l + low_l * feedback;
            self.right[self.write] = *r + low_r * feedback;
            self.write = (self.write + 1) % len;

            *l += (low_l - *l) * mix;
            *r += (low_r - *r) * mix;
        }
    }
}

impl Processor for Delay {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.render(l, r),
            Segment::Event(event) => {
                if let EventKind::Param { id, value } = event.kind {
                    self.set(id, value);
                }
            }
        });
    }

    fn params(&self) -> &[ParamInfo] {
        PARAMS
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
        let events = [param(TIME, 0.1), param(TONE, 16_000.0), param(MIX, 1.0)];
        // Set before any audio, so they start gliding from their defaults: let them settle
        // first.
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
    fn starting_at_settings_skips_the_glide() {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        values[MIX as usize] = 0.0;
        let mut delay = Delay::with_values(1_000.0, &values);
        let (mut left, mut right) = (vec![1.0; 4], vec![1.0; 4]);
        delay.process(&[], &mut left, &mut right);
        assert_eq!(left, [1.0; 4], "dry from the first sample");
    }

    #[test]
    fn mix_at_zero_passes_the_input_through() {
        let dry = impulse(&[param(MIX, 0.0)], 2_000);
        // Starting from the default mix, the rest of the dry signal glides in over 20 samples.
        assert!((dry[0] - 0.715).abs() < 1e-6, "{}", dry[0]);
        assert_eq!(dry[1_500], 0.0);
    }

    #[test]
    fn out_of_range_values_are_clamped_and_unknown_ids_ignored() {
        let out = impulse(&[param(FEEDBACK, 50.0), param(99, 1.0)], 10_000);
        assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    }
}
