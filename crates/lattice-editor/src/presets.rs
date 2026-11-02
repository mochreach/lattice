//! Presets: `:preset NAME` puts one on the track, its instrument, chain and macros all at once,
//! in place of the track's own, and gives the track its face; `:preset-save NAME` saves the
//! track's as one. Lattice comes with some ([`lattice_core::PRESETS`]); the rest are files the
//! frontend reads and writes, asked to with [`Effect::SavePreset`] and [`Effect::LoadPreset`].
//!
//! `:face TITLE` names the track's face, giving it one if it has none, and `:face-show` and
//! `:face-hide` put the instrument's settings forward on it, or take them back.

use lattice_core::{Face, Kind, MAX_CONTROLS, PRESETS, Preset, preset};

use crate::mixer::{Page, Strip};
use crate::{Editor, Effect};

impl Editor {
    /// The track the preset commands work on: the mixer's, fx or instrument page's, or the
    /// pattern cursor's.
    pub fn preset_track(&self) -> Result<usize, String> {
        match self.page {
            Page::Mixer | Page::Fx | Page::Instrument => match self.fx_strip() {
                Strip::Track(t) => Ok(t),
                _ => Err("presets are for tracks; buses have no instrument".into()),
            },
            _ => Ok(self.cursor().track),
        }
    }

    pub(crate) fn load_preset(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        let name = preset_name(name)?;
        let t = self.preset_track()?;
        Ok(match preset(&name) {
            Some(preset) => self.put_preset(t, &name, preset),
            None => vec![Effect::LoadPreset(name)],
        })
    }

    /// `:preset-save NAME`: saves the track's instrument, chain and macros, and its face, or a
    /// new one titled after `NAME`, which the track then has.
    pub(crate) fn save_preset(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        let name = preset_name(name)?;
        if PRESETS.contains(&name.as_str()) {
            return Err(format!(
                "{name} is one Lattice comes with: save it under another name"
            ));
        }
        let t = self.preset_track()?;
        let has_face = (self.grid.tracks.get(t).ok_or("no such track")?)
            .face
            .is_some();
        if !has_face {
            self.checkpoint();
            if let Some(track) = self.grid.tracks.get_mut(t) {
                track.face = Some(Face::new(&title(&name)));
            }
        }
        let track = self.grid.tracks.get(t).ok_or("no such track")?;
        let preset = Preset {
            face: track.face.clone().unwrap_or_default(),
            instrument: track.instrument.clone(),
            effects: track.effects.clone(),
            macros: track.macros.clone(),
        };
        Ok(vec![Effect::SavePreset(name, Box::new(preset))])
    }

    /// Puts `preset`, loaded as `name`, on track `t`, in place of its instrument, chain and
    /// macros. Locks of the old chain and macros go with them, and of the old instrument's
    /// settings unless it's the same kind.
    pub fn put_preset(&mut self, t: usize, name: &str, preset: Preset) -> Vec<Effect> {
        let Some(track) = self.grid.tracks.get(t) else {
            self.message = Some(format!("no track {} for {name}", t + 1));
            return vec![];
        };
        let same = track.instrument.kind == preset.instrument.kind;
        self.checkpoint();
        let Some(track) = self.grid.tracks.get_mut(t) else {
            return vec![];
        };
        let title = preset.face.title.clone();
        track.instrument = preset.instrument;
        track.effects = preset.effects;
        track.macros = preset.macros;
        track.face = Some(preset.face);
        let track_name = track.name.clone();
        self.retain_locks(t, |lock| same && lock.fx.is_none());
        self.fx = Default::default();
        self.mixer.effect = 0;
        self.message = Some(format!("{track_name} is {title} ({name})"));
        vec![Effect::MixerChanged, Effect::TrackChanged(t)]
    }

