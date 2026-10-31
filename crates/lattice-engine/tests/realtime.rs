//! The engine must never allocate on the audio thread. `assert_no_alloc` aborts if it does.

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use lattice_core::instrument::{kick, string};
use lattice_core::{Condition, Grid, InstKind, Instrument, Rule, Step, Track, Transform};
use lattice_engine::{
    AdsrParams, Chain, Command, Comp, Delay, Drive, EffectSlot, Engine, Patch, Processor, Synth,
    Target, TrackPlayer, Wave, delay,
};

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
        mix: Default::default(),
        instrument: Default::default(),
        effects: vec![],
        choke: 0,
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
        groups: vec![],
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
    let synth: Box<dyn Processor> = Box::new(Synth::new(patch, 48_000.0));
    // Tracks to add while playing, built here on the "UI thread".
    let mut players: Vec<_> = (0..2)
        .map(|_| {
            TrackPlayer::new(
                grid.tracks[0].clone(),
                Box::new(Synth::new(patch, 48_000.0)),
                48_000.0,
            )
        })
        .collect();
    let replacement = Box::new(grid.tracks[0].clone());
    let mut muted = grid.tracks[0].clone();
    muted.mix.mute = true;
    muted.mix.send = 0.0;
    let mut muted = Some(Box::new(muted));
    let mut send_delay = Some(EffectSlot::new(Box::new(Delay::new(48_000.0))));
    let mut track_delay = Some(EffectSlot::new(Box::new(Delay::new(48_000.0))));
    let mut track_drive = Some(EffectSlot::new(Box::new(Drive::new(48_000.0))));
    let mut track_comp = Some(EffectSlot::new(Box::new(Comp::new(48_000.0))));
    let mut group_comp = Some(EffectSlot::new(Box::new(Comp::new(48_000.0))));
    let mut drums: Vec<_> = [InstKind::Hat, InstKind::Snare, InstKind::Kick]
        .into_iter()
        .map(|kind| lattice_engine::instrument::build(&Instrument::new(kind), 48_000.0))
        .collect();
    let mut strings = Some(lattice_engine::instrument::build(
        &Instrument::new(InstKind::String),
        48_000.0,
    ));
    let (mut engine, mut handle) = Engine::new(48_000.0, 174.0, &grid, vec![synth]);
    // Commands are handled inside the no-allocation zone too.
    handle.send(Command::Play);
    handle.send(Command::SetBpm(140.0));
    handle.send(Command::Preview {
        track: 0,
        pitch: 72,
        velocity: 100,
    });
    handle.send(Command::SetTrack {
        index: 0,
        track: replacement,
    });
    let mut left = vec![0.0; 256];
    let mut right = vec![0.0; 256];

    let mut peak = 0.0f32;
    assert_no_alloc(|| {
        // Ten seconds: many passes through the track, so transforms are realised repeatedly.
        // Tracks come and go along the way; sending commands doesn't allocate either.
        for block in 0..(48_000 * 10 / 256) {
            match block {
                200 => {
                    handle.send(Command::InsertTrack {
                        index: 0,
                        player: players.pop().unwrap(),
                    });
                }
                400 => {
                    handle.send(Command::MoveTrack { from: 0, to: 1 });
                }
                600 => {
                    handle.send(Command::RemoveTrack(1));
                    handle.send(Command::InsertTrack {
                        index: 1,
                        player: players.pop().unwrap(),
                    });
                }
                700 => {
                    handle.send(Command::InsertEffect {
                        chain: Chain::Send,
                        index: 0,
                        effect: send_delay.take().unwrap(),
                    });
                    handle.send(Command::InsertEffect {
                        chain: Chain::Track(0),
                        index: 0,
                        effect: track_delay.take().unwrap(),
                    });
                    handle.send(Command::InsertEffect {
                        chain: Chain::Track(0),
                        index: 0,
                        effect: track_drive.take().unwrap(),
                    });
                    handle.send(Command::InsertEffect {
                        chain: Chain::Track(0),
                        index: 1,
                        effect: track_comp.take().unwrap(),
                    });
                    handle.send(Command::SetParam {
                        target: Target::Effect {
                            chain: Chain::Send,
                            slot: 0,
                        },
                        id: delay::TIME,
                        value: 0.2,
                    });
                }
                720 => {
                    handle.send(Command::SetGroup {
                        index: 0,
                        group: Some(2),
                    });
                    handle.send(Command::InsertEffect {
                        chain: Chain::Group(2),
                        index: 0,
                        effect: group_comp.take().unwrap(),
                    });
                    handle.send(Command::SetBusLevel {
                        bus: Chain::Group(2),
                        level: -3.0,
                    });
                }
                750 | 760 | 770 => {
                    handle.send(Command::SetInstrument {
                        index: 0,
                        instrument: drums.pop().unwrap(),
                    });
                }
                780 => {
                    handle.send(Command::SetParam {
                        target: Target::Instrument(0),
                        id: kick::DECAY,
                        value: 0.1,
                    });
                }
                785 => {
                    handle.send(Command::SetInstrument {
                        index: 0,
                        instrument: strings.take().unwrap(),
                    });
                }
                790 => {
                    handle.send(Command::SetParam {
                        target: Target::Instrument(0),
                        id: string::BOW,
                        value: 0.5,
                    });
                    handle.send(Command::Preview {
                        track: 0,
                        pitch: 40,
                        velocity: 100,
                    });
                }
                800 => {
                    handle.send(Command::RemoveTrack(0));
                    handle.send(Command::RemoveEffect {
                        chain: Chain::Send,
                        index: 0,
                    });
                }
                1_000 => {
                    handle.send(Command::SetTrack {
                        index: 0,
                        track: muted.take().unwrap(),
                    });
                }
                _ => {}
            }
            engine.process(&mut left, &mut right);
            peak = left.iter().fold(peak, |p, x| p.max(x.abs()));
            handle.status().take_peak(0);
        }
    });
    handle.collect_garbage();
    assert!(left.iter().all(|&x| x == 0.0), "muted by the end");
    assert!(peak > 0.05, "the synth should be audible, peak {peak}");
    assert!(peak <= 1.0);
}
