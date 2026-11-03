//! The song page: the arrangement, a list of entries each playing a pattern some number of times,
//! laid out a bar to a row, with a column for the entries and one for each track. The cursor is
//! on a bar and a column; space plays the song from the bar's entry.

use std::ops::RangeInclusive;

use lattice_core::{Entry, MAX_ENTRIES, MAX_REPEATS};

use crate::{Action, Editor, Effect, Key, Mode, Page, PatternId, PromptKind, TrackId};

/// A copied entry, holding on to its pattern and muted tracks by who they are rather than where,
/// so it pastes the same after they've been added, removed, moved or brought back by undo.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Copied {
    pattern: PatternId,
    repeats: u32,
    muted: Vec<TrackId>,
}

/// Where an entry goes, relative to the cursor's.
#[derive(Clone, Copy)]
enum Place {
    After,
    Before,
}

impl Editor {
    /// The entry under the song page's cursor, kept inside the arrangement.
    pub fn song_cursor(&self) -> usize {
        self.entry_at(self.song_bar())
    }

    /// The entry playing at `bar`, or the last.
    fn entry_at(&self, bar: usize) -> usize {
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
        self.song.bar.min(self.song_bars().saturating_sub(1))
    }

    /// How many bars the song plays for.
    fn song_bars(&self) -> usize {
        self.entry_bars().iter().sum()
    }

    /// The song page's column under the cursor: 0 for the entries, then 1 for the first track
    /// and so on.
    pub fn song_column(&self) -> usize {
        self.song.column.min(self.grid.tracks.len())
    }

    /// The bars and columns selected on the song page: in select mode, from where it began to
    /// the cursor, and otherwise just the cursor's.
    pub fn song_selection(&self) -> (RangeInclusive<usize>, RangeInclusive<usize>) {
        let (bar, column) = (self.song_bar(), self.song_column());
        let (from_bar, from_column) = match self.song.anchor {
            Some((b, c)) if self.mode == Mode::Select => (
                b.min(self.song_bars().saturating_sub(1)),
                c.min(self.grid.tracks.len()),
            ),
            _ => (bar, column),
        };
        (
            bar.min(from_bar)..=bar.max(from_bar),
            column.min(from_column)..=column.max(from_column),
        )
    }

    /// The entries the song page's selection reaches.
    fn song_entries(&self) -> RangeInclusive<usize> {
        let (bars, _) = self.song_selection();
        self.entry_at(*bars.start())..=self.entry_at(*bars.end())
    }

    /// The tracks the song page's selection reaches, if it reaches any.
    fn song_tracks(&self) -> Option<RangeInclusive<usize>> {
        let (_, columns) = self.song_selection();
        let last = columns.end().checked_sub(1)?;
        Some(columns.start().max(&1) - 1..=last)
    }

    /// How many bars each entry plays for.
    pub fn entry_bars(&self) -> Vec<usize> {
        let a = &self.arrangement;
        let pattern_bars: Vec<usize> = (a.all_steps(&self.grid).into_iter().enumerate())
            .map(|(p, steps)| (steps / a.bar(&self.grid, p)) as usize)
            .collect();
        (self.arrangement.entries.iter())
            // An entry of no pattern plays nothing.
            .map(|e| {
                pattern_bars
                    .get(e.pattern)
                    .map_or(0, |b| b * e.repeats as usize)
            })
            .collect()
    }

    /// The bar `entry` starts on.
    pub fn entry_start(&self, entry: usize) -> usize {
        self.entry_bars().iter().take(entry).sum()
    }

    /// The last bar `entry` plays.
    fn entry_end(&self, entry: usize) -> usize {
        let bars = self.entry_bars().get(entry).copied().unwrap_or(1);
        self.entry_start(entry) + bars.max(1) - 1
    }

    /// Puts the song page's cursor on `entry`, on its first bar.
    pub(crate) fn song_to_entry(&mut self, entry: usize) {
        self.song.bar = self.entry_start(entry);
    }

