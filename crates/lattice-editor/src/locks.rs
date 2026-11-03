//! Locks: settings of a track's instrument or effects that change on a step, set with `:lock`
//! and cleared with `:unlock` on every step the selections reach.

use lattice_core::macros::MACRO_INFO;
use lattice_core::{
    Change, Curve, Fx, Grid, Instrument, Kind, Lock, LockColumn, MACRO, MAX_LOCKS, ParamInfo, Step,
    Track,
};

use crate::{Editor, Effect, Row, Strip, TrackId, changed};

/// What the first `L` on the mixer page remembers: a track's settings as they were, and how far
/// back undo went.
#[derive(Debug, Clone)]
pub(crate) struct Locking {
    track: TrackId,
    instrument: Instrument,
    effects: Vec<Fx>,
    macros: Vec<f32>,
    undo: usize,
}

impl Locking {
    /// Whether the track still has the same instrument and effects, so its settings line up.
    pub(crate) fn matches(&self, editor: &Editor) -> bool {
        let track = editor.track_index(self.track);
        track
            .and_then(|t| editor.grid.tracks.get(t))
            .is_some_and(|track| {
                track.instrument.kind == self.instrument.kind
                    && track.effects.len() == self.effects.len()
                    && track.macros.len() == self.macros.len()
                    && (track.effects.iter().zip(&self.effects)).all(|(a, b)| a.kind == b.kind)
            })
    }
}

/// A setting a lock can hold on one track, found by name: one of its instrument's or effects'
/// parameters, or a macro.
struct Setting {
    fx: Option<u8>,
    /// The parameter's id, or the macro's index.
    param: u8,
    info: &'static ParamInfo,
}

impl Setting {
    fn new(fx: Option<u8>, info: &'static ParamInfo) -> Self {
        Self {
            fx,
            param: info.id as u8,
            info,
        }
    }

    fn of_macro(index: usize) -> Self {
        Self {
            fx: Some(MACRO),
            param: index as u8,
            info: &MACRO_INFO,
        }
    }

    /// What the setting is called on `track`: `cutoff` on the instrument, `filter.cutoff` on an
    /// effect, a macro by its name.
    fn name(&self, track: &Track) -> String {
        if self.fx == Some(MACRO) {
            return (track.macros.get(usize::from(self.param)))
                .map_or_else(|| "macro?".into(), |m| m.name.clone());
        }
        match self.fx.and_then(|fx| track.effects.get(usize::from(fx))) {
            Some(effect) => format!("{}.{}", effect.kind.name(), self.info.name),
            None => self.info.name.into(),
        }
    }

    fn lock(&self, value: f32) -> Lock {
        self.column().lock(value, Change::Set)
    }

    fn column(&self) -> LockColumn {
        LockColumn {
            fx: self.fx,
            param: self.param,
        }
    }
}

/// Gives `track` a column for each of `locks`' settings, unless it has them all and there's no
/// room: then says so, and leaves it be.
fn add_columns<'a>(
    track: &mut Track,
    locks: impl IntoIterator<Item = &'a Lock>,
) -> Result<(), String> {
    let mut with = track.clone();
    for lock in locks {
        if !with.add_column(lock.column()) {
            return Err(format!(
                "{} has its {MAX_LOCKS} lock columns already: :unlock one along the whole track",
                track.name
            ));
        }
    }
    track.columns = with.columns;
    Ok(())
}

/// The setting `fx` and `param` name on `track`, if it's still there.
fn setting(track: &Track, fx: Option<u8>, param: u8) -> Option<Setting> {
    let params = match fx {
        Some(MACRO) => {
            return (usize::from(param) < track.macros.len())
                .then(|| Setting::of_macro(usize::from(param)));
        }
        None => track.instrument.kind.params(),
        Some(fx) => track.effects.get(usize::from(fx))?.kind.params(),
    };
    Some(Setting::new(fx, params.get(usize::from(param))?))
}

