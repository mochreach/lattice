//! A chain of effects that can branch: lanes side by side, each a column of rows read top to
//! bottom, as on the tracker. A row is an effect, or a send that copies the sound at that point
//! into another lane. A send into a lane further right feeds it straight away; one into the same
//! lane or a lane to its left is feedback, heard a block later. Each lane ends in a level into
//! the chain's output.
//!
//! The effects are kept in one list, in lane order (the first lane's top to bottom, then the
//! next's), which is how locks and the mixer number them; the lanes say where each goes. A chain
//! of one lane, with no sends and its output at unity, is plain, and works as a list of effects
//! always has.

use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};

use crate::fx::{Fx, MAX_EFFECTS};
use crate::song::MAX_DB;

/// The most lanes in a chain.
pub const MAX_LANES: usize = 4;

/// The most sends in a chain, over all its lanes.
pub const MAX_SENDS: usize = 8;

/// The loudest a feedback send can be, in dB, so the loop dies away rather than growing.
pub const MAX_FEEDBACK_DB: f32 = 0.0;

/// One row of a lane.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FxRow {
    /// The next effect in the chain's list.
    Fx,
    /// Copies the sound here into the lane `to`, at `level` dB.
    Send { to: usize, level: f32 },
}

/// A column of rows, and the level it ends in.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FxLane {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub rows: Vec<FxRow>,
    /// Into the chain's output, in dB; at [`crate::MIN_DB`] or below, silent.
    #[serde(skip_serializing_if = "is_zero")]
    pub out: f32,
}

fn is_zero(x: &f32) -> bool {
    *x == 0.0
}

impl FxLane {
    /// Lanes are known by letter: A, B, C, D.
    pub fn letter(index: usize) -> char {
        (b'A' + index as u8) as char
    }
}

/// A chain of effects in lanes. Dereferences to its effects, in lane order.
#[derive(Debug, Clone, PartialEq)]
pub struct FxChain {
    effects: Vec<Fx>,
    /// At least one. As many [`FxRow::Fx`] rows between them as there are effects.
    lanes: Vec<FxLane>,
}

impl Default for FxChain {
    fn default() -> Self {
        Self {
            effects: Default::default(),
            lanes: vec![FxLane::default()],
        }
    }
}

impl From<Vec<Fx>> for FxChain {
    fn from(effects: Vec<Fx>) -> Self {
        let rows = vec![FxRow::Fx; effects.len()];
        Self {
            effects,
            lanes: vec![FxLane {
                rows,
                ..FxLane::default()
            }],
        }
    }
}

impl Deref for FxChain {
    type Target = [Fx];

    fn deref(&self) -> &[Fx] {
        &self.effects
    }
}

impl DerefMut for FxChain {
    fn deref_mut(&mut self) -> &mut [Fx] {
        &mut self.effects
    }
}

impl<'a> IntoIterator for &'a FxChain {
    type Item = &'a Fx;
    type IntoIter = std::slice::Iter<'a, Fx>;

    fn into_iter(self) -> Self::IntoIter {
        self.effects.iter()
    }
}

/// Where a row is: its lane, and its place in the lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowAt {
    pub lane: usize,
    pub row: usize,
}

impl FxChain {
    /// A chain from its effects and lanes as saved: no lanes is one plain lane.
    pub fn from_parts(effects: Vec<Fx>, lanes: Vec<FxLane>) -> Result<Self, String> {
        if lanes.is_empty() {
            return Ok(effects.into());
        }
        let chain = Self { effects, lanes };
        chain.check()?;
        Ok(chain)
    }

    /// The effects and, unless it's plain, the lanes, to save.
    pub fn into_parts(self) -> (Vec<Fx>, Vec<FxLane>) {
        let lanes = if self.is_plain() { vec![] } else { self.lanes };
        (self.effects, lanes)
    }

