//! What a keyboard that lights its keys one by one shows: every key that does something, by
//! the kind of thing it does; the notes under the fingers while they play, laid out as
//! `:layout` lays them, and the ones sounding; the keys a pending key waits for; the beat; and
//! the F keys holding keys. The frontend adds the transport, and turns each glow into a colour.

use lattice_core::realise;

use crate::keymap::{Action, Binding, Key};
use crate::{Editor, Mode, Page, Pending};

/// The main block of keys, which Lattice darkens where it lights nothing, so what it lights
/// stands out from whatever lights them otherwise.
const MAIN: &str = "`1234567890-=qwertyuiop[]asdfghjkl;'#\\zxcvbnm,./ ";

/// What a lit key means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Glow {
    /// Plays the scale's root, `:scale`'s, as the pads do.
    Root,
    /// Plays a note of the scale.
    InScale,
    /// Plays a note outside it, like a piano's black keys in C major.
    OutOfScale,
    /// Plays the note sounding now on the cursor's track.
    Sounding,
    /// A pad of the folded group.
    Pad,
    /// Does something now, as the key waited for after `g`, `f` or `"`, say.
    Choice,
    /// Bound to an action of this kind.
    Does(Kind),
    /// Opens a menu of more keys, as `g` does.
    Menu,
    /// A digit, starting a count.
    Count,
    /// The beat the song's on, along the number row.
    Beat,
    /// Another beat of the bar.
    Bar,
    /// An F key holding recorded keys.
    Recorded,
    /// The F key keys are being recorded into.
    Recording,
    /// `esc`, in insert mode, which it leaves.
    Insert,
    /// `esc`, in select mode.
    Select,
    /// `esc`, on the command line.
    Command,
    /// Does nothing now.
    Dark,
}

/// The kinds of action, as the manual lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Move,
    Page,
    Select,
    Edit,
    Mode,
    Track,
    Play,
}

/// Where the song's playing, for the lights that follow it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playhead {
    /// The step from the top of the pattern.
    pub step: u64,
    /// The step each track is on, and its pass, if it's playing.
    pub tracks: Vec<Option<(usize, u32)>>,
}

impl Editor {
    /// The keys to light, and what each means, with `playhead` while the song plays. Keys not
    /// given are left as they were. What matters most about a key comes first: a shifted key or
    /// one with Ctrl, which lights the same key, after it on its own.
    pub fn lights(&self, playhead: Option<&Playhead>) -> Vec<(Key, Glow)> {
        let mut lit = vec![];
        match &self.mode {
            Mode::Command(_) => lit.push((Key::Esc, Glow::Command)),
            Mode::Select => lit.push((Key::Esc, Glow::Select)),
            Mode::Insert => lit.push((Key::Esc, Glow::Insert)),
            Mode::Normal => {}
        }
        if let Some(n) = (self.recording.as_ref()).and_then(|(slot, _)| slot.f()) {
            lit.push((Key::F(n), Glow::Recording));
        }
        lit.extend(self.fkeys.keys().map(|&n| (Key::F(n), Glow::Recorded)));
        // Typing on the command line, the keys are letters.
        if matches!(self.mode, Mode::Command(_)) {
            return dedup(lit);
        }
        let playing_notes = matches!(self.pending, Some(Pending::Replace))
            || (self.pending.is_none() && self.mode == Mode::Insert);
        if playing_notes {
            lit.extend(self.sounding(playhead));
            if self.mode == Mode::Insert && self.in_folded_group() {
                // The digits play the group's tracks rather than notes.
                lit.extend(('1'..='9').map(|c| (Key::Char(c), Glow::Pad)));
            }
            lit.extend(self.notes());
        } else if self.pending.is_some() {
            let choices = self.infobox().map(|i| i.items).unwrap_or_default();
            lit.extend(
                (choices.iter())
                    .filter_map(|(key, _)| Key::parse(key))
                    .map(|key| (key, Glow::Choice)),
            );
        }
        if self.pending.is_none() {
            lit.extend(self.beat_lights(playhead));
            if self.mode == Mode::Insert {
                let bound = self.keymap.insert.iter();
                lit.extend(by_key(
                    bound.map(|(&key, &action)| (key, Glow::Does(kind(action)))),
                ));
            } else {
                let bound = self.keymap.normal.iter().map(|(&key, binding)| {
                    let glow = match binding {
                        Binding::Action(action) => Glow::Does(kind(*action)),
                        Binding::Command(_) => Glow::Does(Kind::Edit),
                        Binding::Menu(_) => Glow::Menu,
                    };
                    (key, glow)
                });
                lit.extend(by_key(bound));
                lit.extend(('0'..='9').map(|c| (Key::Char(c), Glow::Count)));
            }
        }
        lit.extend(MAIN.chars().map(|c| (Key::Char(c), Glow::Dark)));
        dedup(lit)
    }

