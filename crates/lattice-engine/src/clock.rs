//! Sample-counted step clock.
//!
//! Step boundaries are absolute sample positions, `k * samples_per_step`, so lateness can't
//! accumulate and the result is identical whatever the buffer size.

#[derive(Debug, Clone)]
pub struct Clock {
    sample_rate: f64,
    steps_per_beat: u32,
    samples_per_step: f64,
    /// Index of the next step to fire.
    next_step: u64,
    /// Absolute sample position of the start of the next block.
    position: u64,
    /// Step `base_step` falls on sample `origin`; later steps are `samples_per_step` apart.
    /// Moved forward on tempo changes, so they take effect from the next step.
    origin: u64,
    base_step: u64,
}

impl Clock {
    pub fn new(sample_rate: f64, bpm: f64, steps_per_beat: u32) -> Self {
        Self {
            sample_rate,
            steps_per_beat,
            samples_per_step: samples_per_step(sample_rate, bpm, steps_per_beat),
            next_step: 0,
            position: 0,
            origin: 0,
            base_step: 0,
        }
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Changes tempo from the next step on, without moving that step.
    pub fn set_bpm(&mut self, bpm: f64) {
        self.origin = self.step_position(self.next_step);
        self.base_step = self.next_step;
        self.samples_per_step = samples_per_step(self.sample_rate, bpm, self.steps_per_beat);
    }

    /// Changes tempo from the step just taken on, so it's that step that's longer or shorter:
    /// for a tempo the step itself sets.
    pub fn retime(&mut self, bpm: f64) {
        let this = self.next_step.saturating_sub(1).max(self.base_step);
        self.origin = self.step_position(this);
        self.base_step = this;
        self.samples_per_step = samples_per_step(self.sample_rate, bpm, self.steps_per_beat);
    }

    /// Splits beats into `steps_per_beat` steps from now on, for tempos set after. The step
    /// playing keeps its length: the frontend sets the tempo the change is meant to keep.
    pub fn set_steps_per_beat(&mut self, steps_per_beat: u32) {
        self.steps_per_beat = steps_per_beat;
    }

    /// How long a step is now, in frames.
    pub fn samples_per_step(&self) -> f64 {
        self.samples_per_step
    }

    /// Starts counting again from step 0, due immediately.
    pub fn restart(&mut self) {
        self.next_step = 0;
        self.base_step = 0;
        self.origin = self.position;
    }

    /// Frames from the current position until the next step starts. Zero means a step is due
    /// now: call [`Clock::take_step`] before rendering any further.
    pub fn frames_until_step(&self) -> u64 {
        self.step_position(self.next_step)
            .saturating_sub(self.position)
    }

    /// The index of the next step, counted since the clock last restarted.
    pub fn step(&self) -> u64 {
        self.next_step
    }

    /// Marks the step due at the current position as handled.
    pub fn take_step(&mut self) {
        self.next_step += 1;
    }

    /// Moves the position on by `frames` rendered samples.
    pub fn skip(&mut self, frames: u64) {
        self.position += frames;
    }

    /// Advance by one block of `frames` samples, calling `on_step` with the offset into the
    /// block of every step that starts within it.
    pub fn advance(&mut self, frames: usize, mut on_step: impl FnMut(usize)) {
        let end = self.position + frames as u64;
        loop {
            let at = self.step_position(self.next_step);
            if at >= end {
                break;
            }
            on_step((at - self.position) as usize);
            self.next_step += 1;
        }
        self.position = end;
    }

    /// Absolute sample position of step `k`. Computed from the index rather than summed, so
    /// rounding error can't build up.
    fn step_position(&self, k: u64) -> u64 {
        self.origin + ((k - self.base_step) as f64 * self.samples_per_step).ceil() as u64
    }
}

fn samples_per_step(sample_rate: f64, bpm: f64, steps_per_beat: u32) -> f64 {
    sample_rate * 60.0 / (bpm.max(1.0) * f64::from(steps_per_beat.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Absolute sample positions of every step in the first `total` samples.
    fn step_positions(block: usize, total: usize) -> Vec<u64> {
        let mut clock = Clock::new(44_100.0, 120.0, 4);
        let mut out = Vec::new();
        let mut start = 0;
        while start < total {
            let frames = block.min(total - start);
            clock.advance(frames, |offset| out.push((start + offset) as u64));
            start += frames;
        }
        out
    }

    #[test]
    fn fractional_step_length_lands_on_exact_samples() {
        // 120 bpm, 16ths at 44.1 kHz: 5512.5 samples per step.
        assert_eq!(
            step_positions(512, 22_051),
            [0, 5513, 11_025, 16_538, 22_050]
        );
    }

    #[test]
    fn block_size_does_not_change_timing() {
        let reference = step_positions(1, 1_000_000);
        for block in [32, 333, 512, 4096] {
            assert_eq!(step_positions(block, 1_000_000), reference, "block {block}");
        }
    }

    #[test]
    fn tempo_change_applies_from_the_next_step() {
        let mut clock = Clock::new(48_000.0, 120.0, 4); // 6,000 samples per step
        let mut steps = Vec::new();
        clock.advance(7_000, |o| steps.push(o as u64));
        clock.set_bpm(240.0); // 3,000 samples per step
        clock.advance(10_000, |o| steps.push(7_000 + o as u64));
        assert_eq!(steps, [0, 6_000, 12_000, 15_000]);
    }

    #[test]
    fn restart_fires_step_zero_immediately() {
        let mut clock = Clock::new(48_000.0, 120.0, 4);
        clock.advance(1_000, |_| {});
        clock.restart();
        assert_eq!(clock.frames_until_step(), 0);
        clock.take_step();
        assert_eq!(clock.frames_until_step(), 6_000);
    }

    #[test]
    fn no_drift_over_an_hour() {
        let mut clock = Clock::new(48_000.0, 137.0, 4);
        let mut last = 0;
        let mut start = 0u64;
        for _ in 0..(48_000 * 3600 / 1024) {
            clock.advance(1024, |offset| last = start + offset as u64);
            start += 1024;
        }
        // The last step lands exactly where k * samples_per_step says it should.
        let k = clock.next_step - 1;
        assert_eq!(last, (k as f64 * clock.samples_per_step).ceil() as u64);
        assert!((k as f64 * clock.samples_per_step) < start as f64);
        assert!((clock.next_step as f64 * clock.samples_per_step).ceil() >= start as f64);
    }
}
