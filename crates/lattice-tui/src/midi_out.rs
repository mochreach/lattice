//! MIDI out. Each MIDI track's instrument stamps what it plays with when it's heard (see
//! [`lattice_engine::midi_out`]); a thread of its own sends each message then, to the port the
//! track names, or to Lattice's own port if it names none. Ports are looked for again each
//! second, so gear plugged in while Lattice runs is played as soon as it's there. The engine's
//! MIDI clock goes the same way, to every port `:clock` names.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use lattice_engine::midi_out::{Messages, Sent};
#[cfg(unix)]
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
    orders: Sender<Order>,
    ports: Vec<String>,
}

/// What the thread's told.
enum Order {
    /// Send a track's messages to its port, by name, or to Lattice's own.
    Track(Messages, Option<String>),
    /// Send the engine's clock to the clock's ports.
    Clock(Messages),
    /// The clock's ports, by name, `lattice` for Lattice's own.
    ClockPorts(Vec<String>),
}

/// Starts counting time, before the audio does, and the thread that sends, which opens
/// Lattice's own port.
pub fn start() {
    EPOCH.get_or_init(Instant::now);
    OUT.get_or_init(|| {
        let (orders, receiver) = channel();
        let started = std::thread::Builder::new()
            .name("midi out".into())
            .spawn(move || run(receiver));
        if let Err(e) = started {
            log!("can't send MIDI: {e}");
        }
        Mutex::new(Out {
            orders,
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

fn order(order: Order) {
    // Not started, as in tests, there's nowhere to send it.
    if let Some(Ok(out)) = OUT.get().map(Mutex::lock) {
        // The thread's gone only if it couldn't start, which it's said.
        let _ = out.orders.send(order);
    }
}

/// Sends a track's messages to `port`, by its name or part of it, or Lattice's own.
pub fn play(messages: Messages, port: Option<String>) {
    order(Order::Track(messages, port));
}

/// Sends the engine's MIDI clock to the clock's ports.
pub fn clock(messages: Messages) {
    order(Order::Clock(messages));
}

/// Sends clock to `ports`, by name or part of one, `lattice` for Lattice's own.
pub fn clock_to(ports: Vec<String>) {
    order(Order::ClockPorts(ports));
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

/// A track sending, or the clock.
struct Track {
    messages: Messages,
    to: To,
}

/// Where a message goes.
#[derive(Debug, Clone, PartialEq)]
enum To {
    /// A port, by name, or Lattice's own.
    Port(Option<String>),
    /// Each of the clock's ports.
    Clock,
}

/// A port being sent to.
struct Port {
    name: String,
    connection: MidiOutputConnection,
}

/// Lattice's own port, which other programs connect to, rather than one it sends to.
#[cfg(unix)]
fn own_port() -> Result<MidiOutputConnection, String> {
    MidiOutput::new(CLIENT)
        .map_err(|e| e.to_string())
        .and_then(|out| out.create_virtual("out").map_err(|e| e.to_string()))
}

/// Windows can't make a port of its own: notes go only to ports named.
#[cfg(not(unix))]
fn own_port() -> Result<MidiOutputConnection, String> {
    Err("Windows has no virtual MIDI ports: name a port to send to".into())
}

/// Sends what's come due, for as long as Lattice runs.
fn run(receiver: Receiver<Order>) {
    let mut own = match own_port() {
        Ok(own) => Some(own),
        Err(e) => {
            log!("can't open Lattice's own MIDI port: {e}");
            None
        }
    };
    let mut tracks: Vec<Track> = vec![];
    let mut clock_ports: Vec<Option<String>> = vec![];
    let mut ports: Vec<Port> = vec![];
    // Messages taken from the tracks, to send when they're due: when, where, what.
    let mut waiting: Vec<(u64, To, Sent)> = vec![];
    let mut scanned: Option<Instant> = None;
    loop {
        for order in receiver.try_iter() {
            match order {
                Order::Track(messages, port) => tracks.push(Track {
                    messages,
                    to: To::Port(port),
                }),
                Order::Clock(messages) => tracks.push(Track {
                    messages,
                    to: To::Clock,
                }),
                Order::ClockPorts(named) => {
                    clock_ports = (named.into_iter())
                        .map(|p| (!p.eq_ignore_ascii_case(CLIENT)).then_some(p))
                        .collect();
                    scanned = None;
                }
            }
        }
        let now = Instant::now();
        if scanned.is_none_or(|at| now.duration_since(at) >= SCAN) {
            scanned = Some(now);
            let wanted = (tracks.iter())
                .filter_map(|t| match &t.to {
                    To::Port(port) => port.as_deref(),
                    To::Clock => None,
                })
                .chain(clock_ports.iter().filter_map(Option::as_deref));
            scan(wanted, &mut ports);
        }
        // Gone once its instrument's dropped, and its last notes off taken.
        tracks.retain_mut(|track| {
            let gone = track.messages.is_abandoned();
            while let Ok(sent) = track.messages.pop() {
                waiting.push((sent.at, track.to.clone(), sent));
            }
            !gone
        });
        waiting.sort_by_key(|w| w.0);
        let due = heard_at(Instant::now());
        let until = waiting.partition_point(|w| w.0 <= due);
        for (_, to, sent) in waiting.drain(..until) {
            let to = match to {
                To::Port(port) => vec![port],
                To::Clock => clock_ports.clone(),
            };
            for port in to {
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
        }
        std::thread::sleep(TICK);
    }
}

/// Connects to the ports `wanted` names that have appeared, lets go of those gone, and notes
/// every port there is for `:port`.
fn scan<'a>(wanted: impl Iterator<Item = &'a str>, ports: &mut Vec<Port>) {
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
    for wanted in wanted {
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
