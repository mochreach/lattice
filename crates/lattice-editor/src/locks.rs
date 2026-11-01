//! Locks: settings of a track's instrument or effects that change on a step, set with `:lock`
//! and cleared with `:unlock` on every step the selections reach.

use lattice_core::{Curve, Kind, Lock, MAX_LOCKS, ParamInfo, Track};

use crate::{Editor, Effect, Row, Strip, changed};

/// A setting a lock can hold on one track, found by name.
struct Setting {
    fx: Option<u8>,
    info: &'static ParamInfo,
}

impl Setting {
    /// What the setting is called on `track`: `cutoff` on the instrument, `filter.cutoff` on an
    /// effect.
    fn name(&self, track: &Track) -> String {
        match self.fx.and_then(|fx| track.effects.get(usize::from(fx))) {
            Some(effect) => format!("{}.{}", effect.kind.name(), self.info.name),
            None => self.info.name.into(),
        }
    }
}

/// The setting called `name` on `track`: a parameter of its instrument, or of its first effect
/// that has one. `EFFECT.PARAM` picks the effect, by name or by number from 1.
fn find(track: &Track, name: &str) -> Option<Setting> {
    let instrument = || {
        let info = track
            .instrument
            .kind
            .params()
            .iter()
            .find(|p| p.name == name)?;
        Some(Setting { fx: None, info })
    };
    let effect = |fx: usize, param: &str| {
        let effect = track.effects.get(fx)?;
        let info = effect.kind.params().iter().find(|p| p.name == param)?;
        Some(Setting {
            fx: Some(fx as u8),
            info,
        })
    };
    match name.split_once('.') {
        Some(("inst", param)) => find(track, param).filter(|s| s.fx.is_none()),
        Some((device, param)) => match device.parse::<usize>() {
            Ok(n) => effect(n.checked_sub(1)?, param),
            Err(_) => (0..track.effects.len())
                .filter(|&fx| track.effects[fx].kind.name() == device)
                .find_map(|fx| effect(fx, param)),
        },
        None => instrument().or_else(|| (0..track.effects.len()).find_map(|fx| effect(fx, name))),
    }
}

/// `word` as a value of `info`: a number in its range, or one of its choices by name.
fn value(info: &ParamInfo, word: &str) -> Result<f32, String> {
    if let Curve::Choice(names) = info.curve
        && let Some(i) = names.iter().position(|n| *n == word)
    {
        return Ok(info.min + i as f32);
    }
    let number: f32 = (word
        .strip_suffix(info.unit)
        .filter(|_| !info.unit.is_empty()))
    .unwrap_or(word)
    .parse()
    .ok()
    .filter(|v: &f32| v.is_finite())
    .ok_or_else(|| match info.curve {
        Curve::Choice(names) => format!("{} is one of {}", info.name, names.join(", ")),
        _ => format!("{word:?} isn't a number"),
    })?;
    if !(info.min..=info.max).contains(&number) {
        return Err(format!(
            "{} goes from {} to {}",
            info.name,
            info.format(info.min),
            info.format(info.max)
        ));
    }
    Ok(info.clamp(number))
}

impl Editor {
    /// `:lock PARAM VALUE`, or `:lock PARAM FROM..TO` to sweep across each selection, evenly
    /// on the setting's own curve.
    pub(crate) fn lock(&mut self, args: &str) -> Result<Vec<Effect>, String> {
        let (name, values) = args
            .split_once(' ')
            .map(|(n, v)| (n, v.trim()))
            .ok_or("lock which setting, at what? :lock cutoff 800")?;
        let (from, to) = values.split_once("..").unwrap_or((values, values));
        let columns: Vec<_> = self
            .selections
            .iter()
            .flat_map(|s| self.columns(s.region()))
            .collect();

        // Every track has to have it, and every step room for it, before anything changes.
        let mut edits = vec![];
        for (t, steps) in &columns {
            let track = &self.grid.tracks[*t];
            let setting = find(track, name).ok_or_else(|| {
                let names: Vec<String> = settings(track).iter().map(|s| s.name(track)).collect();
                format!("{} has no {name}: try {}", track.name, names.join(", "))
            })?;
            let (from, to) = (value(setting.info, from)?, value(setting.info, to)?);
            let (a, b) = (setting.info.fraction(from), setting.info.fraction(to));
            for (i, step) in steps.clone().enumerate() {
                let along = i as f32 / (steps.len().max(2) - 1) as f32;
                let value = if from == to {
                    from
                } else {
                    setting.info.at(a + (b - a) * along)
                };
                let lock = Lock {
                    fx: setting.fx,
                    param: setting.info.id as u8,
                    value,
                };
                let locks = &track.steps[step].locks;
                if locks.find(&lock).is_none() && locks.len() == MAX_LOCKS {
                    return Err(format!("a step holds at most {MAX_LOCKS} locks"));
                }
                edits.push((*t, step, lock));
            }
        }
        self.checkpoint();
        for &(t, step, lock) in &edits {
            self.grid.tracks[t].steps[step].locks.set(lock);
        }
        Ok(changed(edits.into_iter().map(|(t, ..)| t)))
    }

