//! One stream from the log lines of several containers or nodes, in the
//! order the engines recorded them.
//!
//! Every source (a container on a host, or a node of a cluster) delivers
//! its own lines in order. The merge releases the oldest waiting line once
//! no other source can still deliver an older one: each of them has shown
//! a line at least as new, or has ended, or has been given `hold` to do so.
//! Order is by the engines' timestamps, so two nodes whose clocks differ
//! can swap lines that are closer together than that difference.

use std::collections::VecDeque;
use std::time::Duration;

use super::LogRecord;

/// When a line was written, as its engine recorded it: seconds since the
/// Unix epoch, then nanoseconds.
pub type LogInstant = (i64, u32);

/// The instant an engine's RFC 3339 timestamp names; `None` for text that
/// is not a timestamp.
#[must_use]
pub fn log_instant(timestamp: &str) -> Option<LogInstant> {
    time::OffsetDateTime::parse(timestamp, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|instant| (instant.unix_timestamp(), instant.nanosecond()))
}

struct Held {
    at: LogInstant,
    /// When the line reached the merge, on the caller's clock.
    arrived: Duration,
    record: LogRecord,
}

#[derive(Default)]
struct Source {
    ended: bool,
    waiting: VecDeque<Held>,
    /// The newest instant this source has shown. Its later lines are not
    /// older than this.
    newest: Option<LogInstant>,
}

/// Merges the lines of several sources by when they were written.
pub struct LogMerge {
    sources: Vec<Source>,
    /// How long a line waits for a source that has shown nothing as new.
    /// `None` waits until that source ends: a read that ends is complete
    /// before it is ordered.
    hold: Option<Duration>,
    released: u64,
}

impl LogMerge {
    /// A merge of `sources` sources for a read that follows: a line is
    /// held back for at most `hold`.
    #[must_use]
    pub fn following(sources: usize, hold: Duration) -> Self {
        Self::new(sources, Some(hold))
    }

    /// A merge of `sources` sources for a read that ends: nothing is
    /// released out of order, however long a source takes.
    #[must_use]
    pub fn complete(sources: usize) -> Self {
        Self::new(sources, None)
    }

    fn new(sources: usize, hold: Option<Duration>) -> Self {
        Self {
            sources: (0..sources).map(|_| Source::default()).collect(),
            hold,
            released: 0,
        }
    }

    /// Adds a source that appeared later, such as a container that started
    /// while its service is followed, and returns its number.
    pub fn add_source(&mut self) -> usize {
        self.sources.push(Source::default());
        self.sources.len() - 1
    }

    /// Takes a line from `source`. `now` is the caller's monotonic clock.
    /// A line without a timestamp stays behind the line it followed.
    pub fn push(&mut self, source: usize, record: LogRecord, now: Duration) {
        let Some(state) = self.sources.get_mut(source) else {
            return;
        };
        let at = record
            .timestamp
            .as_deref()
            .and_then(log_instant)
            .or(state.newest)
            .unwrap_or((i64::MIN, 0));
        state.newest = Some(state.newest.map_or(at, |newest| newest.max(at)));
        state.waiting.push_back(Held {
            at,
            arrived: now,
            record,
        });
    }

    /// `source` delivers nothing more: it ended, or its node is gone.
    pub fn end(&mut self, source: usize) {
        if let Some(state) = self.sources.get_mut(source) {
            state.ended = true;
        }
    }

    /// Whether every source has ended and every line was released.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.sources
            .iter()
            .all(|source| source.ended && source.waiting.is_empty())
    }

    /// The lines that can go out now, oldest first, numbered in the order
    /// of the merged stream.
    pub fn release(&mut self, now: Duration) -> Vec<LogRecord> {
        let mut released = Vec::new();
        while let Some(source) = self.oldest() {
            let Some(head) = self.sources[source].waiting.front() else {
                break;
            };
            let waited_out = self
                .hold
                .is_some_and(|hold| now.saturating_sub(head.arrived) >= hold);
            if !waited_out && !self.nothing_older_can_come(source, head.at) {
                break;
            }
            let Some(held) = self.sources[source].waiting.pop_front() else {
                break;
            };
            self.released += 1;
            let mut record = held.record;
            record.sequence = self.released;
            released.push(record);
        }
        released
    }

    /// When the oldest waiting line has been held long enough, on the
    /// caller's clock; `None` when nothing waits for time to pass.
    #[must_use]
    pub fn next_release(&self) -> Option<Duration> {
        let hold = self.hold?;
        let source = self.oldest()?;
        let head = self.sources[source].waiting.front()?;
        Some(head.arrived.saturating_add(hold))
    }

    /// The source whose first waiting line is the oldest. Sources that tie
    /// go in their own order, so a merge of the same lines is always the
    /// same.
    fn oldest(&self) -> Option<usize> {
        self.sources
            .iter()
            .enumerate()
            .filter_map(|(index, source)| source.waiting.front().map(|held| (held.at, index)))
            .min()
            .map(|(_, index)| index)
    }

    /// Whether every other source that has nothing waiting has ended or has
    /// already shown a line at least as new as `at`.
    fn nothing_older_can_come(&self, source: usize, at: LogInstant) -> bool {
        self.sources.iter().enumerate().all(|(index, other)| {
            index == source
                || other.ended
                || !other.waiting.is_empty()
                || other.newest.is_some_and(|newest| newest >= at)
        })
    }
}

/// The last `tail` lines of several sources that have ended, as one list in
/// the order they were written, numbered from one.
#[must_use]
pub fn merged_tail(sources: Vec<Vec<LogRecord>>, tail: usize) -> Vec<LogRecord> {
    let mut merge = LogMerge::complete(sources.len());
    for (source, records) in sources.into_iter().enumerate() {
        for record in records {
            merge.push(source, record, Duration::ZERO);
        }
        merge.end(source);
    }
    let mut merged = merge.release(Duration::ZERO);
    let dropped = merged.len().saturating_sub(tail);
    merged.drain(..dropped);
    for (record, sequence) in merged.iter_mut().zip(1_u64..) {
        record.sequence = sequence;
    }
    merged
}
