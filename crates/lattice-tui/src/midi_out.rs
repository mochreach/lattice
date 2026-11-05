//! MIDI out. Each MIDI track's instrument stamps what it plays with when it's heard (see
//! [`lattice_engine::midi_out`]); a thread of its own sends each message then, to the port the
//! track names, or to Lattice's own port if it names none. Ports are looked for again each
//! second, so gear plugged in while Lattice runs is played as soon as it's there.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use lattice_engine::midi_out::{Messages, Sent};
use midir::os::unix::VirtualOutput;
use midir::{MidiOutput, MidiOutputConnection};

use crate::log::log;
use crate::midi::short;

/// What Lattice calls itself to other MIDI software; its own port is "Lattice:out".
const CLIENT: &str = "Lattice";

/// How often to look for ports come and gone.
const SCAN: Duration = Duration::from_secs(1);

/// How often to look for what's due. A millisecond is closer than a drummer plays.
const TICK: Duration = Duration::from_millis(1);

/// What times are counted from, for [`heard_at`].
static EPOCH: OnceLock<Instant> = OnceLock::new();

/// Tracks' messages, for the thread to send, and the ports it knows of.
static OUT: OnceLock<Mutex<Out>> = OnceLock::new();

struct Out {
    tracks: Sender<(Messages, Option<String>)>,
    ports: Vec<String>,
}

/// Starts counting time, before the audio does, and the thread that sends, which opens
/// Lattice's own port.
pub fn start() {
    EPOCH.get_or_init(Instant::now);
    OUT.get_or_init(|| {
        let (tracks, receiver) = channel();
        let started = std::thread::Builder::new()
            .name("midi out".into())
            .spawn(move || run(receiver));
        if let Err(e) = started {
            log!("can't send MIDI: {e}");
        }
        Mutex::new(Out {
            tracks,
            ports: vec![],
        })
    });
}

/// `at`, in nanoseconds from when Lattice started; never 0, which means nothing's said. Called
/// on the audio thread: it neither allocates nor waits.
pub fn heard_at(at: Instant) -> u64 {
    let Some(epoch) = EPOCH.get() else {
        return 0;
    };
    (at.saturating_duration_since(*epoch).as_nanos() as u64).max(1)
}

/// Sends a track's messages to `port`, by its name or part of it, or Lattice's own.
pub fn play(messages: Messages, port: Option<String>) {
    // Not started, as in tests, there's nowhere to send it.
    if let Some(Ok(out)) = OUT.get().map(Mutex::lock) {
        // The thread's gone only if it couldn't start, which it's said.
        let _ = out.tracks.send((messages, port));
    }
}

/// Gives the thread time to send what's left, the notes off of tracks just dropped, before
/// Lattice exits.
pub fn finish() {
    std::thread::sleep(TICK * 20);
}

/// The ports MIDI can go out to, by name, as last looked for.
pub fn ports() -> Vec<String> {
    (OUT.get().and_then(|out| out.lock().ok()))
        .map(|out| out.ports.clone())
        .unwrap_or_default()
}

/// A track sending.
struct Track {
    messages: Messages,
    port: Option<String>,
}

/// A port being sent to.
struct Port {
    name: String,
    connection: MidiOutputConnection,
}

/// Sends what's come due, for as long as Lattice runs.
fn run(receiver: Receiver<(Messages, Option<String>)>) {
    let own = MidiOutput::new(CLIENT)
        .map_err(|e| e.to_string())
        .and_then(|out| out.create_virtual("out").map_err(|e| e.to_string()));
    let mut own = match own {
        Ok(own) => Some(own),
        Err(e) => {
            log!("can't open Lattice's own MIDI port: {e}");
            None
        }
    };
    let mut tracks: Vec<Track> = vec![];
    let mut ports: Vec<Port> = vec![];
    // Messages taken from the tracks, to send when they're due: when, which port, what.
    let mut waiting: Vec<(u64, Option<String>, Sent)> = vec![];
    let mut scanned: Option<Instant> = None;
    loop {
        tracks.extend((receiver.try_iter()).map(|(messages, port)| Track { messages, port }));
        let now = Instant::now();
        if scanned.is_none_or(|at| now.duration_since(at) >= SCAN) {
            scanned = Some(now);
            scan(&tracks, &mut ports);
        }
        // Gone once its instrument's dropped, and its last notes off taken.
        tracks.retain_mut(|track| {
            let gone = track.messages.is_abandoned();
            while let Ok(sent) = track.messages.pop() {
                waiting.push((sent.at, track.port.clone(), sent));
            }
            !gone
        });
        waiting.sort_by_key(|w| w.0);
        let due = heard_at(Instant::now());
        let until = waiting.partition_point(|w| w.0 <= due);
        for (_, port, sent) in waiting.drain(..until) {
            let connection = match &port {
                None => own.as_mut(),
                Some(wanted) => (ports.iter_mut())
                    .find(|p| matches(&p.name, wanted))
                    .map(|p| &mut p.connection),
            };
            if let Some(connection) = connection
                && let Err(e) = connection.send(sent.bytes())
            {
                log!("midi out: {e}");
            }
        }
        std::thread::sleep(TICK);
    }
}

/// Connects to the ports the tracks name that have appeared, lets go of those gone, and notes
/// every port there is for `:port`.
fn scan(tracks: &[Track], ports: &mut Vec<Port>) {
    let Ok(lister) = MidiOutput::new(CLIENT) else {
        return;
    };
    let there: Vec<_> = (lister.ports().into_iter())
        .filter_map(|port| Some((lister.port_name(&port).ok()?, port)))
        .filter(|(name, _)| !name.contains("Through") && !name.starts_with(CLIENT))
        .collect();
    if let Some(Ok(mut out)) = OUT.get().map(Mutex::lock) {
        out.ports = there
            .iter()
            .map(|(name, _)| short(name).to_string())
            .collect();
    }
    ports.retain(|p| {
        let here = there.iter().any(|(name, _)| *name == p.name);
        if !here {
            log!("midi out gone: {}", p.name);
        }
        here
    });
    for wanted in tracks.iter().filter_map(|t| t.port.as_deref()) {
        if ports.iter().any(|p| matches(&p.name, wanted)) {
            continue;
        }
        let Some((name, port)) = there.iter().find(|(name, _)| matches(name, wanted)) else {
            continue;
        };
        let Ok(out) = MidiOutput::new(CLIENT) else {
            continue;
        };
        match out.connect(port, "out") {
            Ok(connection) => {
                log!("midi out: {name}");
                ports.push(Port {
                    name: name.clone(),
                    connection,
                });
            }
            Err(e) => log!("can't send to {name}: {e}"),
        }
    }
}

/// Whether the port `name` is the one `wanted` names, by any part of its name, in any case.
pub fn matches(name: &str, wanted: &str) -> bool {
    short(name).to_lowercase().contains(&wanted.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_port_by_part_of_its_name() {
        let name = "KeyStep 37:KeyStep 37 MIDI 1 20:0";
        assert!(matches(name, "keystep"));
        assert!(matches(name, "Step 3"));
        assert!(!matches(name, "MIDI 1"), "ALSA's part isn't its name");
        assert!(!matches(name, "Minilogue"));
    }
}
