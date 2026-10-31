//! The keys just pressed, shown above the status line for screencasts (`:keys` or `--show-keys`).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use lattice_editor::Key;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Paragraph};

use crate::theme::Theme;

/// How long the keys stay up after the last one.
const LINGER: Duration = Duration::from_millis(2000);
/// Most keys kept; older ones scroll off the left.
const KEPT: usize = 16;

#[derive(Debug, PartialEq)]
struct Entry {
    /// The key's Helix name, or the text typed on the command line.
    text: String,
    /// Presses in a row, shown as `j×4`.
    count: usize,
    /// Typed on the command line, so shown as text rather than as keys.
    typed: bool,
}

#[derive(Debug, Default)]
pub struct KeyLog {
    entries: VecDeque<Entry>,
    last: Option<Instant>,
}

impl KeyLog {
    /// Notes a key, `typing` if it left the editor on the command line, where characters run
    /// together into the text typed, `:keys` rather than `: k e y s`.
    pub fn record(&mut self, key: Key, typing: bool, now: Instant) {
        if !self.showing(now) {
            self.entries.clear();
        }
        self.last = Some(now);
        let last = self.entries.back_mut();
        match (key, last) {
            (Key::Char(c), Some(last)) if typing && last.typed => last.text.push(c),
            (Key::Char(c), _) if typing => self.push(c.to_string(), true),
            (key, Some(last)) if !last.typed && last.text == key.to_string() => last.count += 1,
            (key, _) => self.push(key.to_string(), false),
        }
    }

    fn push(&mut self, text: String, typed: bool) {
        if self.entries.len() == KEPT {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            text,
            count: 1,
            typed,
        });
    }

    fn showing(&self, now: Instant) -> bool {
        self.last
            .is_some_and(|last| now.duration_since(last) < LINGER)
    }

    /// The keys, newest last, as many as fit in `width`.
    fn line(&self, width: usize, theme: &Theme) -> Line<'static> {
        let key = Style::new().fg(theme.accent).add_modifier(Modifier::BOLD);
        let mut spans = vec![];
        let mut used = 0;
        for entry in self.entries.iter().rev() {
            let text = match entry.count {
                1 => entry.text.clone(),
                n => format!("{}×{n}", entry.text),
            };
            let gap = if spans.is_empty() { 0 } else { 2 };
            let len = text.chars().count();
            if used + gap + len > width {
                break;
            }
            used += gap + len;
            if gap > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                text,
                if entry.typed { theme.text } else { key },
            ));
        }
        spans.reverse();
        Line::from(spans)
    }

    /// Draws the keys in a box at the bottom middle of `area`, while they're recent.
    pub fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.showing(Instant::now()) || area.width < 8 || area.height < 3 {
            return;
        }
        let line = self.line(area.width as usize - 6, theme);
        let width = line.width() as u16 + 6;
        let rect = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.bottom() - 3,
            width,
            height: 3,
        };
        frame.render_widget(Clear, rect);
        frame.render_widget(
            Paragraph::new(line).block(
                Block::bordered()
                    .border_style(theme.dim)
                    .padding(Padding::horizontal(2)),
            ),
            rect,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(log: &KeyLog) -> String {
        let theme = Theme::terminal();
        log.line(80, &theme)
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect()
    }

    #[test]
    fn keys_repeat_typing_runs_together_and_they_go_after_a_pause() {
        let mut log = KeyLog::default();
        let now = Instant::now();
        for key in [
            Key::Char('j'),
            Key::Char('j'),
            Key::Char('j'),
            Key::Ctrl('a'),
        ] {
            log.record(key, false, now);
        }
        log.record(Key::Char(':'), true, now);
        for c in "keys".chars() {
            log.record(Key::Char(c), true, now);
        }
        log.record(Key::Enter, false, now);
        assert_eq!(texts(&log), "j×3  C-a  :keys  ret");
        assert!(log.showing(now + LINGER / 2));
        assert!(!log.showing(now + LINGER));

        // After a pause, it starts afresh.
        log.record(Key::Char('g'), false, now + LINGER);
        assert_eq!(texts(&log), "g");
    }

    #[test]
    fn the_oldest_keys_give_way() {
        let mut log = KeyLog::default();
        let now = Instant::now();
        for c in "abcdefghijklmnopqrstuvwxyz".chars() {
            log.record(Key::Char(c), false, now);
        }
        assert_eq!(log.entries.len(), KEPT);
        let theme = Theme::terminal();
        let line: String = log
            .line(7, &theme)
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(line, "x  y  z");
    }
}
