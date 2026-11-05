//! MIDI tracks: tracks whose instrument is gear outside Lattice, played over MIDI. `:port`
//! chooses the gear, and makes a track a MIDI track if it isn't; `:cc` gives it a controller to
//! send. Of a MIDI track's 128 controllers, only those it's been given are shown, or locked.

use lattice_core::instrument::midi::{CC, cc};
use lattice_core::{InstKind, Track};

use crate::{Editor, Effect};

/// Whether track's setting `p` is shown on the instrument page: all of every instrument's, but
/// only a MIDI track's controllers that it sends, or that a lock column changes.
pub(crate) fn shows_param(track: &Track, p: usize) -> bool {
    if track.instrument.kind != InstKind::Midi || p < CC as usize {
        return true;
    }
    let set = track.instrument.values.get(p).is_some_and(|&v| v >= 0.0);
    set || (track.lock_columns().iter()).any(|c| c.fx.is_none() && usize::from(c.param) == p)
}

impl Editor {
    /// `:port [PORT]`: the tracks play the gear on MIDI port `PORT`, by its number in the list
    /// or any part of its name, or on Lattice's own with `none`; alone, lists the ports.
    pub(crate) fn set_port(&mut self, port: &str) -> Result<Vec<Effect>, String> {
        let port = port.trim();
        let ports = &self.midi_outputs;
        if port.is_empty() {
            let listed: Vec<String> = (ports.iter().enumerate())
                .map(|(i, name)| format!("{} {name}", i + 1))
                .collect();
            self.message = Some(match listed.as_slice() {
                [] => "no MIDI gear plugged in; :port none plays Lattice's own port".into(),
                _ => format!(
                    "midi out: {}; or none, for Lattice's own",
                    listed.join(", ")
                ),
            });
            return Ok(vec![]);
        }
        let wanted = match port {
            "none" | "-" => None,
            _ => match port.parse::<usize>() {
                Ok(n) => Some(
                    (n.checked_sub(1).and_then(|i| ports.get(i)))
                        .ok_or_else(|| format!("there's no port {n}: :port lists them"))?
                        .clone(),
                ),
                // As it's written, so gear plugged in later is found by it.
                Err(_) => Some(port.to_string()),
            },
        };
        let plugged = wanted.as_ref().is_none_or(|w| {
            let w = w.to_lowercase();
            ports.iter().any(|p| p.to_lowercase().contains(&w))
        });
        // Tracks that aren't MIDI tracks become them, in the same undo step.
        let mut effects = self.set_instrument("midi")?;
        if effects.is_empty() {
            self.checkpoint();
            effects.push(Effect::MixerChanged);
        }
        let tracks = self.mix_targets()?;
        for &t in &tracks {
            if let Some(track) = self.grid.tracks.get_mut(t) {
                track.instrument.port = wanted.clone();
            }
        }
        let gear = wanted.as_deref().unwrap_or("Lattice's own port");
        let who = match self.only_track(&tracks) {
            Some(track) => format!("{} plays {gear}", track.name),
            None => format!("{} tracks play {gear}", tracks.len()),
        };
        self.message = Some(match plugged {
            true => who,
            false => format!("{who}, once it's plugged in"),
        });
        Ok(effects)
    }

    /// `:cc N [VALUE]`: the MIDI tracks send controller `N` at `VALUE`, 0 to 127; with `none`,
    /// or nothing, stop sending it.
    pub(crate) fn set_cc(&mut self, number: u8, value: &str) -> Result<Vec<Effect>, String> {
        let value = match value.trim() {
            "" | "none" | "-" => -1.0,
            text => match text.parse::<u8>() {
                Ok(v @ 0..=127) => f32::from(v),
                _ => {
                    return Err(format!(
                        "{text:?}: a controller goes from 0 to 127, or none"
                    ));
                }
            },
        };
        let tracks: Vec<usize> = (self.mix_targets()?.into_iter())
            .filter(|&t| {
                (self.grid.tracks.get(t)).is_some_and(|t| t.instrument.kind == InstKind::Midi)
            })
            .collect();
        if tracks.is_empty() {
            return Err("not a MIDI track: :port makes it one".into());
        }
        self.checkpoint();
        let id = cc(number) as usize;
        for &t in &tracks {
            if let Some(v) =
                (self.grid.tracks.get_mut(t)).and_then(|t| t.instrument.values.get_mut(id))
            {
                *v = value;
            }
        }
        self.message = Some(match value < 0.0 {
            true => format!("cc{number} isn't sent"),
            false => format!("cc{number} sends {value}"),
        });
        Ok(vec![Effect::MixerChanged])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Page;
    use lattice_core::{Grid, Step};

    fn editor() -> Editor {
        let track = Track {
            name: "track 1".into(),
            steps: vec![Step::default(); 16],
            ..Track::default()
        };
        let mut e = Editor::new(
            Grid {
                name: "midi".into(),
                steps_per_beat: 4,
                beats_per_bar: 4,
                grouping: vec![],
                tracks: vec![track],
                groups: vec![],
            },
            120.0,
        );
        e.midi_outputs = vec!["Minilogue".into(), "KeyStep 37".into()];
        e
    }

    #[test]
    fn port_makes_a_midi_track_playing_the_gear() {
        let mut e = editor();
        crate::command::run(&mut e, "port 1").unwrap();
        let track = &e.grid.tracks[0];
        assert_eq!(track.instrument.kind, InstKind::Midi);
        assert_eq!(track.instrument.port.as_deref(), Some("Minilogue"));
        assert_eq!(track.name, "midi");
        assert_eq!(e.message.as_deref(), Some("midi plays Minilogue"));
        crate::command::run(&mut e, "port prophet").unwrap();
        assert_eq!(
            e.message.as_deref(),
            Some("midi plays prophet, once it's plugged in")
        );
        crate::command::run(&mut e, "port none").unwrap();
        assert_eq!(e.grid.tracks[0].instrument.port, None);
        assert!(crate::command::run(&mut e, "port 3").is_err());
        e.handle(crate::keymap::Key::Char('u'));
        assert_eq!(e.grid.tracks[0].instrument.port.as_deref(), Some("prophet"));
        e.handle(crate::keymap::Key::Char('u'));
        e.handle(crate::keymap::Key::Char('u'));
        assert_eq!(
            e.grid.tracks[0].instrument.kind,
            InstKind::Synth,
            "one undo each"
        );
        assert_eq!(e.page, Page::Pattern);
    }

    #[test]
    fn shows_only_the_controllers_sent_or_locked() {
        let mut e = editor();
        assert!(
            crate::command::run(&mut e, "cc 74 40").is_err(),
            "not a MIDI track"
        );
        crate::command::run(&mut e, "port 1").unwrap();
        let shown = |e: &Editor| {
            let track = &e.grid.tracks[0];
            (0..track.instrument.params().len())
                .filter(|&p| shows_param(track, p))
                .count()
        };
        assert_eq!(shown(&e), 2, "channel and bend");
        crate::command::run(&mut e, "cc 74 40").unwrap();
        assert_eq!(e.grid.tracks[0].instrument.values[cc(74) as usize], 40.0);
        assert_eq!(shown(&e), 3);
        crate::command::run(&mut e, "cc 74 none").unwrap();
        assert_eq!(shown(&e), 2);
        assert!(crate::command::run(&mut e, "cc 74 128").is_err());
    }
}
