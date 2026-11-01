//! Renders an [`Export`] to a WAV file, on its own thread with its own engine, so playing and
//! editing carry on meanwhile.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use anyhow::{Context, Result};
use lattice_core::Buses;
use lattice_editor::{Editor, Export, Part, file_name};
use lattice_engine::{Command, Cue, Engine, Handle, Section};

use crate::mixer::Mixer;
use crate::tracks::Tracks;

/// Samples rendered at a time: well under a step, so each part is cued in time.
const BLOCK: usize = 256;
/// Below this, the tail is silent: -80 dB.
const SILENT: f32 = 1e-4;
/// The tail ends once it's been silent this long, or at the most after `MAX_TAIL`, in seconds.
const QUIET: f32 = 1.0;
const MAX_TAIL: f32 = 10.0;

/// Exports still rendering.
pub struct Exports {
    sample_rate: f32,
    /// The message to show when each is done, and the file written, if it was.
    pending: Vec<Receiver<(String, Option<PathBuf>)>>,
}

impl Exports {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            pending: vec![],
        }
    }

    /// Starts rendering `export` with the editor's buses and tempo as they are now.
    pub fn start(&mut self, export: Export, editor: &mut Editor) {
        editor.message = Some(format!("exporting {}…", file_name(&export.path)));
        let buses = editor.buses.clone();
        self.pending
            .push(start(export, buses, editor.bpm, self.sample_rate));
    }

    /// Shows the message from each export that's finished, and returns the files written, for
    /// the instruments playing them to read again.
    pub fn poll(&mut self, editor: &mut Editor) -> Vec<PathBuf> {
        let mut written = vec![];
        self.pending.retain(|rx| match rx.try_recv() {
            Ok((message, path)) => {
                editor.message = Some(message);
                written.extend(path);
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        written
    }
}

/// Starts rendering; the receiver gets the message to show when it's done.
fn start(
    export: Export,
    buses: Buses,
    bpm: f64,
    sample_rate: f32,
) -> Receiver<(String, Option<PathBuf>)> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let done = match render(&export, buses, bpm, sample_rate)
            .and_then(|audio| write_wav(&export.path, &audio, sample_rate as u32))
        {
            Ok(seconds) => (
                format!("exported {} ({seconds:.1}s)", file_name(&export.path)),
                std::path::absolute(&export.path).ok(),
            ),
            Err(e) => (format!("export failed: {e:#}"), None),
        };
        let _ = tx.send(done);
    });
    rx
}