    /// Whether it's a list of effects and nothing more: one lane, no sends, at unity.
    pub fn is_plain(&self) -> bool {
        match &self.lanes[..] {
            [lane] => lane.out == 0.0 && lane.rows.iter().all(|r| *r == FxRow::Fx),
            _ => false,
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn lanes(&self) -> &[FxLane] {
        &self.lanes
    }

    pub fn sends(&self) -> usize {
        let rows = self.lanes.iter().flat_map(|l| &l.rows);
        rows.filter(|r| matches!(r, FxRow::Send { .. })).count()
    }

    /// Each row in order, lane by lane, with where it is and, for an effect, its index.
    pub fn rows(&self) -> impl Iterator<Item = (RowAt, FxRow, Option<usize>)> + '_ {
        let mut next = 0;
        self.lanes
            .iter()
            .enumerate()
            .flat_map(move |(lane, l)| {
                l.rows
                    .iter()
                    .enumerate()
                    .map(move |(row, &r)| (lane, row, r))
            })
            .map(move |(lane, row, r)| {
                let effect = (r == FxRow::Fx).then(|| {
                    next += 1;
                    next - 1
                });
                (RowAt { lane, row }, r, effect)
            })
    }

    /// Where effect `index` is.
    pub fn effect_at(&self, index: usize) -> Option<RowAt> {
        self.rows()
            .find(|(_, _, effect)| *effect == Some(index))
            .map(|(at, _, _)| at)
    }

    /// The index the next effect at `at` would have: how many effects come before it.
    fn effects_before(&self, at: RowAt) -> usize {
        self.rows()
            .take_while(|(r, _, _)| (r.lane, r.row) < (at.lane, at.row))
            .filter(|(_, row, _)| *row == FxRow::Fx)
            .count()
    }

    /// Inserts `fx` as effect `index`: just after the effect before it, in that one's lane, or
    /// at the top of the first lane.
    pub fn insert(&mut self, index: usize, fx: Fx) {
        let index = index.min(self.effects.len());
        let at = match index.checked_sub(1).and_then(|i| self.effect_at(i)) {
            Some(before) => RowAt {
                row: before.row + 1,
                ..before
            },
            None => RowAt { lane: 0, row: 0 },
        };
        self.insert_at(at, fx);
    }

    /// Inserts `fx` as the row at `at`, in its lane, returning its index among the effects.
    pub fn insert_at(&mut self, at: RowAt, fx: Fx) -> usize {
        let lane = at.lane.min(self.lanes.len() - 1);
        let row = at.row.min(self.lanes[lane].rows.len());
        let index = self.effects_before(RowAt { lane, row });
        self.lanes[lane].rows.insert(row, FxRow::Fx);
        self.effects.insert(index, fx);
        index
    }

    /// Adds `fx` at the end of the last effect's lane, or the first lane.
    pub fn push(&mut self, fx: Fx) {
        self.insert(self.effects.len(), fx);
    }

    /// Removes effect `index`, and its row.
    pub fn remove(&mut self, index: usize) -> Fx {
        let at = self.effect_at(index).expect("each effect has a row");
        self.lanes[at.lane].rows.remove(at.row);
        self.effects.remove(index)
    }

    /// Checks it's one a song can hold.
    pub fn check(&self) -> Result<(), String> {
        let lanes = self.lanes.len();
        if lanes == 0 || lanes > MAX_LANES {
            return Err(format!("a chain has 1 to {MAX_LANES} lanes, not {lanes}"));
        }
        if self.effects.len() > MAX_EFFECTS {
            return Err(format!("more than {MAX_EFFECTS} effects"));
        }
        let rows = self.lanes.iter().flat_map(|l| &l.rows);
        let placed = rows.filter(|r| **r == FxRow::Fx).count();
        if placed != self.effects.len() {
            return Err(format!(
                "{} effects, but {placed} rows for effects in the lanes",
                self.effects.len()
            ));
        }
        if self.sends() > MAX_SENDS {
            return Err(format!("more than {MAX_SENDS} sends"));
        }
        for (from, lane) in self.lanes.iter().enumerate() {
            let name = FxLane::letter(from);
            if !(lane.out.is_finite() && lane.out <= MAX_DB) {
                return Err(format!("lane {name}'s out goes up to {MAX_DB} dB"));
            }
            for row in &lane.rows {
                let &FxRow::Send { to, level } = row else {
                    continue;
                };
                if to >= lanes {
                    return Err(format!("lane {name} sends to a lane that isn't there"));
                }
                let most = if to <= from { MAX_FEEDBACK_DB } else { MAX_DB };
                if !(level.is_finite() && level <= most) {
                    return Err(format!("lane {name}'s send goes up to {most} dB"));
                }
            }
        }
        Ok(())
    }
}

/// Whether a send from lane `from` into lane `to` is feedback, heard a block later.
pub fn is_feedback(from: usize, to: usize) -> bool {
    to <= from
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::FxKind;

    fn fx(kind: FxKind) -> Fx {
        Fx::new(kind)
    }

    #[test]
    fn effects_are_numbered_in_lane_order() {
        let mut chain = FxChain::from(vec![fx(FxKind::Filter), fx(FxKind::Reverb)]);
        assert!(chain.is_plain());
        chain.lanes.push(FxLane::default());
        chain.lanes[0]
            .rows
            .insert(1, FxRow::Send { to: 1, level: -3.0 });
        let pitch = chain.insert_at(RowAt { lane: 1, row: 0 }, fx(FxKind::Delay));
        assert_eq!(pitch, 2, "after the first lane's");
        chain.lanes[1].rows.push(FxRow::Send { to: 0, level: -6.0 });
        assert!(!chain.is_plain());
        assert_eq!(chain.check(), Ok(()));
        assert_eq!(chain.effect_at(1), Some(RowAt { lane: 0, row: 2 }));
        assert_eq!(chain.effect_at(2), Some(RowAt { lane: 1, row: 0 }));

        // Inserted after effect 0, it's in its lane, and the rest move along.
        chain.insert(1, fx(FxKind::Drive));
        let kinds: Vec<_> = chain.iter().map(|f| f.kind).collect();
        assert_eq!(
            kinds,
            [FxKind::Filter, FxKind::Drive, FxKind::Reverb, FxKind::Delay]
        );
        assert_eq!(chain.effect_at(1), Some(RowAt { lane: 0, row: 1 }));
        assert_eq!(chain.remove(3).kind, FxKind::Delay);
        assert_eq!(chain.lanes[1].rows, [FxRow::Send { to: 0, level: -6.0 }]);
    }

    #[test]
    fn saved_as_a_list_unless_it_branches() {
        let plain = FxChain::from(vec![fx(FxKind::Comp)]);
        let (effects, lanes) = plain.clone().into_parts();
        assert!(lanes.is_empty());
        assert_eq!(FxChain::from_parts(effects, lanes), Ok(plain));

        let mut branched = FxChain::from(vec![fx(FxKind::Comp)]);
        branched.lanes[0].out = -3.0;
        let (effects, lanes) = branched.clone().into_parts();
        assert_eq!(lanes.len(), 1);
        assert_eq!(FxChain::from_parts(effects, lanes), Ok(branched));
    }

    #[test]
    fn mistakes_are_caught() {
        let lane = |rows: Vec<FxRow>| FxLane {
            rows,
            ..FxLane::default()
        };
        let check = |lanes| FxChain::from_parts(vec![fx(FxKind::Comp)], lanes);
        assert!(check(vec![lane(vec![])]).is_err(), "an effect with no row");
        let to = |to, level| FxRow::Send { to, level };
        assert!(check(vec![lane(vec![FxRow::Fx, to(1, 0.0)])]).is_err());
        let loud = vec![lane(vec![FxRow::Fx]), lane(vec![to(0, 3.0)])];
        assert!(check(loud).is_err(), "feedback that grows");
        let forward = vec![lane(vec![FxRow::Fx, to(1, 3.0)]), lane(vec![])];
        assert!(check(forward).is_ok());
    }
}
