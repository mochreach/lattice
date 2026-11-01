//! Patterns: going between them, adding, copying, renaming and removing them, and what plays
//! next. Every pattern shares the tracks; each has its own steps and rules (see
//! [`Arrangement`](lattice_core::Arrangement)). The pattern being edited is always the one
//! playing, so picking another while the song plays cues it for when the playing one ends.

use lattice_core::{BEATS_PER_BAR, Lane, MAX_BARS, MAX_PATTERNS, Step};

use crate::{Editor, Effect, changed};

/// What the frontend should have the engine do when the playing pattern ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Play it again.
    Loop,
    /// Change to this pattern.
    Pattern(usize),
    /// Stop: the song's over.
    Stop,
}

impl Editor {
    /// How many steps the pattern being edited plays for.
    pub fn pattern_steps(&self) -> u64 {
        self.arrangement.steps(&self.grid, self.arrangement.current)
    }

    /// The pattern being edited.
    pub fn pattern_name(&self) -> &str {
        &self.arrangement.patterns[self.arrangement.current].name
    }

    /// The pattern called `name`, or numbered so from 1.
    pub(crate) fn find_pattern(&self, name: &str) -> Result<usize, String> {
        let patterns = &self.arrangement.patterns;
        patterns
            .iter()
            .position(|p| p.name == name)
            .or_else(|| {
                name.parse::<usize>()
                    .ok()
                    .filter(|n| (1..=patterns.len()).contains(n))
                    .map(|n| n - 1)
            })
            .ok_or_else(|| format!("no pattern {name}"))
    }

    /// Goes to pattern `index`: now if stopped, or when the playing pattern ends if not.
    pub(crate) fn go_to_pattern(&mut self, index: usize) -> Vec<Effect> {
        let current = self.arrangement.current;
        if self.transport.is_some() {
            // Played by hand, the song leaves the arrangement.
            self.song_position = None;
            self.cue = (index != current).then_some(index);
            let name = &self.arrangement.patterns[index].name;
            self.message = Some(match self.cue {
                Some(_) => format!("{name} cued, after {}", self.pattern_name()),
                None => format!("staying on {name}"),
            });
            return vec![];
        }
        self.cue = None;
        self.select_pattern(index)
    }

    /// Makes `index` the pattern being edited, at once.
    pub(crate) fn select_pattern(&mut self, index: usize) -> Vec<Effect> {
        if index == self.arrangement.current {
            return vec![];
        }
        self.held.clear();
        self.take = None;
        self.arrangement.select(&mut self.grid, index);
        self.clamp_selection();
        self.message = Some(format!("pattern {}", self.pattern_name()));
        changed(0..self.grid.tracks.len())
    }

    /// The pattern after or before this one, wrapping round.
    pub(crate) fn step_pattern(&mut self, by: isize) -> Vec<Effect> {
        let len = self.arrangement.patterns.len() as isize;
        let from = self.cue.unwrap_or(self.arrangement.current) as isize;
        self.go_to_pattern((from + by).rem_euclid(len) as usize)
    }

    /// Adds a pattern after this one, a copy of it or empty with tracks as long as they are
    /// now, and goes to it.
    pub(crate) fn new_pattern(&mut self, name: &str, copy: bool) -> Result<Vec<Effect>, String> {
        if self.arrangement.patterns.len() >= MAX_PATTERNS {
            return Err(format!("songs have up to {MAX_PATTERNS} patterns"));
        }
        let name = (!name.is_empty()).then(|| name.to_string());
        if let Some(name) = &name
            && self.find_pattern(name).is_ok()
        {
            return Err(format!("there's already a pattern {name}"));
        }
        let lanes = if copy {
            self.arrangement.lanes(&self.grid, self.arrangement.current)
        } else {
            self.grid
                .tracks
                .iter()
                .map(|t| Lane {
                    steps: vec![Step::default(); t.steps.len()],
                    rules: vec![],
                })
                .collect()
        };
        self.checkpoint();
        let index = self.arrangement.insert(name, lanes);
        Ok(self.go_to_pattern(index))
    }

    pub(crate) fn rename_pattern(&mut self, name: &str) -> Result<(), String> {
        if self.find_pattern(name).is_ok() {
            return Err(format!("there's already a pattern {name}"));
        }
        self.checkpoint();
        let current = self.arrangement.current;
        self.arrangement.patterns[current].name = name.into();
        Ok(())
    }

