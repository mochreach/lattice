//! Locks: settings of a track's instrument or effects that change on a step, set with `:lock`
//! and cleared with `:unlock` on every step the selections reach.

use lattice_core::{Curve, Fx, Instrument, Kind, Lock, MAX_LOCKS, ParamInfo, Track};

use crate::{Editor, Effect, Row, Strip, changed};

/// What the first `L` on the mixer page remembers: a track's settings as they were, and how far
/// back undo went.
#[derive(Debug, Clone)]
pub(crate) struct Locking {
    track: usize,
    instrument: Instrument,
    effects: Vec<Fx>,
    undo: usize,
}

impl Locking {
    /// Whether the track still has the same instrument and effects, so its settings line up.
    fn matches(&self, editor: &Editor) -> bool {
        editor.grid.tracks.get(self.track).is_some_and(|track| {
            track.instrument.kind == self.instrument.kind
                && track.effects.len() == self.effects.len()
                && (track.effects.iter().zip(&self.effects)).all(|(a, b)| a.kind == b.kind)
        })
    }
}

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

    /// `L` on the mixer page. The first press remembers the track's settings; change them, and
    /// the second locks each one that changed, at its new value, onto the steps the selections
    /// cover (whichever tracks they were made on), then puts the track's settings back.
    pub(crate) fn toggle_locking(&mut self) -> Result<Vec<Effect>, String> {
        if self.locking.is_some() {
            return self.finish_locking();
        }
        let strip = self.strip(self.mixer_cursor().strip);
        let Strip::Track(t) = strip else {
            return Err("locks are on tracks' steps; buses have none".into());
        };
        // The hints say what to do next.
        let track = &self.grid.tracks[t];
        self.locking = Some(Locking {
            track: t,
            instrument: track.instrument.clone(),
            effects: track.effects.to_vec(),
            undo: self.undo_len(),
        });
        Ok(vec![])
    }

    /// Whether the mixer is waiting for a second `L`, and on which track.
    pub fn locking(&self) -> Option<usize> {
        self.locking.as_ref().map(|l| l.track)
    }

    /// Puts back the settings `L` remembered, and forgets the changes' undo steps, as though
    /// they'd never been made. If the track or its chain changed meanwhile, leaves it be.
    pub(crate) fn cancel_locking(&mut self) -> Vec<Effect> {
        let Some(locking) = self.locking.take() else {
            return vec![];
        };
        if !locking.matches(self) {
            return vec![];
        }
        let track = &mut self.grid.tracks[locking.track];
        track.instrument.values = locking.instrument.values.clone();
        for (fx, was) in track.effects.iter_mut().zip(&locking.effects) {
            fx.values = was.values.clone();
        }
        self.truncate_undo(locking.undo);
        vec![Effect::MixerChanged]
    }

    fn finish_locking(&mut self) -> Result<Vec<Effect>, String> {
        let locking = self.locking.clone().expect("locking");
        if !locking.matches(self) {
            self.locking = None;
            return Err("the track's chain changed; nothing locked".into());
        }
        let t = locking.track;
        let track = &self.grid.tracks[t];
        // Each setting that changed, as a lock.
        let mut new = vec![];
        let devices = std::iter::once((None, &locking.instrument.values, &track.instrument))
            .map(|(fx, was, now)| (fx, was, now.params(), &now.values))
            .chain(
                (locking.effects.iter().zip(&track.effects).enumerate())
                    .map(|(i, (was, now))| (Some(i as u8), &was.values, now.params(), &now.values)),
            );
        for (fx, was, params, now) in devices {
            for (p, info) in params.iter().enumerate() {
                if was.get(p) != now.get(p) {
                    new.push((
                        Setting { fx, info },
                        Lock {
                            fx,
                            param: info.id as u8,
                            value: now[p],
                        },
                    ));
                }
            }
        }
        if new.is_empty() {
            self.locking = None;
            self.message = Some("nothing changed, so nothing locked".into());
            return Ok(vec![]);
        }
        // The selections' steps.
        let len = self.track_len(t);
        let mut steps: Vec<usize> = (self.selections.iter())
            .flat_map(|s| {
                let r = s.region();
                r.first_step.min(len)..(r.last_step + 1).min(len)
            })
            .collect();
        steps.sort_unstable();
        steps.dedup();
        let (Some(&first), Some(&last)) = (steps.first(), steps.last()) else {
            return Err("no steps selected in this track".into());
        };
        for &step in &steps {
            let locks = &track.steps[step].locks;
            let adding = new.iter().filter(|(_, l)| locks.find(l).is_none()).count();
            if locks.len() + adding > MAX_LOCKS {
                return Err(format!(
                    "a step holds at most {MAX_LOCKS} locks: {} has {}, and this adds {adding}",
                    step,
                    locks.len()
                ));
            }
        }
        let names: Vec<String> = (new.iter())
            .map(|(setting, lock)| {
                format!(
                    "{} {}",
                    setting.name(track),
                    setting.info.format(lock.value)
                )
            })
            .collect();
        let at = match (steps.len(), first == last) {
            (_, true) => format!("step {first}"),
            (n, _) if n == last - first + 1 => format!("steps {first}–{last}"),
            (n, _) => format!("{n} steps"),
        };
        let message = format!("◆ {} on {} {at}", names.join(" · "), track.name);

        // Back as they were, then locked, as one undo.
        let mut effects = self.cancel_locking();
        self.checkpoint();
        for &step in &steps {
            for (_, lock) in &new {
                self.grid.tracks[t].steps[step].locks.set(*lock);
            }
        }
        self.message = Some(message);
        effects.extend(changed([t]));
        Ok(effects)
    }

    /// With record armed and the song playing, a setting changed on the mixer is locked on the
    /// step playing, rather than changed, and heard at once. Each pass through the track is one
    /// undo, as with recorded notes. `None` when not recording a track's setting, so it's set
    /// as usual.
    pub(crate) fn record_lock(&mut self, value: f32) -> Option<Vec<Effect>> {
        if !self.record || self.locking.is_some() {
            return None;
        }
        let Row::Param(p) = self.mixer_cursor().row else {
            return None;
        };
        let shown = self.shown()?;
        let Strip::Track(t) = shown.strip else {
            return None;
        };
        let (params, slot) = (shown.params, shown.slot);
        let info = &params[p];
        let len = self.track_len(t);
        let at = (self.transport.as_ref()?).at(t, len)?;
        // Counting in, there's no step yet to lock.
        if at < 0.0 || len == 0 {
            return Some(vec![]);
        }
        let at = at.floor() as u64;
        let (step, pass) = ((at % len as u64) as usize, at / len as u64);
        let lock = Lock {
            fx: slot.map(|s| s as u8),
            param: info.id as u8,
            value: info.clamp(value),
        };
        let locks = &self.grid.tracks[t].steps[step].locks;
        if locks.find(&lock).is_none() && locks.len() == MAX_LOCKS {
            self.message = Some(format!("step {step} already holds {MAX_LOCKS} locks"));
            return Some(vec![]);
        }
        if self.take != Some(pass) {
            self.checkpoint();
            self.take = Some(pass);
        }
        self.grid.tracks[t].steps[step].locks.set(lock);
        self.live_lock = Some((t, lock));
        let track = &self.grid.tracks[t];
        let setting = Setting { fx: lock.fx, info };
        self.message = Some(format!(
            "◆ {} {} on {} step {step}",
            setting.name(track),
            info.format(lock.value),
            track.name
        ));
        Some(vec![
            Effect::TrackChanged(t),
            Effect::Lock { track: t, lock },
        ])
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
        track.effects = vec![Fx::new(FxKind::Delay), Fx::new(FxKind::Filter)].into();
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
    fn l_twice_on_the_mixer_locks_what_changed_and_puts_it_back() {
        use crate::Row;
        use crate::keymap::Key;
        let mut e = editor();
        // Steps 1 to 3, then the filter's settings on the mixer.
        for c in "jvjjgm".chars() {
            e.handle(Key::Char(c));
        }
        assert_eq!(e.page, crate::Page::Mixer);
        e.mixer.instrument = false;
        e.mixer.effect = 1;
        e.mixer.row = Row::Param(0);
        let before = e.grid.tracks[0].effects[1].values.clone();
        let undo = e.undo_len();

        assert_eq!(e.handle(Key::Char('L')), []);
        assert_eq!(e.locking(), Some(0));
        // Change two of the filter's settings.
        e.handle(Key::Ctrl('a'));
        e.mixer.row = Row::Param(1);
        e.handle(Key::Ctrl('a'));
        e.handle(Key::Ctrl('a'));
        let after = e.grid.tracks[0].effects[1].values.clone();
        assert_ne!(after, before);

        let effects = e.handle(Key::Char('L'));
        assert_eq!(effects, [Effect::MixerChanged, Effect::TrackChanged(0)]);
        assert_eq!(e.locking(), None);
        assert_eq!(e.grid.tracks[0].effects[1].values, before, "put back");
        assert_eq!(locks(&e, 0), Vec::<String>::new());
        for step in 1..=3 {
            assert_eq!(locks(&e, step).len(), 2, "{:?}", locks(&e, step));
        }
        assert!(e.message.as_ref().unwrap().contains("steps 1–3"));
        // All one undo.
        assert_eq!(e.undo_len(), undo + 1);
        e.handle(Key::Char('u'));
        assert_eq!(locks(&e, 1), Vec::<String>::new());
        assert_eq!(e.grid.tracks[0].effects[1].values, before);
    }

    #[test]
    fn locking_is_cancelled_by_esc_or_leaving_the_mixer() {
        use crate::Row;
        use crate::keymap::Key;
        let mut e = editor();
        for c in "gm".chars() {
            e.handle(Key::Char(c));
        }
        e.mixer.instrument = false;
        e.mixer.row = Row::Param(0);
        let before = e.grid.tracks[0].effects[0].values.clone();
        for keys in [vec![Key::Esc], vec![Key::Char('g'), Key::Char('p')]] {
            e.handle(Key::Char('g'));
            e.handle(Key::Char('m'));
            e.handle(Key::Char('L'));
            e.handle(Key::Ctrl('a'));
            assert_ne!(e.grid.tracks[0].effects[0].values, before);
            for key in keys {
                e.handle(key);
            }
            assert_eq!(e.locking(), None);
            assert_eq!(e.grid.tracks[0].effects[0].values, before);
            assert_eq!(locks(&e, 0), Vec::<String>::new());
        }
        // Nothing changed, nothing locked.
        e.handle(Key::Char('g'));
        e.handle(Key::Char('m'));
        e.handle(Key::Char('L'));
        assert_eq!(e.handle(Key::Char('L')), []);
        assert!(e.message.as_ref().unwrap().contains("nothing locked"));
    }

    #[test]
    fn changing_a_setting_while_recording_locks_it_on_the_step_playing() {
        use crate::keymap::Key;
        use crate::{Row, Transport};
        let mut e = editor();
        for c in "gm".chars() {
            e.handle(Key::Char(c));
        }
        e.handle(Key::Char('R'));
        assert_eq!(e.mode, crate::Mode::Normal, "record stays on the mixer");
        e.mixer.instrument = false;
        e.mixer.effect = 1;
        e.mixer.row = Row::Param(0);
        let before = e.grid.tracks[0].effects[1].values.clone();
        let at = |e: &mut Editor, step| {
            e.set_transport(Some(Transport {
                positions: vec![Some((step, 0))],
                into_step: 0.6,
                count_in: 0,
            }))
        };
        at(&mut e, 2);
        let effects = e.handle(Key::Ctrl('a'));
        assert!(matches!(
            effects[..],
            [Effect::TrackChanged(0), Effect::Lock { track: 0, .. }]
        ));
        assert_eq!(
            e.grid.tracks[0].effects[1].values, before,
            "the setting stays"
        );
        assert_eq!(locks(&e, 2).len(), 1);
        let first = locks(&e, 2)[0].clone();
        // On the next step, nudged on from the last lock.
        at(&mut e, 3);
        e.handle(Key::Ctrl('a'));
        assert_eq!(locks(&e, 3).len(), 1);
        assert_ne!(locks(&e, 3), [first]);
        // The pass is one undo.
        e.handle(Key::Char('u'));
        assert!(locks(&e, 2).is_empty() && locks(&e, 3).is_empty());
        // Stopped, it's set as usual.
        e.set_transport(None);
        e.handle(Key::Ctrl('a'));
        assert_ne!(e.grid.tracks[0].effects[1].values, before);
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