    /// `L` on the mixer page: locks the setting under the cursor, at the value it has now, onto
    /// the steps the selections cover, in the mixer's track. So a sound found on the mixer can be
    /// put on a few steps without typing its name.
    pub(crate) fn lock_shown(&mut self) -> Result<Vec<Effect>, String> {
        let cursor = self.mixer_cursor();
        let Row::Param(p) = cursor.row else {
            return Err("move onto an instrument or effect setting to lock it".into());
        };
        let shown = self.shown().ok_or("nothing here to lock")?;
        let Strip::Track(t) = shown.strip else {
            return Err("locks are on tracks' steps; buses have none".into());
        };
        let (params, value) = (shown.params, shown.values[p]);
        let info = &params[p];
        let lock = Lock {
            fx: shown.slot.map(|fx| fx as u8),
            param: info.id as u8,
            value,
        };
        // The selections' steps, whichever tracks they're on.
        let len = self.track_len(t);
        let mut steps: Vec<usize> = (self.selections.iter())
            .flat_map(|s| {
                let r = s.region();
                r.first_step.min(len)..(r.last_step + 1).min(len)
            })
            .collect();
        steps.sort_unstable();
        steps.dedup();
        let locks = |step: usize| &self.grid.tracks[t].steps[step].locks;
        if (steps.iter()).any(|&s| locks(s).find(&lock).is_none() && locks(s).len() == MAX_LOCKS) {
            return Err(format!("a step holds at most {MAX_LOCKS} locks"));
        }
        let (Some(&first), Some(&last)) = (steps.first(), steps.last()) else {
            return Err("no steps selected in this track".into());
        };
        self.checkpoint();
        for &step in &steps {
            self.grid.tracks[t].steps[step].locks.set(lock);
        }
        let setting = Setting { fx: lock.fx, info };
        let track = &self.grid.tracks[t];
        let at = match (steps.len(), first == last) {
            (_, true) => format!("step {first}"),
            (n, _) if n == last - first + 1 => format!("steps {first}–{last}"),
            (n, _) => format!("{n} steps"),
        };
        self.message = Some(format!(
            "◆ {} {} on {} {at}",
            setting.name(track),
            info.format(value),
            track.name
        ));
        Ok(changed([t]))
    }

    /// `:unlock [PARAM]`: one setting's locks, or all of them, from every step the selections
    /// reach.
    pub(crate) fn unlock(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        let columns: Vec<_> = self
            .selections
            .iter()
            .flat_map(|s| self.columns(s.region()))
            .collect();
        let mut wanted = vec![];
        for (t, _) in &columns {
            let setting = match name {
                "" => None,
                name => Some(
                    find(&self.grid.tracks[*t], name)
                        .ok_or_else(|| format!("{} has no {name}", self.grid.tracks[*t].name))?,
                ),
            };
            wanted.push(setting.map(|s| (s.fx, s.info.id as u8)));
        }
        self.checkpoint();
        for ((t, steps), wanted) in columns.iter().zip(wanted) {
            for step in &mut self.grid.tracks[*t].steps[steps.clone()] {
                match wanted {
                    Some((fx, param)) => step.locks.retain(|l| (l.fx, l.param) != (fx, param)),
                    None => step.locks.clear(),
                }
            }
        }
        Ok(changed(columns.into_iter().map(|(t, _)| t)))
    }

