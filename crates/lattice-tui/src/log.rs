//! A log of what happened, for when something goes wrong: `~/.local/state/lattice/lattice.log`,
//! with the last run's kept as `lattice.log.1`. Only the UI thread writes to it; the audio
//! threads count their troubles in [`HEALTH`], which the UI looks at each frame (see [`Watch`]).
//! A panic is written there too, with its backtrace, as the terminal is no place for it.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static FILE: Mutex<Option<File>> = Mutex::new(None);

/// Writes a line to the log, if there is one.
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::log::write(&format!($($arg)*))
    };
}
pub(crate) use log;

/// Where the log goes: `$XDG_STATE_HOME/lattice`, or `~/.local/state/lattice`.
pub fn path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("lattice").join("lattice.log"))
}

/// Starts a new log, keeping the last as `lattice.log.1`, and writes panics to it. Without a
/// log, Lattice carries on all the same.
pub fn start() {
    let Some(path) = path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::rename(&path, path.with_extension("log.1"));
    let Ok(file) = File::create(&path) else {
        return;
    };
    *FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(file);
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("unnamed");
        write(&format!("panic in thread {name}: {info}\n{backtrace}"));
        default(info);
    }));
    log!("lattice {} started", env!("CARGO_PKG_VERSION"));
}

pub fn write(line: &str) {
    // A panic while it's held mustn't stop the panic itself being written.
    let mut file = FILE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(file) = file.as_mut() {
        let _ = writeln!(file, "{} {line}", timestamp(SystemTime::now()));
    }
}

/// `2026-11-01 14:03:22.123`, in UTC.
fn timestamp(at: SystemTime) -> String {
    let since = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs();
    let (days, time) = (secs / 86_400, secs % 86_400);
    // Days to a date, after Howard Hinnant's `civil_from_days`.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year}-{month:02}-{day:02} {:02}:{:02}:{:02}.{:03}",
        time / 3600,
        time / 60 % 60,
        time % 60,
        since.subsec_millis()
    )
}

/// What the audio threads count going wrong, without allocating or waiting.
pub struct Health {
    /// Output blocks played as silence because the engine was busy.
    pub missed: AtomicU64,
    /// Output blocks that took longer to render than they last.
    pub late: AtomicU64,
    /// The longest a block has taken, as thousandths of its length, since the UI last looked.
    pub worst: AtomicU64,
    /// Input frames lost because the engine wasn't taking them.
    pub dropped: AtomicU64,
    /// Errors the streams have reported.
    pub errors: AtomicU64,
    /// The last of them, if it could be kept.
    pub error: Mutex<Option<String>>,
}

pub static HEALTH: Health = Health {
    missed: AtomicU64::new(0),
    late: AtomicU64::new(0),
    worst: AtomicU64::new(0),
    dropped: AtomicU64::new(0),
    errors: AtomicU64::new(0),
    error: Mutex::new(None),
};

impl Health {
    /// Called by a stream when it fails. Not on the audio callback, so it may allocate, but it
    /// never waits for the lock.
    pub fn stream_error(&self, stream: &str, error: &dyn std::fmt::Display) {
        self.errors.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut slot) = self.error.try_lock() {
            *slot = Some(format!("{stream}: {error}"));
        }
    }

    /// Times an output block of `frames` at `rate` that started at `start`.
    pub fn block(&self, start: Instant, frames: usize, rate: f32) {
        let length = frames as f64 / f64::from(rate);
        let load = start.elapsed().as_secs_f64() / length;
        if load > 1.0 {
            self.late.fetch_add(1, Ordering::Relaxed);
        }
        self.worst
            .fetch_max((load * 1000.0) as u64, Ordering::Relaxed);
    }
}

/// How often the health is written to the log, at most.
const EVERY: Duration = Duration::from_secs(1);
/// Late blocks in one look that are worth telling the player about.
const STRUGGLING: u64 = 3;

/// The UI's side of [`HEALTH`]: what it had counted when it last looked.
pub struct Watch {
    seen: [u64; 4],
    leaked: u64,
    last: Instant,
    /// The worst load seen since starting, as thousandths.
    peak: u64,
}

impl Watch {
    pub fn new() -> Self {
        Self {
            seen: [0; 4],
            leaked: 0,
            last: Instant::now(),
            peak: 0,
        }
    }

    /// Writes anything new to the log, at most once a second, returning what the player
    /// should hear about.
    pub fn look(&mut self, leaked: u64) -> Option<String> {
        let now = Instant::now();
        if now.duration_since(self.last) < EVERY {
            return None;
        }
        self.last = now;
        let h = &HEALTH;
        let counts = [&h.missed, &h.late, &h.dropped, &h.errors].map(|c| c.load(Ordering::Relaxed));
        let [missed, late, dropped, errors] = std::array::from_fn(|i| counts[i] - self.seen[i]);
        self.seen = counts;
        let leaks = leaked - self.leaked;
        self.leaked = leaked;
        let worst = h.worst.swap(0, Ordering::Relaxed);
        let mut problem = None;
        if errors > 0 {
            let error = h.error.lock().unwrap_or_else(|e| e.into_inner()).take();
            let error = error.unwrap_or_else(|| "unknown".into());
            log!("audio error ({errors} since last): {error}");
            problem = Some(format!("audio error: {error}"));
        }
        if missed + late + dropped + leaks > 0 {
            log!(
                "audio: {late} late, {missed} missed, {dropped} input frames dropped, \
                 {leaks} leaked; worst block {:.0}% of its time",
                worst as f64 / 10.0
            );
        }
        if worst > self.peak.max(900) {
            log!(
                "audio: a block took {:.0}% of its time",
                worst as f64 / 10.0
            );
        }
        self.peak = self.peak.max(worst);
        if late >= STRUGGLING && problem.is_none() {
            problem = Some(format!(
                "audio is struggling: {late} blocks late in the last second"
            ));
        }
        problem
    }
}

impl Drop for Watch {
    /// How close the audio came to falling behind, so a quiet log still says how much room
    /// there was.
    fn drop(&mut self) {
        let peak = self.peak.max(HEALTH.worst.load(Ordering::Relaxed));
        let late = HEALTH.late.load(Ordering::Relaxed);
        let missed = HEALTH.missed.load(Ordering::Relaxed);
        log!(
            "audio this run: the worst block took {:.0}% of its time; {late} late, {missed} missed",
            peak as f64 / 10.0
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_dates() {
        let at = UNIX_EPOCH + Duration::from_millis(1_793_545_402_123);
        assert_eq!(timestamp(at), "2026-11-01 15:03:22.123");
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01 00:00:00.000");
        let leap = UNIX_EPOCH + Duration::from_secs(951_782_400);
        assert_eq!(timestamp(leap), "2000-02-29 00:00:00.000");
    }
}
