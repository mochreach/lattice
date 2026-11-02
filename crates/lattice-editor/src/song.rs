//! The song page: the arrangement, a list of entries each playing a pattern some number of times,
//! laid out a bar to a row, with a column for the entries and one for each track. The cursor is
//! on a bar and a column; space plays the song from the bar's entry.

use lattice_core::{BEATS_PER_BAR, Entry, MAX_ENTRIES, MAX_REPEATS};

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
        let bar = self.song_bar();
        let mut start = 0;
        for (i, bars) in self.entry_bars().into_iter().enumerate() {
            start += bars;
            if bar < start {
                return i;
            }
        }
        self.arrangement.entries.len().saturating_sub(1)
    }

    /// The bar under the song page's cursor, counting from the song's start, kept inside it.
    pub fn song_bar(&self) -> usize {
        let bars: usize = self.entry_bars().iter().sum();
        self.song.bar.min(bars.saturating_sub(1))
    }

    /// The song page's column under the cursor: 0 for the entries, then 1 for the first track
    /// and so on.
    pub fn song_column(&self) -> usize {
        self.song.column.min(self.grid.tracks.len())
    }

    /// How many bars each entry plays for.
    pub fn entry_bars(&self) -> Vec<usize> {
        let bar = u64::from(BEATS_PER_BAR * self.grid.steps_per_beat.max(1));
        let pattern_bars: Vec<usize> = (0..self.arrangement.patterns.len())
            .map(|p| (self.arrangement.steps(&self.grid, p) / bar) as usize)
            .collect();
        (self.arrangement.entries.iter())
            .map(|e| pattern_bars[e.pattern] * e.repeats as usize)
            .collect()
    }

    /// The bar `entry` starts on.
    pub fn entry_start(&self, entry: usize) -> usize {
        self.entry_bars().iter().take(entry).sum()
    }

    /// Puts the song page's cursor on `entry`, on its first bar.
    pub(crate) fn song_to_entry(&mut self, entry: usize) {
        self.song.bar = self.entry_start(entry);
    }

    /// Keeps the cursor on `entry` after it's changed length, on the same bar if it still plays.
    fn keep_on_entry(&mut self, entry: usize) {
        let start = self.entry_start(entry);
        let bars = self.entry_bars().get(entry).copied().unwrap_or(1);
        self.song.bar = self.song.bar.clamp(start, start + bars.max(1) - 1);
    }

    /// Handles `action` on the song page, or `None` to handle it as on any page.
    pub(crate) fn song_act(&mut self, action: Action, n: usize) -> Option<Vec<Effect>> {
        self.song.bar = self.song_bar();
        self.song.column = self.song_column();
        let entry = self.song_cursor();
        let bars: usize = self.entry_bars().iter().sum();
        let last = bars.saturating_sub(1);
        let by = n as isize;
        match action {
            Action::Up => self.song.bar = self.song.bar.saturating_sub(n),
            Action::Down => self.song.bar = (self.song.bar + n).min(last),
            Action::NextNote => {
                let next = (entry + n).min(self.arrangement.entries.len().saturating_sub(1));
                self.song_to_entry(next);
            }
            Action::PrevNote => {
                // Back to the start of this entry first, as w and b do with words.
                let back = if self.song.bar > self.entry_start(entry) {
                    n.saturating_sub(1)
                } else {
                    n
                };
                self.song_to_entry(entry.saturating_sub(back));
            }
            Action::FirstStep => self.song.bar = 0,
            Action::LastStep => self.song.bar = last,
            Action::Left => self.song.column = self.song.column.saturating_sub(n),
            Action::Right => self.song.column = (self.song.column + n).min(self.grid.tracks.len()),
            Action::NoteUp => self.entry_pattern(entry, by),
            Action::NoteDown => self.entry_pattern(entry, -by),
            Action::Longer => self.repeat(entry, by),
            Action::Shorter => self.repeat(entry, -by),
            Action::OpenBelow => self.add_entry(Place::After, None),
            Action::OpenAbove => self.add_entry(Place::Before, None),
            Action::Delete | Action::Clear => self.remove_entry(action == Action::Delete),
            Action::Yank => {
                if let Some(entry) = self.arrangement.entries.get(entry) {
                    self.song.yanked = Some(*entry);
                    self.message = Some("copied the entry".into());
                }
            }
            Action::Paste => self.paste_entry(Place::After),
            Action::PasteInsert => self.paste_entry(Place::Before),
            Action::Open => {
                // Edit the entry's pattern.
                let Some(entry) = self.arrangement.entries.get(entry) else {
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
        if entry != cursor {
            self.song_to_entry(entry);
        }
        let mut effects = self.start_song(entry);
        effects.push(Effect::TogglePlay);
        effects
    }

    /// Plays another pattern in the cursor's entry: the next or previous, wrapping round.
    fn entry_pattern(&mut self, at: usize, by: isize) {
        let patterns = self.arrangement.patterns.len() as isize;
        let Some(entry) = self.arrangement.entries.get(at) else {
            return;
        };
        let pattern = (entry.pattern as isize + by).rem_euclid(patterns) as usize;
        if pattern == entry.pattern {
            return;
        }
        self.checkpoint();
        self.arrangement.entries[at].pattern = pattern;
        self.keep_on_entry(at);
    }

    /// Plays the cursor's entry `by` more times, or fewer.
    fn repeat(&mut self, at: usize, by: isize) {
        let Some(entry) = self.arrangement.entries.get(at) else {
            return;
        };
        let repeats = (entry.repeats as isize + by).clamp(1, MAX_REPEATS as isize) as u32;
        if repeats == entry.repeats {
            return;
        }
        self.checkpoint();
        self.arrangement.entries[at].repeats = repeats;
        self.keep_on_entry(at);
    }

    /// Adds `entry`, or one playing the cursor's pattern once (or the pattern being edited, in an
    /// empty song), and moves onto it.
    fn add_entry(&mut self, place: Place, entry: Option<Entry>) {
        let cursor = self.song_cursor();
        let entries = &self.arrangement.entries;
        if entries.len() >= MAX_ENTRIES {
            self.message = Some(format!("songs have up to {MAX_ENTRIES} entries"));
            return;
        }
        let entry = entry.unwrap_or_else(|| Entry {
            pattern: entries
                .get(cursor)
                .map_or(self.arrangement.current, |e| e.pattern),
            repeats: 1,
        });
        let at = match place {
            Place::After if !entries.is_empty() => cursor + 1,
            _ => cursor,
        };
        self.checkpoint();
        self.arrangement.entries.insert(at, entry);
        self.song_to_entry(at);
        // The song playing carries on from the same entry.
        if let Some((playing, _)) = &mut self.song_position
            && at <= *playing
        {
            *playing += 1;
        }
    }

    /// Removes the cursor's entry, copying it if `yank`.
    fn remove_entry(&mut self, yank: bool) {
        let at = self.song_cursor();
        if at >= self.arrangement.entries.len() {
            return;
        }
        self.checkpoint();
        let entry = self.arrangement.entries.remove(at);
        if yank {
            self.song.yanked = Some(entry);
        }
        // Onto the start of the entry after, or the one before at the end.
        self.song_to_entry(at.min(self.arrangement.entries.len().saturating_sub(1)));
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
    /// The bar under the cursor, counting from the song's start.
    pub(crate) bar: usize,
    /// The column under the cursor: see [`Editor::song_column`].
    pub(crate) column: usize,
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
        keys(&mut e, "o");
        e.handle(Key::Ctrl('a'));
        keys(&mut e, "++");
        assert_eq!(song(&e), [(0, 1), (1, 3)]);
        assert_eq!(e.song_cursor(), 1);
        keys(&mut e, "-");
        assert_eq!(song(&e), [(0, 1), (1, 2)]);

        // Wrapping round the patterns; never fewer than one repeat.
        e.handle(Key::Ctrl('a'));
        keys(&mut e, "3-");
        assert_eq!(song(&e), [(0, 1), (0, 1)]);
        e.handle(Key::Ctrl('x'));
        keys(&mut e, "+k");

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
    fn the_cursor_moves_by_bars_entries_and_tracks() {
        let mut e = editor();
        // The first entry plays three times, then another of the same.
        keys(&mut e, "++o");
        assert_eq!(e.entry_bars(), [3, 1]);
        assert_eq!((e.song_bar(), e.song_cursor()), (3, 1));

        // j and k go a bar at a time, w and b to the start of an entry.
        keys(&mut e, "b");
        assert_eq!(e.song_bar(), 0);
        keys(&mut e, "jj");
        assert_eq!((e.song_bar(), e.song_cursor()), (2, 0));
        keys(&mut e, "w");
        assert_eq!(e.song_bar(), 3);
        keys(&mut e, "kb");
        assert_eq!(e.song_bar(), 0);
        keys(&mut e, "gej");
        assert_eq!(e.song_bar(), 3);

        // h and l go from the entries to the tracks, and no further.
        keys(&mut e, "lll");
        assert_eq!(e.song_column(), 1);
        keys(&mut e, "h");
        assert_eq!(e.song_column(), 0);

        // Fewer repeats keep the cursor on the entry.
        keys(&mut e, "ggjj--");
        assert_eq!(e.entry_bars(), [1, 1]);
        assert_eq!((e.song_bar(), e.song_cursor()), (0, 0));
    }

    #[test]
    fn space_plays_the_song_from_the_cursor() {
        let mut e = editor();
        keys(&mut e, "o");
        e.handle(Key::Ctrl('a'));
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
