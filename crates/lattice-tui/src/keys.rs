use lattice_editor::Key;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

/// Converts a crossterm key event into the editor's own key type.
pub fn convert(event: KeyEvent) -> Option<Key> {
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    let alt = event.modifiers.contains(KeyModifiers::ALT);
    let keypad = event.state.contains(KeyEventState::KEYPAD);
    Some(match event.code {
        KeyCode::Char(c @ '0'..='9') if keypad => Key::Pad(c as u8 - b'0'),
        KeyCode::Char(c @ ('.' | '/' | '*' | '-' | '+')) if keypad => Key::PadOp(c),
        KeyCode::Char(c) if ctrl => Key::Ctrl(c.to_ascii_lowercase()),
        KeyCode::Char(c) if alt => Key::Alt(c),
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Esc => Key::Esc,
        KeyCode::Enter => Key::Enter,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Tab => Key::Tab,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::F(n) => Key::F(n),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_numpad_is_told_apart_when_the_terminal_says_so() {
        let digit = KeyEvent::new(KeyCode::Char('7'), KeyModifiers::NONE);
        assert_eq!(convert(digit), Some(Key::Char('7')));
        let pad = KeyEvent::new_with_kind_and_state(
            KeyCode::Char('7'),
            KeyModifiers::NONE,
            ratatui::crossterm::event::KeyEventKind::Press,
            KeyEventState::KEYPAD,
        );
        assert_eq!(convert(pad), Some(Key::Pad(7)));
    }
}
