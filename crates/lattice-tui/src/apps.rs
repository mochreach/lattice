//! Recording an app's own sound, a browser's say, rather than a device's. PulseAudio (and
//! PipeWire, which serves it) can record what one app plays, from the monitor of the output it
//! plays to, without anything else playing there, Lattice included. cpal finds only devices, so
//! the apps are found and recorded here, through PulseAudio's protocol.
//!
//! An app is known by its name, `app:Firefox`, rather than by its stream: an app opens a new
//! stream each time it starts playing again, and browsers close theirs after a few seconds of
//! silence. So the recording follows the app, looking for its stream every so often and
//! recording whichever it has, if any.

use std::ffi::CStr;
use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use futures::executor::block_on;
use lattice_editor::{APP, Device};
use pulseaudio::protocol::{self, ChannelPosition, Command, Prop};
use rtrb::Producer;

use crate::devices::BUFFER;
use crate::log::{HEALTH, log};

/// How often the app's stream is looked for while it's recorded.
const LOOK: Duration = Duration::from_millis(250);

/// How long to wait for the server to answer before giving up on it.
const TIMEOUT: Duration = Duration::from_secs(2);

/// The app the input `id` records, if it's an app's.
pub fn app(id: &str) -> Option<&str> {
    id.strip_prefix(APP)
}

/// What the list of inputs calls the app.
pub fn name(app: &str) -> String {
    format!("app: {app}")
}

/// The apps playing now, other than Lattice, as inputs.
pub fn list() -> Vec<Device> {
    // Without PulseAudio there are no apps to record.
    let Ok(playing) = Connection::open().and_then(|mut c| c.playing()) else {
        return vec![];
    };
    let mut found: Vec<Device> = vec![];
    for stream in playing {
        let id = format!("{APP}{}", stream.app);
        if !found.iter().any(|d| d.id == id) {
            found.push(Device {
                id,
                name: name(&stream.app),
            });
        }
    }
    found
}

/// Records an app for as long as it's kept: dropping it stops.
pub struct Capture {
    stop: Arc<AtomicBool>,
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Starts recording `app` at `rate` into `frames`, as interleaved stereo, with the latency in
/// frames given to `latency`, whenever it's playing. Says too whether it's playing now.
pub fn record(
    app: &str,
    rate: f32,
    latency: Arc<AtomicU64>,
    frames: Producer<f32>,
) -> Result<(Capture, bool)> {
    let mut connection = Connection::open()?;
    let playing = connection.playing()?.iter().any(|s| s.app == app);
    let stop = Arc::new(AtomicBool::new(false));
    let follower = Follower {
        app: app.to_string(),
        rate,
        latency,
        frames: Arc::new(Mutex::new(frames)),
        stop: Arc::clone(&stop),
    };
    std::thread::spawn(move || follower.run(connection));
    Ok((Capture { stop }, playing))
}

/// A stream an app's playing.
struct Stream {
    /// The server's index of it, a sink input's.
    index: u32,
    /// The output it plays to, a sink.
    sink: u32,
    app: String,
    /// Whether it's paused.
    corked: bool,
}

/// A connection to the server, for asking it things and waiting for each answer.
struct Connection {
    socket: BufReader<UnixStream>,
    version: u16,
    sequence: u32,
}

impl Connection {
    fn open() -> Result<Self> {
        let path = pulseaudio::socket_path_from_env().context("no PulseAudio server")?;
        let cookie = (pulseaudio::cookie_path_from_env())
            .and_then(|p| std::fs::read(p).ok())
            // A server that wants none takes an empty one.
            .unwrap_or_default();
        let socket = UnixStream::connect(&path)
            .with_context(|| format!("can't reach PulseAudio at {}", path.display()))?;
        socket.set_read_timeout(Some(TIMEOUT))?;
        socket.set_write_timeout(Some(TIMEOUT))?;
        let mut connection = Self {
            socket: BufReader::new(socket),
            version: protocol::MAX_VERSION,
            sequence: 0,
        };
        let auth: protocol::AuthReply = connection.ask(Command::Auth(protocol::AuthParams {
            version: protocol::MAX_VERSION,
            supports_shm: false,
            supports_memfd: false,
            cookie,
        }))?;
        connection.version = connection.version.min(auth.version);
        let mut props = protocol::Props::new();
        props.set(Prop::ApplicationName, c"Lattice");
        let _: protocol::SetClientNameReply = connection.ask(Command::SetClientName(props))?;
        Ok(connection)
    }

    fn ask<R: protocol::CommandReply>(&mut self, command: Command) -> Result<R> {
        let sequence = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        protocol::write_command_message(self.socket.get_mut(), sequence, &command, self.version)?;
        let (answered, reply) = protocol::read_reply_message(&mut self.socket, self.version)?;
        anyhow::ensure!(answered == sequence, "PulseAudio answered out of turn");
        Ok(reply)
    }

    /// The streams apps are playing, other than Lattice's.
    fn playing(&mut self) -> Result<Vec<Stream>> {
        let ours = std::process::id().to_string();
        let inputs: protocol::SinkInputInfoList = self.ask(Command::GetSinkInputInfoList)?;
        let streams = inputs.into_iter().filter_map(|input| {
            let prop = |p| input.props.get(p).and_then(text);
            if prop(Prop::ApplicationProcessId).as_deref() == Some(ours.as_str()) {
                return None;
            }
            let app = prop(Prop::ApplicationName).or_else(|| prop(Prop::ApplicationProcessBinary));
            Some(Stream {
                index: input.index,
                sink: input.sink_index,
                app: named(app?),
                corked: input.corked,
            })
        });
        Ok(streams.collect())
    }

