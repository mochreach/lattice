//! Recorded keys, Vim's and Helix's macros: `Qa` records the keys pressed into register `a`
//! until `Q` again, and `qa` presses them again, `3qa` three times over. Everything goes in as
//! it was pressed, moves, modes, notes and whole command lines, so a replay can run commands as
//! well as edit. ("Macros" in Lattice are a track's knobs, so these are recorded keys.)
//!
//! A replay is one undo step, and stops at the first key that fails, as Vim's do: a command
//! that errors, a key that does nothing on the page, or a move that can't go any further. So
//! `100qa` on "do this, then move to the next track" stops at the last track.
//!
//! Keys can be recorded into an F key too, `QF4`, and then the F key alone presses them, so
//! what's done often is one key away. They're kept apart from the registers, as an F key only
//! ever holds keys.

use std::fmt;

use crate::keymap::{Action, Key};
use crate::registers::names_register;
use crate::{Editor, Effect, Infobox, Mode};

/// How deep replays can call replays, so keys that replay themselves can't go on for ever.
const MAX_DEPTH: usize = 8;

/// The recorded keys as text, `<name>` for keys that aren't a character, cut short at `width`.
pub(crate) fn describe_keys(keys: &[Key], width: usize) -> String {
    let text: String = (keys.iter())
        .map(|key| match key {
            Key::Char(c) if *c != ' ' => c.to_string(),
            key => format!("<{key}>"),
        })
        .collect();
    match text.chars().count() > width {
        true => text
            .chars()
            .take(width.saturating_sub(1))
            .chain(['…'])
            .collect(),
        false => text,
    }
}

/// Where keys are recorded: a register, or an F key.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Slot {
    Register(char),
    F(u8),
}

impl Slot {
    /// The one `key` names, after `Q` or `q`.
    fn named(key: Key) -> Option<Slot> {
        match key {
            Key::Char(c) if names_register(c) => Some(Slot::Register(c)),
            Key::F(n) => Some(Slot::F(n)),
            _ => None,
        }
    }
}

impl fmt::Display for Slot {
    /// `a`, or `F4`.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Slot::Register(c) => write!(f, "{c}"),
            Slot::F(n) => write!(f, "F{n}"),
        }
    }
}

impl Editor {
    /// The register or F key keys are being recorded into, if they are: `a`, `F4`.
    pub fn recording_keys(&self) -> Option<String> {
        self.recording.as_ref().map(|(slot, _)| slot.to_string())
    }

    /// The keys recorded into `slot`.
    fn keys_in(&self, slot: Slot) -> Vec<Key> {
        match slot {
            Slot::Register(c) => (self.registers.get(&c)).map_or(vec![], |r| r.keys.clone()),
            Slot::F(n) => self.fkeys.get(&n).cloned().unwrap_or_default(),
        }
    }

    /// Notes `key` as pressed, if recording; the keys a replay presses aren't.
    pub(crate) fn record_key(&mut self, key: Key) {
        if self.replaying == 0
            && let Some((_, keys)) = &mut self.recording
        {
            keys.push(key);
        }
    }

    /// `Q`: stops recording, keeping the keys in the register, or waits for the register to
    /// start recording into.
    pub(crate) fn record_keys(&mut self) {
        let Some((slot, mut keys)) = self.recording.take() else {
            self.pending = Some(crate::Pending::Record);
            return;
        };
        // The key that stopped it.
        keys.pop();
        let press = match slot {
            Slot::F(_) => format!(": {slot} presses them"),
            Slot::Register(_) => String::new(),
        };
        self.message = Some(match keys.len() {
            1 => format!("recorded 1 key into {slot}{press}"),
            n => format!("recorded {n} keys into {slot}{press}"),
        });
        match slot {
            Slot::Register(c) => self.registers.entry(c).or_default().keys = keys,
            // Nothing recorded leaves the F key to what it's bound to.
            Slot::F(n) if keys.is_empty() => {
                self.fkeys.remove(&n);
            }
            Slot::F(n) => {
                self.fkeys.insert(n, keys);
            }
        }
    }

    /// The key after `Q`: starts recording into the register or F key it names.
    pub(crate) fn start_recording(&mut self, key: Key) {
        match Slot::named(key) {
            Some(slot) => {
                self.recording = Some((slot, vec![]));
                self.message = Some(format!("recording keys into {slot}: Q stops"));
            }
            None if key == Key::Esc => {}
            None => self.message = Some("keys go into a letter, a digit or an F key".into()),
        }
    }

