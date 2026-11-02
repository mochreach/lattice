//! The looper's frontend: the audio input, opened once a track's armed, and the takes. While
//! record's on and the song plays, two takes wait in the engine for the armed track's passes, so
//! they record back to back; each that comes back is mixed onto the loop it overdubs, if any,
//! saved as a new file in the song's samples folder, and handed to the editor for the track to
//! play.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use cpal::Host;
use cpal::traits::{DeviceTrait, StreamTrait};
use lattice_editor::{Direction, Editor, Effect};
use lattice_engine::{Command, Handle, Sample, Take};
use rtrb::{Consumer, RingBuffer};

use crate::devices;
use crate::export::write_wav;
use crate::log::{HEALTH, log};
use crate::samples;

/// Takes waiting in the engine at once: the one recording and the one to follow it.
const WAITING: usize = 2;

pub struct Looper {
    sample_rate: f32,
    /// The latency the input stream measures, in frames, for the output stream to add to its
    /// own.
    input_latency: Arc<AtomicU64>,
    /// Keeps the input open; dropping it closes it.
    input: Option<cpal::Stream>,
    /// Why the input couldn't be opened, so it's not tried again until the track's armed anew.
    failed: bool,
    /// The device to record, by id; `None` for the system's default.
    device: Option<String>,
    /// Takes sent to the engine and not back yet.
    waiting: usize,
    /// Whether the engine's been told to stop recording since a take was last sent.
    stopped: bool,
}

impl Looper {
    pub fn new(sample_rate: f32, input_latency: Arc<AtomicU64>, device: Option<String>) -> Self {
        Self {
            sample_rate,
            input_latency,
            input: None,
            failed: false,
            device,
            waiting: 0,
            stopped: true,
        }
    }

    /// Called every frame: opens the input once a track's armed, keeps takes waiting while
    /// there's something to record, and saves the passes recorded. Returns what the editor
    /// makes of them.
    pub fn tick(&mut self, editor: &mut Editor, handle: &mut Handle, host: &Host) -> Vec<Effect> {
        let armed = editor.armed();
        if armed.is_none() {
            self.failed = false;
        } else if self.input.is_none() && !self.failed {
            let latency = Arc::clone(&self.input_latency);
            match open_input(host, self.device.as_deref(), self.sample_rate, latency) {
                Ok((stream, input)) => {
                    handle.send(Command::SetInput(input));
                    self.input = Some(stream);
                }
                Err(e) => {
                    editor.message = Some(format!("can't record: {e:#}"));
                    self.failed = true;
                }
            }
        }

        let recording = editor.record && handle.status().playing() && self.input.is_some();
        match armed.filter(|_| recording) {
            Some(t) => {
                // No more than the passes left, if they're counted.
                let left = editor.passes_left().map_or(WAITING, |n| n as usize);
                while self.waiting < WAITING.min(left) {
                    let take = Take::new(t, self.room(editor, t));
                    if !handle.send(Command::Record(take)) {
                        break;
                    }
                    self.waiting += 1;
                    self.stopped = false;
                }
            }
            None if !self.stopped && handle.send(Command::StopRecording) => self.stopped = true,
            None => {}
        }

        let mut effects = vec![];
        while let Some(take) = handle.recorded() {
            self.waiting = self.waiting.saturating_sub(1);
            if take.complete && !take.is_empty() {
                effects.extend(self.save(&take, editor));
            }
        }
        effects
    }

    /// Records the device `id`, or the system's default, from now on: the input's opened again
    /// when it's next wanted.
    pub fn switch(&mut self, id: Option<String>) {
        self.device = id;
        self.input = None;
        self.failed = false;
    }

    /// Frames to leave room for, for a pass of track `t`: a tenth more, in case the tempo
    /// slows.
    fn room(&self, editor: &Editor, t: usize) -> usize {
        // A track that's gone has nothing to record, and needs only the margin.
        let steps = editor.grid.tracks.get(t).map_or(0, |t| t.steps.len()) as f64;
        let per_step = 60.0 / (editor.bpm * f64::from(editor.grid.steps_per_beat.max(1)));
        (steps * per_step * f64::from(self.sample_rate) * 1.1) as usize + 1024
    }