    /// The monitor of the sink `sink`, which hears what's played to it.
    fn monitor(&mut self, sink: u32) -> Result<u32> {
        let info: protocol::SinkInfo = self.ask(Command::GetSinkInfo(protocol::GetSinkInfo {
            index: Some(sink),
            name: None,
        }))?;
        info.monitor_source_index
            .context("its output can't be listened to")
    }
}

/// What to call an app: cpal's, another Lattice's say, by its program and process, as cpal
/// names them all by process alone.
fn named(app: String) -> String {
    let program = (app.strip_prefix("cpal-pulseaudio-")).and_then(|pid| {
        Some((
            std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?,
            pid,
        ))
    });
    match program {
        Some((program, pid)) => format!("{} {pid}", program.trim()),
        None => app,
    }
}

/// A property's text, if it has any.
fn text(bytes: &[u8]) -> Option<String> {
    let text = CStr::from_bytes_until_nul(bytes).ok()?.to_string_lossy();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// The thread recording an app: it looks for the app's stream, records it, and moves on to the
/// next when it's gone.
struct Follower {
    app: String,
    rate: f32,
    latency: Arc<AtomicU64>,
    /// Shared by each stream recorded in turn; only one writes at a time.
    frames: Arc<Mutex<Producer<f32>>>,
    stop: Arc<AtomicBool>,
}

impl Follower {
    fn run(self, mut connection: Connection) {
        let client = match pulseaudio::Client::from_env(c"Lattice") {
            Ok(client) => client,
            Err(e) => return HEALTH.stream_error("input", &e),
        };
        // The stream being recorded, by the index of the app's it follows.
        let mut recording: Option<(u32, pulseaudio::RecordStream)> = None;
        while !self.stop.load(Ordering::Relaxed) {
            let streams = match connection.playing() {
                Ok(streams) => streams,
                Err(e) => {
                    HEALTH.stream_error("input", &e);
                    break;
                }
            };
            let current = recording.as_ref().map(|(index, _)| *index);
            let next = self.choose(&streams, current);
            if next.map(|s| s.index) != current {
                if let Some((_, stream)) = recording.take() {
                    // It may be gone already, with the app's.
                    let _ = block_on(stream.delete());
                }
                if let Some(next) = next {
                    match self.listen(&client, &mut connection, next) {
                        Ok(stream) => {
                            log!("input: {}'s stream {}", self.app, next.index);
                            recording = Some((next.index, stream));
                        }
                        Err(e) => HEALTH.stream_error("input", &format!("{e:#}")),
                    }
                }
            }
            std::thread::sleep(LOOK);
        }
        if let Some((_, stream)) = recording {
            let _ = block_on(stream.delete());
        }
    }

    /// Which of the app's streams to record: the one being recorded while it plays, or else the
    /// newest playing, or else the newest paused, for when it plays again.
    fn choose<'a>(&self, streams: &'a [Stream], current: Option<u32>) -> Option<&'a Stream> {
        let app = || streams.iter().filter(|s| s.app == self.app);
        let playing = || app().filter(|s| !s.corked);
        (playing().find(|s| Some(s.index) == current))
            .or_else(|| playing().max_by_key(|s| s.index))
            .or_else(|| app().find(|s| Some(s.index) == current))
            .or_else(|| app().max_by_key(|s| s.index))
    }

    /// Records `stream`, and only it, from its output's monitor.
    fn listen(
        &self,
        client: &pulseaudio::Client,
        connection: &mut Connection,
        stream: &Stream,
    ) -> Result<pulseaudio::RecordStream> {
        let monitor = connection.monitor(stream.sink)?;
        // Stereo f32 frames.
        let bytes = BUFFER * 2 * 4;
        let params = protocol::RecordStreamParams {
            sample_spec: protocol::SampleSpec {
                format: protocol::SampleFormat::Float32Le,
                channels: 2,
                sample_rate: self.rate as u32,
            },
            channel_map: protocol::ChannelMap::new([
                ChannelPosition::FrontLeft,
                ChannelPosition::FrontRight,
            ]),
            source_index: Some(monitor),
            direct_on_input_index: Some(stream.index),
            buffer_attr: protocol::stream::BufferAttr {
                max_length: bytes,
                fragment_size: bytes,
                ..Default::default()
            },
            flags: protocol::stream::StreamFlags {
                adjust_latency: true,
                ..Default::default()
            },
            ..Default::default()
        };
        // About a buffer: the server sends each as it fills.
        self.latency.store(u64::from(BUFFER), Ordering::Relaxed);
        let frames = Arc::clone(&self.frames);
        let write = move |data: &[u8]| {
            let Ok(mut frames) = frames.lock() else {
                return;
            };
            let mut samples = data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b));
            while let (Some(l), Some(r)) = (samples.next(), samples.next()) {
                if frames.slots() >= 2 {
                    let _ = frames.push(l);
                    let _ = frames.push(r);
                } else {
                    HEALTH.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        };
        Ok(block_on(client.create_record_stream(params, write))?)
    }
}
