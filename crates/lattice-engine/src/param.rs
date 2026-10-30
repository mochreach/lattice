//! Parameters: how instruments and effects describe their controls (shared with the song, in
//! `lattice_core`), and smoothing so that changing one never clicks.

pub use lattice_core::ParamInfo;

/// A value that glides to each new target over a fixed time instead of jumping, so level, pan
/// and parameter changes don't click.
#[derive(Debug, Clone, Copy)]
pub struct Smoothed {
    value: f32,
    target: f32,
    step: f32,
    remaining: u32,
    ramp: u32,
}

/// How long a change takes to glide in.
pub const SMOOTHING_SECONDS: f32 = 0.02;

impl Smoothed {
    pub fn new(value: f32, sample_rate: f32) -> Self {
        Self {
            value,
            target: value,
            step: 0.0,
            remaining: 0,
            ramp: (SMOOTHING_SECONDS * sample_rate).max(1.0) as u32,
        }
    }

    pub fn set(&mut self, target: f32) {
        if target != self.target {
            self.target = target;
            self.remaining = self.ramp;
            self.step = (target - self.value) / self.ramp as f32;
        }
    }

    /// Jumps straight to `value`, for when nothing is sounding yet.
    pub fn reset(&mut self, value: f32) {
        self.value = value;
        self.target = value;
        self.remaining = 0;
    }

    /// The value for the next sample.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.value = if self.remaining == 0 {
                self.target
            } else {
                self.value + self.step
            };
        }
        self.value
    }

    pub fn is_settled(&self) -> bool {
        self.remaining == 0
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    /// Where it's gliding to, or the value once it's there.
    pub fn target(&self) -> f32 {
        self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glides_to_the_target_then_holds() {
        // 1,000 Hz: a 20-sample ramp.
        let mut s = Smoothed::new(0.0, 1_000.0);
        s.set(1.0);
        let ramp: Vec<f32> = (0..20).map(|_| s.tick()).collect();
        assert!((ramp[0] - 0.05).abs() < 1e-6);
        assert!(ramp.windows(2).all(|w| w[1] > w[0]));
        assert_eq!(ramp[19], 1.0, "lands exactly");
        assert!(s.is_settled());
        assert_eq!(s.tick(), 1.0);
    }

    #[test]
    fn a_new_target_mid_glide_starts_from_where_it_is() {
        let mut s = Smoothed::new(0.0, 1_000.0);
        s.set(1.0);
        for _ in 0..10 {
            s.tick();
        }
        s.set(0.0);
        let first = s.tick();
        assert!(first < 0.5 && first > 0.45, "{first}");
    }
}