    /// Saves a pass, mixed onto the loop it overdubs, as a new file the track plays.
    fn save(&self, take: &Take, editor: &mut Editor) -> Vec<Effect> {
        let path = editor.take_file();
        let under = editor.overdubbing(take.track).map(Path::to_path_buf);
        let under = under.and_then(|p| samples::load(&p).ok());
        let audio = mix(take, under.as_deref(), self.sample_rate);
        match write_wav(&path, &audio, self.sample_rate as u32) {
            Ok(_) => editor.recorded(take.track, path),
            Err(e) => {
                editor.message = Some(format!("can't save the pass: {e:#}"));
                vec![]
            }
        }
    }
}

/// The take, with `under` mixed in beneath it if it's overdubbing, as interleaved stereo at
/// `rate`. The loop underneath is read at its own rate, and stops where it ends.
fn mix(take: &Take, under: Option<&Sample>, rate: f32) -> Vec<f32> {
    let mut audio = Vec::with_capacity(take.len() * 2);
    for (i, (&l, &r)) in take.left.iter().zip(&take.right).enumerate() {
        let (ul, ur) = under.map_or((0.0, 0.0), |s| at(s, i as f64 * f64::from(s.rate / rate)));
        audio.push(l + ul);
        audio.push(r + ur);
    }
    audio
}

/// `sample` between frames, by linear interpolation, silent past its end.
fn at(sample: &Sample, pos: f64) -> (f32, f32) {
    let i = pos as usize;
    let t = (pos - i as f64) as f32;
    let get = |v: &[f32], i: usize| v.get(i).copied().unwrap_or(0.0);
    let lerp = |v: &[f32]| get(v, i) + (get(v, i + 1) - get(v, i)) * t;
    (lerp(&sample.left), lerp(&sample.right))
}

/// Opens the input `id`, or the default, at `rate`, the output's, feeding stereo frames to the
/// engine and its latency to `latency`.
fn open_input(
    host: &Host,
    id: Option<&str>,
    rate: f32,
    latency: Arc<AtomicU64>,
) -> Result<(cpal::Stream, Consumer<f32>)> {
    let device = devices::device(host, Direction::Input, id)?;
    let config = devices::config(&device, Direction::Input, Some(rate as u32))?;
    let channels = usize::from(config.channels).max(1);
    let buffer = config.buffer_size;
    // A second, plenty for the engine to keep up.
    let (mut frames, input) = RingBuffer::new(rate as usize * 2);
    let stream = device.build_input_stream(
        config,
        move |data: &[f32], info: &cpal::InputCallbackInfo| {
            let at = info.timestamp();
            let seconds = at.callback.duration_since(at.capture).as_secs_f64();
            latency.store((seconds * f64::from(rate)) as u64, Ordering::Relaxed);
            for frame in data.chunks_exact(channels) {
                // Mono is heard in both.
                let (l, r) = match *frame {
                    [l, r, ..] => (l, r),
                    [mono] => (mono, mono),
                    [] => continue,
                };
                if frames.slots() >= 2 {
                    let _ = frames.push(l);
                    let _ = frames.push(r);
                } else {
                    HEALTH.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        },
        // Can't print while the terminal UI owns the screen.
        |err| HEALTH.stream_error("input", &err),
        None,
    )?;
    stream.play()?;
    log!("input: {device}, {rate} Hz, {channels} channels, buffer {buffer:?}");
    Ok((stream, input))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(left: Vec<f32>) -> Take {
        let mut take = *Take::new(0, left.len());
        take.right = left.iter().map(|x| -x).collect();
        take.left = left;
        take
    }

    #[test]
    fn a_pass_is_mixed_onto_the_loop_it_overdubs() {
        let pass = take(vec![1.0, 1.0, 1.0, 1.0]);
        assert_eq!(
            mix(&pass, None, 48_000.0),
            [1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0]
        );
        // Underneath, at half the rate and shorter: read at its own speed, silent past its end.
        let under = Sample {
            rate: 24_000.0,
            left: vec![0.0, 2.0],
            right: vec![0.0, 0.0],
        };
        let mixed: Vec<f32> = mix(&pass, Some(&under), 48_000.0)
            .into_iter()
            .step_by(2)
            .collect();
        assert_eq!(mixed, [1.0, 2.0, 3.0, 2.0]);
    }
}
