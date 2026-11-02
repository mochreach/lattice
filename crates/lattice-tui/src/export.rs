//! Renders an [`Export`] to a WAV file, on its own thread with its own engine, so playing and
//! editing carry on meanwhile.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use anyhow::{Context, Result};
use lattice_core::{Buses, MIN_BPM, SongGesture};
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
        let name = file_name(&export.path);
        editor.message = Some(format!("exporting {name}{}…", cues_left_out(&export)));
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

/// What to add to the export's messages if its conductor cues patterns: it plays the parts as
/// arranged, so a `P` would take it somewhere else, and they're left out.
fn cues_left_out(export: &Export) -> &'static str {
    let cues = (export.parts.iter())
        .flat_map(|p| p.conductor.gestures())
        .any(|(_, g)| matches!(g, SongGesture::Pattern(_)));
    if cues {
        ", as arranged: the conductor's P cues are left out"
    } else {
        ""
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
                format!(
                    "exported {} ({seconds:.1}s){}",
                    file_name(&export.path),
                    cues_left_out(&export)
                ),
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
    let first = export.parts.first().context("nothing to export")?;
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
    // Each part is cued while the one before plays, with the song's mutes now and next, and
    // after the last, playback stops. The same pattern again loops it, as the song does, so
    // tracks and the conductor go on round rather than starting again; only the mutes change.
    let mut next = 1;
    let mut cue = || {
        let (before, part) = (export.parts.get(next - 1), export.parts.get(next));
        next += 1;
        let same = |a: &Part, b: &Part| {
            (&a.lanes, a.steps, &a.conductor) == (&b.lanes, b.steps, &b.conductor)
        };
        let now = before.map_or(0, |p| p.mutes);
        let mutes = Command::SetMutes {
            now,
            next: part.map_or(now, |p| p.mutes),
        };
        let cue = match part {
            Some(part) if before.is_some_and(|b| same(b, part)) => Cue::Loop,
            Some(part) => Cue::Pattern(Box::new(Section {
                conductor: part.conductor.clone(),
                lanes: part.lanes.clone(),
                steps: part.steps,
                at_bar: false,
            })),
            None => Cue::Stop,
        };
        [mutes, Command::Cue(cue)]
    };
    let [mutes, first_cue] = cue();
    for command in [
        Command::SetClick(false),
        Command::SetLength(first.steps),
        Command::SetConductor {
            conductor: Box::new(first.conductor.clone()),
            swaps: 0,
        },
        mutes,
        first_cue,
        Command::Play { count_in: 0 },
    ] {
        handle.send(command);
    }
    drain(&mut engine, &mut handle);

    // How long the parts last at the song's tempo, and at the slowest the conductor could
    // take them, after which they must have ended.
    let frames = |bpm: f64| {
        let step = 60.0 / (bpm * f64::from(editor.grid.steps_per_beat.max(1)));
        (export.steps() as f64 * step * f64::from(sample_rate)).round() as usize
    };
    // Loops are selections, which play without a conductor, so this is exact for them.
    let length = frames(bpm);
    let longest = frames(bpm.min(f64::from(MIN_BPM)));
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
            for command in cue() {
                handle.send(command);
            }
            handle.collect_garbage();
        }
        // The parts are played as arranged, whatever patterns the conductor cues.
        while handle.conducted().is_some() {}
        anyhow::ensure!(audio.len() < 2 * longest + 2 * BLOCK, "the song didn't end");
    }
    // Then on while delays and reverbs fade.
    let quiet = (QUIET * sample_rate) as usize;
    let max = audio.len() + 2 * (MAX_TAIL * sample_rate) as usize;
    let mut last_sound = audio.len();
    while audio.len() < max && audio.len() - last_sound < 2 * quiet {
        let from = audio.len();
        render(&mut engine, &mut audio);
        if let Some(i) = audio.iter().skip(from).rposition(|s| s.abs() > SILENT) {
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
    for lap in tail.chunks(len) {
        for (to, sample) in looped.iter_mut().zip(lap) {
            *to += sample;
        }
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
            face: None,
            pattern_mute: false,
            columns: vec![],
            name: "t".into(),
            steps: vec![Step::default(); 4],
            mix: Default::default(),
            instrument: Default::default(),
            effects: Default::default(),
            choke: 0,
            macros: vec![],
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
            conductor: Default::default(),
            mutes: 0,
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
    fn every_preset_is_heard_and_none_clips() {
        for name in lattice_core::PRESETS {
            let preset = lattice_core::preset(name).unwrap();
            let drum = !matches!(
                preset.instrument.kind,
                lattice_core::InstKind::Synth
                    | lattice_core::InstKind::Modular
                    | lattice_core::InstKind::Harpsichord
                    | lattice_core::InstKind::String
            );
            let mut export = export(16);
            let track = &mut export.grid.tracks[0];
            track.instrument = preset.instrument;
            track.effects = preset.effects;
            track.macros = preset.macros;
            track.steps = vec![Step::default(); 16];
            for (i, pitch) in [(0, 48), (4, 55), (8, 51), (12, 60)] {
                let pitch = if drum { lattice_core::DRUM_ROOT } else { pitch };
                track.steps[i].note = Some(Note {
                    pitch,
                    velocity: 100,
                    length: 2,
                });
            }
            export.parts[0].lanes[0].steps = track.steps.clone();
            let audio = render(&export, Buses::default(), 120.0, 48_000.0).unwrap();
            let peak = audio.iter().fold(0f32, |m, s| m.max(s.abs()));

            assert!(peak > 0.05, "{name} is too quiet: {peak}");
            assert!(peak < 1.5, "{name} clips: {peak}");
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
    fn the_conductor_sets_the_tempo_it_renders_at() {
        let rate = 8000.0;
        let mut x = export(4);
        let mut conductor = lattice_core::Conductor::new(4);
        conductor.steps[0][0] = Some("T60".parse().unwrap());
        let silent = Part {
            lanes: vec![Lane {
                mute: false,
                steps: vec![Step::default(); 4],
                rules: vec![],
            }],
            steps: 4,
            conductor,
            mutes: 0,
        };
        x.parts.insert(0, silent);
        let audio = render(&x, Buses::default(), 120.0, rate).unwrap();
        let first = audio.iter().position(|s| s.abs() > 0.0).unwrap() / 2;
        // Four steps at 60 BPM, a second, and the tempo holds into the next part.
        assert_eq!(first, 8000 + 1);
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
            conductor: Default::default(),
            mutes: 0,
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
