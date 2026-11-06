//! Registers: named places to copy to and paste from, as in Helix. `"a` picks register `a` for
//! the next copy, paste or delete; without one, they use the default register, `"`. A register
//! keeps the last of each kind of thing copied into it: steps, a field's values, tracks and the
//! song's entries, so copying steps into it doesn't lose the tracks copied there before.

use std::collections::BTreeMap;

use lattice_core::Track;

use crate::fields::Value;
use crate::keymap::Key;
use crate::song::Copied;
use crate::{Clip, Editor, Infobox};

/// The register copies and pastes use when none's picked.
pub(crate) const DEFAULT: char = '"';

/// What one register holds.
#[derive(Debug, Clone, Default)]
pub(crate) struct Register {
    /// Each selection's steps, as `y` copies them.
    pub(crate) steps: Vec<Clip>,
    /// Each selection's values of a field.
    pub(crate) fields: Vec<Vec<Option<Value>>>,
    pub(crate) tracks: Vec<Track>,
    /// The song's entries.
    pub(crate) entries: Vec<Copied>,
    /// Keys recorded with `Q`.
    pub(crate) keys: Vec<Key>,
    /// Whether a field's values went in after the steps, so `p` on a field pastes them rather
    /// than the steps.
    pub(crate) values_last: bool,
}

pub(crate) type Registers = BTreeMap<char, Register>;

impl Register {
    /// What it holds, in a few words: `16 steps · 2 tracks: BASS, KICK`.
    fn describe(&self) -> String {
        let mut parts = vec![];
        let steps: usize = (self.steps.iter().flatten()).map(Vec::len).sum();
        if steps > 0 {
            parts.push(plural(steps, "step"));
        }
        let values: usize = self.fields.iter().map(Vec::len).sum();
        if values > 0 {
            parts.push(plural(values, "value"));
        }
        if !self.tracks.is_empty() {
            let names: Vec<String> = self.tracks.iter().map(|t| t.name.to_uppercase()).collect();
            parts.push(format!(
                "{}: {}",
                plural(self.tracks.len(), "track"),
                names.join(", ")
            ));
        }
        if !self.entries.is_empty() {
            parts.push(match self.entries.len() {
                1 => "1 entry".into(),
                n => format!("{n} entries"),
            });
        }
        if !self.keys.is_empty() {
            parts.push(format!(
                "keys {}",
                crate::replay::describe_keys(&self.keys, 24)
            ));
        }
        parts.join(" · ")
    }
}

fn plural(n: usize, what: &str) -> String {
    match n {
        1 => format!("1 {what}"),
        n => format!("{n} {what}s"),
    }
}

/// Whether `c` names a register: a letter, a digit, or `"` for the default.
pub(crate) fn names_register(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == DEFAULT
}

impl Editor {
    /// The register picked for the next copy or paste, if one has been: `a` after `"a`.
    pub fn picked_register(&self) -> Option<char> {
        self.picked_register
    }

    /// The register the next copy or paste uses.
    fn register_name(&self) -> char {
        self.picked_register.unwrap_or(DEFAULT)
    }

    /// What's in the register the next paste uses.
    pub(crate) fn register(&self) -> Option<&Register> {
        self.registers.get(&self.register_name())
    }

    /// Whether `p` on a field pastes a field's values: if they were copied after any steps.
    pub(crate) fn pastes_values(&self) -> bool {
        self.register()
            .is_none_or(|r| r.values_last || r.steps.iter().flatten().all(Vec::is_empty))
    }

    /// The register the next copy goes into.
    pub(crate) fn register_mut(&mut self) -> &mut Register {
        self.registers.entry(self.register_name()).or_default()
    }

    /// ` into "a`, to say where a copy went, or nothing for the default register.
    pub(crate) fn copied_into(&self) -> String {
        match self.picked_register {
            Some(c) if c != DEFAULT => format!(" into \"{c}"),
            _ => String::new(),
        }
    }

    /// Why there's nothing to paste of `what`: none copied, or none in the register picked.
    pub(crate) fn nothing_to_paste(&self, what: &str) -> String {
        match self.picked_register {
            Some(c) if c != DEFAULT => format!("no {what} in \"{c}"),
            _ => format!("no {what} copied"),
        }
    }

