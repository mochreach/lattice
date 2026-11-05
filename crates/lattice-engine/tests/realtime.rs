//! The engine must never allocate on the audio thread. `assert_no_alloc` aborts if it does.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::indexing_slicing
)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use lattice_core::instrument::{kick, krell, monochord, switched};
use lattice_core::{
    Condition, Fx, FxKind, Grid, InstKind, Instrument, Lane, Lock, Macro, MacroTarget, Rule, Step,
    Track, Transform,
};
use lattice_engine::{
    AdsrParams, Chain, Command, Comp, Cue, Delay, Drive, EffectSlot, Engine, Patch, Processor,
    Sample, Section, Synth, Take, Target, TrackPlayer, Wave, delay,
};
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn rendering_never_allocates() {
    let track = Track {
        face: None,
        pattern_mute: false,
        columns: vec![],
        name: "lead".into(),
        steps: (0..11)
            .map(|i| {
                let mut step = if i % 2 == 0 {
                    Step::note(60 + i)
                } else {
                    Step::default()
                };
                // Locks on the instrument and on effects, some there and some not.
                if i % 3 == 0 {
                    for fx in [None, Some(0), Some(5)] {
                        step.locks.set(Lock {
                            fx,
                            param: 1,
                            value: 0.3,
                            ..Default::default()
                        });
                    }
                }
                // And on a macro.
                if i % 3 == 1 {
                    step.locks.set(Lock::of_macro(0, 0.9));
                }
                // Every gesture, notes played later in a step and playheads moved.
                let gestures = [
                    ["R9", "D3"],
                    ["r4", "?50"],
                    ["<", "O8"],
                    ["*4", "W2"],
                    ["G6", "R2"],
                    ["A37", "1:2"],
                    ["B", "J?"],
                    ["/3", "J0"],
                    ["X12", "*2"],
                    ["!", "F-"],
                    ["F", "R2"],
                ];
                if let Some(pair) = gestures.get(i as usize) {
                    step.gestures = pair.map(|g| g.parse().ok());
                }
                step
            })
            .collect(),
        mix: Default::default(),
        instrument: Default::default(),
        effects: Default::default(),
        choke: 0,
        // A macro turning the instrument and effects, some there and some not.
        macros: vec![Macro {
            name: "bloom".into(),
            value: 0.4,
            targets: [None, Some(0), Some(5)]
                .into_iter()
                .map(|fx| MacroTarget {
                    fx,
                    param: 1,
                    depth: 0.5,
                })
                .collect(),
        }],
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
        beats_per_bar: 4,
        grouping: vec![],
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
    let mut more_effects: Vec<_> = [
        FxKind::Reverb,
        FxKind::Filter,
        FxKind::Ensemble,
        FxKind::Phaser,
    ]
    .into_iter()
    .map(|kind| EffectSlot::new(lattice_engine::fx::build(&Fx::new(kind), 48_000.0)))
    .collect();
    let mut drums: Vec<_> = [InstKind::Hat, InstKind::Snare, InstKind::Kick]
        .into_iter()
        .map(|kind| lattice_engine::instrument::build(&Instrument::new(kind), 48_000.0, None))
        .collect();
    let mut strings = Some(lattice_engine::instrument::build(
        &Instrument::new(InstKind::Monochord),
        48_000.0,
        None,
    ));
    let mut switched = Some(lattice_engine::instrument::build(
        &Instrument::new(InstKind::Switched),
        48_000.0,
        None,
    ));
    let mut krell = Some(lattice_engine::instrument::build(
        &Instrument::new(InstKind::Krell),
        48_000.0,
        None,
    ));
    // A MIDI track, with somewhere to send.
    let (midi, mut sent) = lattice_engine::midi_out::MidiOut::new(48_000.0, &[2.0]);
    let mut midi: Option<Box<dyn Processor>> = Some(Box::new(midi));
    let sample = std::sync::Arc::new(Sample {
        rate: 44_100.0,
        left: (0..4_410).map(|i| (i as f32 * 0.05).sin()).collect(),
        right: vec![0.1; 4_410],
    });
    let mut sampler = Some(lattice_engine::instrument::build(
        &Instrument::new(InstKind::Sampler),
        48_000.0,
        Some(sample),
    ));
    let (mut engine, mut handle) = Engine::new(48_000.0, 174.0, &grid, vec![synth]);
    let (clock, mut ticks) = lattice_engine::midi_out::clock_queue();
    // Commands are handled inside the no-allocation zone too.
    handle.send(Command::SetClockOut(clock));
    handle.send(Command::Play { count_in: 0 });
    handle.send(Command::SetBpm(140.0));
    handle.send(Command::Fill(true));
    handle.send(Command::SetMacro {
        track: 0,
        index: 0,
        value: 0.7,
    });
    handle.send(Command::Preview {
        track: 0,
        pitch: 72,
        velocity: 100,
        held: true,
    });
    handle.send(Command::SetTrack {
        index: 0,
        track: replacement,
        swaps: 0,
    });
    // Another pattern to change to, with a rule to realise.
    let mut section = Some(Box::new(Section {
        conductor: Default::default(),
        lanes: vec![Lane {
            mute: false,
            steps: vec![Step::note(48); 5],
            rules: vec![Rule {
                transform: Transform::Reverse,
                when: Condition::Always,
            }],
        }],
        steps: 16,
        beats_per_bar: 4,
        groups: 1,
        at_bar: false,
        // Looping round some of it, so jumping back is checked too.
        looped: Some((2, 7)),
    }));
    // The looper, recording an input fed alongside.
    let (mut input, input_rx) = rtrb::RingBuffer::new(48_000);
    let mut input_rx = Some(input_rx);
    let mut takes: Vec<_> = (0..3).map(|_| Take::new(0, 48_000)).collect();
    let mut new_sample = Some(Some(Arc::new(Sample {
        rate: 44_100.0,
        left: vec![0.1; 1000],
        right: vec![0.1; 1000],
    })));
    let mut left = vec![0.0; 256];
    let mut right = vec![0.0; 256];

    let mut peak = 0.0f32;
    assert_no_alloc(|| {
        // Ten seconds: many passes through the track, so transforms are realised repeatedly.
        // Tracks come and go along the way; sending commands doesn't allocate either.
        for block in 0..(48_000 * 10 / 256) {
            for i in 0..512 {
                let _ = input.push((i as f32 * 0.01).sin());
            }
            match block {
                100 => {
                    handle.send(Command::SetInput(input_rx.take().unwrap()));
                    // The third is one too many, and sent straight back.
                    for take in takes.drain(..) {
                        handle.send(Command::Record(take));
                    }
                }
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
                730 | 735 | 740 | 745 => {
                    handle.send(Command::InsertEffect {
                        chain: Chain::Master,
                        index: 0,
                        effect: more_effects.pop().unwrap(),
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
                        id: monochord::BOW,
                        value: 0.5,
                    });
                    handle.send(Command::Preview {
                        track: 0,
                        pitch: 40,
                        velocity: 100,
                        held: true,
                    });
                }
                795 => {
                    handle.send(Command::Release {
                        track: 0,
                        pitch: 40,
                    });
                    handle.send(Command::Repeat {
                        track: 0,
                        pitch: 45,
                        velocity: 90,
                        every: 1,
                        now: true,
                    });
                }
                798 => {
                    handle.send(Command::Release {
                        track: 0,
                        pitch: 45,
                    });
                }
                800 => {
                    handle.send(Command::RemoveTrack(0));
                    handle.send(Command::RemoveEffect {
                        chain: Chain::Send,
                        index: 0,
                    });
                }
                803 => {
                    handle.send(Command::SetInstrument {
                        index: 0,
                        instrument: switched.take().unwrap(),
                    });
                    handle.send(Command::SetParam {
                        target: Target::Instrument(0),
                        id: switched::GRIT,
                        value: 0.8,
                    });
                    handle.send(Command::Preview {
                        track: 0,
                        pitch: 48,
                        velocity: 100,
                        held: true,
                    });
                }
                806 => {
                    handle.send(Command::SetParam {
                        target: Target::Instrument(0),
                        id: switched::STACK,
                        value: 3.0,
                    });
                    handle.send(Command::Release {
                        track: 0,
                        pitch: 48,
                    });
                }
                807 => {
                    handle.send(Command::SetInstrument {
                        index: 0,
                        instrument: krell.take().unwrap(),
                    });
                    handle.send(Command::SetParam {
                        target: Target::Instrument(0),
                        id: krell::STOPS,
                        value: 4.0,
                    });
                    handle.send(Command::Preview {
                        track: 0,
                        pitch: 36,
                        velocity: 100,
                        held: true,
                    });
                }
                808 => {
                    handle.send(Command::SetParam {
                        target: Target::Instrument(0),
                        id: krell::DRIFT,
                        value: 1.0,
                    });
                    handle.send(Command::Release {
                        track: 0,
                        pitch: 36,
                    });
                }
                809 => {
                    handle.send(Command::SetInstrument {
                        index: 0,
                        instrument: midi.take().unwrap(),
                    });
                    handle.send(Command::SetParam {
                        target: Target::Instrument(0),
                        id: lattice_core::instrument::midi::cc(74),
                        value: 64.0,
                    });
                    handle.send(Command::Preview {
                        track: 0,
                        pitch: 60,
                        velocity: 100,
                        held: false,
                    });
                }
                810 => {
                    handle.send(Command::SetInstrument {
                        index: 0,
                        instrument: sampler.take().unwrap(),
                    });
                    handle.send(Command::Preview {
                        track: 0,
                        pitch: 67,
                        velocity: 100,
                        held: true,
                    });
                }
                820 => {
                    handle.send(Command::SetSample {
                        index: 0,
                        sample: new_sample.take().unwrap(),
                    });
                }
                830 => {
                    handle.send(Command::StopRecording);
                }
                850 => {
                    handle.send(Command::SetLength(16));
                    handle.send(Command::Cue(Cue::Pattern(section.take().unwrap())));
                }
                1_000 => {
                    // After the change to the other pattern.
                    handle.send(Command::SetTrack {
                        index: 0,
                        track: muted.take().unwrap(),
                        swaps: 1,
                    });
                }
                1_050 | 1_060 => {
                    // A reverb's tail never quite reaches silence, so take them out again.
                    handle.send(Command::RemoveEffect {
                        chain: Chain::Master,
                        index: 0,
                    });
                }
                1_100 => {
                    handle.send(Command::Cue(Cue::Stop));
                }
                _ => {}
            }
            engine.set_heard_at(1 + block as u64);
            engine.process(&mut left, &mut right);
            peak = left.iter().fold(peak, |p, x| p.max(x.abs()));
            handle.status().take_peak(0);
        }
    });
    handle.collect_garbage();
    let mut recorded = vec![];
    while let Some(take) = handle.recorded() {
        recorded.push(take);
    }
    assert_eq!(recorded.len(), 3, "every take comes back");
    let bytes: Vec<_> = std::iter::from_fn(|| sent.pop().ok()).collect();
    assert!(
        bytes.iter().any(|m| m.bytes() == [0x91, 60, 100]),
        "the MIDI track played on its channel: {bytes:?}"
    );
    assert!(
        ticks.pop().is_ok_and(|t| t.bytes() == [0xfa]),
        "the clock starts"
    );
    assert!(recorded.iter().any(|t| t.complete && !t.is_empty()));
    assert!(left.iter().all(|&x| x == 0.0), "muted by the end");
    assert!(peak > 0.05, "the synth should be audible, peak {peak}");
    assert!(peak <= 1.0);
}
