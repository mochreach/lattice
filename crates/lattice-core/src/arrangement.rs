//! Patterns and the arrangement. Every pattern shares the grid's tracks (instruments, mixer,
//! effects, groups) and has its own steps and rules for each: a lane per track. The pattern
//! being edited keeps its lanes in the grid's tracks, so everything that works on tracks works
//! on it; the others keep theirs here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::song::{Grid, MAX_STEPS, Step, Track};
use crate::transform::Rule;

/// Most patterns a song can have.
pub const MAX_PATTERNS: usize = 64;
/// Most entries in the arrangement.
pub const MAX_ENTRIES: usize = 256;
/// Most bars a pattern can last, and most times an entry repeats.
pub const MAX_BARS: u32 = 256;
pub const MAX_REPEATS: u32 = 64;
/// Beats in a bar.
pub const BEATS_PER_BAR: u32 = 4;

/// One pattern's steps and rules for one track. Saved sparsely, as tracks are.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SparseLane", into = "SparseLane")]
pub struct Lane {
    pub steps: Vec<Step>,
    pub rules: Vec<Rule>,
    /// Muted in this pattern: see [`Track::pattern_mute`](crate::Track::pattern_mute).
    pub mute: bool,
}

impl Lane {
    /// The lane `track` holds, for the pattern being edited.
    pub fn of(track: &Track) -> Self {
        Self {
            steps: track.steps.clone(),
            rules: track.rules.clone(),
            mute: track.pattern_mute,
        }
    }

    /// Puts the lane in `track`, giving back the one it held.
    pub fn swap(&mut self, track: &mut Track) {
        std::mem::swap(&mut self.steps, &mut track.steps);
        std::mem::swap(&mut self.rules, &mut track.rules);
        std::mem::swap(&mut self.mute, &mut track.pattern_mute);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub name: String,
    /// How long it plays before the next, or `None` for as long as its longest track, rounded up
    /// to a whole bar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bars: Option<u32>,
    /// A lane per track, or none for the pattern being edited, whose lanes are in the tracks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lanes: Vec<Lane>,
}

/// One entry in the arrangement: a pattern, played a number of times running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub pattern: usize,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub repeats: u32,
}

fn one() -> u32 {
    1
}

