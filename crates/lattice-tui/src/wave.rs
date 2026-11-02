//! Waveforms for chop tracks, drawn down the pattern page in braille: time runs down a step at a
//! time, four dots to a row, and loudness spreads out from the middle of the column.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lattice_engine::Sample;

use crate::samples;

/// The loudest of so many stretches of a file, as fractions of its loudest.
#[derive(Debug, PartialEq)]
pub struct Peaks(Vec<f32>);

/// Stretches a file's loudness is kept in: enough for 64 steps of four dots, a few times over.
const BUCKETS: usize = 4096;

impl Peaks {
    pub fn new(sample: &Sample) -> Self {
        let n = BUCKETS.min(sample.len()).max(1);
        let mut peaks = vec![0.0f32; n];
        for (i, (l, r)) in sample.left.iter().zip(&sample.right).enumerate() {
            // `i` is less than the length, so the bucket's always there.
            if let Some(peak) = peaks.get_mut(i * n / sample.len()) {
                *peak = peak.max(l.abs()).max(r.abs());
            }
        }
        let loudest = peaks.iter().copied().fold(0.0, f32::max);
        if loudest > 0.0 {
            peaks.iter_mut().for_each(|p| *p /= loudest);
        }
        Self(peaks)
    }

    /// The loudest between fractions `from` and `to` of the file.
    fn loudest(&self, from: f64, to: f64) -> f32 {
        let n = self.0.len();
        let first = ((from.max(0.0) * n as f64) as usize).min(n);
        let last = ((to.min(1.0) * n as f64).ceil() as usize).clamp(first, n);
        let last = if last == first {
            (first + 1).min(n)
        } else {
            last
        };
        // Both are clamped to the peaks, so this is never silent for want of them.
        let peaks = self.0.get(first..last).unwrap_or_default();
        peaks.iter().copied().fold(0.0, f32::max)
    }

    /// One row, `width` cells wide, of the file from fraction `from` to `to`: four dots down, the
    /// first at the top, or the last if it plays backwards.
    pub fn row(&self, from: f64, to: f64, reverse: bool, width: usize) -> String {
        // Braille's dots, each a bit: four rows of two.
        const DOTS: [[u32; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
        let mut cells = vec![0u32; width];
        let quarter = (to - from) / 4.0;
        for (dot_row, bits) in DOTS.iter().enumerate() {
            let at = if reverse { 3 - dot_row } else { dot_row };
            let start = from + quarter * at as f64;
            let level = self.loudest(start, start + quarter);
            // Out from the middle, either way: at least a dot each side for anything audible.
            let mut half = (level * width as f32).round() as usize;
            if level > 0.01 {
                half = half.max(1);
            }
            for dot in width - half.min(width)..width + half.min(width) {
                // Up to twice the width, so each dot's in a cell.
                if let (Some(cell), Some(bit)) = (cells.get_mut(dot / 2), bits.get(dot % 2)) {
                    *cell |= bit;
                }
            }
        }
        cells
            .into_iter()
            .map(|bits| char::from_u32(0x2800 + bits).unwrap_or(' '))
            .collect()
    }
}

/// The files' peaks, worked out once each.
#[derive(Debug, Default)]
pub struct Waves(HashMap<PathBuf, (Arc<Sample>, Arc<Peaks>)>);

impl Waves {
    /// The peaks of the file at `path`, once it's been read for an instrument.
    pub fn get(&mut self, path: &Path) -> Option<Arc<Peaks>> {
        let sample = samples::cached(path)?;
        match self.0.get(path) {
            Some((read, peaks)) if Arc::ptr_eq(read, &sample) => Some(peaks.clone()),
            _ => {
                let peaks = Arc::new(Peaks::new(&sample));
                self.0.insert(path.into(), (sample, peaks.clone()));
                Some(peaks)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Silent, then loud.
    fn half_loud() -> Peaks {
        let left: Vec<f32> = (0..100).map(|i| if i < 50 { 0.0 } else { 0.5 }).collect();
        Peaks::new(&Sample {
            rate: 1_000.0,
            right: left.clone(),
            left,
        })
    }

    #[test]
    fn loudness_spreads_from_the_middle() {
        let peaks = half_loud();
        // Silent above, full width below.
        assert_eq!(peaks.row(0.0, 1.0, false, 2), "⣤⣤");
        // A quiet row is blank.
        assert_eq!(peaks.row(0.0, 0.5, false, 3), "⠀⠀⠀");
    }

    #[test]
    fn backwards_the_end_comes_first() {
        assert_eq!(half_loud().row(0.0, 1.0, true, 2), "⠛⠛");
    }

    #[test]
    fn past_the_end_is_silent() {
        assert_eq!(half_loud().row(1.0, 1.5, false, 2), "⠀⠀");
    }
}
