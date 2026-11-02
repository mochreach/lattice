//! Running a chain that branches: its lanes, the sends between them, and its feedback.
//!
//! A plain chain (one lane, no sends, at unity) runs its effects one after another in place, and
//! has no [`Routing`]. One that branches gets a `Routing`, built off the audio thread with every
//! buffer it needs. Each block, the lanes run left to right: the first starts from the chain's
//! input, the others from silence, and each also hears what was sent back to it. A row runs an
//! effect on the lane, or adds the lane as it is there into another: straight away if that lane
//! is further right, as it hasn't run yet; otherwise into the feedback, which the lane hears
//! [`MAX_BLOCK`] frames later, whatever size the blocks are. The lanes' outputs, each at its
//! level, are the chain's.

use lattice_core::{FxChain, FxRow, MAX_EFFECTS, MAX_LANES, MAX_SENDS, db_to_gain};

use crate::engine::MAX_BLOCK;
use crate::processor::frames;

/// The most rows in a chain: an effect or a send each.
const MAX_OPS: usize = MAX_EFFECTS + MAX_SENDS;

/// Past this, feedback is clipped, so a loop turned up too far can't run away to infinity.
const FEEDBACK_LIMIT: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    /// The next effect.
    Fx,
    /// Adds the lane into lane `to`, at this linear gain.
    Send { to: usize, gain: f32 },
}

type Buffer = [f32; MAX_BLOCK];

/// A chain's lanes and sends, with the buffers to run them.
pub struct Routing {
    ops: [Op; MAX_OPS],
    /// Where each lane's rows end in `ops`; the first starts at 0, each other where the last
    /// ended.
    ends: [usize; MAX_LANES],
    lanes: usize,
    /// Each lane's level into the output, linear.
    out: [f32; MAX_LANES],
    /// The gains each send and output were at, which they glide on from.
    send_now: [f32; MAX_OPS],
    out_now: [f32; MAX_LANES],
    /// Each lane's sound, left and right, as it goes down the lane.
    lane: Box<[[Buffer; 2]; MAX_LANES]>,
    /// What's sent back to each lane this block.
    sent_back: Box<[[Buffer; 2]; MAX_LANES]>,
    /// What each lane hears back, a block's frames late: read from `at`, then written over.
    feedback: Box<[[Buffer; 2]; MAX_LANES]>,
    at: usize,
    mixed: Box<[Buffer; 2]>,
}

impl std::fmt::Debug for Routing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let last = self.lanes.checked_sub(1).and_then(|k| self.ends.get(k));
        let ops = last.and_then(|&end| self.ops.get(..end));
        f.debug_struct("Routing")
            .field("ops", &ops)
            .field("ends", &self.ends.get(..self.lanes))
            .finish_non_exhaustive()
    }
}

impl Routing {
    /// The routing for `chain`, or `None` if it's plain and needs none. Allocates, so call it off
    /// the audio thread.
    pub fn new(chain: &FxChain) -> Option<Box<Self>> {
        if chain.is_plain() {
            return None;
        }
        let mut ops = [Op::Fx; MAX_OPS];
        let mut ends = [0; MAX_LANES];
        let mut out = [0.0; MAX_LANES];
        let mut n = 0;
        for ((lane, end), out) in chain.lanes().iter().zip(&mut ends).zip(&mut out) {
            // Rows past the room for them are left out.
            for (op, row) in ops.iter_mut().skip(n).zip(&lane.rows) {
                *op = match *row {
                    FxRow::Fx => Op::Fx,
                    FxRow::Send { to, level } => Op::Send {
                        to,
                        gain: db_to_gain(level),
                    },
                };
                n += 1;
            }
            *end = n;
            *out = db_to_gain(lane.out);
        }
        let silence = || Box::new([[[0.0; MAX_BLOCK]; 2]; MAX_LANES]);
        let gains = ops.map(|op| match op {
            Op::Send { gain, .. } => gain,
            Op::Fx => 0.0,
        });
        Some(Box::new(Self {
            ops,
            ends,
            lanes: chain.lanes().len().clamp(1, MAX_LANES),
            out,
            send_now: gains,
            out_now: out,
            lane: silence(),
            sent_back: silence(),
            feedback: silence(),
            at: 0,
            mixed: Box::new([[0.0; MAX_BLOCK]; 2]),
        }))
    }

