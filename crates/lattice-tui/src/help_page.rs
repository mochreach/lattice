//! Drawing the help page: every key and command, in two columns when there's room, flowing from
//! the left column's foot to the right's head.

use lattice_editor::{Editor, HelpLine};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::theme::Theme;
use crate::ui::fit;

/// Width of the keys, before what they do.
const KEYS: usize = 18;
/// Narrowest a column gets before there's only one.
const COLUMN: u16 = 76;

pub fn draw(frame: &mut Frame, area: Rect, editor: &Editor, theme: &Theme) {
    let columns = if area.width >= 2 * COLUMN { 2 } else { 1 };
    let width = area.width / columns as u16;
    let height = usize::from(area.height.saturating_sub(1)).max(1);
    // Each line wrapped to the column, noting the row each starts on, as the scroll counts lines.
    let mut rows = vec![];
    let mut starts = vec![];
    for line in editor.help_lines() {
        starts.push(rows.len());
        rows.extend(help_rows(&line, usize::from(width), theme));
    }
    // Scrolled no further than shows the last row at the foot.
    let top = starts
        .get(editor.help.scroll)
        .copied()
        .unwrap_or(0)
        .min(rows.len().saturating_sub(height * columns));
    for column in 0..columns {
        let from = top + column * height;
        let shown: Vec<Line> = rows.iter().skip(from).take(height).cloned().collect();
        let rect = Rect {
            x: area.x + width * column as u16,
            y: area.y + 1,
            width,
            height: area.height.saturating_sub(1),
        };
        frame.render_widget(Paragraph::new(shown), rect);
    }
}

/// A line of help as rows `width` wide, what an item does wrapped under itself.
fn help_rows(line: &HelpLine, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    match line {
        HelpLine::Heading(title) => vec![Line::from(Span::styled(
            format!("  {}", title.to_uppercase()),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))],
        HelpLine::Item { keys, args, what } => {
            let indent = KEYS + 3;
            let room = width.saturating_sub(indent + 1).max(10);
            // The arguments lead, faint, and the words follow on.
            let args = if args.is_empty() {
                String::new()
            } else {
                format!("{args} ")
            };
            let text = format!("{args}{what}");
            let mut rows = vec![];
            for (i, row) in wrap(&text, room).into_iter().enumerate() {
                let lead = if i == 0 {
                    Span::styled(
                        format!("  {} ", fit(keys, KEYS)),
                        theme.text.add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::raw(" ".repeat(indent))
                };
                // Only the first row can hold the arguments; split it where they end.
                let faint = if i == 0 { args.len().min(row.len()) } else { 0 };
                rows.push(Line::from(vec![
                    lead,
                    Span::styled(row[..faint].to_string(), theme.dim),
                    Span::styled(row[faint..].to_string(), theme.text),
                ]));
            }
            rows
        }
        HelpLine::Blank => vec![Line::default()],
    }
}

/// `text` in rows of at most `width` characters, broken between words where it can be.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut rows = vec![];
    let mut row = String::new();
    for word in text.split(' ') {
        let len = row.chars().count();
        if len > 0 && len + 1 + word.chars().count() > width {
            rows.push(std::mem::take(&mut row));
        }
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(word);
    }
    rows.push(row);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_between_words() {
        assert_eq!(wrap("one two three four", 9), ["one two", "three", "four"]);
        assert_eq!(wrap("short", 20), ["short"]);
    }
}
