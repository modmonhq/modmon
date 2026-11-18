//! One series: a ring of the newest steps, with what every question needs
//! kept next to it.
//!
//! The ring holds [`SLOTS`] steps and the step number says where a step
//! lives (`step % SLOTS`), so a sample carries no timestamp and the oldest
//! step falls out when the newest is written over it.
//!
//! Sums over a window come from running totals, not from walking the
//! window. The series counts everything it ever held (`total`) and, each
//! time a block of [`BLOCK`] steps begins, notes what the count was then
//! (`before`). What a window holds is the total at its end minus the total
//! at its start, and a total in the middle of a block is the block's note
//! plus the steps of that one block up to there: at most 63 steps, however
//! long the window is. The notes cost three eighths of a byte per step.
//!
//! "How long has this held" is kept per condition asked about: the first
//! step of the run that holds now, moved on by every step that breaks it.

use super::{Condition, Percent, RETAINED_STEPS, Tally};

/// Steps per note of the running totals.
pub(super) const BLOCK: u64 = 64;
/// Steps a ring holds: four hours and one block more, so that the block in
/// which a four-hour window starts is still whole.
pub(super) const SLOTS: u64 = RETAINED_STEPS as u64 + BLOCK;
const BLOCKS: usize = (SLOTS / BLOCK) as usize;
/// Conditions a series follows at once. Asking about one more replaces the
/// one that was asked about longest ago.
const FOLLOWED: usize = 4;
const NO_SAMPLE: u16 = u16::MAX;

/// Where the steps of a ring are kept.
pub(super) trait Slots {
    fn empty() -> Self;
    fn get(&self, slot: usize) -> Tally;
    fn set(&mut self, slot: usize, tally: Tally);
    fn clear(&mut self);
    /// The bytes one ring's steps take.
    const BYTES: usize;
}

/// A container's samples: two bytes a step.
pub(super) struct Samples(Box<[u16]>);

impl Slots for Samples {
    const BYTES: usize = SLOTS as usize * size_of::<u16>();

    fn empty() -> Self {
        Self(vec![NO_SAMPLE; SLOTS as usize].into_boxed_slice())
    }

    fn get(&self, slot: usize) -> Tally {
        match self.0[slot] {
            NO_SAMPLE => Tally::NONE,
            sample => Tally {
                sum: u32::from(sample),
                count: 1,
            },
        }
    }

    fn set(&mut self, slot: usize, tally: Tally) {
        self.0[slot] = tally
            .average()
            .map_or(NO_SAMPLE, |sample| sample.hundredths());
    }

    fn clear(&mut self) {
        self.0.fill(NO_SAMPLE);
    }
}

/// A service's steps: the sum and the count, six bytes a step in two
/// arrays.
pub(super) struct Tallies {
    sums: Box<[u32]>,
    counts: Box<[u16]>,
}

impl Slots for Tallies {
    const BYTES: usize = SLOTS as usize * (size_of::<u32>() + size_of::<u16>());

    fn empty() -> Self {
        Self {
            sums: vec![0; SLOTS as usize].into_boxed_slice(),
            counts: vec![0; SLOTS as usize].into_boxed_slice(),
        }
    }

    fn get(&self, slot: usize) -> Tally {
        Tally {
            sum: self.sums[slot],
            count: self.counts[slot],
        }
    }

    fn set(&mut self, slot: usize, tally: Tally) {
        self.sums[slot] = tally.sum;
        self.counts[slot] = tally.count;
    }

    fn clear(&mut self) {
        self.sums.fill(0);
        self.counts.fill(0);
    }
}

/// What a stretch of steps adds up to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Totals {
    /// The sum of every sample, in hundredths of a percent.
    pub sum: u64,
    /// How many samples.
    pub count: u64,
    /// How many steps hold at least one sample.
    pub steps: u64,
}

impl Totals {
    fn add(&mut self, tally: Tally) {
        self.sum += u64::from(tally.sum);
        self.count += u64::from(tally.count);
        self.steps += u64::from(tally.count > 0);
    }

    /// What was added after `earlier`, an older state of the same totals.
    const fn since(self, earlier: Self) -> Self {
        Self {
            sum: self.sum.saturating_sub(earlier.sum),
            count: self.count.saturating_sub(earlier.count),
            steps: self.steps.saturating_sub(earlier.steps),
        }
    }
}

/// A condition the series follows, and the first step of the run of steps
/// that satisfy it up to the newest: one past the newest when that one
/// does not.
#[derive(Clone, Copy)]
struct Followed {
    condition: Condition,
    holds_from: u64,
    asked: u64,
}

pub(super) struct Ring<S> {
    slots: S,
    /// For each block in the ring, the totals of everything before it.
    before: [Totals; BLOCKS],
    total: Totals,
    /// The first and the newest step written; `None` for an empty ring.
    span: Option<(u64, u64)>,
    followed: [Option<Followed>; FOLLOWED],
    asks: u64,
}

impl<S: Slots> Ring<S> {
    /// The bytes one ring takes, its steps included.
    pub(super) const BYTES: usize = size_of::<Self>() + S::BYTES;

    pub(super) fn new() -> Self {
        Self {
            slots: S::empty(),
            before: [Totals::default(); BLOCKS],
            total: Totals::default(),
            span: None,
            followed: [None; FOLLOWED],
            asks: 0,
        }
    }