    /// The key after `q`, or a recorded F key: presses the keys recorded in the register or F
    /// key it names, `n` times.
    pub(crate) fn replay(&mut self, key: Key, n: usize) -> Vec<Effect> {
        let register = match Slot::named(key) {
            Some(slot) => slot,
            None if key == Key::Esc => return vec![],
            None => {
                self.message = Some("keys are in a letter, a digit or an F key".into());
                return vec![];
            }
        };
        let keys = self.keys_in(register);
        if keys.is_empty() {
            self.message = Some(format!("no keys in {register}: Q{register} records them"));
            self.key_failed = true;
            return vec![];
        }
        if self.replaying >= MAX_DEPTH {
            self.message = Some(format!("{register} replays itself without end"));
            self.key_failed = true;
            return vec![];
        }
        let undo = self.undo.len();
        self.replaying += 1;
        let mut effects = vec![];
        let mut done = 0;
        'replays: for _ in 0..n {
            for &key in &keys {
                self.key_failed = false;
                effects.extend(self.handle(key));
                // A note's heard as it goes in, not held.
                effects.extend(self.release(key));
                if self.key_failed {
                    break 'replays;
                }
            }
            done += 1;
        }
        self.replaying -= 1;
        // However many edits it made, one undo takes the replay back.
        self.undo.truncate(undo + 1);
        if self.key_failed {
            let why =
                (self.message.take()).unwrap_or_else(|| "a move went as far as it can".into());
            self.message = Some(format!("replay {} of {n} stopped: {why}", done + 1));
        }
        effects
    }

    /// A recorded F key in insert mode, playing: presses its keys as in normal mode, where
    /// they were recorded, so a letter doesn't play a note, then goes back to playing, unless
    /// they went somewhere else themselves.
    pub(crate) fn replay_while_playing(&mut self, key: Key) -> Vec<Effect> {
        self.mode = Mode::Normal;
        let effects = self.replay(key, 1);
        if self.mode == Mode::Normal {
            self.mode = Mode::Insert;
        }
        effects
    }

    /// While `Q` or `q` waits for a register: the keys each holds.
    pub(crate) fn keys_infobox(&self, replay: bool) -> Infobox {
        let held = (self.registers.iter())
            .filter(|(_, r)| replay && !r.keys.is_empty())
            .map(|(c, r)| (c.to_string(), describe_keys(&r.keys, 30)));
        // The F keys either way, as recording into one replaces what it presses.
        let pressed =
            (self.fkeys.iter()).map(|(n, keys)| (format!("F{n}"), describe_keys(keys, 30)));
        let pick = match replay {
            true => "replay the keys in a register",
            false => "record keys into a register",
        };
        let f = match replay {
            true => "replay, as the F key alone does",
            false => "record keys for the F key to press",
        };
        Infobox {
            title: if replay { "replay keys" } else { "record keys" }.into(),
            items: held
                .chain(pressed)
                .chain([
                    ("a–z 0–9".to_string(), pick.to_string()),
                    ("F1–F12".to_string(), f.to_string()),
                ])
                .collect(),
        }
    }

    /// Where every page's cursor is, down to the field on the pattern page, to tell a move
    /// that went nowhere.
    fn position(&self) -> String {
        format!(
            "{:?}",
            (
                self.page,
                &self.selections,
                self.primary,
                self.field,
                &self.open,
                self.mixer,
                self.fx,
                self.inst,
                self.song.bar,
                self.song.column,
                self.help,
                self.manual,
            )
        )
    }

    /// Does `action`, noting, while replaying, a move that can't go any further.
    pub(crate) fn act(&mut self, action: Action, n: usize) -> Vec<Effect> {
        let before = (self.replaying > 0 && action.moves()).then(|| self.position());
        let effects = self.act_now(action, n);
        if before.is_some_and(|before| before == self.position()) {
            self.key_failed = true;
        }
        effects
    }
}

#[cfg(test)]
mod tests {
    use crate::keymap::Key;
    use crate::tests::{editor, keys, notes, pitches};