    /// Whether `other` has the same lanes and sends, going to the same places, so it can carry
    /// on from where this one is.
    fn same_shape(&self, other: &Self) -> bool {
        let shape = |op: &Op| match *op {
            Op::Fx => None,
            Op::Send { to, .. } => Some(to),
        };
        self.lanes == other.lanes
            && self.ends == other.ends
            && self.ops.iter().map(shape).eq(other.ops.iter().map(shape))
    }

    /// Carries on from `old`, if it's the same shape: its feedback still to be heard, and its
    /// levels gliding to these. So changing a level doesn't cut the tails. Doesn't allocate.
    pub fn take_over(&mut self, old: &Self) {
        if !self.same_shape(old) {
            return;
        }
        self.send_now = old.send_now;
        self.out_now = old.out_now;
        self.feedback.copy_from_slice(&old.feedback[..]);
        self.at = old.at;
    }

    /// Runs the chain on `left` and `right`, at most [`MAX_BLOCK`] frames, in place;
    /// `effect(i, ..)` runs effect `i` on a lane.
    pub fn run(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        mut effect: impl FnMut(usize, &mut [f32], &mut [f32]),
    ) {
        let n = left.len().min(right.len()).min(MAX_BLOCK);
        let (left, right) = (frames(left, n), frames(right, n));
        let Self {
            ops,
            ends,
            lanes,
            out,
            send_now,
            out_now,
            lane,
            sent_back,
            feedback,
            at,
            mixed,
        } = self;
        let lanes = *lanes;
        // Where the ring of feedback starts. `n` and this are at most `MAX_BLOCK`, the length of
        // every buffer here, so the splits below are in range.
        let at = *at % MAX_BLOCK;
        let ins = [&*left, &*right];

        // Each lane starts from its input and what's come back to it.
        let starts = lane
            .iter_mut()
            .zip(sent_back.iter_mut())
            .zip(feedback.iter());
        for (k, ((sides, sent), back)) in starts.enumerate().take(lanes) {
            let sides = sides.iter_mut().zip(sent.iter_mut()).zip(back).zip(ins);
            for (((buffer, sent), back), input) in sides {
                let buffer = frames(buffer, n);
                if k == 0 {
                    for (x, &y) in buffer.iter_mut().zip(input) {
                        *x = y;
                    }
                } else {
                    buffer.fill(0.0);
                }
                // The ring read from `at`: to its end, then round from its start.
                let (front, rest) = back.split_at(at);
                let (first, second) = buffer.split_at_mut(rest.len().min(buffer.len()));
                mix_into(first, rest);
                mix_into(second, front);
                frames(sent, n).fill(0.0);
            }
        }
        for side in mixed.iter_mut() {
            frames(side, n).fill(0.0);
        }

        let mut next = 0;
        let mut start = 0;
        let outs = ends.iter().zip(out.iter()).zip(out_now.iter_mut());
        for (k, ((&end, &level), now)) in outs.enumerate().take(lanes) {
            let rows = ops.get(start..end).into_iter().flatten();
            let gains = send_now.get_mut(start..end).into_iter().flatten();
            start = end;
            for (&op, gain_now) in rows.zip(gains) {
                match op {
                    Op::Fx => {
                        if let Some([l, r]) = lane.get_mut(k) {
                            effect(next, frames(l, n), frames(r, n));
                        }
                        next += 1;
                    }
                    Op::Send { to, gain } => {
                        let from = std::mem::replace(gain_now, gain);
                        if to > k {
                            // Straight into a lane further right, which hasn't run yet.
                            if let Some((before, after)) = lane.split_at_mut_checked(to)
                                && let (Some(src), Some(dst)) = (before.get(k), after.first_mut())
                            {
                                add(src, dst, n, from, gain);
                            }
                        } else if let (Some(src), Some(dst)) = (lane.get(k), sent_back.get_mut(to))
                        {
                            add(src, dst, n, from, gain);
                        }
                    }
                }
            }
            let from = std::mem::replace(now, level);
            if let Some(src) = lane.get(k) {
                add(src, mixed, n, from, level);
            }
        }

        // What was sent back is heard a block's frames from now.
        for (sent, back) in sent_back.iter().zip(feedback.iter_mut()).take(lanes) {
            for (sent, back) in sent.iter().zip(back.iter_mut()) {
                // Written to the ring as it's read: from `at` to its end, then from its start.
                let (sent, _) = sent.split_at(n);
                let (front, rest) = back.split_at_mut(at);
                let (first, second) = sent.split_at(rest.len().min(sent.len()));
                for (b, &x) in rest
                    .iter_mut()
                    .zip(first)
                    .chain(front.iter_mut().zip(second))
                {
                    *b = x.clamp(-FEEDBACK_LIMIT, FEEDBACK_LIMIT);
                }
            }
        }
        self.at = (at + n) % MAX_BLOCK;
        for (output, mixed) in [left, right].into_iter().zip(self.mixed.iter()) {
            for (o, &m) in output.iter_mut().zip(mixed) {
                *o = m;
            }
        }
    }
}