    /// Keeps the cursor on `entry` after it's changed length, on the same bar if it still plays.
    pub(crate) fn keep_on_entry(&mut self, entry: usize) {
        let (start, end) = (self.entry_start(entry), self.entry_end(entry));
        self.song.bar = self.song.bar.clamp(start, end);
    }

    /// The first and last entries of the section the song loops round, if it does.
    pub fn song_loop(&self) -> Option<(usize, usize)> {
        self.song
            .looped
            .filter(|&(start, end)| start <= end && end < self.arrangement.entries.len())
    }

    /// Loops the song round the selected entries. With only the cursor's, it stretches a loop
    /// to take it in, or on an entry in the loop, stops looping.
    fn toggle_loop(&mut self) {
        if self.arrangement.entries.is_empty() {
            return;
        }
        let selected = self.song_entries();
        let (first, last) = (*selected.start(), *selected.end());
        let looped = match self.song_loop() {
            _ if first != last => Some((first, last)),
            Some((start, end)) if (start..=end).contains(&first) => None,
            Some((start, end)) => Some((start.min(first), end.max(first))),
            None => Some((first, first)),
        };
        self.song.looped = looped;
        self.message = Some(match looped {
            Some((start, end)) if start == end => format!("looping entry {}", start + 1),
            Some((start, end)) => format!("looping entries {} to {}", start + 1, end + 1),
            None => "not looping".into(),
        });
    }

    /// Keys of the song page's own, in place of what they do on the pattern page: `m` and `s`
    /// mute and solo in the entries, and the track menu's no use here. `None` for the rest.
    pub(crate) fn song_key(&mut self, key: Key) -> Option<Vec<Effect>> {
        match key {
            Key::Char('m') => self.song_act(Action::ToggleMute, 1),
            Key::Char('s') => self.song_act(Action::ToggleSolo, 1),
            Key::Char('t') => {
                self.message = Some("here m mutes and s solos, in the entries selected".into());
                Some(vec![])
            }
            _ => None,
        }
    }

    /// Handles `action` on the song page, or `None` to handle it as on any page.
    pub(crate) fn song_act(&mut self, action: Action, n: usize) -> Option<Vec<Effect>> {
        self.song.bar = self.song_bar();
        self.song.column = self.song_column();
        let entry = self.song_cursor();
        let last = self.song_bars().saturating_sub(1);
        let by = n as isize;
        let selecting = self.mode == Mode::Select;
        match action {
            // Moving extends the selection in select mode, from where it began.
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
            Action::SelectMode if selecting => self.mode = Mode::Normal,
            Action::SelectMode => self.select_from_here(),
            Action::SelectRows => self.select_entries(n),
            Action::Collapse => self.song.anchor = Some((self.song.bar, self.song.column)),
            Action::ToggleMute => self.mute_selected(),
            Action::ToggleSolo => self.solo_selected(),
            Action::Undo
            | Action::Redo
            | Action::CommandLine
            | Action::PlaySong
            | Action::PlayFill
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
            // The rest change the song, and finish the selection, as in Helix.
            _ => {
                let effects = self.song_edit(action, by);
                if !matches!(self.mode, Mode::Command(_)) {
                    self.mode = Mode::Normal;
                }
                return Some(effects);
            }
        }
        Some(vec![])
    }

