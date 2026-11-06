//! Recording the audio input into a track a pass at a time, for the looper.
//!
//! The UI thread sends a [`Take`] with room for a pass; it starts recording when the track next
//! comes round to its first step and stops when it comes round again, so it's exactly one pass
//! long, then goes back to be saved. A second take can wait behind the first, to start the moment
//! it ends, so passes record back to back for as long as recording's on.
//!
//! A take with a threshold waits instead for the input to reach it, and starts where it's
//! heard: its pass starts on the track's next step after that, wherever in the track it is, and
//! goes round to that step again, with what was played before the step kept as its lead. The
//! UI thread turns the pass round to start on the first step, and lays the lead on its end.
//! Stopped, the engine starts the song as the input's heard (see [`Looper::write`]).
//!
//! What's heard comes out late, and what's played to it comes in late, so the takes start and
//! stop that much after the steps: the latency, which the host measures and the user can adjust.

/// Frames recorded from before the input reaches a take's threshold, so the attack that reaches
/// it isn't cut: as many as there are in the block it's heard in.
const PREROLL: usize = 128;

/// Steps remembered while a take waits for the input, at least as many as the latency spans.
const RECENT: usize = 64;

/// One pass of the input, recorded for a track.
#[derive(Debug)]
pub struct Take {
    /// The track's index, followed as tracks are added, removed and moved.
    pub track: usize,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    /// The level, as a sample's size, that starts the take: it waits for the input to reach
    /// it, rather than for the track's first step. `None` for the first step.
    pub threshold: Option<f32>,
    /// The step its pass starts on: the first, unless the input started it.
    pub step: usize,
    /// Frames recorded before its pass starts, at its beginning: what was played before the
    /// step after the input reached the threshold.
    pub lead: usize,
    /// When it starts and ends, in frames since the engine started.
    start: Option<u64>,
    /// When the input reached its threshold, if it has.
    heard: Option<u64>,
    /// When its pass starts, and on which pass of the track.
    zero: Option<(u64, u32)>,
    end: Option<u64>,
    /// Recorded for a whole pass; false if playback stopped, or recording did, before it began
    /// or ended.
    pub complete: bool,
}

impl Take {
    /// Room for `frames` frames, allocated here, off the audio thread: a pass, and some to spare
    /// in case the tempo slows. Recording stops if it fills.
    pub fn new(track: usize, frames: usize) -> Box<Self> {
        Box::new(Self {
            track,
            left: Vec::with_capacity(frames),
            right: Vec::with_capacity(frames),
            threshold: None,
            step: 0,
            lead: 0,
            start: None,
            heard: None,
            zero: None,
            end: None,
            complete: false,
        })
    }

    pub fn len(&self) -> usize {
        self.left.len()
    }

    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }

    /// Its pass starts at frame `at`, on step `step` of the track's pass `pass`.
    fn pass_from(&mut self, at: u64, pass: u32, step: usize) {
        self.zero = Some((at, pass));
        self.step = step;
        self.lead = self
            .start
            .map_or(0, |start| at.saturating_sub(start) as usize);
    }

    /// Records what of `left` and `right`, the input from frame `now` on, falls in the take,
    /// starting it where the input reaches its threshold, if it's waiting for that. Whether
    /// it's ended, and whether the input's just been heard.
    fn write(&mut self, now: u64, left: &[f32], right: &[f32]) -> (bool, bool) {
        let mut heard = false;
        if self.start.is_none()
            && let Some(threshold) = self.threshold
        {
            let loud =
                (left.iter().zip(right)).position(|(l, r)| l.abs().max(r.abs()) >= threshold);
            if let Some(i) = loud {
                self.start = Some(now + i.saturating_sub(PREROLL) as u64);
                self.heard = Some(now + i as u64);
                heard = true;
            }
        }
        let Some(start) = self.start else {
            return (false, false);
        };
        let until = now + left.len() as u64;
        let end = self.end.unwrap_or(u64::MAX);
        let from = start.max(now);
        let to = end.min(until);
        if from < to {
            let room = self.left.capacity() - self.left.len();
            let range = (from - now) as usize..(to - now) as usize;
            let range = range.start..range.end.min(range.start + room);
            // Within capacity, so it never reallocates. The range is within `left`; a `right`
            // too short for it would leave the sides out of step, so neither is recorded.
            if let (Some(l), Some(r)) = (left.get(range.clone()), right.get(range)) {
                self.left.extend_from_slice(l);
                self.right.extend_from_slice(r);
            }
            if self.left.len() == self.left.capacity() {
                self.complete = true;
                return (true, heard);
            }
        }
        if end <= until {
            self.complete = true;
            return (true, heard);
        }
        (false, heard)
    }
}

