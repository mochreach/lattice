//! Patterns: going between them, adding, copying, renaming and removing them, and what plays
//! next. Every pattern shares the tracks; each has its own steps and rules (see
//! [`Arrangement`](lattice_core::Arrangement)). The pattern being edited is always the one
//! playing, so picking another while the song plays cues it for when the playing one ends.

use lattice_core::{
    Entry, Lane, MAX_BARS, MAX_BEATS_PER_BAR, MAX_BPM, MAX_ENTRIES, MAX_PATTERNS,
    MAX_STEPS_PER_BEAT, MIN_BPM, Meter, Mix, Pattern, SongGesture, Step,
};

use crate::{Editor, Effect, Page, PatternId, changed};

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

/// `4`, or groups of beats as `2+2+3`, as many beats as they add up to.
fn parse_meter(word: &str) -> Result<Meter, String> {
    let sizes = (word.split('+'))
        .map(|n| {
            n.parse::<u32>()
                .ok()
                .filter(|n| (1..=MAX_BEATS_PER_BAR).contains(n))
        })
        .collect::<Option<Vec<u32>>>()
        .ok_or_else(|| format!("beats: 1 to {MAX_BEATS_PER_BAR}, or groups of them as 2+2+3"))?;
    let beats: u32 = sizes.iter().sum();
    if beats > MAX_BEATS_PER_BAR {
        return Err(format!("bars have up to {MAX_BEATS_PER_BAR} beats"));
    }
    let grouping = if sizes.len() > 1 { sizes } else { vec![] };
    Ok(Meter { beats, grouping })
}

/// `7 beats a bar (2+2+3), 2 steps a beat`.
fn describe(meter: &Meter, steps: u32) -> String {
    let beats = meter.beats;
    let grouped = match meter.to_string().split_once(' ') {
        Some((_, groups)) => format!(" {groups}"),
        None => String::new(),
    };
    format!(
        "{beats} beat{} a bar{grouped}, {steps} step{} a beat",
        if beats == 1 { "" } else { "s" },
        if steps == 1 { "" } else { "s" }
    )
}

/// How many bars of `bar` steps `pattern`'s `len` steps make, if they make 1 to [`MAX_BARS`]
/// whole ones.
fn whole_bars(pattern: &Pattern, len: u32, bar: u32) -> Result<u32, String> {
    let new = len / bar.max(1);
    if !len.is_multiple_of(bar.max(1)) || !(1..=MAX_BARS).contains(&new) {
        return Err(format!(
            "{} is {len} steps, which isn't 1 to {MAX_BARS} bars of {bar}",
            pattern.name
        ));
    }
    Ok(new)
}

impl Editor {
    /// How many steps the pattern being edited plays for.
    pub fn pattern_steps(&self) -> u64 {
        self.arrangement.current_steps(&self.grid)
    }

    /// The pattern being edited.
    pub fn pattern_name(&self) -> &str {
        // The current pattern is always there; were it not, it would have no name.
        (self.arrangement.patterns.get(self.arrangement.current)).map_or("", |p| &p.name)
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
        let Some(pattern) = self.arrangement.patterns.get(index) else {
            self.message = Some("no such pattern".into());
            return vec![];
        };
        if self.transport.is_some() {
            // Played by hand, the song leaves the arrangement.
            self.song_position = None;
            self.cue = (index != current).then_some(index);
            let name = &pattern.name;
            self.message = Some(match self.cue {
                Some(_) => format!("{name} cued, after {}", self.pattern_name()),
                None => format!("staying on {name}"),
            });
            return vec![];
        }
        self.cue = None;
        self.select_pattern(index)
    }

    /// The conductor's cued pattern `index` (`P3`), as `]p` would. Stopped, it's too late.
    pub fn conducted_pattern(&mut self, index: usize) {
        if self.transport.is_some() {
            self.go_to_pattern(index);
        }
    }