    /// Edits the song as `action` says, on the selected entries.
    fn song_edit(&mut self, action: Action, by: isize) -> Vec<Effect> {
        let patterns = self.arrangement.patterns.len() as isize;
        let turn = move |e: Entry, by: isize| Entry {
            pattern: (e.pattern as isize + by).rem_euclid(patterns) as usize,
            ..e
        };
        let repeat = move |e: Entry, by: isize| Entry {
            repeats: (e.repeats as isize + by).clamp(1, MAX_REPEATS as isize) as u32,
            ..e
        };
        match action {
            Action::NoteUp => self.change_entries(|e| turn(e, by)),
            Action::NoteDown => self.change_entries(|e| turn(e, -by)),
            Action::Longer => self.change_entries(|e| repeat(e, by)),
            Action::Shorter => self.change_entries(|e| repeat(e, -by)),
            Action::OpenBelow | Action::OpenAbove => {
                // One playing the cursor's pattern once, or the pattern being edited in an empty
                // song.
                let pattern = (self.arrangement.entries.get(self.song_cursor()))
                    .map_or(self.arrangement.current, |e| e.pattern);
                let entry = Entry {
                    pattern,
                    repeats: 1,
                    mutes: 0,
                };
                let place = match action {
                    Action::OpenBelow => Place::After,
                    _ => Place::Before,
                };
                self.add_entries(place, vec![entry]);
            }
            Action::Delete | Action::Clear => self.remove_entries(action == Action::Delete),
            Action::Yank => {
                let selected = self.song_entries();
                if let Some(entries) = self.arrangement.entries.get(selected) {
                    self.song.yanked = self.copy_entries(entries);
                    self.message = Some(match entries.len() {
                        1 => "copied the entry".into(),
                        n => format!("copied {n} entries"),
                    });
                }
            }
            Action::Paste | Action::PasteInsert if self.song.yanked.is_empty() => {
                self.message = Some("nothing copied".into());
            }
            Action::Paste | Action::PasteInsert => {
                let place = match action {
                    Action::Paste => Place::After,
                    _ => Place::Before,
                };
                match self.pasted_entries() {
                    entries if entries.is_empty() => {
                        self.message = Some("the copied entries' patterns are gone".into());
                    }
                    entries => self.add_entries(place, entries),
                }
            }
            Action::Open => return self.open_bar(),
            Action::TogglePlay => return self.play_song(false),
            Action::LockSetting => self.toggle_loop(),
            Action::Change if self.song.column == 0 => {
                // Renaming the entry's pattern.
                self.open_prompt(PromptKind::Command);
                if let Mode::Command(prompt) = &mut self.mode {
                    prompt.line = "pattern-name ".into();
                }
            }
            // Pattern editing has nothing to act on here.
            _ => {}
        }
        vec![]
    }

    /// Starts selecting from the cursor.
    fn select_from_here(&mut self) {
        self.song.anchor = Some((self.song.bar, self.song.column));
        self.mode = Mode::Select;
    }

    /// Selects the cursor's entry, all its bars; again, `n` more entries.
    fn select_entries(&mut self, n: usize) {
        let selected = self.song_entries();
        let (first, mut last) = (*selected.start(), *selected.end());
        let (bars, columns) = self.song_selection();
        let whole = self.mode == Mode::Select
            && *bars.start() == self.entry_start(first)
            && *bars.end() == self.entry_end(last);
        if whole {
            last = (last + n).min(self.arrangement.entries.len().saturating_sub(1));
        }
        let column = match self.mode {
            Mode::Select => *columns.start(),
            _ => self.song.column,
        };
        self.song.anchor = Some((self.entry_start(first), column));
        self.song.bar = self.entry_end(last);
        self.mode = Mode::Select;
    }

    /// Says what was done to the selected tracks in the selected entries: `what`, its verb
    /// as for one track.
    fn say_tracks(&mut self, what: &str) {
        let (Some(tracks), entries) = (self.song_tracks(), self.song_entries()) else {
            return;
        };
        let (tracks, what) = match tracks.clone().count() {
            1 => {
                let Some(track) = self.grid.tracks.get(*tracks.start()) else {
                    return;
                };
                (track.name.clone(), what.to_string())
            }
            n => (format!("{n} tracks"), what.replacen("plays", "play", 1)),
        };
        let entries = match entries.count() {
            1 => "this entry".to_string(),
            n => format!("{n} entries"),
        };
        self.message = Some(format!("{tracks} {what} in {entries}"));
    }