/// The take recording, and the one waiting to follow it.
#[derive(Default)]
pub(crate) struct Looper {
    recording: Option<Box<Take>>,
    next: Option<Box<Take>>,
    /// The latency the first take started with, kept while takes follow on, so the measurement
    /// wavering doesn't make passes different lengths.
    latency: Option<u64>,
    /// The steps of the track a take's waiting on the input for, as they're heard, latest last:
    /// when, on which pass, and which step. The input's read a block behind, so the step its
    /// pass starts on may have been sequenced already.
    recent: Recent,
}

/// A ring of the last [`RECENT`] steps, of a fixed size, so remembering one never allocates.
struct Recent {
    steps: [(u64, u32, usize); RECENT],
    len: usize,
    next: usize,
}

impl Default for Recent {
    fn default() -> Self {
        Self {
            steps: [(0, 0, 0); RECENT],
            len: 0,
            next: 0,
        }
    }
}

impl Recent {
    fn push(&mut self, step: (u64, u32, usize)) {
        if let Some(slot) = self.steps.get_mut(self.next) {
            *slot = step;
        }
        self.next = (self.next + 1) % RECENT;
        self.len = (self.len + 1).min(RECENT);
    }

    /// The first step heard at or after frame `at`.
    fn after(&self, at: u64) -> Option<(u64, u32, usize)> {
        (self.steps.iter().take(self.len).copied())
            .filter(|s| s.0 >= at)
            .min_by_key(|s| s.0)
    }

    fn clear(&mut self) {
        self.len = 0;
        self.next = 0;
    }
}

impl Looper {
    /// Takes `take` to record, or hands it back if two are already waiting.
    pub fn arm(&mut self, take: Box<Take>) -> Result<(), Box<Take>> {
        if self.recording.is_none() {
            self.recording = Some(take);
        } else if self.next.is_none() {
            self.next = Some(take);
        } else {
            return Err(take);
        }
        Ok(())
    }

    /// `track` plays step `step` of its pass `pass`, the first of the pass if `new_pass`, at
    /// frame `at`, heard `latency` frames later.
    pub fn step_starts(
        &mut self,
        track: usize,
        (pass, step, new_pass): (u32, usize, bool),
        at: u64,
        latency: u64,
    ) {
        let Some(recording) = self.recording.as_mut().filter(|t| t.track == track) else {
            return;
        };
        let at = at + *self.latency.get_or_insert(latency);
        match (recording.start, recording.zero) {
            (None, _) if recording.threshold.is_none() => {
                if new_pass {
                    recording.start = Some(at);
                    recording.pass_from(at, pass, step);
                }
            }
            // Waiting for the input.
            (None, _) => self.recent.push((at, pass, step)),
            // Heard since the last step: its pass starts here.
            (Some(_), None) => recording.pass_from(at, pass, step),
            (Some(_), Some((_, from))) => {
                let round = pass > from.saturating_add(1) || pass > from && step >= recording.step;
                if recording.end.is_none() && round {
                    recording.end = Some(at);
                    if let Some(next) = self.next.as_mut().filter(|t| t.track == track) {
                        next.start = Some(at);
                        next.pass_from(at, pass, recording.step);
                    }
                }
            }
        }
    }

