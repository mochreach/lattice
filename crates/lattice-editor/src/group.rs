//! Grouping tracks: `tg` groups the tracks the selections reach, or ungroups the group they're
//! in, `tz` folds and unfolds it, and `:group-name` renames it. The numpad's pads play the
//! group's tracks (see `pads.rs`).

use lattice_core::{Group, MAX_GROUPS};

use crate::{Editor, Effect};

impl Editor {
    /// Groups every track from the first the selections reach to the last, or, if they're all in
    /// one group, ungroups it.
    pub(crate) fn toggle_group(&mut self) -> Vec<Effect> {
        let tracks = self.selected_tracks();
        let (Some(&first), Some(&last)) = (tracks.first(), tracks.last()) else {
            self.message = Some("no tracks to group".into());
            return vec![];
        };
        let groups = &self.grid.groups;
        if let Some(g) = self.grid.group_of(first)
            && let Some(group) = groups.get(g)
            && group.contains(last)
        {
            let name = group.name.clone();
            self.checkpoint();
            self.grid.groups.remove(g);
            self.message = Some(format!("ungrouped {name}"));
            return vec![Effect::MixerChanged];
        }
        if let Some(g) = groups
            .iter()
            .find(|g| g.first <= last && first < g.first + g.len)
        {
            self.message = Some(format!("those tracks overlap the group {}", g.name));
            return vec![];
        }
        if groups.len() == MAX_GROUPS {
            self.message = Some(format!("at most {MAX_GROUPS} groups"));
            return vec![];
        }
        // The first `group N` not already taken: of one more than there are groups, one's free.
        let name = (groups.len() + 1..=2 * groups.len() + 1)
            .map(|n| format!("group {n}"))
            .find(|name| groups.iter().all(|g| g.name != *name))
            .unwrap_or_default();
        // At most the length, so the insert can't fail.
        let at = groups.partition_point(|g| g.first < first);
        self.checkpoint();
        self.message = Some(format!("grouped as {name}; :group-name renames it"));
        self.grid
            .groups
            .insert(at, Group::new(name, first..last + 1));
        vec![Effect::MixerChanged]
    }

    /// Folds or unfolds every group the selections reach.
    pub(crate) fn toggle_fold(&mut self) -> Vec<Effect> {
        let mut groups: Vec<usize> = self
            .selected_tracks()
            .into_iter()
            .filter_map(|t| self.grid.group_of(t))
            .collect();
        groups.dedup();
        if groups.is_empty() {
            self.message = Some("only groups fold; tg makes one".into());
            return vec![];
        }
        // Together: if any is unfolded, they all fold.
        let fold = (groups.iter())
            .filter_map(|&g| self.grid.groups.get(g))
            .any(|g| !g.folded);
        self.checkpoint();
        for g in groups {
            if let Some(group) = self.grid.groups.get_mut(g) {
                group.folded = fold;
            }
        }
        vec![]
    }

    /// Whether the cursor's track is in a folded group, where the number row stands in for the
    /// pads in insert mode, for terminals that don't tell the numpad apart. The conductor's left
    /// of the first track, not in its group, and takes digits for its gestures.
    pub fn in_folded_group(&self) -> bool {
        !self.in_conductor()
            && self
                .grid
                .group_of(self.cursor().track)
                .and_then(|g| self.grid.groups.get(g))
                .is_some_and(|g| g.folded)
    }

    /// Renames the group the cursor's track is in.
    pub(crate) fn rename_group(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        let g = self
            .grid
            .group_of(self.cursor().track)
            .ok_or("this track isn't in a group; tg makes one")?;
        self.checkpoint();
        let group = self.grid.groups.get_mut(g).ok_or("no such group")?;
        group.name = name.into();
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use crate::{Editor, Key};
    use lattice_core::{Grid, Track};

    fn editor() -> Editor {
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: (0..5)
                .map(|i| Track {
                    name: i.to_string(),
                    steps: vec![Default::default(); 16],
                    ..Default::default()
                })
                .collect(),
            groups: vec![],
        };
        Editor::new(grid, 120.0)
    }

    fn keys(e: &mut Editor, s: &str) {
        for c in s.chars() {
            e.handle(match c {
                '\n' => Key::Enter,
                '\x1b' => Key::Esc,
                c => Key::Char(c),
            });
        }
    }

    fn groups(e: &Editor) -> Vec<(String, usize, usize)> {
        e.grid
            .groups
            .iter()
            .map(|g| (g.name.clone(), g.first, g.len))
            .collect()
    }