    /// Mutes the selected tracks in the selected entries, or if they're all muted there,
    /// unmutes them.
    fn mute_selected(&mut self) {
        let Some(tracks) = self.song_tracks() else {
            self.message = Some("on a track, m mutes it in the entry".into());
            return;
        };
        let selected = self.song_entries();
        let Some(entries) = self.arrangement.entries.get(selected.clone()) else {
            self.message = Some("the song's empty: o adds an entry".into());
            return;
        };
        let muted = entries.iter().all(|e| tracks.clone().all(|t| e.muted(t)));
        self.checkpoint();
        for entry in self
            .arrangement
            .entries
            .get_mut(selected)
            .into_iter()
            .flatten()
        {
            for t in tracks.clone() {
                entry.set_muted(t, !muted);
            }
        }
        self.say_tracks(if muted { "plays" } else { "muted" });
    }

    /// Mutes every other track in the selected entries, so only the selected play, or if
    /// that's how they are, lets every track play there again.
    fn solo_selected(&mut self) {
        let Some(tracks) = self.song_tracks() else {
            self.message = Some("on a track, s plays it alone in the entry".into());
            return;
        };
        let every = (1u32 << self.grid.tracks.len()) - 1;
        let soloed = tracks.fold(every, |mutes, t| mutes & !(1 << t));
        let selected = self.song_entries();
        let Some(entries) = self.arrangement.entries.get(selected.clone()) else {
            self.message = Some("the song's empty: o adds an entry".into());
            return;
        };
        let alone = entries.iter().all(|e| e.mutes == soloed);
        self.checkpoint();
        for entry in self
            .arrangement
            .entries
            .get_mut(selected)
            .into_iter()
            .flatten()
        {
            entry.mutes = if alone { 0 } else { soloed };
        }
        match alone {
            true => self.say_tracks("plays with every track"),
            false => self.say_tracks("plays alone"),
        }
    }

    /// Changes each selected entry as `change` says, keeping the cursor on its entry.
    fn change_entries(&mut self, change: impl Fn(Entry) -> Entry) {
        let at = self.song_cursor();
        let selected = self.song_entries();
        let Some(entries) = self.arrangement.entries.get(selected.clone()) else {
            return;
        };
        let changed: Vec<Entry> = entries.iter().map(|&e| change(e)).collect();
        if changed == entries {
            return;
        }
        self.checkpoint();
        self.arrangement.entries.splice(selected, changed);
        self.keep_on_entry(at);
    }