    /// Keeps the locks `keep` says to on every step of track `t`, in every pattern, letting it
    /// change them as it goes: for when its chain or instrument changes under them. Says
    /// whether any did change.
    pub(crate) fn retain_locks(
        &mut self,
        t: usize,
        mut keep: impl FnMut(&mut Lock) -> bool,
    ) -> bool {
        let lanes = (self.arrangement.patterns.iter_mut())
            .filter_map(|p| p.lanes.get_mut(t))
            .flat_map(|lane| lane.steps.iter_mut());
        let mut changed = false;
        for step in self.grid.tracks[t].steps.iter_mut().chain(lanes) {
            let before = step.locks;
            step.locks.retain(&mut keep);
            changed |= step.locks != before;
        }
        changed
    }

    /// Moves effects' locks along after an effect's been put in at `index`, or drops its own
    /// and moves the rest back after it's been taken out.
    pub(crate) fn shift_locks(&mut self, t: usize, index: usize, inserted: bool) -> bool {
        let index = index as u8;
        self.retain_locks(t, |lock| match lock.fx {
            Some(fx) if fx == index && !inserted => false,
            Some(fx) if fx >= index => {
                lock.fx = Some(if inserted { fx + 1 } else { fx - 1 });
                true
            }
            _ => true,
        })
    }

    /// The locks on a step, as `cutoff 800Hz`, for showing.
    pub fn describe_locks(&self, track: usize, step: usize) -> Vec<String> {
        let Some(track) = self.grid.tracks.get(track) else {
            return vec![];
        };
        let Some(step) = track.steps.get(step) else {
            return vec![];
        };
        step.locks
            .iter()
            .map(|lock| {
                let params = match lock.fx {
                    None => Some(track.instrument.kind.params()),
                    Some(fx) => track.effects.get(usize::from(fx)).map(|e| e.kind.params()),
                };
                match params.and_then(|p| p.get(usize::from(lock.param))) {
                    Some(info) => {
                        let setting = Setting { fx: lock.fx, info };
                        format!("{} {}", setting.name(track), info.format(lock.value))
                    }
                    // Its effect has gone: it does nothing until one's back in its place.
                    None => format!("fx{}.{}?", lock.fx.map_or(0, |f| f + 1), lock.param),
                }
            })
            .collect()
    }
}