    /// Records the input from frame `now` on, calling `done` with each take that's ended.
    /// Whether a take waiting for the input has just heard it, for the song to start if it's
    /// stopped.
    pub fn write(
        &mut self,
        now: u64,
        left: &[f32],
        right: &[f32],
        mut done: impl FnMut(Box<Take>),
    ) -> bool {
        if let Some(next) = &mut self.next {
            next.write(now, left, right);
        }
        let Some(recording) = self.recording.as_mut() else {
            return false;
        };
        let (ended, heard) = recording.write(now, left, right);
        if heard {
            // Its pass starts on the first step heard after it, if that's been played already.
            if let Some(onset) = recording.heard
                && let Some((at, pass, step)) = self.recent.after(onset)
            {
                recording.pass_from(at, pass, step);
            }
            self.recent.clear();
        }
        if ended && let Some(recording) = self.recording.take() {
            done(recording);
            self.recording = self.next.take();
            self.settle();
        }
        heard
    }

    /// Lets the latency be measured afresh once nothing's recording.
    fn settle(&mut self) {
        if self.recording.is_none() {
            self.latency = None;
            self.recent.clear();
        }
    }

    /// Stops recording: hands back the takes that haven't started, and with `all`, the one
    /// that has, unfinished. Otherwise that one records to the end of its pass.
    pub fn stop(&mut self, all: bool, mut done: impl FnMut(Box<Take>)) {
        if let Some(next) = self.next.take() {
            done(next);
        }
        if (all || self.recording.as_ref().is_some_and(|t| t.start.is_none()))
            && let Some(recording) = self.recording.take()
        {
            done(recording);
        }
        self.settle();
    }

    /// Whether a take has started recording by frame `now`.
    pub fn recording(&self, now: u64) -> bool {
        self.recording
            .as_ref()
            .is_some_and(|t| t.start.is_some_and(|s| s <= now))
    }

    /// Whether a take is waiting for the input to reach its threshold.
    pub fn listening(&self) -> bool {
        (self.recording.as_ref()).is_some_and(|t| t.start.is_none() && t.threshold.is_some())
    }

