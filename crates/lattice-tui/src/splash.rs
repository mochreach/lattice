//! The opening animation: a fractal lattice grows out from the logo in the top corner until it
//! fills the screen, then sweeps away from the same corner to show the editor beneath. Each launch
//! picks its own fractal, scale, shape of front and colours. Any key skips it; `--no-splash`
//! leaves it out.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::theme::Theme;

/// Growing out to the far corner, holding there, then sweeping away.
const GROW: f32 = 1.4;
const HOLD: f32 = 0.25;
const LAUNCH: f32 = 0.5;
/// Width of the bright leading edge, and of each band of colour behind it, in dots.
const EDGE: f32 = 3.0;
const BAND: f32 = 10.0;
/// How fast the bands ripple, in dots a second.
const RIPPLE: f32 = 40.0;
/// The logo, as the header draws it, so it stays put when the editor appears.
const LOGO: &str = " LATTICE ";

/// Which points of the lattice are lit.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Pattern {
    /// Pascal's triangle mod a prime: mod 2 is Sierpiński's triangle, larger primes leave more,
    /// larger holes.
    Pascal(u32),
    /// Sierpiński's carpet: every square of three by three loses its middle.
    Carpet,
    /// Points where `x ^ y` is a multiple of this, which weaves.
    Xor(u32),
}

const PATTERNS: [Pattern; 6] = [
    Pattern::Pascal(2),
    Pattern::Pascal(3),
    Pattern::Pascal(5),
    Pattern::Carpet,
    Pattern::Xor(3),
    Pattern::Xor(5),
];

impl Pattern {
    fn lit(self, mut x: u32, mut y: u32) -> bool {
        match self {
            // By Lucas's theorem, (x + y choose x) isn't a multiple of p when adding x and y in
            // base p carries nothing.
            Pattern::Pascal(p) => {
                while x > 0 || y > 0 {
                    if x % p + y % p >= p {
                        return false;
                    }
                    (x, y) = (x / p, y / p);
                }
                true
            }
            Pattern::Carpet => {
                while x > 0 || y > 0 {
                    if x % 3 == 1 && y % 3 == 1 {
                        return false;
                    }
                    (x, y) = (x / 3, y / 3);
                }
                true
            }
            Pattern::Xor(k) => (x ^ y).is_multiple_of(k),
        }
    }
}

/// The shape of the growing front: how far a dot is from the corner.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Front {
    Circle,
    Diagonal,
    Square,
}

const FRONTS: [Front; 3] = [Front::Circle, Front::Diagonal, Front::Square];

impl Front {
    fn distance(self, x: f32, y: f32) -> f32 {
        match self {
            Front::Circle => x.hypot(y),
            Front::Diagonal => x + y,
            Front::Square => x.max(y),
        }
    }
}

pub struct Splash {
    start: Instant,
    pattern: Pattern,
    /// Dots between points of the lattice, across and down.
    scale: u32,
    front: Front,
    /// The track colour the bands start from.
    hue: usize,
    /// 1 for bands rippling outwards, -1 for inwards.
    ripple: f32,
}

