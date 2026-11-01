//! Chains saved on their own, to put on any strip of any song: `:chain-save NAME` saves the
//! strip's chain, and `:chain-load NAME` puts a saved one on the strip, in place of its own.
//! Lattice comes with some ([`lattice_core::PRESETS`]); the rest are files the frontend reads and
//! writes, asked to with [`Effect::SaveChain`] and [`Effect::LoadChain`].

use lattice_core::{FxChain, PRESETS, preset};

use crate::mixer::{Page, Strip};
use crate::{Editor, Effect};

impl Editor {
    /// The strip the chain commands work on: on the mixer and fx pages the cursor's, elsewhere
    /// the cursor's track.
    fn chain_strip(&self) -> Strip {
        match self.page {
            Page::Mixer | Page::Fx => self.fx_strip(),
            Page::Pattern | Page::Song | Page::Help => Strip::Track(self.cursor().track),
        }
    }

    pub(crate) fn save_chain(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        let name = chain_name(name)?;
        if PRESETS.contains(&name.as_str()) {
            return Err(format!(
                "{name} is one Lattice comes with: save it under another name"
            ));
        }
        let chain = self.chain(self.chain_strip()).clone();
        if chain.is_default() {
            return Err("there's nothing on this chain to save".into());
        }
        Ok(vec![Effect::SaveChain(name, Box::new(chain))])
    }

    pub(crate) fn load_chain(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        let name = chain_name(name)?;
        Ok(match preset(&name) {
            Some(chain) => self.put_chain(&name, chain),
            None => vec![Effect::LoadChain(name)],
        })
    }

    /// Puts `chain`, loaded as `name`, on the strip the chain commands work on, in place of its
    /// own. Locks on the old chain's effects go with them.
    pub fn put_chain(&mut self, name: &str, chain: FxChain) -> Vec<Effect> {
        let strip = self.chain_strip();
        self.checkpoint();
        *self.chain_mut(strip) = chain;
        let mut effects = vec![Effect::MixerChanged];
        if let Strip::Track(t) = strip
            && (self.retain_locks(t, |lock| lock.fx.is_none() || lock.is_macro())
                | self.retain_targets(t, |target| target.fx.is_none()))
        {
            effects.push(Effect::TrackChanged(t));
        }
        self.fx = Default::default();
        if self.page == Page::Fx {
            self.mixer.effect = 0;
        }
        self.message = Some(format!("loaded the {name} chain"));
        effects
    }
}

/// A chain's name, which is its file's: letters, digits, `-` and `_`.
fn chain_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    let ok = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
    if name.is_empty() || !name.chars().all(ok) {
        return Err("a chain's name is letters, digits, - and _".into());
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;
    use lattice_core::{FxKind, FxRow, Grid, Lock, Step, Track};

    #[test]
    fn a_preset_replaces_the_chain_and_its_locks() {
        let track = Track {
            name: "pad".into(),
            steps: vec![Step::default(); 16],
            effects: vec![lattice_core::Fx::new(FxKind::Drive)].into(),
            ..Track::default()
        };
        let grid = Grid {
            name: "test".into(),
            steps_per_beat: 4,
            tracks: vec![track],
            groups: vec![],
        };
        let mut e = Editor::new(grid, 120.0);
        e.grid.tracks[0].steps[0].locks.set(Lock {
            fx: Some(0),
            param: 0,
            value: 0.5,
        });
        let effects = e.run_command("chain-load shimmer");
        assert_eq!(
            effects,
            [Effect::MixerChanged, Effect::TrackChanged(0)],
            "{:?}",
            e.message
        );
        let chain = &e.grid.tracks[0].effects;
        assert_eq!(chain.lanes().len(), 2);
        assert!(matches!(
            chain.lanes()[1].rows[2],
            FxRow::Send { to: 0, .. }
        ));
        assert!(e.grid.tracks[0].steps[0].locks.is_empty());

        // Saving asks the frontend to write it; a preset's name is taken.
        assert!(matches!(
            &e.run_command("chain-save wash")[..],
            [Effect::SaveChain(name, _)] if name == "wash"
        ));
        assert!(e.run_command("chain-save shimmer").is_empty());
        assert!(e.run_command("chain-load my/chain").is_empty());
        assert_eq!(
            e.run_command("chain-load wash"),
            [Effect::LoadChain("wash".into())]
        );
        e.handle(Key::Char('u'));
        assert_eq!(e.grid.tracks[0].effects.len(), 1, "undone");
    }
}
