//! The audio devices: which host to go through, finding the devices for `:input` and `:output`,
//! switching between them, and remembering the choice in `audio.toml`, beside the config.
//!
//! ```toml
//! # Written by Lattice.
//! input = "pulseaudio:alsa_input.usb-Focusrite_Scarlett-00.analog-stereo"
//! output = "pulseaudio:alsa_output.usb-Focusrite_Scarlett-00.analog-stereo"
//!
//! [latency]   # :latency, in milliseconds, for each input
//! "pulseaudio:alsa_input.usb-Focusrite_Scarlett-00.analog-stereo" = -12
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{BufferSize, DeviceId, SampleFormat, StreamConfig, SupportedBufferSize};
use lattice_core::Grid;
use lattice_editor::{Device, Devices, Direction, Editor};
use lattice_engine::{Command, Handle};
use toml::{Table, Value};

use crate::audio::{self, Output};
use crate::config;
use crate::looper::Looper;

/// Frames a buffer, where the device lets it be set: low enough to play along to, and PulseAudio
/// would otherwise buffer a couple of seconds.
const BUFFER: u32 = 512;

/// How often the devices are looked for again while they're being chosen, as they come and go.
const REFRESH: Duration = Duration::from_secs(1);

/// The host Lattice goes through. On Linux, PulseAudio's (PipeWire serves it too), where there's
/// one: through ALSA the devices are a long list of cards and plugins, under names that don't say
/// what they are.
pub fn host() -> cpal::Host {
    #[cfg(target_os = "linux")]
    if cpal::available_hosts().contains(&cpal::HostId::PulseAudio)
        && let Ok(host) = cpal::host_from_id(cpal::HostId::PulseAudio)
    {
        return host;
    }
    cpal::default_host()
}

/// The device with `id`, or the system's default.
pub fn device(host: &cpal::Host, direction: Direction, id: Option<&str>) -> Result<cpal::Device> {
    let way = direction.name();
    match id {
        None => match direction {
            Direction::Input => host.default_input_device(),
            Direction::Output => host.default_output_device(),
        }
        .with_context(|| format!("no {way} device")),
        Some(id) => {
            let parsed: DeviceId = id.parse().map_err(|_| anyhow!("no {way} {id}"))?;
            (host.device_by_id(&parsed)).with_context(|| format!("the {way} {id} isn't there"))
        }
    }
}

/// How to open `device`: as f32, at `rate` (or its own), with as many channels as it usually
/// has, and a short buffer where it can be set.
pub fn config(
    device: &cpal::Device,
    direction: Direction,
    rate: Option<u32>,
) -> Result<StreamConfig> {
    let (default, supported): (_, Vec<_>) = match direction {
        Direction::Input => (
            device.default_input_config()?,
            device.supported_input_configs()?.collect(),
        ),
        Direction::Output => (
            device.default_output_config()?,
            device.supported_output_configs()?.collect(),
        ),
    };
    let rate = rate.unwrap_or(default.sample_rate());
    let floats = || {
        supported
            .iter()
            .filter(|c| c.sample_format() == SampleFormat::F32)
    };
    let found = (floats().filter(|c| c.channels() == default.channels()))
        .chain(floats())
        .find_map(|c| c.try_with_sample_rate(rate));
    let Some(found) = found else {
        if floats().next().is_none() {
            anyhow::bail!(
                "it gives {:?} samples; only f32 is supported so far",
                default.sample_format()
            );
        }
        anyhow::bail!("it can't run at {rate} Hz, as Lattice is");
    };
    let mut config = found.config();
    if let SupportedBufferSize::Range { min, max } = found.buffer_size()
        && (*min..=*max).contains(&BUFFER)
    {
        config.buffer_size = BufferSize::Fixed(BUFFER);
    }
    Ok(config)
}