    /// Follows the tracks as they're added, removed and moved: `to` says where the track at an
    /// index has gone, or `None` if it's been removed, ending its takes.
    pub fn retarget(
        &mut self,
        to: impl Fn(usize) -> Option<usize>,
        mut done: impl FnMut(Box<Take>),
    ) {
        for slot in [&mut self.recording, &mut self.next] {
            if let Some(take) = slot {
                match to(take.track) {
                    Some(track) => take.track = track,
                    None => slot.take().into_iter().for_each(&mut done),
                }
            }
        }
        if self.recording.is_none() {
            self.recording = self.next.take();
        }
        self.settle();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(from: usize, len: usize) -> Vec<f32> {
        (from..from + len).map(|i| i as f32).collect()
    }

    /// Track `track` starting its pass `n` at frame `at`, on its first step.
    fn pass(looper: &mut Looper, track: usize, n: u32, at: u64, latency: u64) {
        looper.step_starts(track, (n, 0, true), at, latency);
    }

    /// A quiet input, but for a loud frame at `loud`, from frame `now`.
    fn quiet(now: u64, len: usize, loud: u64) -> Vec<f32> {
        (now..now + len as u64)
            .map(|f| if f == loud { 1.0 } else { 0.01 })
            .collect()
    }

    #[test]
    fn a_take_with_a_threshold_waits_for_the_input_then_starts_its_pass_on_the_next_step() {
        let mut looper = Looper::default();
        let mut take = Take::new(0, 10_000);
        take.threshold = Some(0.5);
        looper.arm(take).unwrap();
        let mut done = vec![];
        // A track of 4 steps of 100 frames, heard 50 frames late; the input's quiet at first.
        let mut heard = vec![];
        for block in 0..20u64 {
            let now = block * 64;
            for step in 0..4u64 {
                for pass in 0..4u32 {
                    let at = u64::from(pass) * 400 + step * 100;
                    if (now..now + 64).contains(&at) {
                        looper.step_starts(0, (pass, step as usize, step == 0), at, 50);
                    }
                }
            }
            let input = quiet(now, 64, 700);
            heard.push(looper.write(now, &input, &input, |t| done.push(t)));
            if block == 8 {
                assert!(looper.listening(), "still waiting at {now}");
            }
        }
        assert_eq!(heard.iter().filter(|&&h| h).count(), 1, "heard once");
        assert!(!looper.listening());
        // Heard at 700, in pass 1's step 2 (heard from 650); its pass starts on step 3, at
        // 750, and goes round to step 3 of pass 2, at 1150.
        assert_eq!(done.len(), 1);
        let take = &done[0];
        assert!(take.complete);
        assert_eq!(take.step, 3);
        // Recorded from a little before, as far back as the block it was heard in, at 640.
        assert_eq!(take.lead, 750 - 640);
        assert_eq!(take.len(), 1150 - 640);
    }

    #[test]
    fn heard_before_the_step_is_sequenced_its_pass_starts_on_the_first_after() {
        let mut looper = Looper::default();
        let mut take = Take::new(0, 10_000);
        take.threshold = Some(0.5);
        looper.arm(take).unwrap();
        // Steps sequenced ahead of the input, as the latency makes them.
        looper.step_starts(0, (0, 0, true), 0, 300);
        looper.step_starts(0, (0, 1, false), 100, 300);
        looper.step_starts(0, (0, 2, false), 200, 300);
        let input = quiet(256, 64, 310);
        assert!(looper.write(256, &input, &input, |_| panic!()));
        let take = looper.recording.as_ref().unwrap();
        // Heard at 310, so the step heard at 400, the second, recorded from the block's start.
        assert_eq!((take.step, take.lead), (1, 400 - 256));
    }

    #[test]
    fn a_take_records_one_pass_and_the_next_follows_on() {
        let mut looper = Looper::default();
        looper.arm(Take::new(0, 100)).unwrap();
        looper.arm(Take::new(0, 100)).unwrap();
        assert!(looper.arm(Take::new(0, 100)).is_err(), "two at most");
        // Another track's pass is nothing to it.
        pass(&mut looper, 1, 0, 5, 0);
        pass(&mut looper, 0, 0, 10, 0);
        // The latency it started with holds, however it wavers.
        pass(&mut looper, 0, 1, 37, 3);
        let mut done = vec![];
        for block in 0..4 {
            let now = block * 16;
            let input = ramp(now as usize, 16);
            looper.write(now, &input, &input, |t| done.push(t));
        }
        assert_eq!(done.len(), 1);
        assert!(done[0].complete);
        assert_eq!(done[0].left, ramp(10, 27));
        // The next began where it ended.
        pass(&mut looper, 0, 2, 70, 0);
        let input = ramp(64, 16);
        looper.write(64, &input, &input, |t| done.push(t));
        assert_eq!(done[1].left, ramp(37, 33));
    }

    #[test]
    fn stopping_hands_back_what_hasnt_finished() {
        let mut looper = Looper::default();
        looper.arm(Take::new(0, 100)).unwrap();
        looper.arm(Take::new(0, 100)).unwrap();
        pass(&mut looper, 0, 0, 0, 0);
        let mut done = vec![];
        looper.stop(false, |t| done.push(t));
        assert_eq!(done.len(), 1, "the one recording carries on");
        looper.stop(true, |t| done.push(t));
        assert_eq!(done.len(), 2);
        assert!(done.iter().all(|t| !t.complete));
    }

    #[test]
    fn takes_follow_their_track_and_end_with_it() {
        let mut looper = Looper::default();
        looper.arm(Take::new(2, 10)).unwrap();
        looper.retarget(|t| Some(t + 1), |_| panic!());
        assert_eq!(looper.recording.as_ref().unwrap().track, 3);
        let mut done = vec![];
        looper.retarget(|t| (t != 3).then_some(t), |t| done.push(t));
        assert_eq!(done.len(), 1);
        assert!(looper.recording.is_none());
    }

    #[test]
    fn a_full_take_ends_there() {
        let mut take = Take::new(0, 8);
        take.start = Some(0);
        let input = ramp(0, 16);
        assert!(take.write(0, &input, &input).0);
        assert_eq!(take.left, ramp(0, 8));
    }
}