/// What's known of the setting `column` locks on `track`, if it's still there.
pub(crate) fn column_info(track: &Track, column: LockColumn) -> Option<&'static ParamInfo> {
    setting(track, column.fx, column.param).map(|s| s.info)
}

/// A track's lock column, as the grid heads it.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnLabel {
    pub column: LockColumn,
    /// The setting's own name, without its effect's: `cutoff`, `mix`, a macro's. When another
    /// column's setting has the same name, an effect's goes after its number, as `:lock 2.cutoff`
    /// finds it: `2cutoff`.
    pub name: String,
    /// What `:lock` calls it: `cutoff`, `filter.cutoff`.
    pub full: String,
    /// The key that goes to it after `f`: a letter of its name no other field has.
    pub key: char,
}

/// A lock's value as its column shows it, three wide: `45` across the setting's range from 00 to
/// 99, `~90` sliding there, or `--` put back.
pub fn lock_text(track: &Track, lock: &Lock) -> String {
    let Some(setting) = setting(track, lock.fx, lock.param) else {
        return " ??".into();
    };
    let value = (setting.info.fraction(lock.value).clamp(0.0, 1.0) * 99.0).round() as u8;
    match lock.change {
        Change::Set => format!(" {value:02}"),
        Change::Slide => format!("~{value:02}"),
        Change::Reset => " --".into(),
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
        Some(Setting::new(None, info))
    };
    let effect = |fx: usize, param: &str| {
        let effect = track.effects.get(fx)?;
        let info = effect.kind.params().iter().find(|p| p.name == param)?;
        Some(Setting::new(Some(fx as u8), info))
    };
    let macro_named = |name: &str| {
        (track.macros.iter())
            .position(|m| m.name == name)
            .map(Setting::of_macro)
    };
    match name.split_once('.') {
        Some(("inst", param)) => find(track, param).filter(|s| s.fx.is_none()),
        Some(("macro", name)) => macro_named(name),
        Some((device, param)) => match device.parse::<usize>() {
            Ok(n) => effect(n.checked_sub(1)?, param),
            Err(_) => (track.effects.iter().enumerate())
                .filter(|(_, e)| e.kind.name() == device)
                .find_map(|(fx, _)| effect(fx, param)),
        },
        None => instrument()
            .or_else(|| (0..track.effects.len()).find_map(|fx| effect(fx, name)))
            .or_else(|| macro_named(name)),
    }
}