    /// Removes this pattern and its entries in the song, going to the one before it.
    pub(crate) fn remove_pattern(&mut self) -> Result<Vec<Effect>, String> {
        if self.arrangement.patterns.len() == 1 {
            return Err("a song needs a pattern".into());
        }
        self.checkpoint();
        let current = self.arrangement.current;
        let gone = self.pattern_name().to_string();
        let effects = self.select_pattern(current.saturating_sub(1).max(usize::from(current == 0)));
        self.arrangement.remove(current);
        self.cue = None;
        self.song_position = None;
        self.message = Some(format!("removed pattern {gone}"));
        Ok(effects)
    }

    /// `:bars N` or `:bars auto`: how long this pattern plays.
    pub(crate) fn set_bars(&mut self, text: &str) -> Result<(), String> {
        let bars = match text {
            "" | "auto" => None,
            n => Some(
                n.parse::<u32>()
                    .ok()
                    .filter(|b| (1..=MAX_BARS).contains(b))
                    .ok_or_else(|| format!("bars: 1 to {MAX_BARS}, or auto"))?,
            ),
        };
        self.checkpoint();
        let current = self.arrangement.current;
        self.arrangement.patterns[current].bars = bars;
        let bar = u64::from(BEATS_PER_BAR * self.grid.steps_per_beat.max(1));
        let steps = self.pattern_steps();
        self.message = Some(match bars {
            Some(_) => format!("{} plays {} bars", self.pattern_name(), steps / bar),
            None => format!(
                "{} plays once round its loop: {steps} steps",
                self.pattern_name()
            ),
        });
        Ok(())
    }

    /// Starts the song from entry `entry` of the arrangement, before the frontend starts
    /// playback: goes to its pattern at once.
    pub fn start_song(&mut self, entry: usize) -> Vec<Effect> {
        self.cue = None;
        let Some(e) = self.arrangement.entries.get(entry) else {
            self.song_position = None;
            return vec![];
        };
        self.song_position = Some((entry, 0));
        self.select_pattern(e.pattern)
    }

    /// The section after the one playing, in the song: the same entry again, or the next.
    fn next_position(&self, (entry, repeat): (usize, u32)) -> Option<(usize, u32)> {
        let entries = &self.arrangement.entries;
        if repeat + 1 < entries.get(entry)?.repeats {
            Some((entry, repeat + 1))
        } else {
            (entry + 1 < entries.len()).then_some((entry + 1, 0))
        }
    }

    /// What should happen when the playing pattern ends.
    pub fn next(&self) -> Next {
        let current = self.arrangement.current;
        let pattern = match self.song_position {
            Some(position) => match self.next_position(position) {
                Some((entry, _)) => self.arrangement.entries[entry].pattern,
                None => return Next::Stop,
            },
            None => self.cue.unwrap_or(current),
        };
        if pattern == current {
            Next::Loop
        } else {
            Next::Pattern(pattern)
        }
    }

    /// The playing pattern has ended and the next begun: the engine changed to `changed_to`,
    /// or played the same one again.
    pub fn boundary(&mut self, changed_to: Option<usize>) {
        self.take = None;
        if let Some(position) = self.song_position {
            // Moved on, unless the engine did something else, as when a change came too late.
            let next = self.next_position(position);
            let expected = next.map(|(entry, _)| self.arrangement.entries[entry].pattern);
            let pattern = changed_to.unwrap_or(self.arrangement.current);
            if expected == Some(pattern) {
                self.song_position = next;
            }
        }
        if let Some(index) = changed_to {
            if self.cue == Some(index) {
                self.cue = None;
            }
            self.select_pattern(index);
        }
    }

    /// Playback has stopped.
    pub fn stopped(&mut self) {
        self.cue = None;
        self.song_position = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Key, Transport};
    use lattice_core::{Entry, Grid, Track};

    fn editor() -> Editor {
        let track = |len| Track {
            name: "t".into(),
            steps: vec![Step::default(); len],
            ..Track::default()
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![track(16), track(12)],
            groups: vec![],
        };
        Editor::new(grid, 120.0)
    }

