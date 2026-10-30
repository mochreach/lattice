mod demo;

use std::io::BufRead;

use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use lattice_engine::Engine;

// In debug builds, any allocation inside `assert_no_alloc` (the audio callback) aborts.
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

/// Largest block the engine renders in one go; cpal buffers bigger than this are split.
const MAX_BLOCK: usize = 4096;

fn main() -> Result<()> {
    let click = !std::env::args().any(|a| a == "--no-click");

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

    let (grid, patches) = demo::song();
    let mut engine = Engine::new(sample_rate, demo::BPM, &grid, &patches);
    engine.click_enabled = click;
    let mut mono = vec![0.0f32; MAX_BLOCK];

    let stream = device.build_output_stream(
        config.into(),
        move |data: &mut [f32], _| {
            assert_no_alloc::assert_no_alloc(|| {
                for chunk in data.chunks_mut(MAX_BLOCK * channels) {
                    let mono = &mut mono[..chunk.len() / channels];
                    engine.process(mono);
                    for (frame, &sample) in chunk.chunks_exact_mut(channels).zip(mono.iter()) {
                        frame.fill(sample);
                    }
                }
            });
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;
    stream.play()?;

    println!(
        "Playing the demo at {} bpm ({} Hz, {} channels){}.",
        demo::BPM,
        sample_rate,
        channels,
        if click {
            " with a click on every beat"
        } else {
            ""
        }
    );
    for track in &grid.tracks {
        println!(
            "  {:<6} {} steps, {:?}",
            track.name,
            track.steps.len(),
            track.rules
        );
    }
    println!("Press Enter to stop.");
    std::io::stdin().lock().lines().next();
    Ok(())
}