    /// The newest step written.
    pub(super) fn newest(&self) -> Option<u64> {
        self.span.map(|(_, newest)| newest)
    }

    /// Writes `step`. Steps skipped since the newest one hold no sample. A
    /// step that is not newer than the newest is refused: what is written
    /// stays.
    pub(super) fn append(&mut self, step: u64, tally: Tally) -> bool {
        match self.span {
            Some((_, newest)) if step <= newest => return false,
            // Nothing in the ring is recent enough to be asked about.
            Some((_, newest)) if step - newest > SLOTS => self.start_over(step),
            Some((_, newest)) => {
                for skipped in newest + 1..step {
                    self.write(skipped, Tally::NONE);
                }
            }
            None => {}
        }
        if self.span.is_none() {
            self.before[block_of(step)] = self.total;
            self.span = Some((step, step));
        }
        self.write(step, tally);
        true
    }

    /// Empties the ring before `step` is written into it. Every run of
    /// steps that held has ended in the gap.
    fn start_over(&mut self, step: u64) {
        self.slots.clear();
        self.total = Totals::default();
        self.span = None;
        for followed in self.followed.iter_mut().flatten() {
            followed.holds_from = step;
        }
    }

    fn write(&mut self, step: u64, tally: Tally) {
        if step.is_multiple_of(BLOCK) {
            self.before[block_of(step)] = self.total;
        }
        self.slots.set(slot_of(step), tally);
        self.total.add(tally);
        let sample = tally.average();
        for followed in self.followed.iter_mut().flatten() {
            if !followed.condition.holds(sample) {
                followed.holds_from = step + 1;
            }
        }
        if let Some((_, newest)) = &mut self.span {
            *newest = step;
        }
    }

    /// The oldest step the ring can still add up from.
    fn oldest(&self, first: u64, newest: u64) -> u64 {
        let kept_blocks = (newest / BLOCK + 1).saturating_sub(BLOCKS as u64);
        first.max(kept_blocks * BLOCK)
    }

    /// What every step before `step` adds up to.
    fn until(&self, step: u64) -> Totals {
        let Some((first, newest)) = self.span else {
            return Totals::default();
        };
        if step > newest {
            return self.total;
        }
        if step <= first {
            return Totals::default();
        }
        let step = step.max(self.oldest(first, newest));
        let mut totals = self.before[block_of(step)];
        for earlier in step - step % BLOCK..step {
            totals.add(self.slots.get(slot_of(earlier)));
        }
        totals
    }

    /// What the `steps` steps up to and including `end` add up to.
    pub(super) fn window(&self, end: u64, steps: u32) -> Totals {
        let from = (end + 1).saturating_sub(u64::from(steps));
        self.until(end + 1).since(self.until(from))
    }

    /// What `step` holds; nothing for a step the ring does not hold.
    pub(super) fn at(&self, step: u64) -> Tally {
        match self.span {
            Some((first, newest)) if (first..=newest).contains(&step) && newest - step < SLOTS => {
                self.slots.get(slot_of(step))
            }
            _ => Tally::NONE,
        }
    }

    /// The step a question asked at `now` looks back from: `now` when it
    /// is written, and the step before it while it is still being sampled.
    pub(super) fn end(&self, now: u64) -> u64 {
        match self.newest() {
            Some(newest) if newest >= now => now,
            _ => now.saturating_sub(1),
        }
    }

    /// For how many steps up to the newest `condition` has held without a
    /// break. Zero when the newest step is older than the one before
    /// `now`: the steps since then hold no sample.
    ///
    /// The first question about a condition walks back once to find where
    /// its run began; from then on every step written keeps the answer
    /// current.
    pub(super) fn held(&mut self, condition: Condition, now: u64) -> u64 {
        let Some((first, newest)) = self.span else {
            return 0;
        };
        self.asks += 1;
        let asks = self.asks;
        let known = self
            .followed
            .iter_mut()
            .flatten()
            .find(|followed| followed.condition == condition);
        let holds_from = if let Some(followed) = known {
            followed.asked = asks;
            followed.holds_from
        } else {
            let oldest = first.max((newest + 1).saturating_sub(u64::from(RETAINED_STEPS)));
            let holds_from = (oldest..=newest)
                .rev()
                .find(|step| !condition.holds(self.at(*step).average()))
                .map_or(oldest, |broken| broken + 1);
            let place = self
                .followed
                .iter()
                .position(Option::is_none)
                .or_else(|| {
                    (0..FOLLOWED).min_by_key(|place| self.followed[*place].map(|old| old.asked))
                })
                .unwrap_or_default();
            self.followed[place] = Some(Followed {
                condition,
                holds_from,
                asked: asks,
            });
            holds_from
        };
        if newest + 1 < now {
            return 0;
        }
        (newest + 1 - holds_from).min(u64::from(RETAINED_STEPS))
    }

    /// The average sample of what `totals` adds up to.
    pub(super) fn average(totals: Totals) -> Option<Percent> {
        super::average(totals.sum, totals.count)
    }
}

const fn slot_of(step: u64) -> usize {
    (step % SLOTS) as usize
}

const fn block_of(step: u64) -> usize {
    ((step / BLOCK) % BLOCKS as u64) as usize
}