impl Splash {
    /// A different opening each time, seeded from the clock.
    pub fn new(now: Instant) -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64);
        Self::with_seed(now, seed)
    }

    fn with_seed(now: Instant, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        Self {
            start: now,
            pattern: PATTERNS[rng.below(PATTERNS.len())],
            scale: [1, 2, 4][rng.below(3)],
            front: FRONTS[rng.below(FRONTS.len())],
            hue: rng.below(64),
            ripple: if rng.below(3) == 0 { -1.0 } else { 1.0 },
        }
    }

    pub fn done(&self, now: Instant) -> bool {
        now.duration_since(self.start) >= Duration::from_secs_f32(GROW + HOLD + LAUNCH)
    }

    /// Draws over the editor, already drawn into `buf`.
    pub fn draw(&self, buf: &mut Buffer, area: Rect, theme: &Theme, now: Instant) {
        let t = now.duration_since(self.start).as_secs_f32();
        let distance = |x: f32, y: f32| self.front.distance(x, y);
        // Braille gives each cell two dots across and four down, about square.
        let far = distance(f32::from(area.width) * 2.0, f32::from(area.height) * 4.0);
        let grown = far * smoothstep((t / GROW).min(1.0));
        let swept = far * smoothstep(((t - GROW - HOLD) / LAUNCH).clamp(0.0, 1.0));
        let colours = theme.tracks.len();

        for cy in 0..area.height {
            for cx in 0..area.width {
                let mut dots = 0u32;
                let mut front = far;
                for (i, (dx, dy)) in DOTS.iter().enumerate() {
                    let (x, y) = (u32::from(cx) * 2 + dx, u32::from(cy) * 4 + dy);
                    let r = distance(x as f32, y as f32);
                    if r <= grown && self.pattern.lit(x / self.scale, y / self.scale) {
                        dots |= 1 << i;
                        front = front.min(grown - r);
                    }
                }
                let r = distance(f32::from(cx) * 2.0 + 1.0, f32::from(cy) * 4.0 + 2.0);
                if r < swept {
                    continue;
                }
                let cell = &mut buf[(area.x + cx, area.y + cy)];
                cell.reset();
                if dots == 0 {
                    continue;
                }
                cell.set_char(char::from_u32(0x2800 + dots).unwrap_or(' '));
                let style = if front < EDGE || r - swept < EDGE * 2.0 && t > GROW + HOLD {
                    Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
                } else {
                    let band = ((r - self.ripple * t * RIPPLE) / BAND).rem_euclid(colours as f32);
                    Style::new().fg(theme.tracks[(band as usize + self.hue) % colours])
                };
                cell.set_style(style);
            }
        }
        // The header's own logo, there all along.
        buf.set_string(area.x, area.y, LOGO, theme.block(theme.accent));
    }
}

/// Each dot of a braille cell, as (across, down), in the order Unicode gives them bits.
const DOTS: [(u32, u32); 8] = [
    (0, 0),
    (0, 1),
    (0, 2),
    (1, 0),
    (1, 1),
    (1, 2),
    (0, 3),
    (1, 3),
];

fn smoothstep(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

/// Xorshift: plenty for picking an animation.
struct Rng(u64);

impl Rng {
    /// Mixed with splitmix64 first, so nearby seeds, as clock readings are, go far apart.
    fn new(seed: u64) -> Self {
        let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        Self((z ^ (z >> 31)) | 1)
    }

    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_grows_from_the_corner() {
        for pattern in PATTERNS {
            assert!(pattern.lit(0, 0), "{pattern:?}");
        }
    }

    #[test]
    fn the_patterns_are_the_fractals_they_say() {
        // Row 4 of Pascal's triangle is 1 4 6 4 1: only the ends are odd, and none is a
        // multiple of 3 but 6.
        let row = |p| {
            (0..=4)
                .map(|x| Pattern::Pascal(p).lit(x, 4 - x))
                .collect::<Vec<_>>()
        };
        assert_eq!(row(2), [true, false, false, false, true]);
        assert_eq!(row(3), [true, true, false, true, true]);
        assert!(!Pattern::Carpet.lit(1, 1) && !Pattern::Carpet.lit(4, 13));
        assert!(Pattern::Carpet.lit(1, 2));
    }

    #[test]
    fn launches_differ() {
        let now = Instant::now();
        let looks: std::collections::HashSet<_> = (1..50)
            .map(|seed| {
                let s = Splash::with_seed(now, seed);
                (
                    format!("{:?}", s.pattern),
                    s.scale,
                    format!("{:?}", s.front),
                )
            })
            .collect();
        assert!(looks.len() > 10, "{} looks", looks.len());
    }
}