    /// The conductor has muted or soloed track `track` (`M2`, `S-2`) in the engine, until the
    /// song stops: shown over the track's own, which stays as the song has it.
    pub fn conducted_mix(&mut self, track: usize, mute: Option<bool>, solo: Option<bool>) {
        let Some(&id) = self.ids.get(track) else {
            return;
        };
        let at = match self.conducted.iter().position(|(i, ..)| *i == id) {
            Some(at) => at,
            None => {
                self.conducted.push((id, None, None));
                self.conducted.len() - 1
            }
        };
        if let Some((_, m, s)) = self.conducted.get_mut(at) {
            *m = mute.or(*m);
            *s = solo.or(*s);
        }
    }

    /// How track `t` sits in the mix as it's heard: its own, with the conductor's mute and solo
    /// over it while the song plays.
    pub fn heard_mix(&self, t: usize) -> Option<Mix> {
        let mut mix = self.grid.tracks.get(t)?.mix;
        let id = self.ids.get(t);
        if let Some((_, mute, solo)) = self.conducted.iter().find(|(i, ..)| Some(i) == id) {
            mix.mute = mute.unwrap_or(mix.mute);
            mix.solo = solo.unwrap_or(mix.solo);
        }
        Some(mix)
    }

    /// The player's muting or soloing track `t` themselves: the conductor's mute and solo on
    /// it are over, if it had any.
    pub(crate) fn unconduct(&mut self, t: usize) -> Option<Effect> {
        let id = self.ids.get(t)?;
        let at = self.conducted.iter().position(|(i, ..)| i == id)?;
        self.conducted.remove(at);
        Some(Effect::Unconduct(t))
    }

    /// Makes `index` the pattern being edited, at once.
    pub(crate) fn select_pattern(&mut self, index: usize) -> Vec<Effect> {
        if index == self.arrangement.current {
            return vec![];
        }
        let meter = self.meter();
        if !self.arrangement.select(&mut self.grid, index) {
            self.message = Some(format!("no pattern {}", index + 1));
            return vec![];
        }
        self.held.clear();
        self.take = None;
        self.clamp_selection();
        self.message = Some(format!("pattern {}", self.pattern_name()));
        let mut effects = changed(0..self.grid.tracks.len());
        // In a meter of its own: the engine has it already if it changed pattern itself.
        if self.meter() != meter {
            effects.push(self.meter());
        }
        effects
    }

    /// The pattern after or before this one, wrapping round.
    pub(crate) fn step_pattern(&mut self, by: isize) -> Vec<Effect> {
        let len = self.arrangement.patterns.len() as isize;
        let from = self.cue.unwrap_or(self.arrangement.current) as isize;
        self.go_to_pattern((from + by).rem_euclid(len) as usize)
    }