    /// The keys that play notes now, each by where its note falls in the scale.
    fn notes(&self) -> Vec<(Key, Glow)> {
        let scale = self.scale();
        let notes = self.note_keys().map(|(key, pitch)| {
            let degree = (pitch + 12 - self.root % 12) % 12;
            let glow = match degree {
                0 => Glow::Root,
                d if scale.steps.contains(&d) => Glow::InScale,
                _ => Glow::OutOfScale,
            };
            (key, glow)
        });
        by_key(notes)
    }

    /// The keys that play notes, and their notes.
    fn note_keys(&self) -> impl Iterator<Item = (Key, u8)> + '_ {
        (self.keymap.piano.keys())
            .map(|&c| Key::Char(c))
            // A key bound in insert mode does that rather than play.
            .filter(|key| !self.keymap.insert.contains_key(key))
            .filter_map(|key| Some((key, self.piano_pitch(key)?)))
    }

    /// The keys that play the note sounding on the cursor's track, as it's playing this pass.
    fn sounding(&self, playhead: Option<&Playhead>) -> Vec<(Key, Glow)> {
        let track = match self.page {
            Page::Instrument => self.inst_cursor().track,
            _ if self.in_conductor() => return vec![],
            _ => self.cursor().track,
        };
        let (Some((step, pass)), Some(track)) = (
            playhead.and_then(|p| p.tracks.get(track).copied().flatten()),
            self.grid.tracks.get(track),
        ) else {
            return vec![];
        };
        let steps = realise(track, pass);
        // The last note started, at or before the step, if it's still held.
        let len = steps.len();
        let pitch = (0..len).find_map(|back| {
            let at = (step % len + len - back) % len;
            let note = steps.get(at)?.note?;
            Some((usize::from(note.length) > back).then_some(note.pitch))
        });
        let Some(Some(pitch)) = pitch else {
            return vec![];
        };
        let keys = self.note_keys().filter(|&(_, p)| p == pitch);
        by_key(keys.map(|(key, _)| (key, Glow::Sounding)))
    }

    /// The beats of the bar along the number row, the one playing lit, while the song plays.
    fn beat_lights(&self, playhead: Option<&Playhead>) -> Vec<(Key, Glow)> {
        let Some(playhead) = playhead else {
            return vec![];
        };
        let per_beat = u64::from(self.grid.steps_per_beat.max(1));
        let beats = u64::from(self.grid.beats_per_bar.max(1));
        let beat = playhead.step / per_beat % beats;
        // As many beats as there are keys for; a bar of more shows only its first twelve.
        ("1234567890-=".chars().zip(0..beats))
            .map(|(c, b)| {
                let glow = if b == beat { Glow::Beat } else { Glow::Bar };
                (Key::Char(c), glow)
            })
            .collect()
    }
}

