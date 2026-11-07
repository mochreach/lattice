//! Recording an app's own sound, where it can't be done: it goes through PulseAudio, which is
//! Linux's. Elsewhere there are no apps to choose, and an `app:` input saved on Linux says why
//! it can't be recorded.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use anyhow::{Result, bail};
use lattice_editor::{APP, Device};
use rtrb::Producer;

/// The app the input `id` records, if it's an app's.
pub fn app(id: &str) -> Option<&str> {
    id.strip_prefix(APP)
}

/// What the list of inputs calls the app.
pub fn name(app: &str) -> String {
    format!("app: {app}")
}

/// No apps can be recorded here.
pub fn list() -> Vec<Device> {
    vec![]
}

/// Never made: there's nothing to record with.
pub struct Capture;

/// Says that an app can't be recorded here.
pub fn record(
    app: &str,
    _rate: f32,
    _latency: Arc<AtomicU64>,
    _frames: Producer<f32>,
) -> Result<(Capture, bool)> {
    bail!("can't record {app} on its own: that needs PulseAudio, on Linux")
}
