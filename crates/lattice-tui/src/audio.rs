//! Audio output through cpal. The input, for the looper, is opened when it's wanted: see
//! [`crate::looper`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use cpal::traits::{DeviceTrait, StreamTrait};
use lattice_core::Grid;
use lattice_editor::Direction;
use lattice_engine::{Engine, Handle};

use crate::devices;
use crate::log::{HEALTH, log};
use crate::tracks::Tracks;

/// Largest buffer the engine renders in one go; bigger cpal buffers are split.
const MAX_BLOCK: usize = 4096;

/// The output stream, and the engine it plays, which can move to another device.
pub struct Output {
    /// Keeps the stream alive; dropping it stops audio.
    stream: Option<cpal::Stream>,
    /// Only ever locked by the one stream playing it, so the audio thread never waits: when the
    /// output moves, the old stream is gone before the new one starts.
    engine: Arc<Mutex<Engine>>,
    pub sample_rate: f32,
    /// The input's latency in frames, as it measures it, for the looper's input to set.
    pub input_latency: Arc<AtomicU64>,
    /// The device playing, by id; `None` for the system's default.
    pub device: Option<String>,
}

/// Starts the output on the device `id`, or the default if it's not there, saying so.
pub fn start(
    host: &cpal::Host,
    grid: &Grid,
    bpm: f64,
    id: Option<&str>,
) -> Result<(Output, Handle, Option<String>)> {
    let (device, id, problem) = match devices::device(host, Direction::Output, id) {
        Ok(device) => (device, id, None),
        Err(e) => (
            devices::device(host, Direction::Output, None)?,
            None,
            Some(format!("{e:#}; playing through the default")),
        ),
    };
    let config = devices::config(&device, Direction::Output, None)?;
    let sample_rate = config.sample_rate as f32;

    let instruments = Tracks::instruments(grid, sample_rate);
    // Effects and bus levels are sent once it's running; see `Mixer`.
    let (engine, handle) = Engine::new(sample_rate, bpm, grid, instruments);
    let mut output = Output {
        stream: None,
        engine: Arc::new(Mutex::new(engine)),
        sample_rate,
        input_latency: Arc::new(AtomicU64::new(0)),
        device: id.map(String::from),
    };
    output.stream = Some(output.play(&device, config)?);
    Ok((output, handle, problem))
}

impl Output {
    /// Moves to the device `id`, or the default, at the same rate, returning its name. If it
    /// can't, it carries on where it was.
    pub fn switch(&mut self, host: &cpal::Host, id: Option<String>) -> Result<String> {
        let rate = Some(self.sample_rate as u32);
        let device = devices::device(host, Direction::Output, id.as_deref())?;
        let config = devices::config(&device, Direction::Output, rate)?;
        // The old stream stops before the new one starts, so they never share the engine.
        self.stream = None;
        match self.play(&device, config) {
            Ok(stream) => {
                self.stream = Some(stream);
                self.device = id;
                Ok(device.to_string())
            }
            Err(e) => {
                let old = devices::device(host, Direction::Output, self.device.as_deref());
                if let Ok(old) = old
                    && let Ok(config) = devices::config(&old, Direction::Output, rate)
                {
                    self.stream = self.play(&old, config).ok();
                }
                Err(e)
            }
        }
    }

    fn play(&self, device: &cpal::Device, config: cpal::StreamConfig) -> Result<cpal::Stream> {
        let channels = usize::from(config.channels);
        anyhow::ensure!(channels > 0, "the output has no channels");
        let sample_rate = self.sample_rate;
        let engine = Arc::clone(&self.engine);
        let input = Arc::clone(&self.input_latency);
        let mut left = vec![0.0f32; MAX_BLOCK];
        let mut right = vec![0.0f32; MAX_BLOCK];
        let buffer = config.buffer_size;
        let stream = device.build_output_stream(
            config,
            move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                assert_no_alloc::assert_no_alloc(|| {
                    let start = Instant::now();
                    let Ok(mut engine) = engine.try_lock() else {
                        data.fill(0.0);
                        HEALTH.missed.fetch_add(1, Ordering::Relaxed);
                        return;
                    };
                    // What's rendered now is heard this much later, and what's played along
                    // with it takes the input's latency to come back.
                    let at = info.timestamp();
                    let output = at.playback.duration_since(at.callback).as_secs_f64();
                    let output = (output * f64::from(sample_rate)) as u64;
                    engine.set_latency(output + input.load(Ordering::Relaxed));
                    for chunk in data.chunks_mut(MAX_BLOCK * channels) {
                        let frames = chunk.len() / channels;
                        // A chunk is at most a block, so this always fits.
                        let (Some(l), Some(r)) = (left.get_mut(..frames), right.get_mut(..frames))
                        else {
                            chunk.fill(0.0);
                            continue;
                        };
                        engine.process(l, r);
                        for ((frame, &l), &r) in chunk.chunks_exact_mut(channels).zip(&*l).zip(&*r)
                        {
                            if let [first, second, rest @ ..] = frame {
                                *first = l;
                                *second = r;
                                rest.fill(0.0);
                            } else {
                                frame.fill(0.5 * (l + r));
                            }
                        }
                    }
                    HEALTH.block(start, data.len() / channels, sample_rate);
                });
            },
            // Can't print while the terminal UI owns the screen.
            |err| HEALTH.stream_error("output", &err),
            None,
        )?;
        stream.play()?;
        log!("output: {device}, {sample_rate} Hz, {channels} channels, buffer {buffer:?}");
        Ok(stream)
    }
}
