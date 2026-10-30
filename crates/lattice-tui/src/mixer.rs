//! Keeps the engine's effect chains and bus levels in line with the song's, as effects are
//! added, removed and changed.

use std::collections::HashMap;

use lattice_core::Fx;
use lattice_editor::{Editor, TrackId};
use lattice_engine::{Chain, Command, EffectSlot, Handle, Target, fx};

pub struct Mixer {
    sample_rate: f32,
    /// The engine's chains, as far as the commands sent so far go: each track's by id, so they
    /// follow tracks as they move.
    tracks: HashMap<TrackId, Vec<Fx>>,
    send: Vec<Fx>,
    master: Vec<Fx>,
    /// The bus levels sent, in dB. The engine starts at unity.
    levels: [f32; 2],
}

impl Mixer {
    /// For an engine whose chains are all empty.
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            tracks: HashMap::new(),
            send: Vec::new(),
            master: Vec::new(),
            levels: [0.0; 2],
        }
    }

    /// Sends the engine the commands that bring its effects and bus levels in line with the
    /// editor's. Call it after the engine's tracks are in line. Returns false if the command
    /// queue filled up; the rest is sent on the next sync.
    pub fn sync(&mut self, editor: &Editor, handle: &mut Handle) -> bool {
        self.sync_with(editor, |command| handle.send(command))
    }

    fn sync_with(&mut self, editor: &Editor, mut send: impl FnMut(Command) -> bool) -> bool {
        let ids = editor.track_ids();
        // A track that's gone takes its chain with it; if undo brings it back, it comes back
        // with an empty one.
        self.tracks.retain(|id, _| ids.contains(id));
        let sample_rate = self.sample_rate;
        for (index, (id, track)) in ids.iter().zip(&editor.grid.tracks).enumerate() {
            let sent = self.tracks.entry(*id).or_default();
            if !sync_chain(
                Chain::Track(index),
                sent,
                &track.effects,
                sample_rate,
                &mut send,
            ) {
                return false;
            }
        }
        let buses = &editor.buses;
        let [send_level, master_level] = &mut self.levels;
        for (chain, sent, bus, level) in [
            (Chain::Send, &mut self.send, &buses.send, send_level),
            (Chain::Master, &mut self.master, &buses.master, master_level),
        ] {
            if !sync_chain(chain, sent, &bus.effects, sample_rate, &mut send) {
                return false;
            }
            if *level != bus.level {
                if !send(Command::SetBusLevel {
                    bus: chain,
                    level: bus.level,
                }) {
                    return false;
                }
                *level = bus.level;
            }
        }
        true
    }
}

/// Brings one chain in line: effects of a different kind are replaced, keeping those before and
/// after them so one insert or removal doesn't cut the others' tails, then changed settings are
/// sent.
fn sync_chain(
    chain: Chain,
    sent: &mut Vec<Fx>,
    wanted: &[Fx],
    sample_rate: f32,
    send: &mut impl FnMut(Command) -> bool,
) -> bool {
    let same = |a: &Fx, b: &Fx| a.kind == b.kind;
    let before = sent
        .iter()
        .zip(wanted)
        .take_while(|(a, b)| same(a, b))
        .count();
    let after = sent[before..]
        .iter()
        .rev()
        .zip(wanted[before..].iter().rev())
        .take_while(|(a, b)| same(a, b))
        .count();
    for index in (before..sent.len() - after).rev() {
        if !send(Command::RemoveEffect { chain, index }) {
            return false;
        }
        sent.remove(index);
    }
    for (index, fx) in wanted
        .iter()
        .enumerate()
        .take(wanted.len() - after)
        .skip(before)
    {
        let effect = EffectSlot::new(fx::build(fx, sample_rate));
        if !send(Command::InsertEffect {
            chain,
            index,
            effect,
        }) {
            return false;
        }
        sent.insert(index, fx.clone());
    }
    for (slot, (sent, wanted)) in sent.iter_mut().zip(wanted).enumerate() {
        for (id, (old, &new)) in sent.values.iter_mut().zip(&wanted.values).enumerate() {
            if *old != new {
                let target = Target::Effect { chain, slot };
                if !send(Command::SetParam {
                    target,
                    id: id as u32,
                    value: new,
                }) {
                    return false;
                }
                *old = new;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::FxKind;
    use lattice_core::fx::delay::FEEDBACK;

    /// What the engine was sent, without the effects themselves.
    fn sync(mixer: &mut Mixer, editor: &Editor) -> Vec<String> {
        let mut sent = Vec::new();
        assert!(mixer.sync_with(editor, |command| {
            sent.push(match command {
                Command::InsertEffect { chain, index, .. } => format!("insert {chain:?} {index}"),
                other => format!("{other:?}"),
            });
            true
        }));
        sent
    }

    #[test]
    fn chains_follow_the_song() {
        let (grid, _) = crate::demo::song();
        let mut editor = Editor::new(grid, 120.0).with_buses(crate::demo::buses());
        let mut mixer = Mixer::new(48_000.0);
        assert_eq!(sync(&mut mixer, &editor), ["insert Send 0"]);
        assert!(sync(&mut mixer, &editor).is_empty(), "already in line");

        let delay = || Fx::new(FxKind::Delay);
        editor.grid.tracks[1].effects = vec![delay(), delay()];
        editor.buses.master.level = -3.0;
        assert_eq!(
            sync(&mut mixer, &editor),
            [
                "insert Track(1) 0",
                "insert Track(1) 1",
                "SetBusLevel { bus: Master, level: -3.0 }"
            ]
        );

        editor.grid.tracks[1].effects.remove(0);
        editor.grid.tracks[1].effects[0].values[FEEDBACK as usize] = 0.5;
        assert_eq!(
            sync(&mut mixer, &editor),
            [
                "RemoveEffect { chain: Track(1), index: 1 }",
                "SetParam { target: Effect { chain: Track(1), slot: 0 }, id: 1, value: 0.5 }"
            ]
        );
    }
}