    /// `:face TITLE`: titles the track's face, giving it one.
    pub(crate) fn set_face(&mut self, title: &str) -> Result<Vec<Effect>, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("a face needs a title: :face Night Bus".into());
        }
        let t = self.preset_track()?;
        if self.grid.tracks.get(t).is_none() {
            return Err("no such track".into());
        }
        self.checkpoint();
        let track = self.grid.tracks.get_mut(t).ok_or("no such track")?;
        let face = track.face.get_or_insert_default();
        face.title = title.into();
        Ok(vec![])
    }

    /// `:face-show [SETTING]` and `:face-hide [SETTING]`: puts one of the instrument's settings
    /// forward on the track's face, or takes it back; the one under the cursor if none's named.
    pub(crate) fn show_on_face(
        &mut self,
        setting: &str,
        show: bool,
    ) -> Result<Vec<Effect>, String> {
        let t = self.preset_track()?;
        let track = self.grid.tracks.get(t).ok_or("no such track")?;
        let params = track.instrument.params();
        let id = match setting.trim() {
            "" => match self.setting_under_cursor() {
                Some((at, None, id)) if at == t => id,
                _ => {
                    return Err(
                        "put the cursor on one of the instrument's settings, or name one".into(),
                    );
                }
            },
            name => match params.iter().find(|p| p.name == name) {
                Some(p) => p.id as u8,
                None => {
                    let names: Vec<&str> = params.iter().map(|p| p.name).collect();
                    return Err(format!(
                        "{} has no {name}; there's {}",
                        track.instrument.kind.name(),
                        names.join(", ")
                    ));
                }
            },
        };
        let name = (params.iter().find(|p| p.id == u32::from(id)))
            .ok_or_else(|| format!("{} has no setting {id}", track.instrument.kind.name()))?
            .name;
        let shown = track
            .face
            .as_ref()
            .is_some_and(|f| f.controls.contains(&id));
        if shown == show {
            self.message = Some(match show {
                true => format!("{name} is already on the face"),
                false => format!("{name} isn't on the face"),
            });
            return Ok(vec![]);
        }
        if show
            && track
                .face
                .as_ref()
                .is_some_and(|f| f.controls.len() >= MAX_CONTROLS)
        {
            return Err(format!("a face has at most {MAX_CONTROLS} settings on it"));
        }
        self.checkpoint();
        let track = self.grid.tracks.get_mut(t).ok_or("no such track")?;
        let face = (track.face).get_or_insert_with(|| Face::new(&title(&track.name)));
        if show {
            face.controls.push(id);
        } else {
            face.controls.retain(|&c| c != id);
        }
        Ok(vec![])
    }
}

/// A preset's name, which is its file's: letters, digits, `-` and `_`.
fn preset_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    let ok = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
    if name.is_empty() || !name.chars().all(ok) {
        return Err("a preset's name is letters, digits, - and _".into());
    }
    Ok(name.to_string())
}

/// A face's title from a name: `night-bus` is Night Bus.
fn title(name: &str) -> String {
    name.split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            let first = chars.next().into_iter().flat_map(char::to_uppercase);
            first.chain(chars).collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;
    use lattice_core::{FxKind, Grid, InstKind, Lock, Step, Track};

    fn editor() -> Editor {
        let track = Track {
            name: "lead".into(),
            steps: vec![Step::default(); 16],
            effects: vec![lattice_core::Fx::new(FxKind::Comp)].into(),
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

    #[test]
    fn a_preset_brings_its_instrument_chain_macros_and_face() {
        let mut e = editor();
        let lock = |fx, param| Lock {
            fx,
            param,
            value: 0.5,
            ..Default::default()
        };
        e.grid.tracks[0].steps[0].locks.set(lock(None, 5));
        e.grid.tracks[0].steps[1].locks.set(lock(Some(0), 0));
        let effects = e.run_command("preset night-bus");
        assert_eq!(
            effects,
            [Effect::MixerChanged, Effect::TrackChanged(0)],
            "{:?}",
            e.message
        );
        let track = &e.grid.tracks[0];
        assert_eq!(track.instrument.kind, InstKind::Synth);
        assert_eq!(track.effects.lanes().len(), 2, "the shimmer");
        assert_eq!(track.macros[0].name, "dawn");
        assert_eq!(track.face.as_ref().unwrap().title, "Night Bus");
        assert_eq!(track.steps[0].locks.len(), 1, "the synth's lock stays");
        assert!(track.steps[1].locks.is_empty(), "the old chain's goes");

        // A kick has none of a synth's settings.
        e.run_command("preset foundry");
        assert!(e.grid.tracks[0].steps[0].locks.is_empty());
        // Changing its kind takes the face with it.
        e.run_command("inst synth");
        assert_eq!(e.grid.tracks[0].face, None);
        e.handle(Key::Char('u'));
        assert_eq!(e.grid.tracks[0].face.as_ref().unwrap().title, "Foundry");

        // Others are files.
        assert_eq!(
            e.run_command("preset mine"),
            [Effect::LoadPreset("mine".into())]
        );
        assert!(e.run_command("preset my/preset").is_empty());
    }

    #[test]
    fn saving_gives_the_track_a_face() {
        let mut e = editor();
        assert!(e.run_command("preset-save glasshouse").is_empty(), "taken");
        let effects = e.run_command("preset-save cold-iron");
        let [Effect::SavePreset(name, preset)] = &effects[..] else {
            panic!("{effects:?}");
        };
        assert_eq!(name, "cold-iron");
        assert_eq!(preset.face.title, "Cold Iron");
        assert_eq!(e.grid.tracks[0].face.as_ref().unwrap().title, "Cold Iron");

        e.run_command("face Cold Iron Mk II");
        e.run_command("face-show cutoff");
        e.run_command("face-show resonance");
        e.run_command("face-show nothing");
        let face = e.grid.tracks[0].face.clone().unwrap();
        assert_eq!(face.title, "Cold Iron Mk II");
        assert_eq!(face.controls, [5, 7]);
        e.run_command("face-hide cutoff");
        assert_eq!(e.grid.tracks[0].face.as_ref().unwrap().controls, [7]);
    }
}
