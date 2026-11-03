//! Groups: named runs of adjacent tracks, such as a drum kit with one drum on each track. Each
//! track stays a whole track; the group adds a bus its tracks mix into, and can be folded down to
//! narrow columns.
//!
//! Groups are kept by the tracks' indices, so tracks are inserted, removed and moved through
//! [`Grid`]'s methods, which keep the groups in line.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::fx::Bus;
use crate::song::{Grid, Track};

/// Most groups a grid can have. The engine preallocates a bus for each.
pub const MAX_GROUPS: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    /// The first track's index.
    pub first: usize,
    /// How many tracks, at least one.
    pub len: usize,
    /// Shown as narrow columns, a drum machine's steps.
    #[serde(default, skip_serializing_if = "is_false")]
    pub folded: bool,
    /// What the tracks mix into before the master: a level and effects.
    #[serde(default, skip_serializing_if = "Bus::is_default")]
    pub bus: Bus,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Group {
    pub fn new(name: impl Into<String>, tracks: Range<usize>) -> Self {
        Self {
            name: name.into(),
            first: tracks.start,
            len: tracks.len(),
            folded: false,
            bus: Bus::default(),
        }
    }

    pub fn tracks(&self) -> Range<usize> {
        self.first..self.first + self.len
    }

    pub fn contains(&self, track: usize) -> bool {
        self.tracks().contains(&track)
    }
}

impl Grid {
    /// The index in `groups` of the group `track` is in.
    pub fn group_of(&self, track: usize) -> Option<usize> {
        self.groups.iter().position(|g| g.contains(track))
    }

    /// Inserts a track at `index`. Inserted inside a group, between two of its tracks, it joins
    /// the group; at either end it stays outside. False, changing nothing, if `index` is past
    /// the end.
    #[must_use]
    pub fn insert_track(&mut self, index: usize, track: Track) -> bool {
        if index > self.tracks.len() {
            return false;
        }
        self.tracks.insert(index, track);
        for group in &mut self.groups {
            if index <= group.first {
                group.first += 1;
            } else if index < group.first + group.len {
                group.len += 1;
            }
        }
        true
    }

    /// Removes the track at `index`, and its group if it was the group's last track. `None`
    /// if there's no such track.
    pub fn remove_track(&mut self, index: usize) -> Option<Track> {
        if index >= self.tracks.len() {
            return None;
        }
        let track = self.tracks.remove(index);
        for group in &mut self.groups {
            if index < group.first {
                group.first -= 1;
            } else if group.contains(index) {
                group.len -= 1;
            }
        }
        self.groups.retain(|g| g.len > 0);
        Some(track)
    }

    /// Moves a track to `to`. It stays in its group while it moves within it, leaves it when it
    /// moves past either end, and joins a group it lands inside.
    /// False, changing nothing, if either isn't a track.
    #[must_use]
    pub fn move_track(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tracks.len() || to >= self.tracks.len() {
            return false;
        }
        let group = self.group_of(from);
        let track = self.tracks.remove(from);
        for (i, g) in self.groups.iter_mut().enumerate() {
            if from < g.first {
                g.first -= 1;
            } else if Some(i) == group {
                g.len -= 1;
            }
        }
        self.tracks.insert(to, track);
        for (i, g) in self.groups.iter_mut().enumerate() {
            let end = g.first + g.len;
            // A member may land at either end of what's left of its group, and stays in.
            let stays = Some(i) == group && (g.first..=end).contains(&to);
            if stays || (g.first < to && to < end) {
                g.len += 1;
            } else if to <= g.first {
                g.first += 1;
            }
        }
        self.groups.retain(|g| g.len > 0);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Six tracks named by their starting index, with tracks 2 to 4 in a group.
    fn grid() -> Grid {
        Grid {
            name: "test".into(),
            steps_per_beat: 4,
            beats_per_bar: 4,
            grouping: vec![],
            tracks: (0..6)
                .map(|i| Track {
                    name: i.to_string(),
                    ..Default::default()
                })
                .collect(),
            groups: vec![Group::new("kit", 2..5)],
        }
    }

    /// The group's tracks, by name.
    fn members(grid: &Grid) -> Vec<String> {
        grid.groups
            .iter()
            .flat_map(|g| grid.tracks[g.tracks()].iter().map(|t| t.name.clone()))
            .collect()
    }

    #[test]
    fn inserting_inside_joins_and_at_the_ends_stays_out() {
        let mut g = grid();
        assert!(g.insert_track(3, Track::default()));
        assert_eq!(g.groups[0].tracks(), 2..6);
        assert!(g.insert_track(2, Track::default()));
        assert_eq!(g.groups[0].tracks(), 3..7);
        assert!(g.insert_track(7, Track::default()));
        assert_eq!(g.groups[0].tracks(), 3..7);
    }

    #[test]
    fn removing_shrinks_and_empties_the_group() {
        let mut g = grid();
        g.remove_track(0).unwrap();
        assert_eq!(members(&g), ["2", "3", "4"]);
        for _ in 0..3 {
            g.remove_track(1).unwrap();
        }
        assert!(g.groups.is_empty());
    }

    #[test]
    fn moving_within_keeps_members_and_past_the_ends_leaves() {
        let mut g = grid();
        // The first member, one along, and back again.
        assert!(g.move_track(2, 3));
        assert_eq!(members(&g), ["3", "2", "4"]);
        assert!(g.move_track(3, 2));
        assert_eq!(members(&g), ["2", "3", "4"]);
        // The last member, one out.
        assert!(g.move_track(4, 5));
        assert_eq!(members(&g), ["2", "3"]);
        // A track from outside, into the middle.
        assert!(g.move_track(0, 2));
        assert_eq!(members(&g), ["2", "0", "3"]);
        // The first member, one out to the left.
        assert!(g.move_track(1, 0));
        assert_eq!(members(&g), ["0", "3"]);
    }

    #[test]
    fn a_two_track_group_survives_its_members_swapping() {
        let mut g = grid();
        g.groups = vec![Group::new("pair", 1..3)];
        assert!(g.move_track(1, 2));
        assert_eq!(members(&g), ["2", "1"]);
        assert!(g.move_track(2, 1));
        assert_eq!(members(&g), ["1", "2"]);
    }
}
