//! The sequencer and mixer, driven entirely from the audio callback.

use lattice_core::{Grid, Step, Track, realise_into};

use crate::click::Click;
use crate::clock::Clock;
use crate::synth::{Patch, Synth};

const BEATS_PER_BAR: u64 = 4;

#[derive(Debug, Clone)]
struct TrackPlayer {
    track: Track,
    synth: Synth,
    /// The track as realised for the current pass.
    steps: Vec<Step>,
    playhead: usize,
    pass: u32,
}

#[derive(Debug, Clone)]
pub struct Engine {
    clock: Clock,
    steps_per_beat: u64,
    step_index: u64,
    tracks: Vec<TrackPlayer>,
    click: Click,
    pub click_enabled: bool,
}

impl Engine {
    /// `patches[i]` plays `grid.tracks[i]`. Allocates, so build it off the audio thread.
    pub fn new(sample_rate: f32, bpm: f64, grid: &Grid, patches: &[Patch]) -> Self {
        assert_eq!(grid.tracks.len(), patches.len(), "one patch per track");
        let tracks = grid
            .tracks
            .iter()
            .zip(patches)
            .map(|(track, patch)| TrackPlayer {
                track: track.clone(),
                synth: Synth::new(*patch, sample_rate),
                steps: Vec::with_capacity(track.steps.len()),
                playhead: 0,
                pass: 0,
            })
            .collect();
        Self {
            clock: Clock::new(f64::from(sample_rate), bpm, grid.steps_per_beat),
            steps_per_beat: u64::from(grid.steps_per_beat.max(1)),
            step_index: 0,
            tracks,
            click: Click::new(sample_rate),
            click_enabled: true,
        }
    }

    /// Renders the next block of mono audio into `out`. Real-time safe: no allocation, no locks.
    ///
    /// The block is split at step boundaries, so every step starts on its exact sample whatever
    /// the buffer size.
    pub fn process(&mut self, out: &mut [f32]) {
        let mut done = 0;
        while done < out.len() {
            let until = self.clock.frames_until_step();
            if until == 0 {
                self.clock.take_step();
                self.step();
                continue;
            }
            let frames = (until as usize).min(out.len() - done);
            self.render(&mut out[done..done + frames]);
            self.clock.skip(frames as u64);
            done += frames;
        }
    }

    fn step(&mut self) {
        if self.step_index.is_multiple_of(self.steps_per_beat) {
            let beat = self.step_index / self.steps_per_beat;
            self.click.trigger(beat.is_multiple_of(BEATS_PER_BAR));
        }
        self.step_index += 1;

        for t in &mut self.tracks {
            if t.track.steps.is_empty() {
                continue;
            }
            if t.playhead == 0 {
                realise_into(&t.track, t.pass, &mut t.steps);
            }
            t.synth.step();
            if let Some(note) = t.steps[t.playhead].note {
                t.synth.note_on(note.pitch, note.velocity, note.length);
            }
            t.playhead += 1;
            if t.playhead == t.steps.len() {
                t.playhead = 0;
                t.pass += 1;
            }
        }
    }

    fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        for t in &mut self.tracks {
            t.synth.render_add(out);
        }
        if self.click_enabled {
            self.click.render_add(out);
        }
        for sample in out.iter_mut() {
            *sample = sample.tanh();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click_only(sample_rate: f32, bpm: f64) -> Engine {
        let grid = Grid {
            name: "empty".into(),
            steps_per_beat: 4,
            tracks: vec![],
        };
        Engine::new(sample_rate, bpm, &grid, &[])
    }

    /// Samples where the click starts: non-silent after silence.
    fn onsets(audio: &[f32]) -> Vec<usize> {
        (0..audio.len())
            .filter(|&i| audio[i] != 0.0 && (i == 0 || audio[i - 1] == 0.0))
            .collect()
    }

    fn render(engine: &mut Engine, block: usize, total: usize) -> Vec<f32> {
        let mut audio = vec![0.0; total];
        for chunk in audio.chunks_mut(block) {
            engine.process(chunk);
        }
        audio
    }

    #[test]
    fn clicks_land_on_exact_samples() {
        // 137 bpm at 44.1 kHz: 19,313.868... samples per beat.
        let audio = render(&mut click_only(44_100.0, 137.0), 441, 100_000);
        let spb = 44_100.0 * 60.0 / 137.0;
        let expected: Vec<usize> = (0..6).map(|k| (k as f64 * spb).ceil() as usize).collect();
        assert_eq!(onsets(&audio), expected);
    }

    #[test]
    fn output_does_not_depend_on_block_size() {
        let reference = render(&mut click_only(48_000.0, 120.0), 1, 50_000);
        for block in [64, 333, 4096] {
            assert_eq!(
                render(&mut click_only(48_000.0, 120.0), block, 50_000),
                reference
            );
        }
    }
}
