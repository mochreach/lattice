use std::f32::consts::TAU;

use crate::dsp::decay_coef;
use crate::processor::{Event, EventKind, Processor, Segment, segments};

/// Metronome click: a short decaying blip at the note's pitch.
#[derive(Debug, Clone)]
pub struct Click {
    sample_rate: f32,
    phase: f32,
    inc: f32,
    level: f32,
    decay: f32,
}

impl Click {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            phase: 0.0,
            inc: 0.0,
            level: 0.0,
            decay: decay_coef(0.015, sample_rate),
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let mut out = 0.0;
            if self.level >= 1e-4 {
                // Cosine, so the very first sample is at full level and the onset is exact.
                out = (TAU * self.phase).cos() * self.level;
                self.phase = (self.phase + self.inc).fract();
                self.level *= self.decay;
            }
            *l = out;
            *r = out;
        }
    }
}

impl Processor for Click {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left, right, |segment| match segment {
            Segment::Render(l, r) => self.render(l, r),
            Segment::Event(event) => {
                if let EventKind::NoteOn { pitch, velocity } = event.kind {
                    let hz = 440.0 * 2f32.powf((f32::from(pitch) - 69.0) / 12.0);
                    self.phase = 0.0;
                    self.inc = hz / self.sample_rate;
                    self.level = 0.5 * f32::from(velocity) / 127.0;
                }
            }
        });
    }
}