    #[test]
    fn records_keys_and_replays_them_as_one_undo_step() {
        let mut e = editor();
        notes(&mut e, "zzzz");
        keys(&mut e, "ggQa>jQ");
        assert_eq!(e.message.as_deref(), Some("recorded 2 keys into a"));
        assert_eq!(e.recording_keys(), None);
        keys(&mut e, "2qa");
        assert_eq!(
            pitches(&e, 0)[..4],
            [Some(72), Some(72), Some(72), Some(60)]
        );
        keys(&mut e, "u");
        assert_eq!(
            pitches(&e, 0)[..4],
            [Some(72), Some(60), Some(60), Some(60)]
        );
    }

    #[test]
    fn a_replay_stops_where_a_move_can_go_no_further() {
        let mut e = editor();
        let tracks = e.grid.tracks.len();
        notes(&mut e, "z");
        e.auto_step = 0;
        // Copy the first step, then on each track paste it and move on.
        keys(&mut e, "ggyQaptlQ");
        keys(&mut e, "100qa");
        assert!(
            (0..tracks).all(|t| pitches(&e, t)[0] == Some(60)),
            "pasted on every track"
        );
        // Recording did the first track, and each replay one more.
        assert_eq!(
            e.message.as_deref(),
            Some(&*format!(
                "replay {} of 100 stopped: a move went as far as it can",
                tracks - 1
            ))
        );
    }

    #[test]
    fn a_move_between_fields_is_a_move() {
        let mut e = editor();
        // Into the first track's fields and back out onto the second.
        keys(&mut e, "Q1lQ");
        let field = e.field;
        keys(&mut e, "q1");
        assert_ne!(e.field, field);
        assert!(e.message.is_none(), "{:?}", e.message);
    }

    #[test]
    fn a_jump_already_made_doesnt_stop_a_replay() {
        let mut e = editor();
        notes(&mut e, "zzzz");
        keys(&mut e, "ggQa>ggQ");
        keys(&mut e, "3qa");
        assert_eq!(pitches(&e, 0)[0], Some(108), "raised four octaves in all");
    }

    #[test]
    fn an_f_key_presses_the_keys_recorded_into_it() {
        let mut e = editor();
        notes(&mut e, "zzzz");
        keys(&mut e, "ggQ");
        e.handle(Key::F(4));
        assert_eq!(e.recording_keys().as_deref(), Some("F4"));
        keys(&mut e, ">jQ");
        assert_eq!(
            e.message.as_deref(),
            Some("recorded 2 keys into F4: F4 presses them")
        );
        e.handle(Key::F(4));
        keys(&mut e, "2");
        e.handle(Key::F(4));
        assert_eq!(pitches(&e, 0)[..4], [Some(72); 4]);
        // As qF4 does, and an F key with nothing in it is left to its binding, here none.
        keys(&mut e, "ggq");
        e.handle(Key::F(4));
        assert_eq!(pitches(&e, 0)[0], Some(84));
        e.message = None;
        e.handle(Key::F(5));
        assert_eq!(e.message, None);
    }

    #[test]
    fn an_f_key_presses_its_keys_while_playing_as_in_normal_mode() {
        let mut e = editor();
        keys(&mut e, "Q");
        e.handle(Key::F(2));
        keys(&mut e, ":bpm 90");
        e.handle(Key::Enter);
        keys(&mut e, "Q:bpm 120");
        e.handle(Key::Enter);
        keys(&mut e, "gii");
        assert_eq!(e.mode, crate::Mode::Insert);
        e.handle(Key::F(2));
        assert_eq!(
            e.bpm, 90.0,
            "the command ran, rather than its letters playing"
        );
        assert_eq!(e.mode, crate::Mode::Insert, "and it's playing again");
    }

    #[test]
    fn records_command_lines_and_replays_them() {
        let mut e = editor();
        keys(&mut e, "Qb:bpm 90");
        e.handle(Key::Enter);
        keys(&mut e, "Q:bpm 120");
        e.handle(Key::Enter);
        keys(&mut e, "qb");
        assert_eq!(e.bpm, 90.0);
        keys(&mut e, "qc");
        assert_eq!(e.message.as_deref(), Some("no keys in c: Qc records them"));
    }

    #[test]
    fn keys_that_replay_themselves_stop() {
        let mut e = editor();
        keys(&mut e, "QajQ");
        // Now make a press j then replay a, for ever.
        keys(&mut e, "QajqaQ");
        keys(&mut e, "qa");
        assert!(e.message.as_deref().unwrap().contains("without end"));
    }
}