/// The devices one way, with the default's name.
fn find(host: &cpal::Host, direction: Direction) -> Devices {
    let devices = match direction {
        Direction::Input => host.input_devices(),
        Direction::Output => host.output_devices(),
    };
    let found = devices.map_or_else(
        |_| vec![],
        |devices| {
            devices
                .filter_map(|d| {
                    Some(Device {
                        id: d.id().ok()?.to_string(),
                        name: d.to_string(),
                    })
                })
                // ALSA's, which throws the sound away.
                .filter(|d| !d.id.ends_with(":null"))
                .collect()
        },
    );
    Devices {
        found,
        default: device(host, direction, None).ok().map(|d| d.to_string()),
        chosen: None,
        looked: true,
    }
}

/// Looks for the devices on a thread of its own, as it takes a while, while they're being
/// chosen.
struct Finder {
    ask: Sender<Direction>,
    found: Receiver<(Direction, Devices)>,
    /// What was last asked for, and when.
    asked: Option<(Direction, Instant)>,
}

impl Finder {
    fn new() -> Self {
        let (ask, asked) = channel::<Direction>();
        let (tell, found) = channel();
        std::thread::spawn(move || {
            let host = host();
            for direction in asked {
                if tell.send((direction, find(&host, direction))).is_err() {
                    break;
                }
            }
        });
        // Once to start with, so the devices remembered have names.
        for direction in [Direction::Input, Direction::Output] {
            let _ = ask.send(direction);
        }
        Self {
            ask,
            found,
            asked: None,
        }
    }

    /// Hands the editor what's been found, and asks again while the devices are being chosen.
    fn tick(&mut self, editor: &mut Editor) {
        while let Ok((direction, mut devices)) = self.found.try_recv() {
            let shown = editor.devices_mut(direction);
            devices.chosen = shown.chosen.take();
            *shown = devices;
        }
        let Some(direction) = editor.choosing_device() else {
            self.asked = None;
            return;
        };
        let now = Instant::now();
        let due = match self.asked {
            Some((asked, at)) => asked != direction || now.duration_since(at) >= REFRESH,
            None => true,
        };
        if due && self.ask.send(direction).is_ok() {
            self.asked = Some((direction, now));
        }
    }
}

/// What's remembered between runs.
#[derive(Debug, Default, PartialEq)]
struct Settings {
    input: Option<String>,
    output: Option<String>,
    /// `:latency` for each input, by id, or `default`.
    latency: BTreeMap<String, i32>,
}

impl Settings {
    fn read(text: &str) -> Self {
        let Ok(table) = text.parse::<Table>() else {
            return Self::default();
        };
        let text = |key| table.get(key).and_then(Value::as_str).map(String::from);
        let latency = match table.get("latency") {
            Some(Value::Table(latency)) => (latency.iter())
                .filter_map(|(id, ms)| Some((id.clone(), i32::try_from(ms.as_integer()?).ok()?)))
                .collect(),
            _ => BTreeMap::new(),
        };
        Self {
            input: text("input"),
            output: text("output"),
            latency,
        }
    }

    fn write(&self) -> String {
        let mut table = Table::new();
        for (key, id) in [("input", &self.input), ("output", &self.output)] {
            if let Some(id) = id {
                table.insert(key.into(), Value::String(id.clone()));
            }
        }
        if !self.latency.is_empty() {
            let latency = (self.latency.iter())
                .map(|(id, &ms)| (id.clone(), Value::Integer(ms.into())))
                .collect();
            table.insert("latency".into(), Value::Table(latency));
        }
        format!("# Written by Lattice: :input, :output and :latency.\n{table}")
    }

    /// The key an input's latency is kept under.
    fn input_key(id: Option<&str>) -> String {
        id.unwrap_or("default").into()
    }
}

/// The devices in use, and what's remembered about them.
pub struct Machine {
    host: cpal::Host,
    pub output: Output,
    pub looper: Looper,
    settings: Settings,
    /// Where they're remembered; `None` if there's nowhere.
    path: Option<PathBuf>,
    finder: Finder,
}

