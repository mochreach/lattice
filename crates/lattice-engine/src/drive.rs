//! A saturator: soft clipping, pushed a little off centre so it adds even harmonics as well as
//! odd, then a low-pass to take off the fizz.

use std::f32::consts::TAU;

use lattice_core::{FxKind, db_to_gain};

use crate::param::{ParamInfo, Smoothed};
use crate::processor::{Event, EventKind, Processor, Segment, segments};

pub use lattice_core::fx::drive::{DRIVE, LEVEL, MIX, TONE};

const PARAMS: &[ParamInfo] = FxKind::Drive.params();

/// Gain into the clipper with the drive full up: +36 dB.
const MAX_GAIN: f32 = 63.0;
/// How far off centre the clipper is pushed, with the drive full up.
const MAX_BIAS: f32 = 0.3;

pub struct Drive {
    sample_rate: f32,
    drive: Smoothed,
    mix: Smoothed,
    level: Smoothed,
    /// One-pole low-pass coefficient, and its state per channel.
    tone: f32,
    low: [f32; 2],
    /// DC blocker state per channel, as the bias shifts the waveform off centre.
    dc: [[f32; 2]; 2],
    dc_coef: f32,
}

impl Drive {
    pub fn new(sample_rate: f32) -> Self {
        let mut drive = Self {
            sample_rate,
            drive: Smoothed::new(0.0, sample_rate),
            mix: Smoothed::new(0.0, sample_rate),
            level: Smoothed::new(0.0, sample_rate),
            tone: 0.0,
            low: [0.0; 2],
            dc: [[0.0; 2]; 2],
            dc_coef: 1.0 - TAU * 5.0 / sample_rate,
        };
        for info in PARAMS {
            drive.set(info.id, info.default);
        }
        drive.settle();
        drive
    }

    /// Starts at `values` (one per parameter, by id) rather than gliding to them.
    pub fn with_values(sample_rate: f32, values: &[f32]) -> Self {
        let mut drive = Self::new(sample_rate);
        for (id, &value) in values.iter().enumerate() {
            drive.set(id as u32, value);
        }
        drive.settle();
        drive
    }

    fn settle(&mut self) {
        for smoothed in [&mut self.drive, &mut self.mix, &mut self.level] {
            smoothed.reset(smoothed.target());
        }
    }

    fn set_tone(&mut self, hz: f32) {
        self.tone = 1.0 - (-TAU * hz.min(0.45 * self.sample_rate) / self.sample_rate).exp();
    }

    fn set(&mut self, id: u32, value: f32) {
        let Some(info) = PARAMS.get(id as usize) else {
            return;
        };
        let value = info.clamp(value);
        match id {
            DRIVE => self.drive.set(value),
            TONE => self.set_tone(value),
            MIX => self.mix.set(value),
            _ => self.level.set(db_to_gain(value)),
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let drive = self.drive.tick();
            let mix = self.mix.tick();
            let level = self.level.tick();
            // Exponential in drive, so the control feels even across its travel.
            let gain = MAX_GAIN.powf(drive);
            let bias = MAX_BIAS * drive;
            // Scaled so the larger, negative side of a full-scale input peaks at 1, however
            // hard it's driven.
            let norm = 1.0 / ((gain - bias).tanh() + bias.tanh());
            let channels = self.dc.iter_mut().zip(&mut self.low);
            for (x, (dc, low)) in [l, r].into_iter().zip(channels) {
                let shaped = ((*x * gain + bias).tanh() - bias.tanh()) * norm;
                let [last_in, last_out] = *dc;
                let centred = shaped - last_in + self.dc_coef * last_out;
                *dc = [shaped, centred];
                *low += self.tone * (centred - *low);
                *x = (*x + (*low - *x) * mix) * level;
            }
        }
    }
}

impl Processor for Drive {
    fn process(&mut self, events: &[Event], left: &mut [f32], right: &mut [f32]) {
        segments(events, left.len(), |segment| match segment {
            Segment::Render(range) => {
                // The channels are the same length, so both have the range; if one didn't,
                // there'd be no right way to render it.
                if let (Some(l), Some(r)) = (left.get_mut(range.clone()), right.get_mut(range)) {
                    self.render(l, r);
                }
            }
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

    const RATE: f32 = 48_000.0;

    fn values(set: &[(u32, f32)]) -> Vec<f32> {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        for &(id, value) in set {
            values[id as usize] = value;
        }
        values
    }

    /// A tenth of a second of a 100 Hz sine at `amp`, through the drive, as the left channel.
    fn sine(drive: &mut Drive, amp: f32) -> Vec<f32> {
        let len = RATE as usize / 10;
        let mut left: Vec<f32> = (0..len)
            .map(|i| amp * (TAU * 100.0 * i as f32 / RATE).sin())
            .collect();
        let mut right = left.clone();
        drive.process(&[], &mut left, &mut right);
        assert_eq!(left, right);
        left
    }

    fn peak(audio: &[f32]) -> f32 {
        audio.iter().fold(0.0, |m, x| m.max(x.abs()))
    }

    #[test]
    fn driving_harder_squares_off_the_wave_but_keeps_it_in_bounds() {
        let gentle = sine(&mut Drive::with_values(RATE, &values(&[(DRIVE, 0.0)])), 1.0);
        let hard = sine(&mut Drive::with_values(RATE, &values(&[(DRIVE, 1.0)])), 1.0);
        // Once the DC blocker has settled.
        let (gentle, hard) = (&gentle[2_400..], &hard[2_400..]);
        // RMS over peak: about 0.71 for a sine, nearer 1 the squarer it gets.
        let fullness = |audio: &[f32]| {
            let rms = (audio.iter().map(|x| x * x).sum::<f32>() / audio.len() as f32).sqrt();
            rms / peak(audio)
        };
        assert!(peak(hard) <= 1.2 && peak(gentle) <= 1.2);
        assert!(fullness(gentle) < 0.75, "{}", fullness(gentle));
        assert!(fullness(hard) > 0.85, "{}", fullness(hard));
    }

    #[test]
    fn quiet_input_is_boosted_by_the_drive() {
        let clean = sine(
            &mut Drive::with_values(RATE, &values(&[(DRIVE, 0.0)])),
            0.05,
        );
        let hard = sine(
            &mut Drive::with_values(RATE, &values(&[(DRIVE, 1.0)])),
            0.05,
        );
        assert!(peak(&hard[2_400..]) > 5.0 * peak(&clean[2_400..]));
    }

    #[test]
    fn mix_at_zero_passes_the_input_through_at_the_level() {
        let mut drive = Drive::with_values(RATE, &values(&[(MIX, 0.0), (LEVEL, -6.0)]));
        let (mut left, mut right) = (vec![0.5; 4], vec![0.5; 4]);
        drive.process(&[], &mut left, &mut right);
        assert!(
            left.iter()
                .all(|&x| (x - 0.5 * db_to_gain(-6.0)).abs() < 1e-6)
        );
    }

    #[test]
    fn out_of_range_values_are_clamped_and_unknown_ids_ignored() {
        let mut drive = Drive::with_values(RATE, &values(&[(DRIVE, 50.0)]));
        let event = Event {
            offset: 0,
            kind: EventKind::Param { id: 99, value: 1.0 },
        };
        drive.process(&[event], &mut [0.0; 4], &mut [0.0; 4]);
        assert!(
            sine(&mut drive, 1.0)
                .iter()
                .all(|x| x.is_finite() && x.abs() <= 1.2)
        );
    }
}
