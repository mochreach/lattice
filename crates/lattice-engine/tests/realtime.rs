//! The engine must never allocate on the audio thread. `assert_no_alloc` aborts if it does.

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use lattice_core::{Condition, Grid, Rule, Step, Track, Transform};
use lattice_engine::{AdsrParams, Engine, Patch, Wave};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn rendering_never_allocates() {
    let track = Track {
        name: "lead".into(),
        steps: (0..7)
            .map(|i| {
                if i % 2 == 0 {
                    Step::note(60 + i)
                } else {
                    Step::default()
                }
            })
            .collect(),
        rules: vec![
            Rule {
                transform: Transform::Drift(1),
                when: Condition::Always,
            },
            Rule {
                transform: Transform::Reverse,
                when: Condition::Every { n: 2, offset: 1 },
            },
        ],
    };
    let grid = Grid {
        name: "test".into(),
        steps_per_beat: 4,
        tracks: vec![track],
    };
    let patch = Patch {
        wave: Wave::Saw,
        amp: AdsrParams {
            attack: 0.005,
            decay: 0.2,
            sustain: 0.5,
            release: 0.1,
        },
        cutoff: 800.0,
        env_amount: 3_000.0,
        resonance: 0.3,
        gain: 0.3,
    };
    let mut engine = Engine::new(48_000.0, 174.0, &grid, &[patch]);
    let mut buffer = vec![0.0; 256];

    let mut peak = 0.0f32;
    assert_no_alloc(|| {
        // Ten seconds: many passes through the track, so transforms are realised repeatedly.
        for _ in 0..(48_000 * 10 / 256) {
            engine.process(&mut buffer);
            peak = buffer.iter().fold(peak, |p, x| p.max(x.abs()));
        }
    });
    assert!(peak > 0.05, "the synth should be audible, peak {peak}");
    assert!(peak <= 1.0);
}