    /// Plays the song, or stops it: from its start, or else from the song page's cursor, or
    /// from the entry there if it plays the pattern being edited, the first that does if not,
    /// or the top.
    pub(crate) fn play_song(&mut self, start: bool) -> Vec<Effect> {
        if self.transport.is_some() {
            return vec![Effect::TogglePlay];
        }
        let entries = &self.arrangement.entries;
        let current = self.arrangement.current;
        let cursor = self.song_cursor();
        let entry = if start {
            0
        } else if self.page == Page::Song
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

    /// `C-k`: stops the song, and every sound with it.
    pub(crate) fn hush(&mut self) -> Vec<Effect> {
        self.message = Some("hushed".into());
        vec![Effect::Hush]
    }

    /// Edits the cursor's entry's pattern, at the cursor's bar, on its track if it's on one.
    fn open_bar(&mut self) -> Vec<Effect> {
        let at = self.song_cursor();
        let Some(&entry) = self.arrangement.entries.get(at) else {
            return vec![];
        };
        let Some(&bars) = self.entry_bars().get(at) else {
            return vec![];
        };
        let pattern_bars = (bars / entry.repeats.max(1) as usize).max(1);
        let into = (self.song_bar().saturating_sub(self.entry_start(at))) % pattern_bars;
        let column = self.song_column();
        self.page = Page::Pattern;
        let effects = self.go_to_pattern(entry.pattern);
        // Playing, the pattern's only cued, and the grid's still the one playing.
        if self.arrangement.current == entry.pattern {
            let track = column.checked_sub(1).unwrap_or(self.cursor().track);
            let Some(len) = self.grid.tracks.get(track).map(|t| t.steps.len().max(1)) else {
                return effects;
            };
            self.select_point(track, into * self.bar() % len);
        }
        effects
    }

    /// `entries`, to copy.
    fn copy_entries(&self, entries: &[Entry]) -> Vec<Copied> {
        (entries.iter())
            .filter_map(|e| {
                Some(Copied {
                    pattern: *self.pattern_ids.get(e.pattern)?,
                    repeats: e.repeats,
                    muted: (self.ids.iter().enumerate())
                        .filter(|&(t, _)| e.muted(t))
                        .map(|(_, &id)| id)
                        .collect(),
                })
            })
            .collect()
    }

    /// The entries copied, as they are in the song now: without those whose pattern's gone, or
    /// mutes for tracks that are.
    fn pasted_entries(&self) -> Vec<Entry> {
        (self.song.yanked.iter())
            .filter_map(|c| {
                let mut entry = Entry {
                    pattern: self.pattern_ids.iter().position(|&id| id == c.pattern)?,
                    repeats: c.repeats,
                    mutes: 0,
                };
                for t in c.muted.iter().filter_map(|&id| self.track_index(id)) {
                    entry.set_muted(t, true);
                }
                Some(entry)
            })
            .collect()
    }

    /// Adds `new` after the selected entries or before them, and moves onto the first.
    fn add_entries(&mut self, place: Place, new: Vec<Entry>) {
        let entries = &self.arrangement.entries;
        if entries.len() + new.len() > MAX_ENTRIES {
            self.message = Some(format!("songs have up to {MAX_ENTRIES} entries"));
            return;
        }
        let selected = self.song_entries();
        let at = match place {
            Place::After if !entries.is_empty() => selected.end() + 1,
            _ => *selected.start(),
        };
        let n = new.len();
        self.checkpoint();
        self.arrangement.entries.splice(at..at, new);
        self.song_to_entry(at);
        // The loop keeps to the same entries, taking in those added inside it.
        if let Some((start, end)) = &mut self.song.looped {
            if at <= *start {
                *start += n;
            }
            if at <= *end {
                *end += n;
            }
        }
        // The song playing carries on from the same entry.
        if let Some((playing, _)) = &mut self.song_position
            && at <= *playing
        {
            *playing += n;
        }
    }

    /// Removes the selected entries, copying them if `yank`.
    fn remove_entries(&mut self, yank: bool) {
        let selected = self.song_entries();
        if selected.is_empty() || *selected.end() >= self.arrangement.entries.len() {
            return;
        }
        self.checkpoint();
        let playing_gone = (self.song_position).is_some_and(|(p, _)| selected.contains(&p));
        let gone: Vec<Entry> = self.arrangement.entries.drain(selected.clone()).collect();
        if yank {
            self.song.yanked = self.copy_entries(&gone);
        }
        for at in selected.clone().rev() {
            if let Some((playing, _)) = &mut self.song_position
                && at < *playing
            {
                *playing -= 1;
            }
            self.song.looped = match self.song.looped {
                Some((start, end)) if (start, end) == (at, at) => None,
                Some((start, end)) => Some((
                    start - usize::from(at < start),
                    end - usize::from(at <= end),
                )),
                None => None,
            };
        }
        // The pattern playing plays on; then the song goes on with the entry after those gone.
        if playing_gone && let Some(position) = self.song_position {
            self.song_next = Some((position, *selected.start()));
        }
        // Onto the start of the entry after, or the one before at the end.
        let len = self.arrangement.entries.len();
        self.song_to_entry((*selected.start()).min(len.saturating_sub(1)));
    }
}

/// The song page's state, besides the arrangement itself.
#[derive(Debug, Clone, Default)]
pub(crate) struct SongCursor {
    /// The bar under the cursor, counting from the song's start.
    pub(crate) bar: usize,
    /// The column under the cursor: see [`Editor::song_column`].
    pub(crate) column: usize,
    /// In select mode, the bar and column the selection began at.
    anchor: Option<(usize, usize)>,
    /// The entries last copied.
    pub(crate) yanked: Vec<Copied>,
    /// The first and last entries of the section the song loops round.
    pub(crate) looped: Option<(usize, usize)>,
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
            beats_per_bar: 4,
            grouping: vec![],
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
    fn an_empty_song_has_nothing_to_mute() {
        let mut e = editor();
        e.arrangement.entries.clear();
        keys(&mut e, "lm");
        assert_eq!(
            e.message.as_deref(),
            Some("the song's empty: o adds an entry")
        );
        e.message = None;
        keys(&mut e, "s");
        assert!(e.message.is_some());
        assert!(e.arrangement.entries.is_empty());
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

    /// The test song with three tracks, each a bar long.
    fn three_tracks() -> Editor {
        let mut e = editor();
        for name in ["kick", "snare", "hat"] {
            e.handle(Key::Char(':'));
            keys(&mut e, &format!("track-add {name}"));
            e.handle(Key::Enter);
        }
        e.handle(Key::Esc);
        keys(&mut e, "gs");
        assert_eq!(e.grid.tracks.len(), 4);
        // Each added track took the cursor's column; back to the entries.
        e.song.column = 0;
        e
    }

    fn mutes(e: &Editor) -> Vec<u32> {
        e.arrangement.entries.iter().map(|e| e.mutes).collect()
    }

    #[test]
    fn m_and_s_mute_and_solo_in_the_entries_selected() {
        let mut e = three_tracks();
        keys(&mut e, "oo");
        assert_eq!(e.arrangement.entries.len(), 3);

        // On the entries' column there's no track to mute; t's menu isn't here.
        keys(&mut e, "m");
        assert_eq!(mutes(&e), [0, 0, 0]);
        keys(&mut e, "tm");
        assert_eq!(mutes(&e), [0, 0, 0]);

        // m on a track mutes it in the cursor's entry, and again unmutes it.
        keys(&mut e, "lm");
        assert_eq!(mutes(&e), [0, 0, 0b1]);
        keys(&mut e, "m");
        assert_eq!(mutes(&e), [0, 0, 0]);

        // v selects bars and tracks: two tracks in the first two entries.
        keys(&mut e, "ggvjlm");
        assert_eq!(e.mode, Mode::Select, "m keeps the selection");
        assert_eq!(mutes(&e), [0b11, 0b11, 0]);
        keys(&mut e, "m");
        assert_eq!(mutes(&e), [0, 0, 0]);

        // s plays them alone there, and again lets every track play.
        keys(&mut e, "s");
        assert_eq!(mutes(&e), [0b1100, 0b1100, 0]);
        keys(&mut e, "s");
        assert_eq!(mutes(&e), [0, 0, 0]);
        keys(&mut e, "s");
        e.handle(Key::Esc);
        assert_eq!(e.mode, Mode::Normal);

        // Playing the song, the engine's told the entry playing's mutes and the next's.
        keys(&mut e, "gg ");
        assert_eq!(e.song_mutes(), (0b1100, 0b1100));
        e.stopped();
        keys(&mut e, "u");
        assert_eq!(mutes(&e), [0, 0, 0]);
    }

    #[test]
    fn a_selection_of_entries_is_copied_moved_and_looped() {
        let mut e = editor();
        keys(&mut e, "o+o++");
        assert_eq!(song(&e), [(0, 1), (0, 2), (0, 3)]);

        // x selects the cursor's entry, and again the next one too.
        keys(&mut e, "ggxx");
        assert_eq!(e.song_selection().0, 0..=2);
        keys(&mut e, "y");
        assert_eq!(e.mode, Mode::Normal, "y finishes the selection");
        keys(&mut e, "gep");
        assert_eq!(song(&e), [(0, 1), (0, 2), (0, 3), (0, 1), (0, 2)]);
        assert_eq!(e.song_cursor(), 3);

        // d takes them all out at once, and one undo puts them back.
        keys(&mut e, "xxd");
        assert_eq!(song(&e), [(0, 1), (0, 2), (0, 3)]);
        keys(&mut e, "u");
        assert_eq!(e.arrangement.entries.len(), 5);

        // + and L act on every entry selected.
        keys(&mut e, "ggxx+");
        assert_eq!(song(&e), [(0, 2), (0, 3), (0, 3), (0, 1), (0, 2)]);
        keys(&mut e, "ggxxL");
        assert_eq!(e.song_loop(), Some((0, 1)));
    }

    #[test]
    fn the_cursors_pattern_is_renamed_and_lengthened() {
        let mut e = editor();
        keys(&mut e, "o");
        e.handle(Key::Ctrl('a'));
        keys(&mut e, "k");
        assert_eq!(e.arrangement.current, 1, "editing the second pattern");

        // c on the entries renames the cursor's pattern, not the one being edited.
        keys(&mut e, "cchorus");
        e.handle(Key::Enter);
        assert_eq!(e.arrangement.patterns[0].name, "chorus");
        assert_eq!(e.arrangement.patterns[1].name, "B");

        // :bars too, and the cursor stays on its entry.
        keys(&mut e, "j:bars 3");
        e.handle(Key::Enter);
        assert_eq!(e.arrangement.patterns[1].bars, Some(3));
        assert_eq!(e.entry_bars(), [1, 3]);
        assert_eq!((e.song_cursor(), e.song_bar()), (1, 1));
        keys(&mut e, "k:bars 2");
        e.handle(Key::Enter);
        assert_eq!(e.entry_bars(), [2, 3]);
        assert_eq!((e.song_cursor(), e.song_bar()), (0, 0));
    }

    #[test]
    fn enter_opens_the_pattern_at_the_cursors_bar() {
        let mut e = editor();
        // The first pattern two bars long, played twice.
        e.handle(Key::Enter);
        assert_eq!(e.arrangement.current, 0);
        e.grid.tracks[0].steps.resize(32, Step::default());
        keys(&mut e, "gs+");
        assert_eq!(e.entry_bars(), [4]);

        // The fourth bar is the pattern's second, from step 16.
        keys(&mut e, "gel");
        e.handle(Key::Enter);
        assert_eq!(e.page, Page::Pattern);
        assert_eq!(e.cursor(), crate::Cursor { track: 0, step: 16 });
        keys(&mut e, "gsk");
        e.handle(Key::Enter);
        assert_eq!(e.cursor().step, 0);
    }

    #[test]
    fn the_song_loops_round_a_section() {
        let mut e = editor();
        keys(&mut e, "oo");
        assert_eq!(e.song_cursor(), 2);
        keys(&mut e, "kL");
        assert_eq!(e.song_loop(), Some((1, 1)));
        keys(&mut e, "jL");
        assert_eq!(e.song_loop(), Some((1, 2)));

        // It keeps to its entries as others come and go before it.
        keys(&mut e, "ggO");
        assert_eq!(e.song_loop(), Some((2, 3)));
        keys(&mut e, "d");
        assert_eq!(e.song_loop(), Some((1, 2)));

        // Played, the end goes back to the start.
        keys(&mut e, "w ");
        assert_eq!(e.song_position, Some((1, 0)));
        e.boundary(None);
        assert_eq!(e.song_position, Some((2, 0)));
        e.boundary(None);
        assert_eq!(e.song_position, Some((1, 0)));
        e.stopped();

        // L inside the loop ends it, and the song runs on to the end.
        keys(&mut e, "L");
        assert_eq!(e.song_loop(), None);
        keys(&mut e, "j ");
        assert_eq!(e.next(), crate::Next::Stop);
    }

    #[test]
    fn undo_and_redo_take_the_loop_with_the_entries() {
        let mut e = editor();
        keys(&mut e, "ooL");
        assert_eq!(e.song_loop(), Some((2, 2)));
        // Undoing the entry it loops round takes the loop too, and redoing brings it back.
        keys(&mut e, "u");
        assert_eq!(e.song.looped, None);
        keys(&mut e, "U");
        assert_eq!(e.song_loop(), Some((2, 2)));
    }

    #[test]
    fn copied_entries_follow_the_tracks_and_patterns_through_undo() {
        let mut e = three_tracks();
        keys(&mut e, "o");
        e.arrangement.entries[0].mutes = 0b0100;
        keys(&mut e, "ggy");
        let pasted = |e: &mut Editor| {
            keys(e, "p");
            let entry = e.arrangement.entries[e.song_cursor()];
            keys(e, "u");
            (entry.pattern, entry.mutes)
        };
        assert!(e.move_track(2, 0));
        assert_eq!(pasted(&mut e), (0, 0b0001), "the muted track moved first");
        keys(&mut e, "u");
        assert_eq!(e.grid.tracks[2].name, "snare", "the move's undone");
        assert_eq!(pasted(&mut e), (0, 0b0100), "and the mute's back with it");
        assert!(e.remove_track(2));
        assert_eq!(pasted(&mut e), (0, 0), "the muted track's gone");

        // A second pattern's entry, copied, then the pattern before it removed.
        let mut e = three_tracks();
        crate::command::run(&mut e, "pn").unwrap();
        keys(&mut e, "gsgey");
        assert_eq!(e.song.yanked.len(), 1);
        e.select_pattern(0);
        e.remove_pattern().unwrap();
        // The test song has two patterns; the new one's the third.
        assert_eq!(pasted(&mut e).0, 1, "its pattern's moved down");
        keys(&mut e, "u");
        assert_eq!(e.arrangement.patterns.len(), 3);
        assert_eq!(pasted(&mut e).0, 2, "and back up");
    }

    #[test]
    fn removing_the_entry_playing_goes_on_to_the_one_after() {
        let mut e = editor();
        let entry = |pattern, repeats| Entry {
            pattern,
            repeats,
            mutes: 0,
        };
        e.arrangement.entries = vec![entry(0, 1), entry(1, 2), entry(0, 3)];
        e.song_position = Some((1, 1));
        e.song.bar = e.entry_start(1);
        e.page = Page::Song;
        keys(&mut e, "d");
        assert_eq!(e.arrangement.entries.len(), 2);
        // The pattern playing (1) plays on; pattern 0 follows, from its first repeat of three.
        assert_eq!(e.next(), crate::Next::Pattern(0));
        e.boundary(Some(0));
        assert_eq!(e.song_position, Some((1, 0)));
        // Removing the last entry, while it plays, ends the song after it.
        e.song.bar = e.entry_start(1);
        keys(&mut e, "d");
        assert_eq!(e.next(), crate::Next::Stop);
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

        // From the pattern page, A-space plays the song from the entry playing this pattern.
        let effects = e.handle(Key::Alt(' '));
        assert_eq!(effects.last(), Some(&Effect::TogglePlay));
        assert_eq!(e.song_position, Some((1, 0)));

        // And C-space from its start.
        e.stopped();
        let effects = e.handle(Key::Ctrl(' '));
        assert_eq!(effects.last(), Some(&Effect::TogglePlay));
        assert_eq!(e.song_position, Some((0, 0)));
        assert_eq!(e.arrangement.current, 0);
    }
}