fn step_mut(grid: &mut Grid, t: usize, step: usize) -> Option<&mut Step> {
    grid.tracks.get_mut(t)?.steps.get_mut(step)
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
    /// on the setting's own curve. `~VALUE` slides there from the setting's lock before, and
    /// `--` puts it back to the mixer's. With no value, it gives the tracks a column for the
    /// setting, to fill in on the grid.
    pub(crate) fn lock(&mut self, args: &str) -> Result<Vec<Effect>, String> {
        let (name, values) = match args.trim().split_once(' ') {
            Some((n, v)) => (n, v.trim()),
            None => (args.trim(), ""),
        };
        if name.is_empty() {
            return Err("lock which setting, at what? :lock cutoff 800".into());
        }
        let (change, values) = match values {
            "--" => (Change::Reset, ""),
            v => match v.strip_prefix('~') {
                Some(v) => (Change::Slide, v),
                None => (Change::Set, v),
            },
        };
        let (from, to) = values.split_once("..").unwrap_or((values, values));
        let columns: Vec<_> = self
            .selections
            .iter()
            .flat_map(|s| self.columns(s.region()))
            .collect();

        // Every track has to have it, and room for its column, before anything changes.
        let mut edits = vec![];
        let mut tracks = vec![];
        for (t, steps) in &columns {
            let track = self.grid.tracks.get(*t).ok_or("no such track")?;
            let setting = find(track, name).ok_or_else(|| {
                let names: Vec<String> = settings(track).iter().map(|s| s.name(track)).collect();
                format!("{} has no {name}: try {}", track.name, names.join(", "))
            })?;
            let mut with = track.clone();
            add_columns(&mut with, [&setting.lock(0.0)])?;
            tracks.push((*t, with.columns));
            if values.is_empty() && change != Change::Reset {
                continue;
            }
            let (from, to) = match change {
                Change::Reset => (setting.info.min, setting.info.min),
                _ => (value(setting.info, from)?, value(setting.info, to)?),
            };
            let (a, b) = (setting.info.fraction(from), setting.info.fraction(to));
            for (i, step) in steps.clone().enumerate() {
                let along = i as f32 / (steps.len().max(2) - 1) as f32;
                let value = if from == to {
                    from
                } else {
                    setting.info.at(a + (b - a) * along)
                };
                edits.push((*t, step, setting.column().lock(value, change)));
            }
        }
        self.checkpoint();
        for (t, columns) in tracks {
            if let Some(track) = self.grid.tracks.get_mut(t) {
                track.columns = columns;
            }
        }
        // Each step was found above, and its track has a column for every lock, so room.
        for &(t, step, lock) in &edits {
            if let Some(step) = step_mut(&mut self.grid, t, step) {
                step.locks.set(lock);
            }
        }
        Ok(changed(columns.into_iter().map(|(t, _)| t)))
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
        let (Some(track), Some(&id)) = (self.grid.tracks.get(t), self.ids.get(t)) else {
            return Err("no such track".into());
        };
        self.locking = Some(Locking {
            track: id,
            instrument: track.instrument.clone(),
            effects: track.effects.to_vec(),
            macros: track.macros.iter().map(|m| m.value).collect(),
            undo: self.undo_len(),
        });
        Ok(vec![])
    }

    /// Whether the mixer is waiting for a second `L`, and on which track.
    pub fn locking(&self) -> Option<usize> {
        self.locking
            .as_ref()
            .and_then(|l| self.track_index(l.track))
    }

    /// Puts back the settings `L` remembered, and forgets the changes' undo steps, as though
    /// they'd never been made. If the track or its chain changed meanwhile, leaves it be.
    pub(crate) fn cancel_locking(&mut self) -> Vec<Effect> {
        let Some(locking) = self.locking.take() else {
            return vec![];
        };
        let Some(t) = self
            .track_index(locking.track)
            .filter(|_| locking.matches(self))
        else {
            return vec![];
        };
        let Some(track) = self.grid.tracks.get_mut(t) else {
            return vec![];
        };
        track.instrument.values = locking.instrument.values.clone();
        for (fx, was) in track.effects.iter_mut().zip(&locking.effects) {
            fx.values = was.values.clone();
        }
        for (m, &was) in track.macros.iter_mut().zip(&locking.macros) {
            m.value = was;
        }
        self.truncate_undo(locking.undo);
        vec![Effect::MixerChanged, Effect::TrackChanged(t)]
    }

    fn finish_locking(&mut self) -> Result<Vec<Effect>, String> {
        let Some(locking) = self.locking.clone() else {
            return Err("nothing's being locked".into());
        };
        let Some((t, track)) = (self.track_index(locking.track))
            .filter(|_| locking.matches(self))
            .and_then(|t| Some((t, self.grid.tracks.get(t)?)))
        else {
            self.locking = None;
            return Err("the track or its chain changed; nothing locked".into());
        };
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
                if let Some(&value) = now.get(p)
                    && was.get(p) != Some(&value)
                {
                    let setting = Setting::new(fx, info);
                    let lock = setting.lock(value);
                    new.push((setting, lock));
                }
            }
        }
        for (m, (&was, now)) in locking.macros.iter().zip(&track.macros).enumerate() {
            if was != now.value {
                let setting = Setting::of_macro(m);
                let lock = setting.lock(now.value);
                new.push((setting, lock));
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
        let mut with = track.clone();
        add_columns(&mut with, new.iter().map(|(_, lock)| lock))?;
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
        if let Some(track) = self.grid.tracks.get_mut(t) {
            track.columns = with.columns;
        }
        for &step in &steps {
            for (_, lock) in &new {
                if let Some(step) = step_mut(&mut self.grid, t, step) {
                    step.locks.set(*lock);
                }
            }
        }
        self.message = Some(message);
        // Putting it back changed the track already.
        for effect in changed([t]) {
            if !effects.contains(&effect) {
                effects.push(effect);
            }
        }
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
        let info = shown.params.get(p)?;
        let lock = Lock {
            fx: shown.slot.map(|s| s as u8),
            param: info.id as u8,
            value: info.clamp(value),
            ..Default::default()
        };
        self.record_lock_of(t, lock)
    }

    /// Locks `lock` on track `t`'s step playing, if recording: see [`Editor::record_lock`].
    pub(crate) fn record_lock_of(&mut self, t: usize, lock: Lock) -> Option<Vec<Effect>> {
        if !self.record || self.locking.is_some() {
            return None;
        }
        let len = self.track_len(t);
        let at = (self.transport.as_ref()?).at(t, len)?;
        // Counting in, there's no step yet to lock.
        if at < 0.0 || len == 0 {
            return Some(vec![]);
        }
        let at = at.floor() as u64;
        let (step, pass) = ((at % len as u64) as usize, at / len as u64);
        let mut with = self.grid.tracks.get(t)?.clone();
        if let Err(message) = add_columns(&mut with, [&lock]) {
            self.message = Some(message);
            return Some(vec![]);
        }
        if self.take != Some(pass) {
            self.checkpoint();
            self.take = Some(pass);
        }
        let Some(track) = self.grid.tracks.get_mut(t) else {
            return Some(vec![]);
        };
        track.columns = with.columns;
        // The step playing is within the track, as `len` is its length.
        if let Some(step) = track.steps.get_mut(step) {
            step.locks.set(lock);
        }
        self.message = Some(format!(
            "◆ {} on {} step {step}",
            describe(track, &lock),
            track.name
        ));
        self.live_lock = self.ids.get(t).map(|&id| (id, lock));
        Some(vec![
            Effect::TrackChanged(t),
            Effect::Lock { track: t, lock },
        ])
    }

    /// `:unlock [PARAM]`: one setting's locks, or all of them, from every step the selections
    /// reach. Along a whole track, its column goes too.
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
                name => {
                    let track = self.grid.tracks.get(*t).ok_or("no such track")?;
                    Some(find(track, name).ok_or_else(|| format!("{} has no {name}", track.name))?)
                }
            };
            wanted.push(setting.map(|s| (s.fx, s.param)));
        }
        self.checkpoint();
        for ((t, steps), wanted) in columns.iter().zip(wanted) {
            let Some(track) = self.grid.tracks.get_mut(*t) else {
                continue;
            };
            for step in track.steps.get_mut(steps.clone()).into_iter().flatten() {
                match wanted {
                    Some((fx, param)) => step.locks.retain(|l| (l.fx, l.param) != (fx, param)),
                    None => step.locks.clear(),
                }
            }
            if steps.start == 0 && steps.end >= track.steps.len() {
                track.columns = track.lock_columns();
                match wanted {
                    Some((fx, param)) => track.columns.retain(|c| (c.fx, c.param) != (fx, param)),
                    None => track.columns.clear(),
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
        // A track there isn't has no steps of its own, but its patterns' lanes are still seen to.
        let steps = (self.grid.tracks.get_mut(t)).map_or(&mut [][..], |track| &mut track.steps);
        let mut changed = false;
        for step in steps.iter_mut().chain(lanes) {
            let before = step.locks;
            step.locks.retain(&mut keep);
            changed |= step.locks != before;
        }
        if let Some(track) = self.grid.tracks.get_mut(t) {
            track.columns.retain_mut(|column| {
                let mut lock = column.lock(0.0, Change::Set);
                let kept = keep(&mut lock);
                changed |= !kept || lock.column() != *column;
                *column = lock.column();
                kept
            });
        }
        changed
    }

    /// Moves effects' locks along after an effect's been put in at `index`, or drops its own
    /// and moves the rest back after it's been taken out.
    pub(crate) fn shift_locks(&mut self, t: usize, index: usize, inserted: bool) -> bool {
        let index = index as u8;
        self.retain_locks(t, |lock| match lock.fx {
            _ if lock.is_macro() => true,
            Some(fx) if fx == index && !inserted => false,
            Some(fx) if fx >= index => {
                lock.fx = Some(if inserted { fx + 1 } else { fx - 1 });
                true
            }
            _ => true,
        })
    }

    /// Track `t`'s lock columns, named, each with its key. `n`, `v` and `g` go to the note,
    /// velocity and gestures, so a column takes the first letter of its name that's free, or a
    /// number.
    pub fn lock_columns(&self, t: usize) -> Vec<ColumnLabel> {
        let Some(track) = self.grid.tracks.get(t) else {
            return vec![];
        };
        let columns = track.lock_columns();
        let settings: Vec<_> = (columns.iter())
            .map(|c| setting(track, c.fx, c.param))
            .collect();
        let own = |s: &Option<Setting>| match s {
            Some(s) if s.fx == Some(MACRO) => s.name(track),
            Some(s) => s.info.name.to_string(),
            None => "?".into(),
        };
        let names: Vec<String> = settings.iter().map(own).collect();
        let mut taken = vec!['n', 'v', 'g', 'G', 'C'];
        (columns.into_iter().zip(&settings).zip(&names).enumerate())
            .map(|(i, ((column, setting), own))| {
                let clashes = names.iter().filter(|n| *n == own).count() > 1;
                let name = match column.fx {
                    Some(fx) if clashes && fx != MACRO => format!("{}{own}", fx + 1),
                    _ => own.clone(),
                };
                let full = setting
                    .as_ref()
                    .map_or_else(|| "?".into(), |s| s.name(track));
                let key = (name.chars())
                    .filter(char::is_ascii_alphabetic)
                    .find(|c| !taken.contains(c))
                    .or_else(|| char::from_digit(i as u32 + 1, 10))
                    .unwrap_or('?');
                taken.push(key);
                ColumnLabel {
                    column,
                    name,
                    full,
                    key,
                }
            })
            .collect()
    }

    /// The gestures on a step, as `R3: retrigger, 3 hits`, for showing.
    pub fn describe_gestures(&self, track: usize, step: usize) -> Vec<String> {
        let Some(track) = self.grid.tracks.get(track) else {
            return vec![];
        };
        let Some(step) = track.steps.get(step) else {
            return vec![];
        };
        let kind = track.instrument.kind;
        (step.gestures())
            .map(|g| match g.works_on(kind) {
                true => g.describe(),
                false => format!("{} (nothing on a {})", g.describe(), kind.name()),
            })
            .collect()
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
            .map(|lock| describe(track, lock))
            .collect()
    }
}

/// A lock, as `cutoff 800Hz`, `cutoff ~800Hz` sliding, or `cutoff --` put back, for showing.
fn describe(track: &Track, lock: &Lock) -> String {
    let format = |info: &ParamInfo| match lock.change {
        Change::Set => info.format(lock.value),
        Change::Slide => format!("~{}", info.format(lock.value)),
        Change::Reset => "--".into(),
    };
    if lock.is_macro() {
        let setting = Setting::of_macro(usize::from(lock.param));
        return format!("{} {}", setting.name(track), format(&MACRO_INFO));
    }
    let params = match lock.fx {
        None => Some(track.instrument.kind.params()),
        Some(fx) => track.effects.get(usize::from(fx)).map(|e| e.kind.params()),
    };
    match params.and_then(|p| p.get(usize::from(lock.param))) {
        Some(info) => {
            let setting = Setting::new(lock.fx, info);
            format!("{} {}", setting.name(track), format(info))
        }
        // Its effect has gone: it does nothing until one's back in its place.
        None => format!("fx{}.{}?", lock.fx.map_or(0, |f| f + 1), lock.param),
    }
}

/// Every setting on `track`, for suggesting.
fn settings(track: &Track) -> Vec<Setting> {
    let instrument = track
        .instrument
        .kind
        .params()
        .iter()
        .map(|info| Setting::new(None, info));
    let effects = track.effects.iter().enumerate().flat_map(|(fx, e)| {
        (e.kind.params().iter()).map(move |info| Setting::new(Some(fx as u8), info))
    });
    let macros = (0..track.macros.len()).map(Setting::of_macro);
    instrument.chain(effects).chain(macros).collect()
}

/// Every setting on `track` by the name `:lock` finds it by, with its value now, for the
/// command line's suggestions.
pub(crate) fn suggest_settings(track: &Track) -> Vec<(String, String)> {
    (settings(track).iter())
        .map(|setting| {
            let param = usize::from(setting.param);
            let value = match setting.fx {
                Some(MACRO) => track.macros.get(param).map(|m| m.value),
                Some(fx) => {
                    (track.effects.get(usize::from(fx))).and_then(|e| e.values.get(param).copied())
                }
                None => track.instrument.values.get(param).copied(),
            };
            let value = value.unwrap_or(setting.info.default);
            (setting.name(track), setting.info.format(value))
        })
        .collect()
}

/// The choices of the setting called `name` on `track`, a filter's types, say, if it has them.
pub(crate) fn suggest_choices(track: &Track, name: &str) -> Vec<&'static str> {
    match find(track, name).map(|setting| setting.info.curve) {
        Some(Curve::Choice(choices)) => choices.to_vec(),
        _ => vec![],
    }
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
                beats_per_bar: 4,
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
        assert!(run(&mut e, "lock").is_err());
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
            "acid has its 4 lock columns already: :unlock one along the whole track"
        );
        assert_eq!(locks(&e, 0).len(), 4, "and nothing changed");
    }

    #[test]
    fn columns_of_settings_with_the_same_name_say_whose() {
        let mut e = editor();
        e.grid.tracks[0].effects.push(Fx::new(FxKind::Delay));
        for line in [
            "lock cutoff",
            "lock filter.cutoff",
            "lock 1.mix",
            "lock 3.mix",
        ] {
            run(&mut e, line).unwrap();
        }
        let labels: Vec<(String, String, char)> = (e.lock_columns(0).into_iter())
            .map(|l| (l.name, l.full, l.key))
            .collect();
        assert_eq!(
            labels,
            [
                ("cutoff".into(), "cutoff".into(), 'c'),
                ("2cutoff".into(), "filter.cutoff".into(), 'u'),
                ("1mix".into(), "delay.mix".into(), 'm'),
                ("3mix".into(), "delay.mix".into(), 'i'),
            ]
        );
    }

    #[test]
    fn lock_adds_columns_slides_and_puts_back() {
        let mut e = editor();
        let columns = |e: &Editor| e.grid.tracks[0].columns.len();
        // With no value, just the column.
        run(&mut e, "lock cutoff").unwrap();
        assert_eq!(columns(&e), 1);
        assert!(locks(&e, 0).is_empty());
        run(&mut e, "lock decay ~0.2").unwrap();
        e.handle(crate::Key::Char('j'));
        run(&mut e, "lock cutoff --").unwrap();
        assert_eq!(locks(&e, 0), ["decay ~0.2s"]);
        assert_eq!(locks(&e, 1), ["cutoff --"]);
        assert_eq!(columns(&e), 2);
        // Unlocked along the whole track, the column goes; on a step, it stays.
        run(&mut e, "unlock cutoff").unwrap();
        assert_eq!(columns(&e), 2);
        e.handle(crate::Key::Char('%'));
        run(&mut e, "unlock cutoff").unwrap();
        assert_eq!(columns(&e), 1);
        assert_eq!(locks(&e, 0), ["decay ~0.2s"]);
        // Its effect gone, so is its column.
        run(&mut e, "lock delay.mix").unwrap();
        e.grid.tracks[0].effects.remove(0);
        e.shift_locks(0, 0, false);
        assert_eq!(columns(&e), 1);
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