fn is_one(n: &u32) -> bool {
    *n == 1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arrangement {
    pub patterns: Vec<Pattern>,
    /// The pattern being edited, whose lanes are in the grid's tracks.
    pub current: usize,
    /// The song, in order.
    pub entries: Vec<Entry>,
}

impl Default for Arrangement {
    /// One pattern, played once.
    fn default() -> Self {
        Self {
            patterns: vec![Pattern {
                name: pattern_name(0),
                bars: None,
                lanes: vec![],
            }],
            current: 0,
            entries: vec![Entry {
                pattern: 0,
                repeats: 1,
            }],
        }
    }
}

/// A pattern's default name: A to Z, then A2 and on.
pub fn pattern_name(index: usize) -> String {
    let letter = char::from(b'A' + (index % 26) as u8);
    match index / 26 {
        0 => letter.to_string(),
        n => format!("{letter}{}", n + 1),
    }
}

impl Arrangement {
    /// Makes `index` the pattern being edited: the current pattern's lanes go back here, and
    /// `index`'s go into the tracks.
    pub fn select(&mut self, grid: &mut Grid, index: usize) {
        if index == self.current || index >= self.patterns.len() {
            return;
        }
        let mut lanes = std::mem::take(&mut self.patterns[index].lanes);
        for (track, lane) in grid.tracks.iter_mut().zip(&mut lanes) {
            lane.swap(track);
        }
        self.patterns[self.current].lanes = lanes;
        self.current = index;
    }

    /// Pattern `index`'s lanes, wherever they are.
    pub fn lanes(&self, grid: &Grid, index: usize) -> Vec<Lane> {
        if index == self.current {
            grid.tracks.iter().map(Lane::of).collect()
        } else {
            self.patterns[index].lanes.clone()
        }
    }

    /// How many steps pattern `index` plays for.
    pub fn steps(&self, grid: &Grid, index: usize) -> u64 {
        let bar = u64::from(BEATS_PER_BAR * grid.steps_per_beat.max(1));
        match self.patterns[index].bars {
            Some(bars) => u64::from(bars) * bar,
            None => {
                let longest = if index == self.current {
                    grid.tracks.iter().map(|t| t.steps.len()).max()
                } else {
                    self.patterns[index]
                        .lanes
                        .iter()
                        .map(|l| l.steps.len())
                        .max()
                };
                (longest.unwrap_or(0) as u64).div_ceil(bar).max(1) * bar
            }
        }
    }

    /// A new pattern after the current one, with `lanes`, named `name` or else the first
    /// letter not taken. Doesn't select it. Returns its index.
    pub fn insert(&mut self, name: Option<String>, lanes: Vec<Lane>) -> usize {
        let index = self.current + 1;
        let name = name.unwrap_or_else(|| {
            (0..)
                .map(pattern_name)
                .find(|n| self.patterns.iter().all(|p| &p.name != n))
                .expect("names never run out")
        });
        let bars = self.patterns[self.current].bars;
        self.patterns.insert(index, Pattern { name, bars, lanes });
        for entry in &mut self.entries {
            if entry.pattern >= index {
                entry.pattern += 1;
            }
        }
        index
    }

    /// Removes pattern `index`, which mustn't be the current one or the last, and the entries
    /// that play it.
    pub fn remove(&mut self, index: usize) {
        assert!(index != self.current && self.patterns.len() > 1);
        self.patterns.remove(index);
        if self.current > index {
            self.current -= 1;
        }
        self.entries.retain(|e| e.pattern != index);
        for entry in &mut self.entries {
            if entry.pattern > index {
                entry.pattern -= 1;
            }
        }
    }

    /// A track was inserted at `index`, `len` steps long: the other patterns get an empty lane
    /// for it.
    pub fn insert_track(&mut self, index: usize, len: usize) {
        for pattern in self.others_mut() {
            let lane = Lane {
                steps: vec![Step::default(); len],
                ..Lane::default()
            };
            pattern.lanes.insert(index, lane);
        }
    }

    /// The track at `index` was removed: so are its lanes.
    pub fn remove_track(&mut self, index: usize) {
        for pattern in self.others_mut() {
            pattern.lanes.remove(index);
        }
    }

    /// The track at `from` moved to `to`: so do its lanes.
    pub fn move_track(&mut self, from: usize, to: usize) {
        for pattern in self.others_mut() {
            let lane = pattern.lanes.remove(from);
            pattern.lanes.insert(to, lane);
        }
    }

    fn others_mut(&mut self) -> impl Iterator<Item = &mut Pattern> {
        let current = self.current;
        self.patterns
            .iter_mut()
            .enumerate()
            .filter(move |(i, _)| *i != current)
            .map(|(_, p)| p)
    }

    /// Catches what the types allow but the editor and engine don't.
    pub fn check(&self, tracks: usize) -> Result<(), String> {
        if !(1..=MAX_PATTERNS).contains(&self.patterns.len()) {
            return Err(format!("a song has 1 to {MAX_PATTERNS} patterns"));
        }
        if self.current >= self.patterns.len() {
            return Err(format!("pattern {} doesn't exist", self.current));
        }
        if self.entries.len() > MAX_ENTRIES {
            return Err(format!(
                "the arrangement has more than {MAX_ENTRIES} entries"
            ));
        }
        for entry in &self.entries {
            if entry.pattern >= self.patterns.len() {
                return Err(format!(
                    "the arrangement plays pattern {}, which doesn't exist",
                    entry.pattern
                ));
            }
            if !(1..=MAX_REPEATS).contains(&entry.repeats) {
                return Err(format!("entries repeat 1 to {MAX_REPEATS} times"));
            }
        }
        for (i, pattern) in self.patterns.iter().enumerate() {
            if pattern.bars.is_some_and(|b| !(1..=MAX_BARS).contains(&b)) {
                return Err(format!(
                    "pattern {:?} lasts 1 to {MAX_BARS} bars",
                    pattern.name
                ));
            }
            let lanes = if i == self.current { 0 } else { tracks };
            if pattern.lanes.len() != lanes {
                return Err(format!(
                    "pattern {:?} has {} lanes for {tracks} tracks",
                    pattern.name,
                    pattern.lanes.len()
                ));
            }
        }
        Ok(())
    }
}

/// How a [`Lane`] is written to a file.
#[derive(Serialize, Deserialize)]
#[serde(rename = "Lane")]
struct SparseLane {
    length: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    steps: BTreeMap<usize, Step>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "is_false")]
    mute: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

