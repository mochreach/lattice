//! Macros: knobs on a track that each turn several of its settings, a depth each (see
//! [`lattice_core::macros`]). `:macro NAME` adds one, and `:map NAME [DEPTH]` on a setting, on
//! the mixer or fx page, has the macro turn it. They're on the fx page, in a column left of the
//! lanes, where `C-a` `C-x` turn a macro, or on a setting under it, change its depth, `d`
//! removes either, and locking works as for any setting.

use lattice_core::macros::DEPTH_INFO;
use lattice_core::{Kind, Lock, MAX_MACROS, MAX_TARGETS, Macro, MacroTarget, Track};

use crate::mixer::{Page, Row, Strip};
use crate::{Editor, Effect};

/// A new target's depth, unless `:map` says.
const DEPTH: f32 = 0.5;

/// What a target turns, as `cutoff` on the instrument or `reverb.mix` on an effect.
pub fn target_name(track: &Track, target: &MacroTarget) -> String {
    let param = |params: &'static [lattice_core::ParamInfo]| {
        params
            .get(usize::from(target.param))
            .map_or("?", |p| p.name)
    };
    match target.fx {
        None => param(track.instrument.params()).into(),
        Some(fx) => match track.effects.get(usize::from(fx)) {
            Some(effect) => format!("{}.{}", effect.kind.name(), param(effect.params())),
            None => "?".into(),
        },
    }
}

impl Editor {
    /// The track the macro commands work on: the mixer's or fx page's, or the pattern cursor's.
    pub(crate) fn macro_track(&self) -> Result<usize, String> {
        match self.page {
            Page::Mixer | Page::Fx => match self.fx_strip() {
                Strip::Track(t) => Ok(t),
                _ => Err("macros are on tracks; buses have none".into()),
            },
            _ => Ok(self.cursor().track),
        }
    }

    /// The macro called `name` on track `t`, by name or by number from 1.
    fn find_macro(&self, t: usize, name: &str) -> Option<usize> {
        let macros = &self.grid.tracks[t].macros;
        macros.iter().position(|m| m.name == name).or_else(|| {
            let n: usize = name.parse().ok()?;
            (1..=macros.len()).contains(&n).then(|| n - 1)
        })
    }

    /// `:macro NAME`: adds a macro to the track.
    pub(crate) fn add_macro(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        let t = self.macro_track()?;
        let name = macro_name(name)?;
        let track = &self.grid.tracks[t];
        if track.macros.iter().any(|m| m.name == name) {
            return Err(format!("{} already has a macro called {name}", track.name));
        }
        if track.macros.len() == MAX_MACROS {
            return Err(format!("a track has at most {MAX_MACROS} macros"));
        }
        self.checkpoint();
        self.grid.tracks[t].macros.push(Macro::new(&name));
        self.message = Some(format!(
            "added {name}: :map {name} on a setting to have it turn it"
        ));
        if self.page == Page::Fx {
            self.goto_macro(self.grid.tracks[t].macros.len() - 1);
        }
        Ok(vec![Effect::TrackChanged(t)])
    }

    /// The track's setting under the mixer cursor, or the fx page's, as the effect it's on (or
    /// `None` for the instrument) and its id.
    fn setting_under_cursor(&self) -> Option<(usize, Option<u8>, u8)> {
        if !matches!(self.page, Page::Mixer | Page::Fx) {
            return None;
        }
        let Row::Param(p) = self.mixer_cursor().row else {
            return None;
        };
        if self.page == Page::Fx && !self.on_fx_param() {
            return None;
        }
        let shown = self.shown()?;
        let Strip::Track(t) = shown.strip else {
            return None;
        };
        let id = shown.params.get(p)?.id as u8;
        Some((t, shown.slot.map(|s| s as u8), id))
    }