    /// The key after `"`: picks the register it names, or cancels.
    pub(crate) fn pick_register(&mut self, key: Key) {
        match key {
            Key::Char(c) if names_register(c) => self.picked_register = Some(c),
            Key::Esc => {
                self.picked_register = None;
                self.count = None;
            }
            _ => {
                self.picked_register = None;
                self.count = None;
                self.message = Some("a register is a letter or a digit".into());
            }
        }
    }

    /// The song's entries are copied by pattern, which a new song doesn't have.
    pub(crate) fn forget_entries(&mut self) {
        for register in self.registers.values_mut() {
            register.entries.clear();
        }
    }

    /// While `"` waits for a register: what each holds, the default first.
    pub(crate) fn registers_infobox(&self) -> Infobox {
        let default = (self.registers.get(&DEFAULT)).map(|r| (DEFAULT, r));
        let named = (self.registers.iter()).filter(|&(&c, _)| c != DEFAULT);
        let items = default
            .into_iter()
            .chain(named.map(|(&c, r)| (c, r)))
            .map(|(c, r)| (c.to_string(), r.describe()))
            .filter(|(_, held)| !held.is_empty())
            .chain([("a–z 0–9".to_string(), "pick a register".to_string())])
            .collect();
        Infobox {
            title: "registers".into(),
            items,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::keymap::Key;
    use crate::tests::{editor, keys, notes, pitches};

    #[test]
    fn copies_and_pastes_through_a_register_without_touching_the_default() {
        let mut e = editor();
        notes(&mut e, "zcb");
        keys(&mut e, "ggvjj\"ay");
        assert_eq!(e.message.as_deref(), Some("copied 3 steps into \"a"));
        assert_eq!(e.picked_register(), None, "picked for one action");
        keys(&mut e, "jjjp");
        assert_eq!(e.message.as_deref(), Some("no steps copied"));
        keys(&mut e, "\"ap");
        let p = pitches(&e, 0);
        assert_eq!(p[5..8], p[0..3]);
        keys(&mut e, "\"bp");
        assert_eq!(e.message.as_deref(), Some("no steps in \"b"));
    }

    #[test]
    fn a_register_keeps_each_kind_and_lists_them() {
        let mut e = editor();
        notes(&mut e, "z");
        keys(&mut e, "gg\"aty");
        keys(&mut e, "\"ay");
        keys(&mut e, "\"");
        let infobox = e.infobox().unwrap();
        assert_eq!(infobox.title, "registers");
        let a = infobox.items.iter().find(|(k, _)| k == "a").unwrap();
        assert!(a.1.starts_with("1 step · 1 track: "), "{}", a.1);
        e.handle(Key::Esc);
        assert_eq!(e.picked_register(), None);
        assert!(e.infobox().is_none());
        // The tracks copied into a are still there to paste.
        let tracks = e.grid.tracks.len();
        keys(&mut e, "\"atp");
        assert_eq!(e.grid.tracks.len(), tracks + 1);
    }

    #[test]
    fn a_field_pastes_whichever_was_copied_last() {
        let mut e = editor();
        notes(&mut e, "zc");
        // Steps copied, then pasted with the cursor on the velocity.
        keys(&mut e, "ggyljjp");
        assert_eq!(pitches(&e, 0)[2], Some(60));
        // Then a velocity copied: p in the field pastes it, not the steps.
        keys(&mut e, "ggy");
        assert_eq!(e.message.as_deref(), Some("copied 1 values"));
        keys(&mut e, "jjjjp");
        assert_eq!(pitches(&e, 0)[4], None, "no step pasted");
        // Back on the note to copy the step, then into the velocity to paste it.
        keys(&mut e, "gghyljjjjp");
        assert_eq!(pitches(&e, 0)[4], Some(60), "steps again once copied again");
    }

    #[test]
    fn a_count_can_come_either_side_of_the_register() {
        let mut e = editor();
        notes(&mut e, "z");
        e.auto_step = 0;
        keys(&mut e, "gg\"ay");
        keys(&mut e, "j2\"ap");
        keys(&mut e, ";j\"a2p");
        let p = pitches(&e, 0);
        assert_eq!(p[0..5], [p[0]; 5]);
    }
}
