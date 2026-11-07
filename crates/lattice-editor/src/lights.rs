//! What a keyboard that lights its keys one by one shows: the notes under the fingers while
//! they play, laid out as `:layout` lays them, and the keys a pending key waits for. The
//! frontend adds the transport, and turns each glow into a colour.

use crate::keymap::Key;
use crate::{Editor, Mode, Pending};

/// What a lit key means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Glow {
    /// Plays the scale's root, `:scale`'s, as the pads do.
    Root,
    /// Plays a note of the scale.
    InScale,
    /// Plays a note outside it, like a piano's black keys in C major.
    OutOfScale,
    /// A pad of the folded group.
    Pad,
    /// Does something now, as the key waited for after `g`, `f` or `"`, say.
    Choice,
    /// `esc`, in insert mode, which it leaves.
    Insert,
    /// `esc`, in select mode.
    Select,
    /// `esc`, on the command line.
    Command,
}

impl Editor {
    /// The keys to light, and what each means. Keys not given are left as they were.
    pub fn lights(&self) -> Vec<(Key, Glow)> {
        let mut lit = vec![];
        match &self.mode {
            Mode::Command(_) => lit.push((Key::Esc, Glow::Command)),
            Mode::Select => lit.push((Key::Esc, Glow::Select)),
            Mode::Insert => lit.push((Key::Esc, Glow::Insert)),
            Mode::Normal => {}
        }
        match self.pending {
            Some(Pending::Replace) => lit.extend(self.notes()),
            Some(_) => {
                let choices = self.infobox().map(|i| i.items).unwrap_or_default();
                lit.extend(
                    (choices.iter())
                        .filter_map(|(key, _)| Key::parse(key))
                        .map(|key| (key, Glow::Choice)),
                );
            }
            None if self.mode == Mode::Insert => {
                lit.extend(self.notes());
                if self.in_folded_group() {
                    // The digits play the group's tracks rather than notes.
                    lit.retain(|(key, _)| !matches!(key, Key::Char('1'..='9')));
                    lit.extend(('1'..='9').map(|c| (Key::Char(c), Glow::Pad)));
                }
            }
            None => {}
        }
        lit
    }

    /// The keys that play notes now, each by where its note falls in the scale.
    fn notes(&self) -> Vec<(Key, Glow)> {
        let scale = self.scale();
        let mut notes: Vec<(Key, Glow)> = (self.keymap.piano.keys())
            .map(|&c| Key::Char(c))
            // A key bound in insert mode does that rather than play.
            .filter(|key| !self.keymap.insert.contains_key(key))
            .filter_map(|key| {
                let pitch = self.piano_pitch(key)?;
                let degree = (pitch + 12 - self.root % 12) % 12;
                let glow = match degree {
                    0 => Glow::Root,
                    d if scale.steps.contains(&d) => Glow::InScale,
                    _ => Glow::OutOfScale,
                };
                Some((key, glow))
            })
            .collect();
        // In the order of the keys, so the same notes always come out the same.
        notes.sort_by_key(|(key, _)| key.to_string());
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{editor, keys};

    fn glow(lit: &[(Key, Glow)], c: char) -> Option<Glow> {
        lit.iter()
            .find(|(k, _)| *k == Key::Char(c))
            .map(|(_, g)| *g)
    }

    #[test]
    fn insert_mode_lights_the_notes_by_the_scale() {
        let mut e = editor();
        assert!(e.lights().is_empty(), "nothing in normal mode");
        keys(&mut e, "i");
        let lit = e.lights();
        assert!(lit.contains(&(Key::Esc, Glow::Insert)));
        // C major on the piano: z is C, s C#, x D.
        assert_eq!(glow(&lit, 'z'), Some(Glow::Root));
        assert_eq!(glow(&lit, 's'), Some(Glow::OutOfScale));
        assert_eq!(glow(&lit, 'x'), Some(Glow::InScale));
        assert_eq!(glow(&lit, 'a'), None, "not a note");
        // In A minor, A is the root and C# is out.
        e.handle(Key::Esc);
        e.run_command("scale minor a");
        keys(&mut e, "i");
        let lit = e.lights();
        assert_eq!(glow(&lit, 'n'), Some(Glow::Root));
        assert_eq!(glow(&lit, 'z'), Some(Glow::InScale));
    }

    #[test]
    fn a_pending_key_lights_the_keys_it_waits_for() {
        let mut e = editor();
        keys(&mut e, "g");
        let lit = e.lights();
        for c in ['g', 'e', 'm'] {
            assert_eq!(glow(&lit, c), Some(Glow::Choice), "{c}");
        }
        e.handle(Key::Esc);
        keys(&mut e, "r");
        assert_eq!(
            glow(&e.lights(), 'z'),
            Some(Glow::Root),
            "r waits for a note"
        );
    }
}