    /// `:map NAME [DEPTH]`: has the macro `NAME` turn the setting under the cursor, by `DEPTH`
    /// of its range at full turn, from -1 to 1. Adds the macro if there's none of that name,
    /// and changes the depth if it already turns the setting.
    pub(crate) fn map_macro(&mut self, args: &str) -> Result<Vec<Effect>, String> {
        let mut words = args.split_whitespace();
        let word = words.next().unwrap_or("");
        let depth = match words.next() {
            Some(word) => word
                .parse::<f32>()
                .ok()
                .filter(|d| (-1.0..=1.0).contains(d))
                .ok_or("the depth goes from -1 to 1: the share of the setting's range")?,
            None => DEPTH,
        };
        let (t, fx, param) = self.setting_under_cursor().ok_or(
            "map works on a track's setting: move onto one on the mixer (gm) or fx page (gf)",
        )?;
        // An existing macro by name or number, or a new one's name.
        let index = self.find_macro(t, word);
        let name = match index {
            Some(m) => self.grid.tracks[t].macros[m].name.clone(),
            None => macro_name(word)?,
        };
        let macros = &self.grid.tracks[t].macros;
        let existing = index.and_then(|m| macros[m].targets.iter().position(|g| g.is(fx, param)));
        if index.is_none() && macros.len() == MAX_MACROS {
            return Err(format!("a track has at most {MAX_MACROS} macros"));
        }
        if let Some(m) = index
            && existing.is_none()
            && macros[m].targets.len() == MAX_TARGETS
        {
            return Err(format!("a macro turns at most {MAX_TARGETS} settings"));
        }
        self.checkpoint();
        let macros = &mut self.grid.tracks[t].macros;
        let m = index.unwrap_or_else(|| {
            macros.push(Macro::new(&name));
            macros.len() - 1
        });
        let target = MacroTarget { fx, param, depth };
        match existing {
            Some(i) => macros[m].targets[i] = target,
            None => macros[m].targets.push(target),
        }
        let track = &self.grid.tracks[t];
        self.message = Some(format!(
            "{name} turns {} by {depth:+.2}",
            target_name(track, &target)
        ));
        Ok(vec![Effect::TrackChanged(t)])
    }

    /// `:unmap NAME`: stops the macro turning the setting under the cursor.
    pub(crate) fn unmap_macro(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        let (t, fx, param) = self
            .setting_under_cursor()
            .ok_or("unmap works on a track's setting: move onto one")?;
        let m =
            (self.find_macro(t, name.trim())).ok_or_else(|| format!("no macro called {name:?}"))?;
        let at = (self.grid.tracks[t].macros[m].targets.iter())
            .position(|g| g.is(fx, param))
            .ok_or_else(|| format!("{name} doesn't turn this"))?;
        Ok(self.remove_target(t, m, at))
    }

    pub(crate) fn remove_target(&mut self, t: usize, m: usize, at: usize) -> Vec<Effect> {
        self.checkpoint();
        self.grid.tracks[t].macros[m].targets.remove(at);
        vec![Effect::TrackChanged(t)]
    }

    /// Removes macro `m`, and its locks.
    pub(crate) fn remove_macro(&mut self, t: usize, m: usize) -> Vec<Effect> {
        self.checkpoint();
        let removed = self.grid.tracks[t].macros.remove(m);
        let index = m as u8;
        self.retain_locks(t, |lock| match lock.is_macro() {
            true if lock.param == index => false,
            true if lock.param > index => {
                lock.param -= 1;
                true
            }
            _ => true,
        });
        self.message = Some(format!("removed {}", removed.name));
        vec![Effect::TrackChanged(t)]
    }

    /// Turns macro `m` to `value`, or with record armed and playing, locks it on the step
    /// playing.
    pub(crate) fn set_macro(&mut self, t: usize, m: usize, value: f32) -> Vec<Effect> {
        let value = value.clamp(0.0, 1.0);
        if let Some(effects) = self.record_lock_of(t, Lock::of_macro(m, value)) {
            return effects;
        }
        if self.grid.tracks[t].macros[m].value == value {
            return vec![];
        }
        self.checkpoint();
        self.grid.tracks[t].macros[m].value = value;
        vec![Effect::TrackChanged(t)]
    }

    pub(crate) fn set_depth(&mut self, t: usize, m: usize, at: usize, depth: f32) -> Vec<Effect> {
        let depth = DEPTH_INFO.clamp(depth);
        if self.grid.tracks[t].macros[m].targets[at].depth == depth {
            return vec![];
        }
        self.checkpoint();
        self.grid.tracks[t].macros[m].targets[at].depth = depth;
        vec![Effect::TrackChanged(t)]
    }

    /// Moves the macros' targets along after an effect's put in at `index`, or drops those on
    /// it and moves the rest back after it's taken out. Says whether any changed.
    pub(crate) fn shift_targets(&mut self, t: usize, index: usize, inserted: bool) -> bool {
        let index = index as u8;
        self.retain_targets(t, |target| match target.fx {
            Some(fx) if fx == index && !inserted => false,
            Some(fx) if fx >= index => {
                target.fx = Some(if inserted { fx + 1 } else { fx - 1 });
                true
            }
            _ => true,
        })
    }

    /// Keeps the targets `keep` says to on track `t`'s macros, letting it change them. Says
    /// whether any changed.
    pub(crate) fn retain_targets(
        &mut self,
        t: usize,
        mut keep: impl FnMut(&mut MacroTarget) -> bool,
    ) -> bool {
        let mut changed = false;
        for m in &mut self.grid.tracks[t].macros {
            let before = m.targets.clone();
            m.targets.retain_mut(&mut keep);
            changed |= m.targets != before;
        }
        changed
    }
}