    /// Adds a pattern after this one, a copy of it or empty with tracks as long as they are
    /// now, plays it once at the end of the song, and goes to it.
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
            self.grid.tracks.iter().map(Lane::of).collect()
        } else {
            self.grid
                .tracks
                .iter()
                .map(|t| Lane {
                    steps: vec![Step::default(); t.steps.len()],
                    ..Lane::default()
                })
                .collect()
        };
        self.checkpoint();
        let conductor = match copy {
            true => (self.arrangement.conductor(self.arrangement.current))
                .cloned()
                .unwrap_or_default(),
            false => Default::default(),
        };
        let index = self.arrangement.insert(name, lanes, conductor);
        let id = PatternId(self.next_pattern_id);
        self.next_pattern_id += 1;
        self.pattern_ids
            .insert(index.min(self.pattern_ids.len()), id);
        if self.arrangement.entries.len() < MAX_ENTRIES {
            self.arrangement.entries.push(Entry {
                pattern: index,
                repeats: 1,
                mutes: 0,
            });
        }
        Ok(self.go_to_pattern(index))
    }

    /// The pattern `:bars` and `:pattern-name` change: on the song page the cursor's entry's,
    /// and elsewhere the one being edited.
    fn named_pattern(&self) -> usize {
        match self.page {
            Page::Song => (self.arrangement.entries.get(self.song_cursor()))
                .map_or(self.arrangement.current, |e| e.pattern),
            _ => self.arrangement.current,
        }
    }

    pub(crate) fn rename_pattern(&mut self, name: &str) -> Result<(), String> {
        if self.find_pattern(name).is_ok() {
            return Err(format!("there's already a pattern {name}"));
        }
        let pattern = self.named_pattern();
        if self.arrangement.patterns.get(pattern).is_none() {
            return Err("no such pattern".into());
        }
        self.checkpoint();
        if let Some(pattern) = self.arrangement.patterns.get_mut(pattern) {
            pattern.name = name.into();
        }
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
        if !self.arrangement.remove(current) {
            return Err(format!("couldn't remove pattern {gone}"));
        }
        if current < self.pattern_ids.len() {
            self.pattern_ids.remove(current);
        }
        self.cue = None;
        self.song_position = None;
        self.song.looped = None;
        self.message = Some(format!("removed pattern {gone}"));
        Ok(effects)
    }

    /// `:meter BEATS [STEPS]`: beats in a bar and steps in a beat, or with nothing, what they
    /// are. The music doesn't move: patterns keep their steps, now counted in the new bars, and a
    /// change of steps per beat changes the tempo and the conductor's tempos to keep each step as
    /// long as it was.
    pub(crate) fn set_meter(&mut self, text: &str) -> Result<Vec<Effect>, String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let number = |word: Option<&&str>, what: &str, max: u32, now: u32| match word {
            None => Ok(now),
            Some(word) => word
                .parse::<u32>()
                .ok()
                .filter(|n| (1..=max).contains(n))
                .ok_or_else(|| format!("{what}: 1 to {max}")),
        };
        let grid = &self.grid;
        let Some(first) = words.first() else {
            self.message = Some(describe(&grid.meter(), grid.steps_per_beat));
            return Ok(vec![]);
        };
        if words.len() > 2 {
            return Err("meter: BEATS (or groups, as 2+2+3), then optionally STEPS".into());
        }
        let meter = parse_meter(first)?;
        let steps = number(
            words.get(1),
            "steps",
            MAX_STEPS_PER_BEAT,
            grid.steps_per_beat,
        )?;
        // Each pattern of so many bars keeps its steps, which must make whole bars, in its own
        // meter or the song's.
        let bar_of = |p: &Pattern, song: &Meter, steps| {
            p.meter.as_ref().unwrap_or(song).steps_per_bar(steps)
        };
        let mut bars = vec![];
        for pattern in &self.arrangement.patterns {
            let Some(old) = pattern.bars else {
                bars.push(None);
                continue;
            };
            let len = old * bar_of(pattern, &grid.meter(), grid.steps_per_beat);
            bars.push(Some(whole_bars(
                pattern,
                len,
                bar_of(pattern, &meter, steps),
            )?));
        }
        // Tempos count beats, so they scale with the steps in one, keeping the steps' length.
        let scale = f64::from(grid.steps_per_beat) / f64::from(steps);
        let tempos = f64::from(MIN_BPM)..=f64::from(MAX_BPM);
        let bpm = self.bpm * scale;
        if !tempos.contains(&bpm) {
            return Err(format!(
                "that makes the tempo {bpm:.1}, outside {MIN_BPM}–{MAX_BPM}"
            ));
        }
        let rescale = |bpm: u16| -> Option<u16> {
            let new = (f64::from(bpm) * scale).round();
            tempos.contains(&new).then_some(new as u16)
        };
        let conducted = (self.arrangement.patterns.iter())
            .flat_map(|p| p.conductor.steps.iter().flatten().flatten())
            .filter_map(|g| g.tempo());
        if let Some(t) = conducted.into_iter().find(|&t| rescale(t).is_none()) {
            return Err(format!(
                "the conductor's tempo {t} would be outside {MIN_BPM}–{MAX_BPM}"
            ));
        }
        self.checkpoint();
        for (pattern, bars) in self.arrangement.patterns.iter_mut().zip(bars) {
            pattern.bars = bars;
            let gestures = pattern.conductor.steps.iter_mut().flatten().flatten();
            for gesture in gestures {
                match gesture {
                    SongGesture::Tempo(t) | SongGesture::Slide(t) => *t = rescale(*t).unwrap_or(*t),
                    _ => {}
                }
            }
        }
        self.message = Some(format!("{} at {bpm:.1} BPM", describe(&meter, steps)));
        self.grid.beats_per_bar = meter.beats;
        self.grid.grouping = meter.grouping;
        self.grid.steps_per_beat = steps;
        self.bpm = bpm;
        Ok(vec![self.meter(), Effect::Bpm(bpm)])
    }

    /// `:pattern-meter BEATS|a+b+c`: this pattern's own beats in a bar, in the song's steps;
    /// `song` for the song's again; nothing to say what it is.
    pub(crate) fn set_pattern_meter(&mut self, text: &str) -> Result<Vec<Effect>, String> {
        let index = self.arrangement.current;
        let pattern = self
            .arrangement
            .patterns
            .get(index)
            .ok_or("no such pattern")?;
        let (song, steps) = (self.grid.meter(), self.grid.steps_per_beat);
        let meter = match text.trim() {
            "" => {
                let own = pattern.meter.as_ref();
                let meter = own.unwrap_or(&song);
                let whose = if own.is_some() {
                    "its own"
                } else {
                    "the song's"
                };
                self.message = Some(format!(
                    "{} is in {whose} meter: {}",
                    pattern.name,
                    describe(meter, steps)
                ));
                return Ok(vec![]);
            }
            "song" => None,
            word => Some(parse_meter(word)?),
        };
        // So many bars keep their steps, which must make whole bars of the new meter.
        let bars = match pattern.bars {
            Some(old) => {
                let len = old * pattern.meter.as_ref().unwrap_or(&song).steps_per_bar(steps);
                let new = meter.as_ref().unwrap_or(&song).steps_per_bar(steps);
                Some(whole_bars(pattern, len, new)?)
            }
            None => None,
        };
        let message = match &meter {
            Some(m) => format!("{} has {}", pattern.name, describe(m, steps)),
            None => format!("{} is in the song's meter again", pattern.name),
        };
        self.checkpoint();
        if let Some(p) = self.arrangement.patterns.get_mut(index) {
            p.meter = meter;
            p.bars = bars;
        }
        self.message = Some(message);
        Ok(vec![self.meter()])
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
        let pattern = self.named_pattern();
        if self.arrangement.patterns.get(pattern).is_none() {
            return Err("no such pattern".into());
        }
        self.checkpoint();
        // The song page's cursor stays on its entry, as far into it as it can.
        let entry = self.song_cursor();
        let into = self.song_bar().saturating_sub(self.entry_start(entry));
        if let Some(p) = self.arrangement.patterns.get_mut(pattern) {
            p.bars = bars;
        }
        self.song.bar = self.entry_start(entry) + into;
        self.keep_on_entry(entry);
        let bar = self.arrangement.bar(&self.grid, pattern);
        let steps = (self.arrangement.steps(&self.grid, pattern)).ok_or("no such pattern")?;
        let auto = if bars.is_some() {
            ""
        } else {
            ", as long as its longest track"
        };
        let name = (self.arrangement.patterns.get(pattern)).map_or("", |p| &p.name);
        self.message = Some(format!("{name} plays {} bars{auto}", steps / bar));
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
    /// The end of the loop goes back to its start.
    fn next_position(&self, (entry, repeat): (usize, u32)) -> Option<(usize, u32)> {
        let entries = &self.arrangement.entries;
        if repeat + 1 < entries.get(entry)?.repeats {
            Some((entry, repeat + 1))
        } else if let Some((start, end)) = self.song_loop()
            && entry == end
        {
            Some((start, 0))
        } else {
            (entry + 1 < entries.len()).then_some((entry + 1, 0))
        }
    }

    /// What should happen when the playing pattern ends.
    pub fn next(&self) -> Next {
        let current = self.arrangement.current;
        let pattern = match self.song_position {
            Some(position) => match self.next_position(position) {
                Some((entry, _)) => match self.arrangement.entries.get(entry) {
                    Some(e) => e.pattern,
                    None => return Next::Stop,
                },
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

    /// The tracks the song mutes, a bit each: in the entry playing, and in the one after.
    pub fn song_mutes(&self) -> (u32, u32) {
        let Some(position) = self.song_position else {
            return (0, 0);
        };
        let mutes =
            |(entry, _): (usize, u32)| (self.arrangement.entries.get(entry)).map_or(0, |e| e.mutes);
        (
            mutes(position),
            self.next_position(position).map_or(0, mutes),
        )
    }

    /// Whether the entry playing mutes track `t`, as the song page shows with `─`.
    pub fn entry_mutes(&self, t: usize) -> bool {
        let (now, _) = self.song_mutes();
        u32::try_from(t)
            .ok()
            .and_then(|t| now.checked_shr(t))
            .is_some_and(|bits| bits & 1 == 1)
    }

    /// The playing pattern has ended and the next begun: the engine changed to `changed_to`,
    /// or played the same one again.
    pub fn boundary(&mut self, changed_to: Option<usize>) {
        self.take = None;
        if let Some(position) = self.song_position {
            // Moved on, unless the engine did something else, as when a change came too late.
            let next = self.next_position(position);
            let expected = next
                .and_then(|(entry, _)| self.arrangement.entries.get(entry))
                .map(|e| e.pattern);
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
        self.conducted.clear();
        self.song_position = None;
        self.fill = false;
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
            beats_per_bar: 4,
            grouping: vec![],
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
    fn going_to_another_pattern_changes_nothing() {
        let mut e = editor();
        run(&mut e, "pattern-new");
        e.saved("song.lattice".into());
        run(&mut e, "pattern A");
        assert_eq!(e.arrangement.current, 0);
        assert!(!e.modified(), "only another pattern open");
        run(&mut e, "bpm 100");
        assert!(e.modified());
        run(&mut e, "bpm 120");
        run(&mut e, "pattern B");
        assert!(!e.modified(), "and back again");
        e.handle(Key::Char('i'));
        e.handle(Key::Char('z'));
        assert!(e.modified(), "a note in it");
    }

    #[test]
    fn the_conductors_mutes_are_heard_until_the_song_stops_or_the_player_takes_over() {
        let mut e = editor();
        e.conducted_mix(0, Some(true), None);
        e.conducted_mix(1, None, Some(true));
        assert!(e.heard_mix(0).unwrap().mute && e.heard_mix(1).unwrap().solo);
        assert!(!e.grid.tracks[0].mix.mute, "the song's own is as it was");
        // Muting it themselves, the player takes over from the conductor: it was heard muted,
        // so it's unmuted.
        let effects = crate::command::run(&mut e, "mute").unwrap();
        assert!(effects.contains(&Effect::Unconduct(0)));
        assert!(!e.heard_mix(0).unwrap().mute);
        e.stopped();
        assert!(!e.heard_mix(1).unwrap().solo);
    }

    #[test]
    fn the_entry_playing_says_which_tracks_it_mutes() {
        let mut e = editor();
        e.arrangement.entries = vec![lattice_core::Entry {
            pattern: 0,
            repeats: 1,
            mutes: 0b10,
        }];
        assert!(!e.entry_mutes(1), "only while the song plays");
        e.start_song(0);
        assert!(!e.entry_mutes(0) && e.entry_mutes(1));
        assert!(!e.entry_mutes(40), "no such track");
        e.stopped();
        assert!(!e.entry_mutes(1));
    }

    #[test]
    fn a_pattern_mute_stays_with_its_pattern() {
        let mut e = editor();
        e.handle(Key::Char('t'));
        assert_eq!(e.handle(Key::Char('M')), [Effect::TrackChanged(0)]);
        assert!(e.grid.tracks[0].pattern_mute);
        run(&mut e, "pn");
        assert!(
            !e.grid.tracks[0].pattern_mute,
            "a new pattern plays everything"
        );
        run(&mut e, "pat A");
        assert!(e.grid.tracks[0].pattern_mute);
        assert!(!e.arrangement.lanes(&e.grid, 1).unwrap()[0].mute);
        // And it's undone like any edit.
        e.handle(Key::Char('t'));
        e.handle(Key::Char('M'));
        assert!(!e.grid.tracks[0].pattern_mute);
        e.handle(Key::Char('u'));
        assert!(e.grid.tracks[0].pattern_mute);
    }

    #[test]
    fn patterns_share_tracks_and_keep_their_own_steps() {
        let mut e = editor();
        e.handle(Key::Char('i'));
        e.handle(Key::Char('z'));
        assert_eq!(e.pattern_name(), "A");
        assert_eq!(e.pattern_steps(), 16, "the longest track");

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
        assert_eq!(e.pattern_steps(), 16);
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
                mutes: 0,
            },
            Entry {
                pattern: 0,
                repeats: 1,
                mutes: 0,
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
