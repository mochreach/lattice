//! The pieces of a waveguide string, which Monochord and Krell share: a delay line, a string
//! of two of them that meet where a bow would sit, and a pluck to put on it.

use crate::rhythmicon::Noise;

/// Where the bow sits, as a fraction of the string from the bridge: a violin's usual spot.
const BOW_POINT: f32 = 0.127;

/// Takes each of `burst`'s samples, as far as there's one `gap` on, from that one, in place: each
/// is read before it's written.
#[expect(
    clippy::indexing_slicing,
    reason = "`k + gap` is under the burst's length for every `k` the loop takes"
)]
fn less_ahead(burst: &mut [f32], gap: usize) {
    for k in 0..burst.len().saturating_sub(gap) {
        burst[k] = burst[k + gap] - burst[k];
    }
}

/// A delay line, written a sample at a time.
#[derive(Debug, Clone)]
pub(crate) struct Line {
    /// A power of two long, so a mask wraps an index round it.
    samples: Box<[f32]>,
    /// Where the next sample goes.
    write: usize,
}

impl Line {
    /// At least `len` long.
    pub(crate) fn new(len: usize) -> Self {
        Self {
            samples: vec![0.0; len.next_power_of_two()].into_boxed_slice(),
            write: 0,
        }
    }

    fn mask(&self) -> usize {
        self.samples.len() - 1
    }

    /// Where the sample written `delay` samples ago sits.
    #[inline]
    fn at(&self, delay: usize) -> usize {
        // Wrapping, so a delay longer than the line wraps round it too.
        (self.write + self.samples.len()).wrapping_sub(delay) & self.mask()
    }

    /// The sample `delay` (at least 1) samples back.
    #[inline]
    #[expect(
        clippy::indexing_slicing,
        reason = "`at` masks the index to the line's length, a power of two"
    )]
    pub(crate) fn read(&self, delay: usize) -> f32 {
        self.samples[self.at(delay)]
    }

    /// The line `delay` samples back, read with third-order Lagrange interpolation, which keeps
    /// the high harmonics a linear read would dull. `delay` is at least 2.
    #[inline]
    pub(crate) fn read_between(&self, delay: f32) -> f32 {
        let whole = delay as usize;
        // The four taps sit at delays whole - 1 to whole + 2, so `d` is between 1 and 2.
        let d = delay - whole as f32 + 1.0;
        let tap = |k: usize| self.read(whole - 1 + k);
        let (dm1, dm2, dm3) = (d - 1.0, d - 2.0, d - 3.0);
        -dm1 * dm2 * dm3 / 6.0 * tap(0) + d * dm2 * dm3 / 2.0 * tap(1)
            - d * dm1 * dm3 / 2.0 * tap(2)
            + d * dm1 * dm2 / 6.0 * tap(3)
    }

    /// Mixes `x` into the sample `delay` back, over what's there times `keep`.
    #[inline]
    #[expect(
        clippy::indexing_slicing,
        reason = "`at` masks the index to the line's length, a power of two"
    )]
    pub(crate) fn blend(&mut self, delay: usize, keep: f32, x: f32) {
        let at = self.at(delay);
        self.samples[at] = self.samples[at] * keep + x;
    }

    #[inline]
    #[expect(
        clippy::indexing_slicing,
        reason = "`write` is kept masked to the line's length, a power of two"
    )]
    pub(crate) fn write(&mut self, x: f32) {
        self.samples[self.write] = x;
        self.write = (self.write + 1) & self.mask();
    }
}

/// One string: the line from the bow to the bridge and back, and from the bow to the nut and
/// back.
#[derive(Debug, Clone)]
pub(crate) struct Course {
    pub(crate) bridge: Line,
    pub(crate) neck: Line,
    /// The bridge side's delay, which takes the fraction that tunes the string.
    pub(crate) bridge_delay: f32,
    /// The neck side's, a whole number of samples.
    pub(crate) neck_delay: usize,
    /// The period, in samples.
    pub(crate) period: f32,
    /// What's left after each trip round the string.
    pub(crate) loss: f32,
    /// The wave last back from the bridge, for the low-pass there.
    pub(crate) last: f32,
}

impl Course {
    pub(crate) fn new(len: usize) -> Self {
        Self {
            bridge: Line::new(len),
            neck: Line::new(len),
            bridge_delay: 2.0,
            neck_delay: 1,
            period: 3.0,
            loss: 0.0,
            last: 0.0,
        }
    }

