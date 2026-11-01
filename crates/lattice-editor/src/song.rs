//! The song page: the arrangement, a list of entries each playing a pattern some number of times.
//! The cursor is on an entry; space plays the song from there.

use lattice_core::{Entry, MAX_ENTRIES, MAX_REPEATS};

use crate::{Action, Editor, Effect, Mode, Page};

/// Where an entry goes, relative to the cursor's.
#[derive(Clone, Copy)]
enum Place {
    After,
    Before,
}

impl Editor {
    /// The entry under the song page's cursor, kept inside the arrangement.
    pub fn song_cursor(&self) -> usize {
        self.song
            .cursor
            .min(self.arrangement.entries.len().saturating_sub(1))
    }

    /// Handles `action` on the song page, or `None` to handle it as on any page.
    pub(crate) fn song_act(&mut self, action: Action, n: usize) -> Option<Vec<Effect>> {
        self.song.cursor = self.song_cursor();
        let last = self.arrangement.entries.len().saturating_sub(1);
        let by = n as isize;
        match action {
            Action::Up => self.song.cursor = self.song.cursor.saturating_sub(n),
            Action::Down => self.song.cursor = (self.song.cursor + n).min(last),
            Action::FirstStep => self.song.cursor = 0,
            Action::LastStep => self.song.cursor = last,
            Action::Left => self.entry_pattern(-by),
            Action::Right => self.entry_pattern(by),
            Action::NoteUp | Action::Longer => self.repeat(by),
            Action::NoteDown | Action::Shorter => self.repeat(-by),
            Action::OpenBelow => self.add_entry(Place::After, None),
            Action::OpenAbove => self.add_entry(Place::Before, None),
            Action::Delete | Action::Clear => self.remove_entry(action == Action::Delete),
            Action::Yank => {
                if let Some(entry) = self.arrangement.entries.get(self.song.cursor) {
                    self.song.yanked = Some(*entry);
                    self.message = Some("copied the entry".into());
                }
            }
            Action::Paste => self.paste_entry(Place::After),
            Action::PasteInsert => self.paste_entry(Place::Before),
            Action::Open => {
                // Edit the entry's pattern.
                let Some(entry) = self.arrangement.entries.get(self.song.cursor) else {
                    return Some(vec![]);
                };
                let pattern = entry.pattern;
                self.page = Page::Pattern;
                return Some(self.go_to_pattern(pattern));
            }
            Action::TogglePlay => return Some(self.play_song()),
            Action::Undo
            | Action::Redo
            | Action::CommandLine
            | Action::PlaySong
            | Action::ToggleRecord
            | Action::Escape
            | Action::Quit
            | Action::NextPattern
            | Action::PrevPattern
            | Action::GotoMixer
            | Action::GotoPattern
            | Action::GotoSong
            | Action::GotoInstrument
            | Action::GotoEffects => return None,
            // Pattern editing has nothing to act on here.
            _ => {}
        }
        self.mode = Mode::Normal;
        Some(vec![])
    }

    /// Plays the song, or stops it: from the song page's cursor, or else from the entry there
    /// if it plays the pattern being edited, the first that does if not, or the top.
    pub(crate) fn play_song(&mut self) -> Vec<Effect> {
        if self.transport.is_some() {
            return vec![Effect::TogglePlay];
        }
        let entries = &self.arrangement.entries;
        let current = self.arrangement.current;
        let cursor = self.song_cursor();
        let entry = if self.page == Page::Song
            || entries.get(cursor).is_some_and(|e| e.pattern == current)
        {
            cursor
        } else {
            entries
                .iter()
                .position(|e| e.pattern == current)
                .unwrap_or(0)
        };
        self.song.cursor = entry;
        let mut effects = self.start_song(entry);
        effects.push(Effect::TogglePlay);
        effects
    }

    /// Plays another pattern in the cursor's entry: the next or previous, wrapping round.
    fn entry_pattern(&mut self, by: isize) {
        let patterns = self.arrangement.patterns.len() as isize;
        let Some(entry) = self.arrangement.entries.get(self.song.cursor) else {
            return;
        };
        let pattern = (entry.pattern as isize + by).rem_euclid(patterns) as usize;
        if pattern == entry.pattern {
            return;
        }
        self.checkpoint();
        self.arrangement.entries[self.song.cursor].pattern = pattern;
    }

    /// Plays the cursor's entry `by` more times, or fewer.
    fn repeat(&mut self, by: isize) {
        let Some(entry) = self.arrangement.entries.get(self.song.cursor) else {
            return;
        };
        let repeats = (entry.repeats as isize + by).clamp(1, MAX_REPEATS as isize) as u32;
        if repeats == entry.repeats {
            return;
        }
        self.checkpoint();
        self.arrangement.entries[self.song.cursor].repeats = repeats;
    }