fn mix_into(out: &mut [f32], src: &[f32]) {
    for (o, &s) in out.iter_mut().zip(src) {
        *o += s;
    }
}

/// Adds the first `n` frames of `from` into `into`, the gain gliding from `start` to `end`.
fn add(from: &[Buffer; 2], into: &mut [Buffer; 2], n: usize, start: f32, end: f32) {
    let step = (end - start) / n.max(1) as f32;
    for (into, from) in into.iter_mut().zip(from) {
        let pairs = frames(into, n).iter_mut().zip(from);
        for (i, (y, &x)) in pairs.enumerate() {
            *y += x * (start + step * (i + 1) as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::{Fx, FxKind, FxLane, RowAt};

    /// A chain: lane A with an effect, then sends; lane B with an effect and sends.
    fn chain(a: Vec<FxRow>, b: Vec<FxRow>, outs: [f32; 2]) -> FxChain {
        let effects = (a.iter().chain(&b))
            .filter(|r| **r == FxRow::Fx)
            .map(|_| Fx::new(FxKind::Drive))
            .collect();
        let lanes = [a, b]
            .into_iter()
            .zip(outs)
            .map(|(rows, out)| FxLane {
                rows,
                out,
                ..FxLane::default()
            })
            .collect();
        FxChain::from_parts(effects, lanes).unwrap()
    }

    /// Runs `blocks` blocks of `len` frames of an impulse, then silence, with each effect
    /// scaling by `1 + i`, and returns the left channel.
    fn run(routing: &mut Routing, blocks: usize, len: usize) -> Vec<f32> {
        let mut heard = vec![];
        for b in 0..blocks {
            let mut left = vec![0.0; len];
            if b == 0 {
                left[0] = 1.0;
            }
            let mut right = left.clone();
            routing.run(&mut left, &mut right, |i, l, r| {
                for x in l.iter_mut().chain(r.iter_mut()) {
                    *x *= 1.0 + i as f32;
                }
            });
            heard.extend(left);
        }
        heard
    }

    #[test]
    fn a_plain_chain_needs_no_routing() {
        let plain = FxChain::from(vec![Fx::new(FxKind::Drive)]);
        assert!(Routing::new(&plain).is_none());
    }

    #[test]
    fn a_branch_runs_beside_the_main_lane() {
        // A: effect 0 (×1), send to B; B: effect 1 (×2). Both out at unity.
        let send = FxRow::Send { to: 1, level: 0.0 };
        let mut routing =
            Routing::new(&chain(vec![FxRow::Fx, send], vec![FxRow::Fx], [0.0, 0.0])).unwrap();
        let heard = run(&mut routing, 1, 8);
        assert_eq!(
            heard[0],
            1.0 + 2.0,
            "the main lane plus the branch, doubled"
        );
        assert!(heard[1..].iter().all(|&x| x == 0.0));
    }

    #[test]
    fn feedback_comes_round_a_block_later_whatever_the_blocks() {
        // A: send back to A at -6 dB, out at unity: an echo every MAX_BLOCK frames.
        let back = FxRow::Send { to: 0, level: -6.0 };
        let mut routing = Routing::new(&chain(vec![back], vec![], [0.0, -60.0])).unwrap();
        // Blocks of an odd size, so the echoes fall across them.
        let heard = run(&mut routing, 12, 100);
        let half = db_to_gain(-6.0);
        assert_eq!(heard[0], 1.0);
        assert!(
            (heard[MAX_BLOCK] - half).abs() < 1e-6,
            "{}",
            heard[MAX_BLOCK]
        );
        assert!((heard[2 * MAX_BLOCK] - half * half).abs() < 1e-6);
        let echoes = heard.iter().filter(|&&x| x != 0.0).count();
        assert_eq!(echoes, 1 + (1200 - 1) / MAX_BLOCK);
    }

    #[test]
    fn a_new_level_keeps_the_feedback_going() {
        let lanes = |level| chain(vec![FxRow::Send { to: 0, level }], vec![], [0.0, -60.0]);
        let mut routing = Routing::new(&lanes(-6.0)).unwrap();
        run(&mut routing, 1, MAX_BLOCK);
        let mut louder = Routing::new(&lanes(-3.0)).unwrap();
        louder.take_over(&routing);
        let heard = run(&mut louder, 1, MAX_BLOCK);
        // The echo of the first impulse, on top of this one.
        assert!(
            (heard[0] - 1.0 - db_to_gain(-6.0)).abs() < 1e-6,
            "the echo still comes"
        );

        // A different shape starts afresh.
        let mut other = chain(vec![FxRow::Send { to: 0, level: -6.0 }], vec![], [0.0, 0.0]);
        other.insert_at(RowAt { lane: 1, row: 0 }, Fx::new(FxKind::Drive));
        let mut other = Routing::new(&other).unwrap();
        other.take_over(&routing);
        assert_eq!(run(&mut other, 1, MAX_BLOCK)[0], 1.0, "just the input");
    }

    #[test]
    fn the_shimmer_rings_on_and_dies_away() {
        const RATE: f32 = 48_000.0;
        let chain = lattice_core::built_in_chain("shimmer").unwrap();
        let mut effects: Vec<_> = (chain.iter())
            .map(|fx| crate::fx::build(fx, RATE))
            .collect();
        let mut routing = Routing::new(&chain).unwrap();
        // A tenth of a second of a 220 Hz sine, then 20 seconds of silence, in blocks.
        let mut energy = vec![];
        for b in 0..(20.1 * RATE) as usize / MAX_BLOCK {
            let mut left: Vec<f32> = (0..MAX_BLOCK)
                .map(|i| {
                    let t = (b * MAX_BLOCK + i) as f32 / RATE;
                    if t < 0.1 {
                        (std::f32::consts::TAU * 220.0 * t).sin() * 0.5
                    } else {
                        0.0
                    }
                })
                .collect();
            let mut right = left.clone();
            routing.run(&mut left, &mut right, |i, l, r| {
                effects[i].process(&[], l, r);
            });
            assert!(left.iter().all(|x| x.is_finite()));
            energy.push(left.iter().map(|x| x * x).sum::<f32>());
        }
        let second = |s: f32| {
            let blocks = RATE as usize / MAX_BLOCK;
            let at = (s * RATE) as usize / MAX_BLOCK;
            energy[at..at + blocks].iter().sum::<f32>()
        };
        assert!(second(2.0) > 1e-3, "still ringing: {}", second(2.0));
        assert!(
            second(19.0) < second(2.0) * 1e-3,
            "dying away: {}",
            second(19.0)
        );
    }
}
