use std::f32::consts::TAU;

use crate::dsp::decay_coef;

/// Metronome click: a short decaying blip, higher on the first beat of the bar.
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

    pub fn trigger(&mut self, accent: bool) {
        let (hz, level) = if accent {
            (1_760.0, 0.5)
        } else {
            (880.0, 0.35)
        };
        self.phase = 0.0;
        self.inc = hz / self.sample_rate;
        self.level = level;
    }

    pub fn render_add(&mut self, out: &mut [f32]) {
        for sample in out.iter_mut() {
            if self.level < 1e-4 {
                self.level = 0.0;
                return;
            }
            // Cosine, so the very first sample is at full level and the onset is exact.
            *sample += (TAU * self.phase).cos() * self.level;
            self.phase = (self.phase + self.inc).fract();
            self.level *= self.decay;
        }
    }
}
