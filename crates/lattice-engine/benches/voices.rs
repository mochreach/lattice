//! CPU budget: how much of one core each instrument needs. Run with `cargo bench`, and on every
//! machine we care about (laptop, Raspberry Pi 4, Pi 3) to see where the limits are.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::indexing_slicing
)]

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
    all::run();
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

/// Every instrument, holding eight notes; every effect, on noise; and the whole engine playing a
/// track of each instrument, every step a note, through a shimmer's lanes and every effect.
mod all {
    use super::*;
    use lattice_core::{Fx, FxKind, Grid, InstKind, Instrument, Kind, Step, Track, built_in_chain};
    use lattice_engine::{Chain, Command, EffectSlot, Engine, Routing, fx, instrument};

    pub fn run() {
        for kind in InstKind::ALL {
            let mut processor = instrument::build(&Instrument::new(kind), SAMPLE_RATE, None);
            let notes: Vec<Event> = (0..8)
                .map(|i| Event {
                    offset: 0,
                    kind: EventKind::NoteOn {
                        pitch: 36 + 5 * i,
                        velocity: 100,
                    },
                })
                .collect();
            let report = budget(processor.as_mut(), &notes);
            // Long after its notes have ended, as most tracks are most of the time.
            let mut idle = instrument::build(&Instrument::new(kind), SAMPLE_RATE, None);
            let idle = budget(idle.as_mut(), &[]);
            println!(
                "{:>10}: {report:>6.0}x real time, idle {idle:>8.0}x",
                kind.name()
            );
        }
        for kind in FxKind::ALL {
            let mut processor = fx::build(&Fx::new(kind), SAMPLE_RATE);
            let report = effect_budget(processor.as_mut(), 1.0);
            let mut idle = fx::build(&Fx::new(kind), SAMPLE_RATE);
            let idle = effect_budget(idle.as_mut(), 0.0);
            println!(
                "{:>10}: {report:>6.0}x real time, on silence {idle:>8.0}x",
                kind.name()
            );
        }
        println!("    engine: {:>8.1}x real time", engine_budget());
    }

    /// Runs `effect` on noise, at `level`: 0 for silence.
    fn effect_budget(effect: &mut dyn Processor, level: f32) -> f64 {
        let blocks = SECONDS * SAMPLE_RATE as usize / BLOCK;
        let mut seed = 1u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1 << 24) as f32 - 0.5
        };
        let input: Vec<f32> = (0..BLOCK).map(|_| noise() * level).collect();
        let (mut left, mut right) = (input.clone(), input.clone());
        let start = Instant::now();
        for _ in 0..blocks {
            left.copy_from_slice(&input);
            right.copy_from_slice(&input);
            effect.process(&[], &mut left, &mut right);
            black_box((&left, &right));
        }
        (blocks * BLOCK) as f64 / f64::from(SAMPLE_RATE) / start.elapsed().as_secs_f64()
    }

    fn engine_budget() -> f64 {
        let tracks: Vec<Track> = (InstKind::ALL.iter().enumerate())
            .map(|(i, kind)| Track {
                name: kind.name().into(),
                steps: vec![Step::note(36 + i as u8 * 3); 16],
                instrument: Instrument::new(*kind),
                ..Track::default()
            })
            .collect();
        let instruments = (tracks.iter())
            .map(|t| instrument::build(&t.instrument, SAMPLE_RATE, None))
            .collect();
        let grid = Grid {
            name: "bench".into(),
            steps_per_beat: 4,
            beats_per_bar: 4,
            grouping: vec![],
            tracks,
            groups: vec![],
        };
        let (mut engine, mut handle) = Engine::new(SAMPLE_RATE, 140.0, &grid, instruments);
        let shimmer = built_in_chain("shimmer").unwrap_or_default();
        for t in 0..grid.tracks.len() {
            let chain = Chain::Track(t);
            for (index, fx) in shimmer.iter().enumerate() {
                let effect = EffectSlot::with_values(fx::build(fx, SAMPLE_RATE), &fx.values);
                handle.send(Command::InsertEffect {
                    chain,
                    index,
                    effect,
                });
            }
            let after = shimmer.len();
            for (i, kind) in FxKind::ALL.into_iter().enumerate() {
                let effect = EffectSlot::new(fx::build(&Fx::new(kind), SAMPLE_RATE));
                handle.send(Command::InsertEffect {
                    chain,
                    index: after + i,
                    effect,
                });
            }
            let mut routed = shimmer.clone();
            for kind in FxKind::ALL {
                routed.push(Fx::new(kind));
            }
            handle.send(Command::SetRouting {
                chain,
                routing: Routing::new(&routed),
            });
        }
        engine.play();
        let mut left = vec![0.0; BLOCK];
        let mut right = vec![0.0; BLOCK];
        let blocks = SECONDS * SAMPLE_RATE as usize / BLOCK / 4;
        engine.process(&mut left, &mut right);
        let start = Instant::now();
        for _ in 0..blocks {
            engine.process(&mut left, &mut right);
            black_box((&left, &right));
        }
        (blocks * BLOCK) as f64 / f64::from(SAMPLE_RATE) / start.elapsed().as_secs_f64()
    }
}