/// A macro's name, which `:lock` finds it by: letters, digits, `-` and `_`, not starting with a
/// digit.
fn macro_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    let ok = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
    if name.is_empty() || !name.chars().all(ok) || name.starts_with(|c: char| c.is_ascii_digit()) {
        return Err("a macro's name is letters, digits, - and _: :macro bloom".into());
    }
    Ok(name.to_string())
}

/// How far a nudge turns a macro or a depth.
pub(crate) const NUDGE: f32 = 0.01;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FxLine, Key};
    use lattice_core::{Grid, Step};

    fn editor() -> Editor {
        let track = Track {
            name: "a".into(),
            steps: vec![Step::default(); 16],
            ..Track::default()
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![track],
            groups: vec![],
        };
        Editor::new(grid, 120.0)
    }

    fn keys(e: &mut Editor, s: &str) -> Vec<Effect> {
        let key = |c| match c {
            'A' => Key::Ctrl('a'),
            'X' => Key::Ctrl('x'),
            '\n' => Key::Enter,
            c => Key::Char(c),
        };
        s.chars().flat_map(|c| e.handle(key(c))).collect()
    }

    fn targets(e: &Editor) -> Vec<String> {
        let track = &e.grid.tracks[0];
        (track.macros.iter())
            .flat_map(|m| {
                m.targets
                    .iter()
                    .map(|g| format!("{} {}", target_name(track, g), g.depth))
            })
            .collect()
    }

    #[test]
    fn a_macro_turns_the_settings_mapped_to_it() {
        let mut e = editor();
        // A filter's cutoff and resonance, after a delay, mapped to bloom, which :map adds.
        keys(
            &mut e,
            "gf:fx-add delay\n:fx-add filter\njj:map bloom -0.25\nj:map bloom\n",
        );
        assert_eq!(e.grid.tracks[0].macros.len(), 1);
        assert_eq!(targets(&e), ["filter.cutoff -0.25", "filter.resonance 0.5"]);
        // Mapping it again changes the depth.
        keys(&mut e, ":map bloom 1\n");
        assert_eq!(targets(&e)[1], "filter.resonance 1");
        // Or by its number.
        keys(&mut e, ":map 1 0.75\n");
        assert_eq!(targets(&e)[1], "filter.resonance 0.75");
        assert!(keys(&mut e, ":map 9lives\n").is_empty());
        assert!(e.message.as_ref().unwrap().contains("macro's name"));

        // Left of lane A, onto the last line of the macros' column, under the macro.
        keys(&mut e, "h");
        assert_eq!(
            e.fx_line(),
            FxLine::Target {
                index: 0,
                target: 1
            }
        );
        keys(&mut e, "gg");
        assert_eq!(e.fx_line(), FxLine::Macro { index: 0 });
        keys(&mut e, "10A");
        assert!((e.grid.tracks[0].macros[0].value - 0.1).abs() < 1e-6);
        // Under it, a target's depth.
        keys(&mut e, "jX");
        assert_eq!(
            e.fx_line(),
            FxLine::Target {
                index: 0,
                target: 0
            }
        );
        assert_eq!(targets(&e)[0], "filter.cutoff -0.26");

        // L, turn it, L: locked on the step, and put back.
        keys(&mut e, "kL5AL");
        assert_eq!(e.describe_locks(0, 0), ["bloom 0.15"]);
        assert!((e.grid.tracks[0].macros[0].value - 0.1).abs() < 1e-6);
        keys(&mut e, "j");

        // Locked by name, as any setting.
        crate::command::run(&mut e, "lock bloom 0.5").unwrap();
        assert_eq!(e.describe_locks(0, 0), ["bloom 0.5"]);

        // Taking the delay out moves the targets back with the filter; taking the filter out
        // takes them, but not the macro or its locks.
        let fx = |e: &Editor| {
            (e.grid.tracks[0].macros[0].targets.iter())
                .map(|g| g.fx)
                .collect::<Vec<_>>()
        };
        assert_eq!(fx(&e), [Some(1), Some(1)]);
        keys(&mut e, "lggd");
        assert_eq!(fx(&e), [Some(0), Some(0)]);
        assert_eq!(targets(&e)[0], "filter.cutoff -0.26");
        keys(&mut e, "ggd");
        assert!(fx(&e).is_empty());
        assert_eq!(e.describe_locks(0, 0), ["bloom 0.5"]);

        // Removing the macro takes its locks.
        keys(&mut e, "hggd");
        assert!(e.grid.tracks[0].macros.is_empty());
        assert!(e.describe_locks(0, 0).is_empty());
        keys(&mut e, "u");
        assert_eq!(e.grid.tracks[0].macros.len(), 1, "undone");
    }
}
