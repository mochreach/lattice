use lattice_editor::Key;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, ModifierKeyCode,
};

/// Sees Right Alt tapped on its own, among the presses and releases of every key, which
/// terminals report kitty's way: AltGr, as it is on a UK keyboard, too. Also tells a held key's
/// repeats from presses where the terminal doesn't, as Windows' console doesn't.
#[derive(Default)]
pub struct Tap {
    /// Whether Right Alt is down with nothing pressed since.
    down: bool,
    /// Whether releases are reported, so a key pressed again without one is being held.
    releases: bool,
    /// The key last pressed, until a key's let go.
    last: Option<KeyCode>,
}

impl Tap {
    pub fn new(releases: bool) -> Self {
        Self {
            releases,
            ..Self::default()
        }
    }

    /// The key `event` is, and how: `Key::RightAlt` pressed once Right Alt's let go untouched.
    pub fn key(&mut self, event: KeyEvent) -> Option<(Key, KeyEventKind)> {
        let right_alt = matches!(
            event.code,
            KeyCode::Modifier(ModifierKeyCode::RightAlt | ModifierKeyCode::IsoLevel3Shift)
        );
        match event.kind {
            KeyEventKind::Press if right_alt => self.down = true,
            KeyEventKind::Release if right_alt && std::mem::take(&mut self.down) => {
                return Some((Key::RightAlt, KeyEventKind::Press));
            }
            // A key let go that was down before doesn't spoil the tap.
            KeyEventKind::Release => {}
            _ if !right_alt => self.down = false,
            _ => {}
        }
        // Windows repeats a held key as more presses. Only the last key pressed repeats, and
        // any key let go stops it, which also forgets a key that's changed case with Shift.
        let kind = match event.kind {
            KeyEventKind::Press if self.releases && self.last == Some(event.code) => {
                KeyEventKind::Repeat
            }
            KeyEventKind::Press => {
                self.last = Some(event.code);
                KeyEventKind::Press
            }
            KeyEventKind::Release => {
                self.last = None;
                KeyEventKind::Release
            }
            KeyEventKind::Repeat => KeyEventKind::Repeat,
        };
        convert(event).map(|key| (key, kind))
    }
}

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

    #[test]
    fn right_alt_tapped_alone_is_a_key_but_not_held_for_a_chord() {
        let event = |code, kind| KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind);
        let altgr = KeyCode::Modifier(ModifierKeyCode::IsoLevel3Shift);
        let mut tap = Tap::default();
        assert_eq!(tap.key(event(altgr, KeyEventKind::Press)), None);
        assert_eq!(tap.key(event(altgr, KeyEventKind::Repeat)), None);
        assert_eq!(
            tap.key(event(altgr, KeyEventKind::Release)),
            Some((Key::RightAlt, KeyEventKind::Press))
        );

        // Held for a chord, it's only the chord.
        let space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::ALT);
        let right_alt = KeyCode::Modifier(ModifierKeyCode::RightAlt);
        tap.key(event(right_alt, KeyEventKind::Press));
        assert_eq!(tap.key(space), Some((Key::Alt(' '), KeyEventKind::Press)));
        assert_eq!(tap.key(event(right_alt, KeyEventKind::Release)), None);

        // A key let go meanwhile, pressed before, doesn't spoil it.
        tap.key(event(right_alt, KeyEventKind::Press));
        tap.key(event(KeyCode::Char('q'), KeyEventKind::Release));
        assert!(tap.key(event(right_alt, KeyEventKind::Release)).is_some());
    }

    #[test]
    fn a_key_pressed_again_before_its_release_is_held() {
        let event = |c, kind| KeyEvent::new_with_kind(KeyCode::Char(c), KeyModifiers::NONE, kind);
        let mut tap = Tap::new(true);
        let kind = |tap: &mut Tap, c, kind| tap.key(event(c, kind)).map(|(_, kind)| kind);
        assert_eq!(
            kind(&mut tap, 'z', KeyEventKind::Press),
            Some(KeyEventKind::Press)
        );
        assert_eq!(
            kind(&mut tap, 'z', KeyEventKind::Press),
            Some(KeyEventKind::Repeat)
        );
        assert_eq!(
            kind(&mut tap, 'z', KeyEventKind::Release),
            Some(KeyEventKind::Release)
        );
        assert_eq!(
            kind(&mut tap, 'z', KeyEventKind::Press),
            Some(KeyEventKind::Press)
        );
        // Another key between them, or Shift let go first, breaks the hold.
        assert_eq!(
            kind(&mut tap, 'x', KeyEventKind::Press),
            Some(KeyEventKind::Press)
        );
        assert_eq!(
            kind(&mut tap, 'Z', KeyEventKind::Release),
            Some(KeyEventKind::Release)
        );
        assert_eq!(
            kind(&mut tap, 'x', KeyEventKind::Press),
            Some(KeyEventKind::Press)
        );

        // Without releases, every press is one.
        let mut tap = Tap::new(false);
        kind(&mut tap, 'z', KeyEventKind::Press);
        assert_eq!(
            kind(&mut tap, 'z', KeyEventKind::Press),
            Some(KeyEventKind::Press)
        );
    }
}