impl From<Lane> for SparseLane {
    fn from(lane: Lane) -> Self {
        Self {
            length: lane.steps.len(),
            steps: lane
                .steps
                .into_iter()
                .enumerate()
                .filter(|(_, s)| !s.is_empty())
                .collect(),
            rules: lane.rules,
            mute: lane.mute,
        }
    }
}

impl TryFrom<SparseLane> for Lane {
    type Error = String;

    fn try_from(sparse: SparseLane) -> Result<Self, String> {
        if !(1..=MAX_STEPS).contains(&sparse.length) {
            return Err(format!(
                "lanes have 1 to {MAX_STEPS} steps, not {}",
                sparse.length
            ));
        }
        let mut steps = vec![Step::default(); sparse.length];
        for (index, step) in sparse.steps {
            *steps
                .get_mut(index)
                .ok_or_else(|| format!("a lane has a step at {index}, past its length"))? = step;
        }
        Ok(Self {
            steps,
            rules: sparse.rules,
            mute: sparse.mute,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::Track;

    fn grid(lens: &[usize]) -> Grid {
        Grid {
            name: "g".into(),
            steps_per_beat: 4,
            tracks: lens
                .iter()
                .map(|&len| Track {
                    name: "t".into(),
                    steps: vec![Step::default(); len],
                    ..Track::default()
                })
                .collect(),
            groups: vec![],
        }
    }

    #[test]
    fn selecting_a_pattern_swaps_its_lanes_into_the_tracks() {
        let mut g = grid(&[16, 8]);
        g.tracks[0].steps[0] = Step::note(60);
        let mut a = Arrangement::default();
        let b = a.insert(
            None,
            vec![
                Lane {
                    mute: false,
                    steps: vec![Step::note(40); 4],
                    rules: vec![]
                };
                2
            ],
        );
        assert_eq!(a.patterns[b].name, "B");
        a.select(&mut g, b);
        assert_eq!(g.tracks[0].steps, vec![Step::note(40); 4]);
        assert!(a.patterns[b].lanes.is_empty());
        assert_eq!(a.patterns[0].lanes[0].steps[0], Step::note(60));
        a.check(2).unwrap();
        a.select(&mut g, 0);
        assert_eq!(g.tracks[0].steps.len(), 16);
        assert_eq!(a.lanes(&g, 1)[1].steps.len(), 4);
    }

    #[test]
    fn tracks_added_moved_and_removed_follow_into_every_pattern() {
        let mut a = Arrangement::default();
        let lane = |len| Lane {
            mute: false,
            steps: vec![Step::default(); len],
            rules: vec![],
        };
        a.insert(None, vec![lane(1), lane(2)]);
        a.insert_track(1, 5);
        assert_eq!(
            a.patterns[1]
                .lanes
                .iter()
                .map(|l| l.steps.len())
                .collect::<Vec<_>>(),
            [1, 5, 2]
        );
        a.move_track(0, 2);
        assert_eq!(
            a.patterns[1]
                .lanes
                .iter()
                .map(|l| l.steps.len())
                .collect::<Vec<_>>(),
            [5, 2, 1]
        );
        a.remove_track(1);
        assert_eq!(
            a.patterns[1]
                .lanes
                .iter()
                .map(|l| l.steps.len())
                .collect::<Vec<_>>(),
            [5, 1]
        );
        assert!(
            a.patterns[0].lanes.is_empty(),
            "the current pattern's lanes are the tracks"
        );
    }

    #[test]
    fn a_pattern_lasts_its_bars_or_its_longest_track() {
        let mut a = Arrangement::default();
        assert_eq!(a.steps(&grid(&[16, 12]), 0), 16);
        assert_eq!(a.steps(&grid(&[7, 20]), 0), 32, "rounded up to whole bars");
        let g = grid(&[16, 12]);
        a.patterns[0].bars = Some(2);
        assert_eq!(a.steps(&g, 0), 32);
    }

    #[test]
    fn removing_a_pattern_takes_its_entries() {
        let mut a = Arrangement::default();
        a.insert(None, vec![]);
        a.entries = vec![
            Entry {
                pattern: 1,
                repeats: 2,
            },
            Entry {
                pattern: 0,
                repeats: 1,
            },
            Entry {
                pattern: 1,
                repeats: 1,
            },
        ];
        a.remove(1);
        assert_eq!(
            a.entries,
            [Entry {
                pattern: 0,
                repeats: 1
            }]
        );
        assert_eq!(pattern_name(27), "B2");
    }
}