/// Every setting on `track`, for suggesting.
fn settings(track: &Track) -> Vec<Setting> {
    let instrument = track
        .instrument
        .kind
        .params()
        .iter()
        .map(|info| Setting { fx: None, info });
    let effects = track.effects.iter().enumerate().flat_map(|(fx, e)| {
        e.kind.params().iter().map(move |info| Setting {
            fx: Some(fx as u8),
            info,
        })
    });
    instrument.chain(effects).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::fx::filter;
    use lattice_core::{Fx, FxKind, Grid, Step};

    fn editor() -> Editor {
        let mut track = Track {
            name: "acid".into(),
            steps: vec![Step::note(60); 5],
            ..Track::default()
        };
        track.effects = vec![Fx::new(FxKind::Delay), Fx::new(FxKind::Filter)];
        Editor::new(
            Grid {
                name: "locks".into(),
                steps_per_beat: 4,
                tracks: vec![track],
                groups: vec![],
            },
            120.0,
        )
    }

    fn run(e: &mut Editor, line: &str) -> Result<Vec<Effect>, String> {
        crate::command::run(e, line)
    }

    fn locks(e: &Editor, step: usize) -> Vec<String> {
        e.describe_locks(0, step)
    }

    #[test]
    fn lock_finds_the_setting_by_name() {
        let mut e = editor();
        assert_eq!(
            run(&mut e, "lock cutoff 800"),
            Ok(vec![Effect::TrackChanged(0)])
        );
        assert_eq!(locks(&e, 0), ["cutoff 800Hz"]);
        // An effect's, by its name or number; the first effect with it otherwise.
        run(&mut e, "lock filter.type band").unwrap();
        run(&mut e, "lock 1.mix 0.5").unwrap();
        run(&mut e, "lock feedback 0.7").unwrap();
        assert_eq!(
            locks(&e, 0),
            [
                "cutoff 800Hz",
                "filter.type band",
                "delay.mix 0.5",
                "delay.feedback 0.7"
            ]
        );
        // Locking it again changes it in place.
        run(&mut e, "lock cutoff 900").unwrap();
        assert_eq!(locks(&e, 0)[0], "cutoff 900Hz");
        assert_eq!(locks(&e, 1), Vec::<String>::new(), "only where selected");
    }

    #[test]
    fn l_on_the_mixer_locks_the_setting_under_the_cursor() {
        use crate::keymap::Key;
        let mut e = editor();
        // Steps 1 to 3, then the filter's first setting on the mixer.
        for c in "jvjjgm".chars() {
            e.handle(Key::Char(c));
        }
        assert_eq!(e.page, crate::Page::Mixer);
        e.mixer.instrument = false;
        e.mixer.effect = 1;
        e.mixer.row = Row::Param(0);
        let name = e.shown().unwrap().params[0].name;
        assert_eq!(e.handle(Key::Char('L')), [Effect::TrackChanged(0)]);
        assert_eq!(locks(&e, 0), Vec::<String>::new());
        for step in 1..=3 {
            assert_eq!(locks(&e, step).len(), 1);
            assert!(locks(&e, step)[0].starts_with(&format!("filter.{name} ")));
        }
        assert!(e.message.as_ref().unwrap().contains("steps 1–3"));
        // Not on the strip's level.
        e.mixer.row = Row::Strips;
        assert_eq!(e.handle(Key::Char('L')), []);
        assert!(e.message.as_ref().unwrap().contains("move onto"));
    }

    #[test]
    fn lock_says_what_is_wrong() {
        let mut e = editor();
        assert!(
            run(&mut e, "lock wobble 3")
                .unwrap_err()
                .contains("acid has no wobble: try")
        );
        assert_eq!(
            run(&mut e, "lock cutoff 3").unwrap_err(),
            "cutoff goes from 20Hz to 16000Hz"
        );
        assert_eq!(
            run(&mut e, "lock filter.type notch").unwrap_err(),
            "type is one of low, band, high"
        );
        assert!(run(&mut e, "lock cutoff").is_err());
        for line in [
            "lock cutoff 100",
            "lock attack 0.01",
            "lock 2.mix 1",
            "lock 1.mix 1",
        ] {
            run(&mut e, line).unwrap();
        }
        assert_eq!(
            run(&mut e, "lock decay 0.1").unwrap_err(),
            "a step holds at most 4 locks"
        );
        assert_eq!(locks(&e, 0).len(), 4, "and nothing changed");
    }

    #[test]
    fn locks_follow_their_effects_and_go_with_the_instrument() {
        let mut e = editor();
        for line in [
            "lock cutoff 100",
            "lock delay.mix 0.2",
            "lock filter.mix 0.3",
        ] {
            run(&mut e, line).unwrap();
        }
        // A new first effect, from the pattern page's end of the chain... then the mixer's.
        run(&mut e, "fx-add drive").unwrap();
        e.grid.tracks[0].effects.rotate_right(1);
        e.shift_locks(0, 0, true);
        assert_eq!(
            locks(&e, 0),
            ["cutoff 100Hz", "delay.mix 0.2", "filter.mix 0.3"]
        );
        e.grid.tracks[0].effects.remove(1);
        e.shift_locks(0, 1, false);
        assert_eq!(locks(&e, 0), ["cutoff 100Hz", "filter.mix 0.3"]);
        run(&mut e, "inst kick").unwrap();
        assert_eq!(locks(&e, 0), ["filter.mix 0.3"]);
    }

    #[test]
    fn a_range_sweeps_across_the_selection() {
        let mut e = editor();
        // Down the whole track: evenly by octave, as the cutoff is on a log curve.
        e.handle(crate::Key::Char('%'));
        run(&mut e, "lock 2.cutoff 100..1600").unwrap();
        let cutoffs: Vec<f32> = (e.grid.tracks[0].steps.iter())
            .map(|s| s.locks.iter().next().unwrap().value)
            .collect();
        for (cutoff, want) in cutoffs.iter().zip([100.0, 200.0, 400.0, 800.0, 1600.0]) {
            assert!((cutoff - want).abs() < 0.5, "{cutoffs:?}");
        }
        assert_eq!(
            e.grid.tracks[0].steps[0].locks.iter().next().unwrap().fx,
            Some(1)
        );
        assert_eq!(
            e.grid.tracks[0].steps[0].locks.iter().next().unwrap().param,
            filter::CUTOFF as u8
        );

        run(&mut e, "lock cutoff 300").unwrap();
        run(&mut e, "unlock filter.cutoff").unwrap();
        assert!(locks(&e, 2) == ["cutoff 300Hz"]);
        run(&mut e, "unlock").unwrap();
        assert!(e.grid.tracks[0].steps.iter().all(|s| s.locks.is_empty()));
        assert!(run(&mut e, "unlock wobble").is_err());
    }
}