    fn run(e: &mut Editor, line: &str) -> Vec<Effect> {
        e.handle(Key::Esc);
        e.handle(Key::Char(':'));
        for c in line.chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Enter)
    }

    fn playing(e: &mut Editor) {
        e.set_transport(Some(Transport {
            positions: vec![Some((0, 0)); 2],
            into_step: 0.0,
            count_in: 0,
        }));
    }

    #[test]
    fn patterns_share_tracks_and_keep_their_own_steps() {
        let mut e = editor();
        e.handle(Key::Char('i'));
        e.handle(Key::Char('z'));
        assert_eq!(e.pattern_name(), "A");
        assert_eq!(e.pattern_steps(), 48, "16 and 12 come round together");

        // A copy, then an empty one.
        let effects = run(&mut e, "pattern-dup");
        assert_eq!(effects, changed(0..2));
        assert_eq!(e.pattern_name(), "B");
        assert_eq!(e.grid.tracks[0].steps[0], Step::note(60));
        run(&mut e, "pattern-new verse");
        assert_eq!(e.pattern_name(), "verse");
        assert!(e.grid.tracks[0].steps[0].is_empty());
        assert_eq!(e.grid.tracks[1].steps.len(), 12);
        assert_eq!(
            e.arrangement
                .patterns
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["A", "B", "verse"]
        );

        // Back to the first, by number, then on with `]p`, wrapping round.
        run(&mut e, "pattern 1");
        assert_eq!(e.grid.tracks[0].steps[0], Step::note(60));
        e.handle(Key::Char('['));
        e.handle(Key::Char('p'));
        assert_eq!(e.pattern_name(), "verse");

        // Undo takes back the new pattern, and goes to where the change was made.
        e.handle(Key::Char('u'));
        assert_eq!(e.arrangement.patterns.len(), 2);
        assert_eq!(e.pattern_name(), "B");

        // A track added is added to every pattern; removing a pattern takes its entries.
        run(&mut e, "track-add");
        assert!(e.arrangement.patterns[0].lanes.len() == 3);
        run(&mut e, "pattern-remove");
        assert_eq!(e.pattern_name(), "A");
        assert_eq!(e.arrangement.patterns.len(), 1);
        assert!(run(&mut e, "pattern-remove").is_empty());
        assert!(e.message.as_deref().unwrap().contains("needs a pattern"));
    }

    #[test]
    fn bars_set_how_long_a_pattern_plays() {
        let mut e = editor();
        run(&mut e, "bars 2");
        assert_eq!(e.pattern_steps(), 32);
        run(&mut e, "bars auto");
        assert_eq!(e.pattern_steps(), 48);
        run(&mut e, "bars 0");
        assert!(e.message.as_deref().unwrap().contains("1 to 256"));
    }

    #[test]
    fn picking_a_pattern_while_playing_cues_it() {
        let mut e = editor();
        run(&mut e, "pattern-new");
        run(&mut e, "pattern 1");
        playing(&mut e);
        assert_eq!(e.next(), Next::Loop);
        assert!(run(&mut e, "pattern B").is_empty());
        assert_eq!(e.pattern_name(), "A", "still playing A");
        assert_eq!(e.next(), Next::Pattern(1));
        // A loop the engine played instead, the change having come too late, changes nothing.
        e.boundary(None);
        assert_eq!(e.next(), Next::Pattern(1));
        e.boundary(Some(1));
        assert_eq!(e.pattern_name(), "B");
        assert_eq!(e.next(), Next::Loop);
    }

    #[test]
    fn the_song_plays_its_entries_in_order_then_stops() {
        let mut e = editor();
        run(&mut e, "pattern-new");
        e.arrangement.entries = vec![
            Entry {
                pattern: 1,
                repeats: 2,
            },
            Entry {
                pattern: 0,
                repeats: 1,
            },
        ];
        assert_eq!(e.start_song(0), vec![]);
        assert_eq!(e.pattern_name(), "B");
        playing(&mut e);
        assert_eq!(e.next(), Next::Loop);
        e.boundary(None);
        assert_eq!(e.song_position, Some((0, 1)));
        assert_eq!(e.next(), Next::Pattern(0));
        e.boundary(Some(0));
        assert_eq!((e.song_position, e.pattern_name()), (Some((1, 0)), "A"));
        assert_eq!(e.next(), Next::Stop);
        e.stopped();
        assert_eq!(e.song_position, None);
    }
}