    /// Divides a period of `period` samples between the two sides of the bow, less `damping`,
    /// the low-pass's delay.
    pub(crate) fn tune(&mut self, period: f32, damping: f32) {
        self.period = period;
        let length = (period - damping).max(3.0);
        self.neck_delay = ((length * (1.0 - BOW_POINT)) as usize).clamp(1, (length - 2.0) as usize);
        self.bridge_delay = length - self.neck_delay as f32;
    }

    /// Changes the period a little on the bridge's side alone, so a string that's sounding
    /// changes pitch smoothly rather than jumping, as it would if the two sides were divided
    /// afresh.
    pub(crate) fn stretch(&mut self, period: f32, damping: f32) {
        self.period = period;
        self.bridge_delay = ((period - damping).max(3.0) - self.neck_delay as f32).max(2.0);
    }

    /// One sample: the waves arriving at the bow from each end, reflected, pass the bow on to the
    /// other side, with whatever the bow adds. `bow` is the bow's speed and grip, if it's on the
    /// string. Returns the wave back from the bridge.
    #[inline]
    pub(crate) fn tick(&mut self, damping: f32, buzz: f32, bow: Option<(f32, f32)>) -> f32 {
        // A loud string hits the bridge and plays shorter.
        let delay = (self.bridge_delay * (1.0 - buzz * self.last.abs().min(1.0))).max(2.0);
        let from_bridge = self.bridge.read_between(delay);
        let bridge = -self.loss * ((1.0 - damping) * from_bridge + damping * self.last);
        self.last = from_bridge;
        let nut = -self.neck.read(self.neck_delay);
        let push = match bow {
            Some((speed, grip)) => {
                // How fast the bow moves against the string: slowly, it sticks and drags the
                // string along; fast, it slips. The friction curve is STK's.
                let slip = speed - (bridge + nut);
                let friction = ((slip + 0.001) * grip).abs() + 0.75;
                slip * friction.powi(-4).min(1.0)
            }
            None => 0.0,
        };
        self.neck.write(bridge + push);
        self.bridge.write(nut + push);
        from_bridge
    }
    /// Puts a pluck on the string: one period of it, `wave`, goes round it, over what's left of
    /// the string times `keep`.
    pub(crate) fn place(&mut self, wave: &[f32], keep: f32) {
        let len = wave.len() as isize;
        if len == 0 {
            return;
        }
        // The pluck as the bridge's line will give it back, `k` samples from now. The wrapped
        // index is under `len`, so always in the wave.
        let wave = |k: isize| wave.get(k.rem_euclid(len) as usize).copied().unwrap_or(0.0);
        // A sample `j` back in the bridge's line comes out in `bridge - j` samples, and one `j`
        // back in the neck's goes over to the bridge's, inverted, `neck - j` from now.
        let bridge = self.bridge_delay as isize;
        let neck = self.neck_delay as isize;
        for j in 1..=bridge + 3 {
            self.bridge.blend(j as usize, keep, wave(bridge - j));
        }
        for j in 1..=neck {
            self.neck.blend(j as usize, keep, -wave(neck - j + bridge));
        }
        self.last *= keep;
    }
}

/// One period of a pluck, `len` samples long, made in `scratch`, which needs room for `len + gap`:
/// noise low-passed by `smooth` (a one-pole's coefficient), less itself `gap` samples on, which
/// cancels the harmonics with a node `gap` samples along the string, then centred and scaled to
/// an RMS of `level`. `None` if `scratch` is too short.
pub(crate) fn pluck<'a>(
    scratch: &'a mut [f32],
    noise: &mut Noise,
    len: usize,
    gap: usize,
    smooth: f32,
    level: f32,
) -> Option<&'a [f32]> {
    let burst = scratch.get_mut(..len + gap)?;
    let mut y = 0.0;
    // Enough first for the filter to settle by the period that's kept: it's within 1% of
    // where it's going in five time constants, and a time constant is `1 / smooth` samples.
    let settle = ((5.0 / smooth.max(1e-6)) as usize).min(len);
    for _ in 0..settle {
        y += smooth * (noise.next() - y);
    }
    for x in burst.iter_mut().take(len) {
        y += smooth * (noise.next() - y);
        *x = y;
    }
    // The period, less itself `gap` samples on, going round.
    burst.copy_within(..gap, len);
    less_ahead(burst, gap);
    let wave = burst.get_mut(..len)?;
    let mean = wave.iter().sum::<f32>() / len as f32;
    let rms = (wave.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / len as f32)
        .sqrt()
        .max(1e-9);
    let scale = level / rms;
    for x in wave.iter_mut() {
        *x = (*x - mean) * scale;
    }
    Some(wave)
}
