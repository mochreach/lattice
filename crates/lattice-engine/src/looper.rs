//! Recording the audio input into a track a pass at a time, for the looper.
//!
//! The UI thread sends a [`Take`] with room for a pass; it starts recording when the track next
//! comes round to its first step and stops when it comes round again, so it's exactly one pass
//! long, then goes back to be saved. A second take can wait behind the first, to start the moment
//! it ends, so passes record back to back for as long as recording's on.
//!
//! What's heard comes out late, and what's played to it comes in late, so the takes start and
//! stop that much after the steps: the latency, which the host measures and the user can adjust.

/// One pass of the input, recorded for a track.
#[derive(Debug)]
pub struct Take {
    /// The track's index, followed as tracks are added, removed and moved.
    pub track: usize,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    /// When it starts and ends, in frames since the engine started.
    start: Option<u64>,
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
            start: None,
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

    /// Records what of `left` and `right`, the input from frame `now` on, falls in the take.
    /// True once it's ended.
    fn write(&mut self, now: u64, left: &[f32], right: &[f32]) -> bool {
        let Some(start) = self.start else {
            return false;
        };
        let until = now + left.len() as u64;
        let end = self.end.unwrap_or(u64::MAX);
        let from = start.max(now);
        let to = end.min(until);
        if from < to {
            let room = self.left.capacity() - self.left.len();
            let range = (from - now) as usize..(to - now) as usize;
            let range = range.start..range.end.min(range.start + room);
            // Within capacity, so it never reallocates.
            self.left.extend_from_slice(&left[range.clone()]);
            self.right.extend_from_slice(&right[range]);
            if self.left.len() == self.left.capacity() {
                self.complete = true;
                return true;
            }
        }
        if end <= until {
            self.complete = true;
            return true;
        }
        false
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

    /// `track` has come round to its first step, played at frame `at` and heard `latency`
    /// frames later.
    pub fn pass_starts(&mut self, track: usize, at: u64, latency: u64) {
        let Some(recording) = self.recording.as_mut().filter(|t| t.track == track) else {
            return;
        };
        let at = at + *self.latency.get_or_insert(latency);
        if recording.start.is_none() {
            recording.start = Some(at);
        } else if recording.end.is_none() {
            recording.end = Some(at);
            if let Some(next) = self.next.as_mut().filter(|t| t.track == track) {
                next.start = Some(at);
            }
        }
    }

    /// Records the input from frame `now` on, calling `done` with each take that's ended.
    pub fn write(
        &mut self,
        now: u64,
        left: &[f32],
        right: &[f32],
        mut done: impl FnMut(Box<Take>),
    ) {
        if let Some(next) = &mut self.next {
            next.write(now, left, right);
        }
        if let Some(recording) = &mut self.recording
            && recording.write(now, left, right)
        {
            done(self.recording.take().unwrap());
            self.recording = self.next.take();
            self.settle();
        }
    }

    /// Lets the latency be measured afresh once nothing's recording.
    fn settle(&mut self) {
        if self.recording.is_none() {
            self.latency = None;
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
                    None => done(slot.take().unwrap()),
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

    #[test]
    fn a_take_records_one_pass_and_the_next_follows_on() {
        let mut looper = Looper::default();
        looper.arm(Take::new(0, 100)).unwrap();
        looper.arm(Take::new(0, 100)).unwrap();
        assert!(looper.arm(Take::new(0, 100)).is_err(), "two at most");
        // Another track's pass is nothing to it.
        looper.pass_starts(1, 5, 0);
        looper.pass_starts(0, 10, 0);
        // The latency it started with holds, however it wavers.
        looper.pass_starts(0, 37, 3);
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
        looper.pass_starts(0, 70, 0);
        let input = ramp(64, 16);
        looper.write(64, &input, &input, |t| done.push(t));
        assert_eq!(done[1].left, ramp(37, 33));
    }

    #[test]
    fn stopping_hands_back_what_hasnt_finished() {
        let mut looper = Looper::default();
        looper.arm(Take::new(0, 100)).unwrap();
        looper.arm(Take::new(0, 100)).unwrap();
        looper.pass_starts(0, 0, 0);
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
        assert!(take.write(0, &input, &input));
        assert_eq!(take.left, ramp(0, 8));
    }
}
