//! CPU budget: how much of one core each instrument needs. Run with `cargo bench`, and on every
//! machine we care about (laptop, Raspberry Pi 4, Pi 3) to see where the limits are.

use std::hint::black_box;
use std::time::Instant;

use lattice_engine::{AdsrParams, Event, EventKind, Patch, Processor, Synth, Wave};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK: usize = 128;
const SECONDS: usize = 20;

fn main() {
    lattice_engine::denormal::flush_to_zero();
    let patch = Patch {
        wave: Wave::Saw,
        amp: AdsrParams {
            attack: 0.01,
            decay: 0.3,
            sustain: 0.6,
            release: 0.2,
        },
        cutoff: 800.0,
        env_amount: 3_000.0,
        resonance: 0.4,
        gain: 0.1,
    };
    let mut synth = Synth::new(patch, SAMPLE_RATE);
    // Hold all eight voices for the whole run: the worst case.
    let notes: Vec<Event> = (0..8)
        .map(|i| Event {
            offset: 0,
            kind: EventKind::NoteOn {
                pitch: 48 + 5 * i,
                velocity: 100,
            },
        })
        .collect();

    let report = budget(&mut synth, &notes);
    println!(
        "test synth, 8 voices: {:.0}x real time, {:.2}% of one core ({:.3}% per voice)",
        report,
        100.0 / report,
        100.0 / report / 8.0
    );
}

/// Renders `SECONDS` of audio in `BLOCK`-sized buffers; returns how many times faster than real
/// time that was.
fn budget(instrument: &mut dyn Processor, first_events: &[Event]) -> f64 {
    let mut left = vec![0.0; BLOCK];
    let mut right = vec![0.0; BLOCK];
    let blocks = SECONDS * SAMPLE_RATE as usize / BLOCK;
    instrument.process(first_events, &mut left, &mut right);

    let start = Instant::now();
    for _ in 0..blocks {
        instrument.process(&[], &mut left, &mut right);
        black_box((&left, &right));
    }
    let elapsed = start.elapsed().as_secs_f64();
    let audio = (blocks * BLOCK) as f64 / f64::from(SAMPLE_RATE);
    audio / elapsed
}
