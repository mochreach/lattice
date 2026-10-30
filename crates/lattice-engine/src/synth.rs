//! Polyphonic subtractive synth: oscillator → low-pass filter → amplifier.

use crate::dsp::{Adsr, AdsrParams, Osc, Svf, Wave};

const VOICES: usize = 8;

#[derive(Debug, Clone, Copy)]
pub struct Patch {
    pub wave: Wave,
    pub amp: AdsrParams,
    /// Filter cutoff in Hz with the envelope closed.
    pub cutoff: f32,
    /// Hz added to the cutoff at full envelope.
    pub env_amount: f32,
    pub resonance: f32,
    pub gain: f32,
}

#[derive(Debug, Clone, Default)]
struct Voice {
    osc: Osc,
    env: Adsr,
    filter: Svf,
    velocity: f32,
    /// Steps until the note is released; 0 once released.
    steps_left: u16,
    /// When the note started, for stealing the oldest voice.
    started: u64,
}

#[derive(Debug, Clone)]
pub struct Synth {
    patch: Patch,
    sample_rate: f32,
    voices: [Voice; VOICES],
    notes_started: u64,
}

impl Synth {
    pub fn new(patch: Patch, sample_rate: f32) -> Self {
        Self {
            patch,
            sample_rate,
            voices: Default::default(),
            notes_started: 0,
        }
    }

    /// Starts a note lasting `steps` steps, taking a free voice or else stealing the oldest.
    pub fn note_on(&mut self, pitch: u8, velocity: u8, steps: u16) {
        let i = self
            .voices
            .iter()
            .position(|v| v.env.is_idle())
            .or_else(|| (0..VOICES).min_by_key(|&i| self.voices[i].started))
            .unwrap_or(0);
        self.notes_started += 1;
        let v = &mut self.voices[i];
        v.osc.set_freq(midi_to_hz(pitch), self.sample_rate);
        v.env.trigger();
        v.velocity = f32::from(velocity) / 127.0;
        v.steps_left = steps.max(1);
        v.started = self.notes_started;
    }

    /// Called at each step boundary, before that step's notes: releases notes whose length is up.
    pub fn step(&mut self) {
        for v in &mut self.voices {
            if v.steps_left > 0 {
                v.steps_left -= 1;
                if v.steps_left == 0 {
                    v.env.release();
                }
            }
        }
    }

    /// Mixes the sounding voices into `out`.
    pub fn render_add(&mut self, out: &mut [f32]) {
        let p = &self.patch;
        for v in self.voices.iter_mut().filter(|v| !v.env.is_idle()) {
            for sample in out.iter_mut() {
                let env = v.env.next(&p.amp, self.sample_rate);
                let x = v.osc.next(p.wave);
                let y = v.filter.lowpass(
                    x,
                    p.cutoff + p.env_amount * env,
                    p.resonance,
                    self.sample_rate,
                );
                *sample += y * env * v.velocity * p.gain;
            }
        }
    }
}

fn midi_to_hz(pitch: u8) -> f32 {
    440.0 * 2f32.powf((f32::from(pitch) - 69.0) / 12.0)
}