/// Plays `export.grid`'s tracks through each part in turn, from the top, through the song's
/// mixer, then lets it ring out until it's quiet. Returns interleaved stereo.
pub fn render(export: &Export, buses: Buses, bpm: f64, sample_rate: f32) -> Result<Vec<f32>> {
    let mut grid = export.grid.clone();
    let mut parts = export.parts.iter();
    let first = parts.next().context("nothing to export")?;
    for (track, lane) in grid.tracks.iter_mut().zip(&first.lanes) {
        lane.clone().swap(track);
    }
    let editor = Editor::new(grid, bpm).with_buses(buses);
    let instruments = Tracks::instruments(&editor.grid, sample_rate);
    let (mut engine, mut handle) = Engine::new(sample_rate, bpm, &editor.grid, instruments);
    let mut left = vec![0.0; BLOCK];
    let mut right = vec![0.0; BLOCK];

    // Rendering nothing runs the commands sent so far, making room for more.
    let drain = |engine: &mut Engine, handle: &mut Handle| {
        engine.process(&mut [], &mut []);
        handle.collect_garbage();
    };
    let mut mixer = Mixer::new(sample_rate);
    let mut tries = 0;
    while !mixer.sync(&editor, &mut handle) {
        drain(&mut engine, &mut handle);
        tries += 1;
        anyhow::ensure!(tries < 1000, "the song's mixer wouldn't load");
    }
    // Each part is cued while the one before plays, and after the last, playback stops.
    let cue = |parts: &mut std::slice::Iter<Part>| match parts.next() {
        Some(part) => Cue::Pattern(Box::new(Section {
            lanes: part.lanes.clone(),
            steps: part.steps,
            at_bar: false,
        })),
        None => Cue::Stop,
    };
    for command in [
        Command::SetClick(false),
        Command::SetLength(first.steps),
        Command::Cue(cue(&mut parts)),
        Command::Play { count_in: 0 },
    ] {
        handle.send(command);
    }
    drain(&mut engine, &mut handle);

    let step = 60.0 / (bpm * f64::from(editor.grid.steps_per_beat.max(1)));
    let length = (export.steps() as f64 * step * f64::from(sample_rate)).round() as usize;
    let mut audio = Vec::with_capacity(2 * length);
    let mut render = |engine: &mut Engine, audio: &mut Vec<f32>| {
        let (l, r) = (&mut left[..], &mut right[..]);
        engine.process(l, r);
        audio.extend(l.iter().zip(&*r).flat_map(|(&l, &r)| [l, r]));
    };
    let mut boundaries = 0;
    while handle.status().playing() {
        render(&mut engine, &mut audio);
        if handle.status().boundaries() > boundaries {
            boundaries += 1;
            handle.send(Command::Cue(cue(&mut parts)));
            handle.collect_garbage();
        }
        anyhow::ensure!(audio.len() < 4 * length + 2 * BLOCK, "the song didn't end");
    }

    // Then on while delays and reverbs fade.
    let quiet = (QUIET * sample_rate) as usize;
    let max = audio.len() + 2 * (MAX_TAIL * sample_rate) as usize;
    let mut last_sound = audio.len();
    while audio.len() < max && audio.len() - last_sound < 2 * quiet {
        let from = audio.len();
        render(&mut engine, &mut audio);
        if let Some(i) = audio[from..].iter().rposition(|s| s.abs() > SILENT) {
            last_sound = from + i + 1;
        }
    }
    // Ending on a whole frame.
    audio.truncate(last_sound.next_multiple_of(2));
    if export.looped {
        fold(&mut audio, length);
    }
    Ok(audio)
}

/// Cuts interleaved stereo to `frames`, mixing what's after back in from the start, as it would
/// sound looping.
fn fold(audio: &mut Vec<f32>, frames: usize) {
    let len = 2 * frames;
    if len == 0 {
        return audio.clear();
    }
    audio.resize(audio.len().max(len), 0.0);
    let (looped, tail) = audio.split_at_mut(len);
    for (i, &sample) in tail.iter().enumerate() {
        looped[i % len] += sample;
    }
    audio.truncate(len);
}

