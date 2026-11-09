//! A word of thanks after the opening, with where to find more: my site, videos, the Discord and
//! a way to support Lattice. Space or Esc closes it; `--no-splash` leaves it out with the opening.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Paragraph};

use lattice_editor::Key;

use crate::theme::Theme;

/// What each link is, and where.
pub const LINKS: [(&str, &str); 4] = [
    ("website", "https://mochreach.dev"),
    ("videos", "https://youtube.com/@mochreach"),
    ("discord", "https://discord.gg/NJbzRZav7w"),
    ("support", "https://ko-fi.com/mochreach"),
];

/// Whether a key closes it.
pub fn closes(key: Key) -> bool {
    matches!(key, Key::Char(' ') | Key::Esc)
}

/// Centred over the editor.
pub fn draw(frame: &mut Frame, theme: &Theme) {
    let area = frame.area();
    let accent = Style::new().fg(theme.accent).add_modifier(Modifier::BOLD);
    let label = LINKS.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    let mut lines = vec![
        Line::from("Thanks for using Lattice! I hope you make"),
        Line::from("something you love with it."),
        Line::default(),
    ];
    lines.extend(LINKS.iter().map(|(what, link)| {
        Line::from(vec![
            Span::styled(format!("{what:<label$}  "), theme.dim),
            Span::styled(*link, theme.text),
        ])
    }));
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("space", accent),
        Span::styled(" or ", theme.dim),
        Span::styled("esc", accent),
        Span::styled(" to close", theme.dim),
    ]));
    let width = (lines.iter().map(Line::width).max().unwrap_or(0) as u16 + 6).min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    let block = Block::bordered()
        .border_style(Style::new().fg(theme.accent))
        .padding(Padding::horizontal(2))
        .title(Span::styled(" WELCOME ", accent));
    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_and_esc_close_it_and_nothing_else_does() {
        assert!(closes(Key::Char(' ')) && closes(Key::Esc));
        assert!(!closes(Key::Char('j')) && !closes(Key::Enter));
    }
}
