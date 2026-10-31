//! Grouping tracks: `tg` groups the tracks the selections reach, or ungroups the group they're
//! in, and `:group-name` renames it.

use lattice_core::{Group, MAX_GROUPS};

use crate::{Editor, Effect};

impl Editor {
    /// Groups every track from the first the selections reach to the last, or, if they're all in
    /// one group, ungroups it.
    pub(crate) fn toggle_group(&mut self) -> Vec<Effect> {
        let tracks = self.selected_tracks();
        let (first, last) = (tracks[0], tracks[tracks.len() - 1]);
        let groups = &self.grid.groups;
        if let Some(g) = self.grid.group_of(first)
            && groups[g].contains(last)
        {
            let name = groups[g].name.clone();
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
        // The first `group N` not already taken.
        let name = (groups.len() + 1..)
            .map(|n| format!("group {n}"))
            .find(|name| groups.iter().all(|g| g.name != *name))
            .unwrap();
        let at = groups.partition_point(|g| g.first < first);
        self.checkpoint();
        self.message = Some(format!("grouped as {name}; :group-name renames it"));
        self.grid
            .groups
            .insert(at, Group::new(name, first..last + 1));
        vec![Effect::MixerChanged]
    }

    /// Renames the group the cursor's track is in.
    pub(crate) fn rename_group(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        let g = self
            .grid
            .group_of(self.cursor().track)
            .ok_or("this track isn't in a group; tg makes one")?;
        self.checkpoint();
        self.grid.groups[g].name = name.into();
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
        keys(&mut e, "lvlltg");
        assert_eq!(groups(&e), [("group 1".into(), 1, 3)]);
        keys(&mut e, ":group-name kit\n");
        assert_eq!(groups(&e)[0].0, "kit");

        // Overlapping another group isn't allowed.
        keys(&mut e, "\x1bggghvltg");
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
    fn groups_follow_tracks_as_they_move() {
        let mut e = editor();
        keys(&mut e, "lvltg\x1b");
        assert_eq!(groups(&e), [("group 1".into(), 1, 2)]);
        // Removing a track before the group moves it along; adding one inside grows it.
        keys(&mut e, "ghtd");
        assert_eq!(groups(&e), [("group 1".into(), 0, 2)]);
        keys(&mut e, "ta");
        assert_eq!(groups(&e), [("group 1".into(), 0, 3)]);
        keys(&mut e, "uu");
        assert_eq!(groups(&e), [("group 1".into(), 1, 2)]);
    }
}