impl Machine {
    /// Starts the output on the device chosen last time, or the default, with the engine
    /// playing `grid`, and sets the editor up with the devices and latency remembered.
    pub fn start(grid: &Grid, bpm: f64, editor: &mut Editor) -> Result<(Self, Handle)> {
        let host = host();
        let path = config::default_path().map(|p| p.with_file_name("audio.toml"));
        let text = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
        let settings = Settings::read(text.as_deref().unwrap_or(""));
        let (output, handle, problem) = audio::start(&host, grid, bpm, settings.output.as_deref())?;
        if let Some(problem) = problem {
            editor.message = Some(problem);
        }
        editor.outputs.chosen = output.device.clone();
        editor.inputs.chosen = settings.input.clone();
        let looper = Looper::new(
            output.sample_rate,
            output.input_latency.clone(),
            settings.input.clone(),
        );
        let machine = Self {
            host,
            output,
            looper,
            settings,
            path,
            finder: Finder::new(),
        };
        let mut handle = handle;
        let key = Settings::input_key(machine.settings.input.as_deref());
        let latency = machine.settings.latency.get(&key).copied().unwrap_or(0);
        editor.latency = latency;
        machine.send_latency(latency, &mut handle);
        Ok((machine, handle))
    }

    /// Called every frame: keeps the lists of devices fresh while they're chosen, and runs the
    /// looper.
    pub fn tick(
        &mut self,
        editor: &mut Editor,
        handle: &mut Handle,
    ) -> Vec<lattice_editor::Effect> {
        self.finder.tick(editor);
        self.looper.tick(editor, handle, &self.host)
    }

    /// Switches to the device `id`, or the system's default, saying how it went.
    pub fn choose(
        &mut self,
        direction: Direction,
        id: Option<String>,
        editor: &mut Editor,
        handle: &mut Handle,
    ) {
        let way = direction.name();
        let switched = match direction {
            Direction::Output => self.output.switch(&self.host, id.clone()),
            Direction::Input => device(&self.host, direction, id.as_deref()).map(|device| {
                self.looper.switch(id.clone());
                device.to_string()
            }),
        };
        let name = match switched {
            Ok(name) => name,
            Err(e) => {
                editor.message = Some(format!("can't use that {way}: {e:#}"));
                return;
            }
        };
        editor.devices_mut(direction).chosen = id.clone();
        let mut message = format!("{way}: {name}");
        match direction {
            Direction::Output => self.settings.output = id,
            Direction::Input => {
                // Each input has its own latency.
                let key = Settings::input_key(id.as_deref());
                let latency = self.settings.latency.get(&key).copied().unwrap_or(0);
                if latency != 0 {
                    message += &format!(" (latency adjusted by {latency} ms)");
                }
                editor.latency = latency;
                self.send_latency(latency, handle);
                self.settings.input = id;
            }
        }
        editor.message = Some(self.save().unwrap_or(message));
    }

    /// `:latency`: kept for the input in use.
    pub fn set_latency(&mut self, ms: i32, editor: &mut Editor, handle: &mut Handle) {
        let key = Settings::input_key(self.settings.input.as_deref());
        if ms == 0 {
            self.settings.latency.remove(&key);
        } else {
            self.settings.latency.insert(key, ms);
        }
        self.send_latency(ms, handle);
        if let Some(problem) = self.save() {
            editor.message = Some(problem);
        }
    }

    fn send_latency(&self, ms: i32, handle: &mut Handle) {
        let frames = i64::from(ms) * self.output.sample_rate as i64 / 1000;
        handle.send(Command::SetLatencyOffset(frames));
    }

    /// Writes the settings, saying what went wrong if anything did.
    fn save(&self) -> Option<String> {
        let path = self.path.as_ref()?;
        let written = std::fs::create_dir_all(path.parent()?)
            .and_then(|_| std::fs::write(path, self.settings.write()));
        written
            .err()
            .map(|e| format!("can't remember it in {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_read_back_as_written() {
        let settings = Settings {
            input: Some("pulseaudio:mic".into()),
            output: None,
            latency: [("pulseaudio:mic".into(), -12), ("default".into(), 5)].into(),
        };
        let text = settings.write();
        assert_eq!(Settings::read(&text), settings);
        assert_eq!(Settings::read("not toml ["), Settings::default());
    }
}