/// In the order of the keys, so the same keys always come out the same, keys without Ctrl or
/// Alt first, and unshifted before shifted, as what a key does on its own matters most.
fn by_key(keys: impl Iterator<Item = (Key, Glow)>) -> Vec<(Key, Glow)> {
    let mut keys: Vec<(Key, Glow)> = keys.collect();
    let order = |key: &Key| match key {
        Key::Char(c) if c.is_ascii_uppercase() => 1,
        Key::Char(_) => 0,
        Key::Ctrl(_) | Key::Alt(_) => 2,
        _ => 0,
    };
    keys.sort_by_key(|(key, _)| (order(key), key.to_string()));
    keys
}

/// Each key once, as it first comes.
fn dedup(lit: Vec<(Key, Glow)>) -> Vec<(Key, Glow)> {
    let mut seen = std::collections::HashSet::new();
    lit.into_iter()
        .filter(|(key, _)| seen.insert(*key))
        .collect()
}

/// What kind of thing `action` does, as the manual's list of actions sorts them.
fn kind(action: Action) -> Kind {
    use Action::*;
    match action {
        Left | Right | Up | Down | HalfPageUp | HalfPageDown | PageUp | PageDown | FirstStep
        | LastStep | FirstTrack | LastTrack | NextBeat | PrevBeat | NextBar | PrevBar
        | NextPattern | PrevPattern | NextNote | PrevNote | PrevTrack | NextTrack | GotoField
        | MatchEnd => Kind::Move,
        GotoMixer | GotoPattern | GotoSong | GotoInstrument | GotoEffects | ToggleView
        | ToggleOpen | ToggleFold => Kind::Page,
        SelectRows | SelectAll | SelectTracks | Collapse | Flip | SelectMode | CopyDown
        | CopyUp | SplitRows | MergeAll | KeepPrimary | RemovePrimary | RotateForward
        | RotateBackward | SelectNotes | KeepNotes | RemoveNotes | SelectInside | SelectAround => {
            Kind::Select
        }
        ReverseSteps | RepeatSteps | RotateStepsForward | RotateStepsBackward | Delete | Clear
        | ClearAdvance | Yank | Paste | PasteInsert | Replace | SelectRegister | RecordKeys
        | ReplayKeys | Undo | Redo | NoteUp | NoteDown | NoteOctaveUp | NoteOctaveDown | Longer
        | Shorter | LockSetting | Fill | SurroundAdd | SurroundReplace | SurroundDelete => {
            Kind::Edit
        }
        Insert | Append | OpenBelow | OpenAbove | Open | Change | CommandLine | Escape
        | KeyboardOctaveUp | KeyboardOctaveDown | Quit => Kind::Mode,
        ToggleMute | TogglePatternMute | ToggleSolo | AddTrack | RemoveTrack | YankTracks
        | PasteTracksAfter | PasteTracksBefore | MoveTrackLeft | MoveTrackRight | ToggleGroup => {
            Kind::Track
        }
        TogglePlay | PlaySong | PlayFromHere | Hush | PlayFill | ToggleRecord | LoopPedal
        | LoopSelection => Kind::Play,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{editor, keys, notes};

    fn glow(lit: &[(Key, Glow)], c: char) -> Option<Glow> {
        lit.iter()
            .find(|(k, _)| *k == Key::Char(c))
            .map(|(_, g)| *g)
    }

    #[test]
    fn normal_mode_lights_the_keys_that_do_something_by_kind() {
        let e = editor();
        let lit = e.lights(None);
        assert_eq!(glow(&lit, 'j'), Some(Glow::Does(Kind::Move)));
        assert_eq!(glow(&lit, 'd'), Some(Glow::Does(Kind::Edit)));
        assert_eq!(glow(&lit, 'i'), Some(Glow::Does(Kind::Mode)));
        assert_eq!(glow(&lit, ' '), Some(Glow::Does(Kind::Play)));
        assert_eq!(glow(&lit, 'g'), Some(Glow::Menu));
        assert_eq!(glow(&lit, '5'), Some(Glow::Count));
        // u undoes; U, on the same key, redoes, and the key on its own wins.
        assert_eq!(glow(&lit, 'u'), Some(Glow::Does(Kind::Edit)));
        let at = |key| lit.iter().position(|(k, _)| *k == key);
        assert!(
            at(Key::Char('u')) < at(Key::Char('U')),
            "first, as what's lit first wins"
        );
        assert_eq!(glow(&lit, '#'), Some(Glow::Dark));
    }

    #[test]
    fn insert_mode_lights_the_notes_by_the_scale() {
        let mut e = editor();
        keys(&mut e, "i");
        let lit = e.lights(None);
        assert!(lit.contains(&(Key::Esc, Glow::Insert)));
        // C major on the piano: z is C, s C#, x D.
        assert_eq!(glow(&lit, 'z'), Some(Glow::Root));
        assert_eq!(glow(&lit, 's'), Some(Glow::OutOfScale));
        assert_eq!(glow(&lit, 'x'), Some(Glow::InScale));
        assert_eq!(glow(&lit, 'a'), Some(Glow::Dark), "not a note");
        // In A minor, A is the root and C# is out.
        e.handle(Key::Esc);
        e.run_command("scale minor a");
        keys(&mut e, "i");
        let lit = e.lights(None);
        assert_eq!(glow(&lit, 'n'), Some(Glow::Root));
        assert_eq!(glow(&lit, 'z'), Some(Glow::InScale));
    }

    #[test]
    fn a_pending_key_lights_only_the_keys_it_waits_for() {
        let mut e = editor();
        keys(&mut e, "g");
        let lit = e.lights(None);
        for c in ['g', 'e', 'm'] {
            assert_eq!(glow(&lit, c), Some(Glow::Choice), "{c}");
        }
        assert_eq!(glow(&lit, 'j'), Some(Glow::Dark), "the rest go dark");
        e.handle(Key::Esc);
        keys(&mut e, "r");
        assert_eq!(
            glow(&e.lights(None), 'z'),
            Some(Glow::Root),
            "r waits for a note"
        );
    }

    #[test]
    fn playing_it_lights_the_beat_and_the_note_sounding() {
        let mut e = editor();
        // C on 00, two steps long, then D on 02.
        notes(&mut e, "zx");
        e.grid.tracks[0].steps[0].note.as_mut().unwrap().length = 2;
        e.grid.tracks[0].steps[1] = Default::default();
        e.grid.tracks[0].steps[2].note = Some(lattice_core::Note {
            pitch: 62,
            velocity: 100,
            length: 1,
        });
        let at = |step| Playhead {
            step: step as u64,
            tracks: vec![Some((step, 0))],
        };
        let lit = e.lights(Some(&at(5)));
        assert_eq!(
            glow(&lit, '2'),
            Some(Glow::Beat),
            "step 05 is in the second beat"
        );
        assert_eq!(glow(&lit, '1'), Some(Glow::Bar));
        assert_eq!(glow(&lit, '5'), Some(Glow::Count), "a bar of four beats");
        keys(&mut e, "i");
        assert_eq!(
            glow(&e.lights(Some(&at(1))), 'z'),
            Some(Glow::Sounding),
            "still held"
        );
        assert_eq!(glow(&e.lights(Some(&at(2))), 'z'), Some(Glow::Root));
        assert_eq!(glow(&e.lights(Some(&at(2))), 'x'), Some(Glow::Sounding));
        assert_eq!(
            glow(&e.lights(Some(&at(3))), 'x'),
            Some(Glow::InScale),
            "and let go"
        );
    }

    #[test]
    fn f_keys_holding_keys_light() {
        let mut e = editor();
        e.fkeys.insert(3, vec![Key::Char('j')]);
        assert!(e.lights(None).contains(&(Key::F(3), Glow::Recorded)));
        assert!(!e.lights(None).iter().any(|(k, _)| *k == Key::F(4)));
    }
}