    /// Adds `entry`, or one playing the cursor's pattern once (or the pattern being edited, in an
    /// empty song), and moves onto it.
    fn add_entry(&mut self, place: Place, entry: Option<Entry>) {
        let entries = &self.arrangement.entries;
        if entries.len() >= MAX_ENTRIES {
            self.message = Some(format!("songs have up to {MAX_ENTRIES} entries"));
            return;
        }
        let entry = entry.unwrap_or_else(|| Entry {
            pattern: entries
                .get(self.song.cursor)
                .map_or(self.arrangement.current, |e| e.pattern),
            repeats: 1,
        });
        let at = match place {
            Place::After if !entries.is_empty() => self.song.cursor + 1,
            _ => self.song.cursor,
        };
        self.checkpoint();
        self.arrangement.entries.insert(at, entry);
        self.song.cursor = at;
        // The song playing carries on from the same entry.
        if let Some((playing, _)) = &mut self.song_position
            && at <= *playing
        {
            *playing += 1;
        }
    }

    /// Removes the cursor's entry, copying it if `yank`.
    fn remove_entry(&mut self, yank: bool) {
        let at = self.song.cursor;
        if at >= self.arrangement.entries.len() {
            return;
        }
        self.checkpoint();
        let entry = self.arrangement.entries.remove(at);
        if yank {
            self.song.yanked = Some(entry);
        }
        if let Some((playing, _)) = &mut self.song_position
            && at < *playing
        {
            *playing -= 1;
        }
    }

    fn paste_entry(&mut self, place: Place) {
        match self.song.yanked {
            Some(entry) => self.add_entry(place, Some(entry)),
            None => self.message = Some("nothing copied".into()),
        }
    }
}

/// The song page's state, besides the arrangement itself.
#[derive(Debug, Clone, Default)]
pub(crate) struct SongCursor {
    /// The entry under the cursor.
    pub(crate) cursor: usize,
    /// The entry last copied.
    yanked: Option<Entry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;
    use lattice_core::{Grid, Step, Track};

    fn editor() -> Editor {
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![Track {
                name: "t".into(),
                steps: vec![Step::default(); 16],
                ..Track::default()
            }],
            groups: vec![],
        };
        let mut e = Editor::new(grid, 120.0);
        e.handle(Key::Char(':'));
        for c in "pattern-new".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter);
        // A new pattern plays once at the end of the song.
        assert_eq!(song(&e), [(0, 1), (1, 1)]);
        e.arrangement.entries.truncate(1);
        e.handle(Key::Char('g'));
        e.handle(Key::Char('s'));
        e
    }

    fn keys(e: &mut Editor, keys: &str) -> Vec<Effect> {
        keys.chars().flat_map(|c| e.handle(Key::Char(c))).collect()
    }

    fn song(e: &Editor) -> Vec<(usize, u32)> {
        e.arrangement
            .entries
            .iter()
            .map(|e| (e.pattern, e.repeats))
            .collect()
    }

    #[test]
    fn entries_are_added_changed_and_moved() {
        let mut e = editor();
        assert_eq!(e.page, Page::Song);
        assert_eq!(song(&e), [(0, 1)]);

        // Another of the same after it, playing the next pattern three times.
        keys(&mut e, "ol++");
        assert_eq!(song(&e), [(0, 1), (1, 3)]);
        assert_eq!(e.song_cursor(), 1);
        keys(&mut e, "-");
        assert_eq!(song(&e), [(0, 1), (1, 2)]);

        // Wrapping round the patterns; never fewer than one repeat.
        keys(&mut e, "l3-");
        assert_eq!(song(&e), [(0, 1), (0, 1)]);
        keys(&mut e, "h+k");

        // Delete and paste moves an entry.
        keys(&mut e, "dp");
        assert_eq!(song(&e), [(1, 2), (0, 1)]);
        keys(&mut e, "yggP");
        assert_eq!(song(&e), [(0, 1), (1, 2), (0, 1)]);

        // Undo takes back each change.
        keys(&mut e, "uuu");
        assert_eq!(song(&e), [(0, 1), (1, 2)]);
    }

    #[test]
    fn space_plays_the_song_from_the_cursor() {
        let mut e = editor();
        keys(&mut e, "ol");
        assert_eq!(e.arrangement.current, 1, "the new pattern's being edited");
        keys(&mut e, "k");
        let effects = e.handle(Key::Char(' '));
        assert_eq!(effects.last(), Some(&Effect::TogglePlay));
        assert_eq!(e.song_position, Some((0, 0)));
        assert_eq!(e.arrangement.current, 0, "the entry's pattern, at once");

        // Enter edits the entry's pattern.
        e.stopped();
        keys(&mut e, "j");
        e.handle(Key::Enter);
        assert_eq!((e.page, e.arrangement.current), (Page::Pattern, 1));

        // From the pattern page, ^space plays the song from the entry playing this pattern.
        e.handle(Key::Char('['));
        e.handle(Key::Char('p'));
        assert_eq!(e.arrangement.current, 0);
        let effects = e.handle(Key::Ctrl(' '));
        assert_eq!(effects.last(), Some(&Effect::TogglePlay));
        assert_eq!(e.song_position, Some((0, 0)));
    }
}
