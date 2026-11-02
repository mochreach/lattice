//! A face's art: one of the splash's fractals, drawn in braille, mirrored about the middle so it
//! looks back at you, and played by the track. The title picks the fractal, its scale and which
//! way it flows; the track's level lights it from the middle out, each new note sends a ring
//! out from the middle, and the note's pitch picks the colour, a step round the colour wheel for
//! each step round the circle of fifths. The first macro zooms in on it, and the second hurries
//! its flow. At rest it's a faint outline.

use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::splash::{DOTS, PATTERNS, Pattern, Rng};
use crate::theme::Theme;

/// How fast the art flows, in dots a second, and how much faster the second macro makes it.
const FLOW: f32 = 1.5;
const HURRY: f32 = 8.0;
/// How far in the first macro zooms, as a multiple.
const ZOOM: f32 = 2.0;
/// How fast a note's ring goes out, in radii a second, and how long it lasts.
const RING_SPEED: f32 = 3.0;
const RING_TIME: f32 = 0.35;
const RING_WIDTH: f32 = 0.12;
/// Bands of colour across the radius, and how fast they ripple outwards, in bands a second.
const BANDS: f32 = 4.0;
const RIPPLE: f32 = 1.0;
/// A velocity above which the colours are bold.
const LOUD: u8 = 100;

/// What a track's doing, for its face to show.
#[derive(Debug, Clone, Copy, Default)]
pub struct Feel {
    /// From 0, silent, to 1, full scale.
    pub level: f32,
    /// The last note's pitch and velocity.
    pub note: Option<(u8, u8)>,
    /// Seconds since the last note started.
    pub since_note: f32,
    /// The first two macros, from 0 to 1.
    pub macros: [f32; 2],
}

/// The art a title gives.
struct Look {
    pattern: Pattern,
    /// Dots between points of the fractal.
    scale: f32,
    /// Whether it flows across, down, or both.
    flow: (f32, f32),
    /// Where on the colour wheel it starts, before any note.
    hue: usize,
}

impl Look {
    fn of(title: &str) -> Self {
        // FNV-1a: the same title always gives the same art.
        let hash = (title.bytes()).fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
        });
        let mut rng = Rng::new(hash);
        Self {
            pattern: rng.pick(&PATTERNS, Pattern::Carpet),
            scale: rng.pick(&[1.0, 2.0, 3.0], 1.0),
            flow: rng.pick(&[(1.0, 0.0), (0.0, 1.0), (1.0, 1.0)], (1.0, 0.0)),
            hue: rng.below(64),
        }
    }
}

/// Where the art's time starts, so it flows on smoothly from frame to frame.
#[derive(Debug)]
pub struct Clock(Instant);

impl Default for Clock {
    fn default() -> Self {
        Self(Instant::now())
    }
}

impl Clock {
    pub fn seconds(&self) -> f32 {
        self.0.elapsed().as_secs_f32()
    }
}

/// Draws the art for `title` into `area` of `buf`, as `feel` says, `time` seconds in.
pub fn draw(buf: &mut Buffer, area: Rect, title: &str, feel: &Feel, theme: &Theme, time: f32) {
    let look = Look::of(title);
    let (w, h) = (f32::from(area.width) * 2.0, f32::from(area.height) * 4.0);
    if w == 0.0 || h == 0.0 {
        return;
    }
    let zoom = look.scale * (1.0 + ZOOM * feel.macros[0]);
    let flowed = time * (FLOW + HURRY * feel.macros[1]);
    // How far out from the middle it's lit, as a share of the way to the edge.
    let glow = 0.15 + 0.85 * feel.level.clamp(0.0, 1.0).sqrt();
    let ring = (feel.since_note < RING_TIME).then_some(feel.since_note * RING_SPEED);
    // A step round the wheel for each two fifths, as there are six colours to twelve notes.
    let hues = theme.hues.len();
    let hue = match feel.note {
        Some((pitch, _)) => usize::from(pitch % 12) * 7 % 12 * hues / 12,
        None => look.hue,
    };
    let bold = feel.note.is_some_and(|(_, velocity)| velocity > LOUD);
    // How far a point is from the middle, 1 at the edge of the ellipse filling the area.
    let radius = |x: f32, y: f32| ((x - w / 2.0) / (w / 2.0)).hypot((y - h / 2.0) / (h / 2.0));

    for cy in 0..area.height {
        for cx in 0..area.width {
            let mut dots = 0u32;
            for (i, (dx, dy)) in DOTS.iter().enumerate() {
                let (x, y) = (
                    f32::from(cx) * 2.0 + *dx as f32 + 0.5,
                    f32::from(cy) * 4.0 + *dy as f32 + 0.5,
                );
                if radius(x, y) >= 1.0 {
                    continue;
                }
                // Mirrored about the middle, and flowing in towards it.
                let px = (x - w / 2.0).abs() / zoom + flowed * look.flow.0;
                let py = (y - h / 2.0).abs() / zoom + flowed * look.flow.1;
                if look.pattern.lit(px as u32, py as u32) {
                    dots |= 1 << i;
                }
            }
            if dots == 0 {
                continue;
            }
            let r = radius(f32::from(cx) * 2.0 + 1.0, f32::from(cy) * 4.0 + 2.0);
            let style = if ring.is_some_and(|ring| (r - ring).abs() < RING_WIDTH) {
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
            } else if r < glow {
                // Two neighbouring colours, in bands rippling out.
                let band = (r * BANDS - time * RIPPLE).floor().rem_euclid(2.0) as usize;
                // With no hues, the band is left in the default colour.
                let colour = theme.hues.iter().cycle().nth(hue + band);
                let style = Style::new().fg(colour.copied().unwrap_or_default());
                if bold {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    style
                }
            } else {
                theme.faint
            };
            // The area's in the buffer; if it weren't, there'd be nothing there to draw on.
            let Some(cell) = buf.cell_mut((area.x + cx, area.y + cy)) else {
                continue;
            };
            cell.set_char(char::from_u32(0x2800 + dots).unwrap_or(' '));
            cell.set_style(style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(buf: &Buffer) -> usize {
        buf.content.iter().filter(|c| c.symbol() != " ").count()
    }

    #[test]
    fn a_face_is_its_titles_and_lights_up_as_it_plays() {
        let theme = Theme::terminal();
        let area = Rect::new(0, 0, 20, 6);
        let draw_with = |title: &str, feel: Feel| {
            let mut buf = Buffer::empty(area);
            draw(&mut buf, area, title, &feel, &theme, 0.0);
            buf
        };
        let rest = draw_with("Night Bus", Feel::default());
        assert!(lit(&rest) > 0, "an outline at rest");
        assert_eq!(
            rest,
            draw_with("Night Bus", Feel::default()),
            "the same each time"
        );
        let looks: std::collections::HashSet<String> =
            ["Night Bus", "Rubber", "Foundry", "Monsoon"]
                .iter()
                .map(|t| format!("{:?}", draw_with(t, Feel::default()).content))
                .collect();
        assert!(looks.len() > 1, "titles differ");

        // Playing, it's in colour, from the middle.
        let colours = |buf: &Buffer| {
            (buf.content.iter())
                .filter(|c| c.fg != theme.faint.fg.unwrap())
                .filter(|c| c.symbol() != " ")
                .count()
        };
        assert_eq!(colours(&rest), 0);
        let loud = Feel {
            level: 1.0,
            note: Some((60, 120)),
            since_note: 1.0,
            macros: [0.0; 2],
        };
        assert!(colours(&draw_with("Night Bus", loud)) > 0);
    }
}
