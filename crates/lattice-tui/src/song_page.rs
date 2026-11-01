//! Drawing the song page: the arrangement as a strip across the top, each entry as wide as it
//! plays for, then the entries one to a line, and the patterns there are to choose from.

use lattice_core::BEATS_PER_BAR;
use lattice_editor::Editor;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::theme::Theme;
use crate::ui::fit;

/// Width of a pattern's name in the list of entries.
const NAME: usize = 12;

pub fn draw(frame: &mut Frame, area: Rect, editor: &Editor, theme: &Theme) {
    let arrangement = &editor.arrangement;
    let steps_per_beat = u64::from(editor.grid.steps_per_beat.max(1));
    let bar = u64::from(BEATS_PER_BAR) * steps_per_beat;
    let pattern_steps: Vec<u64> = (0..arrangement.patterns.len())
        .map(|p| arrangement.steps(&editor.grid, p))
        .collect();
    let lengths: Vec<u64> = arrangement
        .entries
        .iter()
        .map(|e| pattern_steps[e.pattern] * u64::from(e.repeats))
        .collect();
    let total: u64 = lengths.iter().sum();
    let time = |steps: u64| {
        let seconds = steps as f64 / steps_per_beat as f64 * 60.0 / editor.bpm;
        let seconds = seconds.round() as u64;
        format!("{}:{:02}", seconds / 60, seconds % 60)
    };
    let length = |steps: u64| {
        if steps.is_multiple_of(bar) {
            let bars = steps / bar;
            format!("{bars} bar{}", if bars == 1 { "" } else { "s" })
        } else {
            format!("{steps} steps")
        }
    };
    let playing = editor.song_position;
    let cursor = editor.song_cursor();
    let width = area.width as usize;

    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                " {} entries · {} · {}",
                arrangement.entries.len(),
                length(total),
                time(total)
            ),
            theme.dim,
        )),
        strip(editor, theme, &lengths, total, width.saturating_sub(2)),
        Line::raw(""),
    ];

    // The entries, scrolled to keep the cursor in view, leaving room for the patterns below.
    let rows = (area.height as usize)
        .saturating_sub(lines.len() + 4)
        .max(1);
    let first = cursor
        .saturating_sub(rows / 2)
        .min(arrangement.entries.len().saturating_sub(rows));
    if arrangement.entries.is_empty() {
        lines.push(Line::styled(
            "   nothing to play yet: o adds an entry",
            theme.dim,
        ));
    }
    let mut start: u64 = lengths[..first].iter().sum();
    for (i, entry) in arrangement
        .entries
        .iter()
        .enumerate()
        .skip(first)
        .take(rows)
    {
        let pattern = &arrangement.patterns[entry.pattern];
        let colour = theme.track(entry.pattern);
        let marker = match playing {
            Some((at, repeat)) if at == i => {
                format!("▶ {}/{}", repeat + 1, entry.repeats)
            }
            _ => String::new(),
        };
        let repeats = if entry.repeats > 1 {
            format!("×{}", entry.repeats)
        } else {
            String::new()
        };
        let mut spans = vec![
            Span::styled(format!(" {:>3} ", i + 1), theme.dim),
            Span::styled(format!("{:>5}  ", time(start)), theme.dim),
            Span::styled(fit(&pattern.name, NAME), Style::new().fg(colour)),
            Span::styled(
                format!(" {:<10}", length(pattern_steps[entry.pattern])),
                theme.text,
            ),
            Span::styled(format!("{repeats:<4}"), theme.text),
            Span::styled(marker, Style::new().fg(theme.playing)),
        ];
        if i == cursor {
            for span in &mut spans {
                span.style = span.style.bg(theme.selection);
            }
        }
        lines.push(Line::from(spans));
        start += lengths[i];
    }

    // The patterns to choose from, the one being edited underlined.
    lines.push(Line::raw(""));
    let mut patterns = vec![Span::styled(" patterns  ", theme.dim)];
    for (p, pattern) in arrangement.patterns.iter().enumerate() {
        let mut style = Style::new().fg(theme.track(p));
        if p == arrangement.current {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        patterns.push(Span::styled(pattern.name.clone(), style));
        patterns.push(Span::styled(
            format!(" {}   ", length(pattern_steps[p])),
            theme.dim,
        ));
    }
    lines.push(Line::from(patterns));
    frame.render_widget(Paragraph::new(lines), area);
}

/// The song in one line, each entry as wide as it plays for, coloured by its pattern and named
/// where there's room, with the entry playing marked.
fn strip<'a>(
    editor: &'a Editor,
    theme: &Theme,
    lengths: &[u64],
    total: u64,
    width: usize,
) -> Line<'a> {
    let mut spans = vec![Span::raw(" ")];
    if total == 0 || width == 0 {
        return Line::from(spans);
    }
    let playing = editor.song_position.map(|(e, _)| e);
    let mut start = 0;
    let mut drawn = 0;
    for (i, entry) in editor.arrangement.entries.iter().enumerate() {
        start += lengths[i];
        // Rounded at the ends, so the strip fills the width exactly.
        let end = (start * width as u64).div_ceil(total) as usize;
        let cells = end.saturating_sub(drawn);
        if cells == 0 {
            continue;
        }
        drawn = end;
        // A gap between entries, where there's room for one.
        let block = if cells > 1 { cells - 1 } else { 1 };
        let name = &editor.arrangement.patterns[entry.pattern].name;
        let mut text: String = format!(" {name}").chars().take(block).collect();
        if playing == Some(i) {
            text = format!("▶{}", text.chars().skip(1).collect::<String>());
        }
        let pad = block - text.chars().count();
        spans.push(Span::styled(
            format!("{text}{}", " ".repeat(pad)),
            theme.block(theme.track(entry.pattern)),
        ));
        if cells > 1 {
            spans.push(Span::raw(" "));
        }
    }
    Line::from(spans)
}