/// Writes interleaved stereo as 24-bit PCM, which samplers and DAWs all read. Returns its
/// length in seconds.
pub fn write_wav(path: &Path, audio: &[f32], sample_rate: u32) -> Result<f32> {
    const CHANNELS: u16 = 2;
    const BYTES: u16 = 3;
    let data = u32::try_from(audio.len() * usize::from(BYTES)).context("too long for a WAV")?;
    if let Some(folder) = path.parent().filter(|f| !f.as_os_str().is_empty()) {
        std::fs::create_dir_all(folder)
            .with_context(|| format!("can't make {}", folder.display()))?;
    }
    let file = File::create(path).with_context(|| format!("can't write {}", path.display()))?;
    let mut w = BufWriter::new(file);
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&CHANNELS.to_le_bytes())?;
    w.write_all(&sample_rate.to_le_bytes())?;
    let align = CHANNELS * BYTES;
    w.write_all(&(sample_rate * u32::from(align)).to_le_bytes())?;
    w.write_all(&align.to_le_bytes())?;
    w.write_all(&(8 * BYTES).to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data.to_le_bytes())?;
    for &s in audio {
        let s = (s.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
        w.write_all(&s.to_le_bytes()[..3])?;
    }
    w.flush()?;
    Ok(audio.len() as f32 / f32::from(CHANNELS) / sample_rate as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{Grid, Lane, Note, Step, Track};

    fn export(steps: u64) -> Export {
        let mut track = Track {
            pattern_mute: false,
            name: "t".into(),
            steps: vec![Step::default(); 4],
            mix: Default::default(),
            instrument: Default::default(),
            effects: Default::default(),
            choke: 0,
            rules: vec![],
        };
        track.steps[0].note = Some(Note {
            pitch: 60,
            velocity: 100,
            length: 1,
        });
        let part = Part {
            lanes: vec![Lane {
                mute: false,
                steps: track.steps.clone(),
                rules: vec![],
            }],
            steps,
        };
        Export {
            path: "x.wav".into(),
            grid: Grid {
                name: "test".into(),
                steps_per_beat: 4,
                tracks: vec![track],
                groups: vec![],
            },
            parts: vec![part],
            looped: false,
        }
    }

    #[test]
    fn a_loop_is_exactly_its_steps_with_the_tail_from_the_start() {
        let rate = 8000.0;
        let plain = render(&export(4), Buses::default(), 120.0, rate).unwrap();
        let looped = Export {
            looped: true,
            ..export(4)
        };
        let audio = render(&looped, Buses::default(), 120.0, rate).unwrap();
        assert_eq!(audio.len(), 2 * 4000, "half a second, to the frame");
        assert!(plain.len() > audio.len(), "there was a tail");
        // The tail's mixed into the start, the rest is as it was.
        assert_eq!(audio[0], plain[0] + plain[8000]);
        assert_eq!(
            audio[7999],
            plain[7999] + plain.get(15_999).copied().unwrap_or(0.0)
        );
    }

    #[test]
    fn folding_wraps_round_as_often_as_it_takes() {
        let mut audio = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        fold(&mut audio, 2);
        assert_eq!(
            audio,
            [1.0 + 5.0 + 9.0, 2.0 + 6.0 + 10.0, 3.0 + 7.0, 4.0 + 8.0]
        );
        // A short render is padded with silence.
        let mut audio = vec![1.0, 1.0];
        fold(&mut audio, 2);
        assert_eq!(audio, [1.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn renders_the_steps_then_the_tail_until_quiet() {
        let rate = 8000.0;
        // Four steps at 120 bpm are half a second.
        let audio = render(&export(4), Buses::default(), 120.0, rate).unwrap();
        let frames = audio.len() / 2;
        assert!(frames < 4000 + (QUIET * rate) as usize, "{frames} frames");
        assert!(audio[..2000].iter().any(|s| s.abs() > 0.01), "the note");
        // Past the first pass, nothing plays again: the tail only fades.
        let after = audio[2 * 4000..].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let during = audio[..2000].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(after < during);
    }

    #[test]
    fn renders_each_part_in_turn() {
        let rate = 8000.0;
        // Silent for four steps, then the note in the second part.
        let mut x = export(4);
        let silent = Part {
            lanes: vec![Lane {
                mute: false,
                steps: vec![Step::default(); 4],
                rules: vec![],
            }],
            steps: 4,
        };
        x.parts.insert(0, silent);
        let audio = render(&x, Buses::default(), 120.0, rate).unwrap();
        let first = audio.iter().position(|s| s.abs() > 0.0).unwrap() / 2;
        // The note starts on the step; its attack rises from zero.
        assert_eq!(first, 4000 + 1);
    }

    #[test]
    fn writes_a_wav_header_and_24_bit_frames() {
        let dir = std::env::temp_dir().join(format!("lattice-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.wav");
        let seconds = write_wav(&path, &[0.0, 1.0, -1.0, 0.5], 48000).unwrap();
        assert_eq!(seconds, 2.0 / 48000.0);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 44 + 12);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 48000);
        assert_eq!(&bytes[44..50], &[0, 0, 0, 0xff, 0xff, 0x7f]);
        assert_eq!(&bytes[50..53], &[0x01, 0x00, 0x80]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
