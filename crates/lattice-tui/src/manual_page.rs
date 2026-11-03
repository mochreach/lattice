//! Drawing the manual: its lines as they're laid out, the headings picked out, and Markdown's
//! `code`, **bold** and links styled rather than shown.

use lattice_editor::{Editor, ManualLine, manual_wrap};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::theme::Theme;

pub fn draw(frame: &mut Frame, area: Rect, editor: &Editor, theme: &Theme) {
    let lines = editor.manual_lines();
    let height = usize::from(area.height.saturating_sub(1)).max(1);
    // Two columns in, the manual laid out to fit (see `Editor::fit_manual`).
    let room = usize::from(area.width).saturating_sub(4).max(20);
    let top = editor.manual.scroll.min(lines.len().saturating_sub(1));
    let mut rows = vec![];
    for line in lines.iter().skip(top) {
        if rows.len() >= height {
            break;
        }
        rows.extend(manual_rows(line, room, theme));
    }
    rows.truncate(height);
    let rect = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    frame.render_widget(Paragraph::new(rows), rect);
}

fn manual_rows(line: &ManualLine, room: usize, theme: &Theme) -> Vec<Line<'static>> {
    match line {
        ManualLine::Heading { level, text } => {
            let style = Style::new().fg(theme.accent).add_modifier(Modifier::BOLD);
            let (text, style) = match level {
                1 => (
                    text.to_uppercase(),
                    style.add_modifier(Modifier::UNDERLINED),
                ),
                2 => (text.clone(), style),
                _ => (text.clone(), theme.text.add_modifier(Modifier::BOLD)),
            };
            vec![Line::from(Span::styled(format!("  {text}"), style))]
        }
        ManualLine::Code(code) => vec![Line::from(Span::styled(
            format!("      {code}"),
            Style::new().fg(theme.visual),
        ))],
        ManualLine::Text(text) => {
            // Laid out to fit, a line is wrapped again only if it's a table too wide to narrow
            // further; its wrapped rows stay under its text.
            let first = text.len() - text.trim_start().len();
            let indent = text.len() - text.trim_start_matches([' ', '-', '*', '|']).len();
            (manual_wrap(text.trim_start(), room.saturating_sub(first))
                .into_iter()
                .enumerate())
            .map(|(i, row)| {
                let lead = 2 + if i == 0 { first } else { indent };
                let mut spans = vec![Span::raw(" ".repeat(lead))];
                spans.extend(inline(&row, theme));
                Line::from(spans)
            })
            .collect()
        }
    }
}

/// A row of prose: `code` in the accent colour, **bold** bold, `[text](link)` as its text
/// underlined, and a table's pipes faint.
fn inline(text: &str, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = vec![];
    let mut plain = String::new();
    let mut rest = text;
    let flush = |plain: &mut String, spans: &mut Vec<Span<'static>>| {
        if !plain.is_empty() {
            spans.push(Span::styled(std::mem::take(plain), theme.text));
        }
    };
    while let Some(c) = rest.chars().next() {
        let after = rest.get(c.len_utf8()..).unwrap_or("");
        if c == '`'
            && let Some(end) = after.find('`')
        {
            flush(&mut plain, &mut spans);
            let code = after.get(..end).unwrap_or("");
            spans.push(Span::styled(
                code.to_string(),
                Style::new().fg(theme.accent),
            ));
            rest = after.get(end + 1..).unwrap_or("");
        } else if let Some(bold) = rest.strip_prefix("**")
            && let Some(end) = bold.find("**")
        {
            flush(&mut plain, &mut spans);
            let words = bold.get(..end).unwrap_or("");
            spans.push(Span::styled(
                words.to_string(),
                theme.text.add_modifier(Modifier::BOLD),
            ));
            rest = bold.get(end + 2..).unwrap_or("");
        } else if c == '['
            && let Some(close) = after.find("](")
            && let Some(end) = after.get(close..).and_then(|t| t.find(')'))
        {
            flush(&mut plain, &mut spans);
            let words = after.get(..close).unwrap_or("");
            spans.push(Span::styled(
                words.to_string(),
                theme.text.add_modifier(Modifier::UNDERLINED),
            ));
            rest = after.get(close + end + 1..).unwrap_or("");
        } else if c == '|' {
            flush(&mut plain, &mut spans);
            spans.push(Span::styled("│", theme.faint));
            rest = after;
        } else {
            plain.push(c);
            rest = after;
        }
    }
    flush(&mut plain, &mut spans);
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(spans: &[Span]) -> Vec<String> {
        spans.iter().map(|s| s.content.to_string()).collect()
    }

    #[test]
    fn styles_markdown_rather_than_showing_it() {
        let theme = Theme::default();
        let spans = inline(
            "press `space`, **then** see [the song](06-song.md#x) | ok",
            &theme,
        );
        assert_eq!(
            words(&spans),
            [
                "press ", "space", ", ", "then", " see ", "the song", " ", "│", " ok"
            ]
        );
        // An unclosed mark is just a character.
        assert_eq!(words(&inline("a ` b", &theme)), ["a ` b"]);
    }
}