    #[test]
    fn tg_groups_and_ungroups_the_selected_tracks() {
        let mut e = editor();
        keys(&mut e, "tlvtltltg");
        assert_eq!(groups(&e), [("group 1".into(), 1, 3)]);
        keys(&mut e, ":group-name kit\n");
        assert_eq!(groups(&e)[0].0, "kit");

        // Overlapping another group isn't allowed.
        keys(&mut e, "\x1bggghvtltg");
        assert_eq!(groups(&e).len(), 1);
        assert!(
            e.message
                .as_deref()
                .unwrap()
                .contains("overlap the group kit")
        );

        // From inside the group, it comes apart, and undo puts it back.
        keys(&mut e, "\x1bglhhtg");
        assert!(groups(&e).is_empty());
        keys(&mut e, "u");
        assert_eq!(groups(&e), [("kit".into(), 1, 3)]);
    }

    #[test]
    fn a_group_has_a_bus_strip_on_the_mixer_page() {
        use crate::Strip;
        let mut e = editor();
        keys(&mut e, "vltg\x1b");
        assert_eq!(e.strips(), 5 + 1 + 2);
        assert_eq!(e.strip(5), Strip::Group(0));
        assert_eq!(e.strip(6), Strip::Send);
        // Its level and effects, set from the mixer page.
        keys(&mut e, "gmglhh:level -3\n:fx-add comp\n");
        let bus = &e.grid.groups[0].bus;
        assert_eq!(bus.level, -3.0);
        assert_eq!(bus.effects.len(), 1);
        keys(&mut e, "u");
        assert!(e.grid.groups[0].bus.effects.is_empty(), "undoable");
    }

    #[test]
    fn tz_folds_and_unfolds() {
        let mut e = editor();
        keys(&mut e, "tz");
        assert!(e.message.as_deref().unwrap().contains("only groups fold"));
        keys(&mut e, "vtltg\x1btz");
        assert!(e.grid.groups[0].folded);
        keys(&mut e, "tz");
        assert!(!e.grid.groups[0].folded);
    }

    #[test]
    fn groups_follow_tracks_as_they_move() {
        let mut e = editor();
        keys(&mut e, "tlvtltg\x1b");
        assert_eq!(groups(&e), [("group 1".into(), 1, 2)]);
        // Removing a track before the group moves it along; adding one inside grows it.
        keys(&mut e, "ghtd");
        assert_eq!(groups(&e), [("group 1".into(), 0, 2)]);
        keys(&mut e, "ta");
        assert_eq!(groups(&e), [("group 1".into(), 0, 3)]);
        keys(&mut e, "uu");
        assert_eq!(groups(&e), [("group 1".into(), 1, 2)]);
    }

    #[test]
    fn pads_play_the_groups_tracks_and_enter_notes_in_insert_mode() {
        use crate::Effect;
        use lattice_core::{DRUM_ROOT, Group};

        let mut e = editor();
        let pitch = DRUM_ROOT;
        // Outside a group they play a scale instead (see `pads.rs`).
        assert_eq!(
            e.handle(Key::Pad(1)),
            [Effect::Preview {
                track: 0,
                pitch: 60
            }]
        );

        e.grid.groups = vec![Group::new("kit", 1..4)];
        keys(&mut e, "tl");
        assert_eq!(e.handle(Key::Pad(2)), [Effect::Preview { track: 2, pitch }]);
        assert!(e.handle(Key::Pad(4)).is_empty(), "past the group's end");
        assert!(
            e.grid
                .tracks
                .iter()
                .all(|t| t.steps.iter().all(|s| s.is_empty()))
        );

        // In insert mode they enter notes on the cursor's step, without moving on.
        keys(&mut e, "jji");
        e.handle(Key::Pad(1));
        e.handle(Key::Pad(3));
        assert_eq!(e.grid.tracks[1].steps[2].note.unwrap().pitch, pitch);
        assert_eq!(e.grid.tracks[3].steps[2].note.unwrap().pitch, pitch);
        assert_eq!(e.cursor().step, 2);

        // Unfolded, the number row is still the piano; folded, it stands in for the pads.
        keys(&mut e, "2");
        assert!(e.grid.tracks[2].steps[2].is_empty());
        e.grid.groups[0].folded = true;
        keys(&mut e, "\x1bk");
        keys(&mut e, "i2");
        assert_eq!(e.grid.tracks[2].steps[2].note.unwrap().pitch, pitch);

        // On the command line, the numpad types digits.
        keys(&mut e, "\x1b:len ");
        e.handle(Key::Pad(8));
        keys(&mut e, "\n");
        assert_eq!(e.grid.tracks[1].steps.len(), 8);
    }

    #[test]
    fn the_conductor_left_of_a_folded_group_takes_digits() {
        use lattice_core::{Gesture, Group};

        let mut e = editor();
        e.grid.groups = vec![Group::new("kit", 0..2)];
        e.grid.groups[0].folded = true;
        keys(&mut e, "fgiM1\n");
        assert_eq!(
            e.conductor().unwrap().get(0, 0),
            Some(Gesture::Mute { track: 1, on: true })
        );
    }
}
