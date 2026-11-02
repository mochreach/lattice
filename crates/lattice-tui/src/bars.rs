//! Bars with their values written over them, for levels, pan and effect settings, and meters.

use std::ops::Range;

use lattice_core::{MAX_DB, MIN_DB, ParamInfo};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::theme::Theme;

/// The quietest level a meter shows, in dB.
const METER_FLOOR: f32 = -48.0;

/// Left-aligned eighths of a cell, from one eighth to seven.
const LEFT_EIGHTHS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];

/// A level as a fader, `width` cells wide: filled from the left, from -60 dB to +6 dB.
pub fn level(db: f32, width: usize, style: Style, theme: &Theme) -> Vec<Span<'static>> {
    let filled = (db - MIN_DB) / (MAX_DB - MIN_DB) * width as f32;
    bar(width, 0.0..filled, &level_label(db), style, theme)
}

/// A pan position: filled from the centre out to the side it's panned to.
pub fn pan(pan: f32, width: usize, style: Style, theme: &Theme) -> Vec<Span<'static>> {
    let centre = width as f32 / 2.0;
    let to = centre + pan * centre;
    bar(
        width,
        centre.min(to)..centre.max(to),
        &pan_label(pan),
        style,
        theme,
    )
}

/// A macro's depth, from -1 to 1: filled from the centre, as pan is, and signed.
pub fn depth(depth: f32, width: usize, style: Style, theme: &Theme) -> Vec<Span<'static>> {
    let centre = width as f32 / 2.0;
    let to = centre + depth * centre;
    let label = format!("{depth:+.2}");
    bar(width, centre.min(to)..centre.max(to), &label, style, theme)
}

/// An effect's setting, filled from the left along its range.
pub fn param(
    info: &ParamInfo,
    value: f32,
    width: usize,
    style: Style,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let filled = info.fraction(value) * width as f32;
    bar(width, 0.0..filled, &info.format(value), style, theme)
}

/// `0dB`, `-6.5dB`, or `off` at the bottom of the range.
pub fn level_label(db: f32) -> String {
    if db <= MIN_DB {
        "off".to_string()
    } else {
        format!("{}dB", decibels(db))
    }
}

/// `C`, `L40`, `R100`.
pub fn pan_label(pan: f32) -> String {
    match (pan * 100.0).round() as i32 {
        0 => "C".to_string(),
        p if p < 0 => format!("L{}", -p),
        p => format!("R{p}"),
    }
}

/// A level to a tenth of a decibel, without the fraction when it's whole: `-6`, `-6.5`, `3`.
fn decibels(db: f32) -> String {
    let tenths = (db * 10.0).round() as i32;
    if tenths % 10 == 0 {
        (tenths / 10).to_string()
    } else {
        format!("{:.1}", tenths as f32 / 10.0)
    }
}

/// A bar `width` cells wide, filled over `fill` (in cells, to an eighth of one at the ends), with
/// `label` centred over it. Filled cells are reverse video, so the label reads on either part.
/// The unfilled part is shaded like the beat rows, to show the bar's extent.
pub fn bar(
    width: usize,
    fill: Range<f32>,
    label: &str,
    style: Style,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let empty = theme.beat_row.map_or(style, |bg| style.bg(bg));
    let full = style.add_modifier(Modifier::REVERSED);
    let label: Vec<char> = label.chars().take(width).collect();
    let start = (width - label.len()) / 2;
    (0..width)
        .map(|i| {
            let x = i as f32;
            let covered = (fill.end.min(x + 1.0) - fill.start.max(x)).clamp(0.0, 1.0);
            if let Some(&ch) = i.checked_sub(start).and_then(|k| label.get(k)) {
                let style = if covered >= 0.5 { full } else { empty };
                return Span::styled(ch.to_string(), style.add_modifier(Modifier::BOLD));
            }
            let (ch, style) = if covered >= 0.875 {
                (' ', full)
            } else if covered < 0.125 {
                (' ', empty)
            } else if fill.start > x {
                // Filled from the right of the cell: only an eighth and a half exist.
                (if covered < 0.3 { '▕' } else { '▐' }, empty)
            } else {
                let eighths = (covered * 8.0).round() as usize;
                // Clamped, so it's always there.
                let eighths = eighths.clamp(1, LEFT_EIGHTHS.len()) - 1;
                (LEFT_EIGHTHS.get(eighths).copied().unwrap_or(' '), empty)
            };
            Span::styled(ch.to_string(), style)
        })
        .collect()
}

/// A meter `width` cells wide, lit from the left as loud as `level` (linear), and red once it
/// reaches full scale, where the output starts to saturate.
pub fn meter(level: f32, width: usize, colour: Color, theme: &Theme) -> [Span<'static>; 2] {
    let db = 20.0 * level.max(1e-6).log10();
    let lit = (((db - METER_FLOOR) / -METER_FLOOR).clamp(0.0, 1.0) * width as f32).round() as usize;
    let lit_colour = if level >= 1.0 { theme.error } else { colour };
    [
        Span::styled("━".repeat(lit), Style::new().fg(lit_colour)),
        Span::styled(
            "─".repeat(width - lit),
            Style::new().fg(colour).add_modifier(Modifier::DIM),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(level_label(0.0), "0dB");
        assert_eq!(level_label(-6.5), "-6.5dB");
        assert_eq!(level_label(MIN_DB), "off");
        assert_eq!(pan_label(0.0), "C");
        assert_eq!(pan_label(-0.4), "L40");
        assert_eq!(pan_label(1.0), "R100");
    }

    #[test]
    fn bars_fill_to_an_eighth_with_the_label_on_top() {
        let theme = Theme::terminal();
        let style = Style::new().fg(Color::Red);
        let text = |spans: &[Span]| {
            spans
                .iter()
                .map(|s| s.content.to_string())
                .collect::<String>()
        };
        let reversed = |spans: &[Span]| {
            spans
                .iter()
                .map(|s| s.style.add_modifier.contains(Modifier::REVERSED))
                .collect::<Vec<_>>()
        };

        let spans = bar(11, 0.0..2.5, "ab", style, &theme);
        assert_eq!(text(&spans), "  ▌ ab     ");
        assert_eq!(reversed(&spans)[..4], [true, true, false, false]);
        // From the centre to the right.
        let spans = bar(11, 5.5..11.0, "", style, &theme);
        assert_eq!(text(&spans), "     ▐     ");
        assert!(reversed(&spans)[6..].iter().all(|&r| r));
    }
}
