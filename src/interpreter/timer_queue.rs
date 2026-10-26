//! The host timer queue behind `setTimeout` / `setInterval`.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

use rustc_hash::FxHashMap;

use crate::types::JsValue;

/// A host timer armed by `setTimeout` or `setInterval`.
struct Timer {
    callback: JsValue,
    args: Vec<JsValue>,
    /// Deadline, or `None` when the timer is not in the index. That happens
    /// two ways: the requested delay was too far out to represent (e.g.
    /// `setTimeout(fn, Infinity)`, which never fires but still keeps the event
    /// loop alive, matching the thread-per-timer model it replaced), or a
    /// one-shot has been claimed by the current batch and is about to run.
    fire_at: Option<Instant>,
    /// `Some` for `setInterval`; the timer re-arms after every fire.
    interval: Option<Duration>,
    /// False once `unref` has been called (Node host): the timer still fires
    /// while the loop is alive but does not keep it alive.
    refed: bool,
}

/// Timers owned by the interpreter and serviced on the event loop, replacing a
/// thread per `setTimeout` call (issue #254).
///
/// `timers` is the source of truth and the GC root set. `heap` is a deadline
/// index that may hold stale entries: cancelling a timer drops it from `timers`
/// only, and re-arming an interval leaves the previous deadline behind. An
/// entry is stale when its id is gone or its deadline no longer matches the
/// map, and is skipped on pop — so cancellation stays O(1). Stale entries are
/// only reclaimed on pop, so the index is compacted once they outnumber the live
/// timers; without that, arm-then-cancel churn on far-future deadlines
/// (lodash-style `debounce`/`throttle`) grows the heap without bound, because no
/// pop ever reaches them.
#[derive(Default)]
pub(crate) struct TimerQueue {
    next_id: u64,
    timers: FxHashMap<u64, Timer>,
    heap: BinaryHeap<Reverse<(Instant, u64)>>,
    /// Fuzz-only: when set, "now" is this instant, moved forward only by
    /// [`TimerQueue::advance_clock_to`], so ordering never depends on real time.
    virtual_now: Option<Instant>,
}

impl TimerQueue {
    pub(crate) fn now(&self) -> Instant {
        self.virtual_now.unwrap_or_else(Instant::now)
    }

    pub(crate) fn is_virtual(&self) -> bool {
        self.virtual_now.is_some()
    }

    pub(crate) fn use_virtual_clock(&mut self) {
        self.virtual_now = Some(Instant::now());
    }

    pub(crate) fn advance_clock_to(&mut self, at: Instant) {
        if let Some(now) = &mut self.virtual_now {
            *now = (*now).max(at);
        }
    }

    /// Arm a timer and return its id. Ids start at 1, so an id is always truthy
    /// in JS and `clearTimeout(0)` remains a no-op.
    pub(crate) fn add(
        &mut self,
        callback: JsValue,
        args: Vec<JsValue>,
        delay: Duration,
        repeating: bool,
    ) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let fire_at = self.now().checked_add(delay);
        if let Some(at) = fire_at {
            self.heap.push(Reverse((at, id)));
        }
        self.timers.insert(
            id,
            Timer {
                callback,
                args,
                fire_at,
                interval: repeating.then_some(delay),
                refed: true,
            },
        );
        self.compact_index_if_stale();
        id
    }

    pub(crate) fn clear(&mut self, id: u64) {
        self.timers.remove(&id);
        self.compact_index_if_stale();
    }

    /// Rebuild the deadline index from the live timers once stale entries
    /// outnumber them. Each rebuild drops at least half the index, so the
    /// amortised cost per arm/cancel stays constant, and a queue with no stale
    /// entries never pays it.
    fn compact_index_if_stale(&mut self) {
        const MIN_INDEX_LEN: usize = 32;
        if self.heap.len() <= MIN_INDEX_LEN || self.heap.len() <= 2 * self.timers.len() {
            return;
        }
        self.heap = self
            .timers
            .iter()
            .filter_map(|(&id, timer)| timer.fire_at.map(|at| Reverse((at, id))))
            .collect();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.timers.is_empty()
    }

    pub(crate) fn set_ref(&mut self, id: u64, refed: bool) {
        if let Some(timer) = self.timers.get_mut(&id) {
            timer.refed = refed;
        }
    }

    /// Whether any armed timer still keeps the event loop alive.
    pub(crate) fn has_refed(&self) -> bool {
        self.timers.values().any(|t| t.refed)
    }

    /// Earliest live deadline, pruning stale heap entries off the top. `None`
    /// when nothing is armed, or when every armed timer has an unrepresentable
    /// deadline.
    pub(crate) fn next_deadline(&mut self) -> Option<Instant> {
        while let Some(&Reverse((at, id))) = self.heap.peek() {
            if self.timers.get(&id).is_some_and(|t| t.fire_at == Some(at)) {
                return Some(at);
            }
            self.heap.pop();
        }
        None
    }

    /// Ids of every timer due at `now`, in deadline then arming order.
    ///
    /// The timers stay in the queue: a callback earlier in the batch must be
    /// able to cancel a sibling that has not run yet (as on Node), and until a
    /// timer actually fires the queue is what keeps its callback GC-rooted.
    /// Call [`TimerQueue::take_for_firing`] for each id in turn to collect it.
    ///
    /// Repeating timers re-arm relative to `now` — measuring from the deadline
    /// instead would let a long synchronous block queue a burst of catch-up
    /// ticks. Re-armed entries land in the index only after the batch is
    /// collected, so a zero-delay interval fires at most once per loop turn.
    pub(crate) fn take_due(&mut self, now: Instant) -> Vec<u64> {
        let mut due = Vec::new();
        let mut rearmed = Vec::new();
        while let Some(&Reverse((at, id))) = self.heap.peek() {
            if at > now {
                break;
            }
            self.heap.pop();
            let Some(timer) = self.timers.get_mut(&id) else {
                continue; // cancelled
            };
            if timer.fire_at != Some(at) {
                continue; // superseded by a re-arm
            }
            match timer.interval {
                Some(interval) => {
                    timer.fire_at = now.checked_add(interval);
                    if let Some(next) = timer.fire_at {
                        rearmed.push(Reverse((next, id)));
                    }
                }
                // Keep the timer, but off the index: it is spoken for by this
                // batch and must not be collected twice.
                None => timer.fire_at = None,
            }
            due.push(id);
        }
        self.heap.extend(rearmed);
        due
    }

    /// Callback and arguments for a due timer, or `None` if a callback earlier
    /// in the same batch cancelled it. A one-shot leaves the queue here, at the
    /// moment it is about to run.
    pub(crate) fn take_for_firing(&mut self, id: u64) -> Option<(JsValue, Vec<JsValue>)> {
        let timer = self.timers.get(&id)?;
        if timer.interval.is_some() {
            // An interval stays armed, so hand out a copy of what to call.
            return Some((timer.callback.clone(), timer.args.clone()));
        }
        let timer = self.timers.remove(&id)?;
        Some((timer.callback, timer.args))
    }

    #[cfg(test)]
    pub(crate) fn index_len(&self) -> usize {
        self.heap.len()
    }

    /// GC roots: an armed timer keeps its callback and arguments alive.
    pub(crate) fn iter_roots(&self) -> impl Iterator<Item = (&JsValue, &[JsValue])> {
        self.timers
            .values()
            .map(|t| (&t.callback, t.args.as_slice()))
    }
}
