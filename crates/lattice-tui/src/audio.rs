//! Audio output through cpal.

use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use lattice_core::Grid;
use lattice_engine::{Engine, Handle, Instrument, Patch, Synth};

/// Largest buffer the engine renders in one go; bigger cpal buffers are split.
const MAX_BLOCK: usize = 4096;

/// Keeps the stream alive; dropping it stops audio.
pub struct Audio {
    _stream: cpal::Stream,
    pub handle: Handle,
}

pub fn start(grid: &Grid, patches: Vec<Patch>, bpm: f64) -> Result<Audio> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("no audio output device")?;
    let config = device.default_output_config()?;
    if config.sample_format() != cpal::SampleFormat::F32 {
        bail!(
            "output device wants {:?} samples; only f32 is supported so far",
            config.sample_format()
        );
    }
    let channels = usize::from(config.channels());
    let sample_rate = config.sample_rate() as f32;

    let instruments = patches
        .into_iter()
        .map(|p| Box::new(Synth::new(p, sample_rate)) as Box<dyn Instrument>)
        .collect();
    let (mut engine, handle) = Engine::new(sample_rate, bpm, grid, instruments);
    let mut left = vec![0.0f32; MAX_BLOCK];
    let mut right = vec![0.0f32; MAX_BLOCK];

    let stream = device.build_output_stream(
        config.into(),
        move |data: &mut [f32], _| {
            assert_no_alloc::assert_no_alloc(|| {
                for chunk in data.chunks_mut(MAX_BLOCK * channels) {
                    let frames = chunk.len() / channels;
                    let (l, r) = (&mut left[..frames], &mut right[..frames]);
                    engine.process(l, r);
                    for ((frame, &l), &r) in chunk.chunks_exact_mut(channels).zip(&*l).zip(&*r) {
                        if let [first, second, rest @ ..] = frame {
                            *first = l;
                            *second = r;
                            rest.fill(0.0);
                        } else {
                            frame.fill(0.5 * (l + r));
                        }
                    }
                }
            });
        },
        // Can't print while the terminal UI owns the screen.
        |_err| {},
        None,
    )?;
    stream.play()?;

    Ok(Audio {
        _stream: stream,
        handle,
    })
}
