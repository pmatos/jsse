use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rustc_hash::FxHashMap;

use crate::types::JsValue;

use super::AsyncFunctionState;
use super::AsyncGenRequest;
use super::Completion;
use super::Interpreter;

pub(crate) type MicrotaskJob = Box<dyn FnOnce(&mut Interpreter) -> Completion>;

/// A host timer armed by `setTimeout` or `setInterval`.
struct Timer {
    callback: JsValue,
    args: Vec<JsValue>,
    /// `None` when the requested delay is too far out to represent, e.g.
    /// `setTimeout(fn, Infinity)`. Such a timer never fires but still keeps the
    /// event loop alive, matching the thread-per-timer model it replaced.
    fire_at: Option<Instant>,
    /// `Some` for `setInterval`; the timer re-arms after every fire.
    interval: Option<Duration>,
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
}

impl TimerQueue {
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
        let fire_at = Instant::now().checked_add(delay);
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
        if self.timers.get(&id)?.interval.is_some() {
            let timer = &self.timers[&id];
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

#[derive(Default)]
pub(crate) struct JobScheduler {
    microtask_queue: Vec<(Vec<JsValue>, MicrotaskJob)>,
    async_gen_queues: FxHashMap<u64, VecDeque<AsyncGenRequest>>,
    async_gen_yield_pending: bool,
    async_function_states: FxHashMap<u64, AsyncFunctionState>,
    next_async_function_id: u64,
    /// Count of host-async worker jobs that may later enqueue completions.
    pending_async_jobs: Arc<AtomicUsize>,
    /// Promise IDs whose resolution is blocked on a host-async worker thread
    /// (e.g. Atomics.waitAsync, $262.agent.getReportAsync).
    pending_async_promise_ids: Arc<Mutex<HashSet<u64>>>,
    /// Timers armed by setTimeout/setInterval. Tracked separately from
    /// promise-backed host async jobs so detached Atomics.waitAsync jobs do not
    /// keep the process alive.
    timers: TimerQueue,
}

impl JobScheduler {
    pub(crate) fn enqueue_microtask(&mut self, item: (Vec<JsValue>, MicrotaskJob)) {
        self.microtask_queue.push(item);
    }

    pub(crate) fn pop_microtask(&mut self) -> Option<(Vec<JsValue>, MicrotaskJob)> {
        if self.microtask_queue.is_empty() {
            None
        } else {
            Some(self.microtask_queue.remove(0))
        }
    }

    pub(crate) fn iter_microtask_roots(&self) -> impl Iterator<Item = &[JsValue]> {
        self.microtask_queue
            .iter()
            .map(|(roots, _)| roots.as_slice())
    }

    pub(crate) fn async_gen_queue_or_default(
        &mut self,
        gen_id: u64,
    ) -> &mut VecDeque<AsyncGenRequest> {
        self.async_gen_queues.entry(gen_id).or_default()
    }

    pub(crate) fn async_gen_queue(&self, gen_id: u64) -> Option<&VecDeque<AsyncGenRequest>> {
        self.async_gen_queues.get(&gen_id)
    }

    pub(crate) fn async_gen_queue_mut(
        &mut self,
        gen_id: u64,
    ) -> Option<&mut VecDeque<AsyncGenRequest>> {
        self.async_gen_queues.get_mut(&gen_id)
    }

    pub(crate) fn set_async_gen_yield_pending(&mut self, value: bool) {
        self.async_gen_yield_pending = value;
    }

    pub(crate) fn is_async_gen_yield_pending(&self) -> bool {
        self.async_gen_yield_pending
    }

    pub(crate) fn alloc_async_function_id(&mut self) -> u64 {
        let id = self.next_async_function_id;
        self.next_async_function_id += 1;
        id
    }

    pub(crate) fn insert_async_function_state(&mut self, id: u64, state: AsyncFunctionState) {
        self.async_function_states.insert(id, state);
    }

    pub(crate) fn remove_async_function_state(&mut self, id: u64) -> Option<AsyncFunctionState> {
        self.async_function_states.remove(&id)
    }

    pub(crate) fn iter_async_function_states(&self) -> impl Iterator<Item = &AsyncFunctionState> {
        self.async_function_states.values()
    }

    pub(crate) fn pending_async_jobs_handle(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.pending_async_jobs)
    }

    pub(crate) fn incr_pending_async_jobs(&self) {
        self.pending_async_jobs.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn pending_async_jobs_count(&self) -> usize {
        self.pending_async_jobs.load(Ordering::SeqCst)
    }

    pub(crate) fn pending_async_promise_ids_handle(&self) -> Arc<Mutex<HashSet<u64>>> {
        Arc::clone(&self.pending_async_promise_ids)
    }

    pub(crate) fn pending_async_promise_ids_lock(&self) -> MutexGuard<'_, HashSet<u64>> {
        self.pending_async_promise_ids.lock().unwrap()
    }

    pub(crate) fn add_timer(
        &mut self,
        callback: JsValue,
        args: Vec<JsValue>,
        delay: Duration,
        repeating: bool,
    ) -> u64 {
        self.timers.add(callback, args, delay, repeating)
    }

    pub(crate) fn clear_timer(&mut self, id: u64) {
        self.timers.clear(id);
    }

    pub(crate) fn has_timers(&self) -> bool {
        !self.timers.is_empty()
    }

    pub(crate) fn next_timer_deadline(&mut self) -> Option<Instant> {
        self.timers.next_deadline()
    }

    pub(crate) fn take_due_timers(&mut self, now: Instant) -> Vec<u64> {
        self.timers.take_due(now)
    }

    pub(crate) fn take_timer_for_firing(&mut self, id: u64) -> Option<(JsValue, Vec<JsValue>)> {
        self.timers.take_for_firing(id)
    }

    pub(crate) fn iter_timer_roots(&self) -> impl Iterator<Item = (&JsValue, &[JsValue])> {
        self.timers.iter_roots()
    }

    #[cfg(test)]
    pub(crate) fn microtask_queue_is_empty(&self) -> bool {
        self.microtask_queue.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn clear_microtasks(&mut self) {
        self.microtask_queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JsValue;

    #[test]
    fn microtask_queue_round_trip() {
        let mut sched = JobScheduler::default();

        assert!(
            sched.microtask_queue_is_empty(),
            "fresh scheduler must be idle"
        );

        let job: MicrotaskJob = Box::new(|_interp| Completion::Normal(JsValue::UNDEFINED));
        sched.enqueue_microtask((Vec::new(), job));

        assert!(
            !sched.microtask_queue_is_empty(),
            "queue must be non-empty after enqueue"
        );

        let popped = sched.pop_microtask();
        assert!(popped.is_some(), "pop must return the queued job");

        assert!(
            sched.microtask_queue_is_empty(),
            "queue must be empty after the only job is popped"
        );
        assert!(
            sched.pop_microtask().is_none(),
            "pop on empty queue returns None"
        );
    }

    #[test]
    fn microtask_queue_drains_in_fifo_order() {
        // Tag each job via its roots — the roots vector holds a single Number.
        fn job() -> MicrotaskJob {
            Box::new(|_interp| Completion::Normal(JsValue::UNDEFINED))
        }
        fn tag(n: f64) -> Vec<JsValue> {
            vec![JsValue::number(n)]
        }

        let mut sched = JobScheduler::default();
        sched.enqueue_microtask((tag(1.0), job()));
        sched.enqueue_microtask((tag(2.0), job()));
        sched.enqueue_microtask((tag(3.0), job()));

        let popped_tags: Vec<f64> = std::iter::from_fn(|| sched.pop_microtask())
            .map(|(roots, _)| match roots.as_slice() {
                [value] => value.as_number().expect("root must be a Number"),
                _ => panic!("unexpected roots shape"),
            })
            .collect();

        assert_eq!(popped_tags, vec![1.0, 2.0, 3.0]);
    }

    fn next_request(tag: f64) -> super::super::AsyncGenRequest {
        super::super::AsyncGenRequest {
            kind: super::super::AsyncGenRequestKind::Next,
            value: JsValue::number(tag),
            promise: JsValue::UNDEFINED,
            resolve_fn: JsValue::UNDEFINED,
            reject_fn: JsValue::UNDEFINED,
        }
    }

    #[test]
    fn async_gen_queues_are_isolated_per_generator() {
        let mut sched = JobScheduler::default();
        sched
            .async_gen_queue_or_default(1)
            .push_back(next_request(1.0));

        assert!(
            sched.async_gen_queue(2).is_none(),
            "pushing to gen 1 must not create or populate gen 2"
        );
        assert_eq!(
            sched.async_gen_queue(1).map(|q| q.len()),
            Some(1),
            "gen 1 must hold exactly one request"
        );
    }

    #[test]
    fn async_gen_requests_pop_in_fifo_order() {
        let mut sched = JobScheduler::default();
        let q = sched.async_gen_queue_or_default(42);
        q.push_back(next_request(1.0));
        q.push_back(next_request(2.0));
        q.push_back(next_request(3.0));

        let q = sched
            .async_gen_queue_mut(42)
            .expect("gen 42 must have a queue");
        let tags: Vec<f64> = std::iter::from_fn(|| q.pop_front())
            .map(|r| r.value.as_number().expect("request value must be a Number"))
            .collect();

        assert_eq!(tags, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn pending_async_jobs_count_reflects_increments() {
        let sched = JobScheduler::default();
        assert_eq!(sched.pending_async_jobs_count(), 0);
        sched.incr_pending_async_jobs();
        sched.incr_pending_async_jobs();
        assert_eq!(sched.pending_async_jobs_count(), 2);
    }

    #[test]
    fn pending_async_jobs_handle_shares_state_with_scheduler() {
        let sched = JobScheduler::default();
        let handle = sched.pending_async_jobs_handle();
        handle.fetch_add(3, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            sched.pending_async_jobs_count(),
            3,
            "external handle increments must be visible through the scheduler"
        );
    }

    #[test]
    fn pending_async_promise_ids_handle_shares_state_with_scheduler() {
        let sched = JobScheduler::default();
        let handle = sched.pending_async_promise_ids_handle();
        handle.lock().unwrap().insert(42);
        assert!(
            sched.pending_async_promise_ids_lock().contains(&42),
            "external handle updates must be visible through the scheduler"
        );
    }

    #[test]
    fn async_function_ids_are_monotonic_and_unique() {
        let mut sched = JobScheduler::default();
        let ids: Vec<u64> = (0..3).map(|_| sched.alloc_async_function_id()).collect();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    fn timer_tag(n: f64) -> JsValue {
        JsValue::number(n)
    }

    /// Collect a due batch the way the event loop does — take the ids, then
    /// fire each in turn — and return the callback tags in order.
    fn drain_due(q: &mut TimerQueue, now: Instant) -> Vec<f64> {
        q.take_due(now)
            .into_iter()
            .filter_map(|id| q.take_for_firing(id))
            .map(|(cb, _)| cb.as_number().expect("callback tag must be a Number"))
            .collect()
    }

    #[test]
    fn timer_ids_are_monotonic_unique_and_never_zero() {
        let mut q = TimerQueue::default();
        let ids: Vec<u64> = (0..3)
            .map(|i| q.add(timer_tag(i as f64), Vec::new(), Duration::ZERO, false))
            .collect();

        assert_eq!(
            ids,
            vec![1, 2, 3],
            "ids start at 1 so a timer id is always truthy in JS"
        );
    }

    #[test]
    fn due_timers_fire_in_deadline_then_arming_order() {
        let mut q = TimerQueue::default();
        q.add(timer_tag(1.0), Vec::new(), Duration::from_millis(50), false);
        q.add(timer_tag(2.0), Vec::new(), Duration::ZERO, false);
        q.add(timer_tag(3.0), Vec::new(), Duration::ZERO, false);

        let now = Instant::now() + Duration::from_millis(100);
        assert_eq!(
            drain_due(&mut q, now),
            vec![2.0, 3.0, 1.0],
            "same-deadline timers keep arming order; earlier deadlines come first"
        );
        assert!(q.is_empty(), "one-shot timers are removed once they fire");
    }

    #[test]
    fn cleared_timer_never_fires_and_drops_its_roots() {
        let mut q = TimerQueue::default();
        let id = q.add(timer_tag(1.0), Vec::new(), Duration::ZERO, false);
        q.add(timer_tag(2.0), Vec::new(), Duration::ZERO, false);

        q.clear(id);
        assert_eq!(
            q.iter_roots().count(),
            1,
            "a cleared timer must stop rooting its callback"
        );

        assert_eq!(
            drain_due(&mut q, Instant::now() + Duration::from_millis(10)),
            vec![2.0]
        );
    }

    #[test]
    fn clearing_an_unknown_id_is_a_no_op() {
        let mut q = TimerQueue::default();
        q.add(timer_tag(1.0), Vec::new(), Duration::ZERO, false);

        q.clear(0);
        q.clear(999);

        assert_eq!(
            drain_due(&mut q, Instant::now() + Duration::from_millis(10)),
            vec![1.0]
        );
    }

    #[test]
    fn interval_rearms_but_does_not_refire_within_one_batch() {
        let mut q = TimerQueue::default();
        q.add(timer_tag(7.0), Vec::new(), Duration::ZERO, true);

        // A zero-delay interval re-arms to exactly `now`; it must still fire
        // only once per batch, or `take_due` would never terminate.
        let now = Instant::now() + Duration::from_millis(10);
        assert_eq!(drain_due(&mut q, now), vec![7.0]);
        assert!(!q.is_empty(), "an interval stays armed after firing");

        assert_eq!(
            drain_due(&mut q, now + Duration::from_millis(1)),
            vec![7.0],
            "the re-armed interval fires again on the next turn"
        );
    }

    #[test]
    fn cleared_interval_stops_firing() {
        let mut q = TimerQueue::default();
        let id = q.add(timer_tag(7.0), Vec::new(), Duration::ZERO, true);

        let now = Instant::now() + Duration::from_millis(10);
        assert_eq!(drain_due(&mut q, now), vec![7.0]);

        q.clear(id);
        assert!(q.is_empty());
        assert!(
            q.take_due(now + Duration::from_secs(1)).is_empty(),
            "the heap entry left behind by the re-arm must be skipped as stale"
        );
    }

    #[test]
    fn effectively_infinite_delay_never_fires_but_keeps_the_loop_alive() {
        // What `setTimeout(fn, Infinity)` produces. The deadline is
        // representable — hundreds of millions of years out — so the timer is
        // armed normally and simply never comes due.
        let mut q = TimerQueue::default();
        q.add(
            timer_tag(1.0),
            Vec::new(),
            Duration::from_millis(u64::MAX),
            false,
        );

        assert!(!q.is_empty(), "the timer still keeps the event loop alive");
        assert!(
            q.take_due(Instant::now() + Duration::from_secs(60))
                .is_empty(),
            "it must not fire"
        );
    }

    #[test]
    fn unrepresentable_deadline_is_armed_without_overflowing() {
        // A delay past what `Instant` can represent must not panic; the timer
        // holds the loop open but has no deadline to wait on.
        let mut q = TimerQueue::default();
        q.add(timer_tag(1.0), Vec::new(), Duration::MAX, false);

        assert!(!q.is_empty(), "the timer still keeps the event loop alive");
        assert!(q.next_deadline().is_none(), "it has no reachable deadline");
        assert!(
            q.take_due(Instant::now() + Duration::from_secs(60))
                .is_empty()
        );
    }

    #[test]
    fn a_timer_can_be_cancelled_after_its_batch_is_collected() {
        // Node semantics: a timer cleared from inside a sibling's callback in
        // the same tick does not run. The batch is collected as ids, so the
        // sibling is still in the queue and `clear` still reaches it.
        let mut q = TimerQueue::default();
        q.add(timer_tag(1.0), Vec::new(), Duration::ZERO, false);
        let doomed = q.add(timer_tag(2.0), Vec::new(), Duration::ZERO, false);

        let due = q.take_due(Instant::now() + Duration::from_millis(10));
        assert_eq!(due.len(), 2, "both timers are due");

        // Fire the first, and let it cancel the second.
        assert!(q.take_for_firing(due[0]).is_some());
        q.clear(doomed);

        assert!(
            q.take_for_firing(due[1]).is_none(),
            "a timer cancelled mid-batch must not fire"
        );
    }

    #[test]
    fn a_due_timer_stays_rooted_until_it_actually_fires() {
        // Collecting the batch must not drop the GC roots of timers that have
        // not run yet, or a collection during an earlier callback would take
        // them with it.
        let mut q = TimerQueue::default();
        q.add(timer_tag(1.0), Vec::new(), Duration::ZERO, false);
        q.add(timer_tag(2.0), Vec::new(), Duration::ZERO, false);

        let due = q.take_due(Instant::now() + Duration::from_millis(10));
        assert_eq!(
            q.iter_roots().count(),
            2,
            "both callbacks are still rooted while the batch is pending"
        );

        q.take_for_firing(due[0]);
        assert_eq!(
            q.iter_roots().count(),
            1,
            "a timer stops being rooted only once it is taken to run"
        );
    }

    #[test]
    fn arm_then_cancel_churn_does_not_grow_the_index() {
        // The workload from the issue: debounce/throttle arms a far-future
        // timer and cancels it, over and over. Those deadlines are never
        // reached by a pop, so without compaction the index grows for ever.
        let mut q = TimerQueue::default();
        for _ in 0..10_000 {
            let id = q.add(timer_tag(1.0), Vec::new(), Duration::from_secs(3600), false);
            q.clear(id);
        }

        assert!(q.is_empty(), "every timer was cancelled");
        assert!(
            q.index_len() <= 64,
            "stale index entries must be reclaimed, found {}",
            q.index_len()
        );
    }

    #[test]
    fn compaction_preserves_the_live_timers_and_their_order() {
        // Interleave keep/cancel pairs so compaction runs with live timers
        // present, then check every survivor still fires, in order.
        let mut q = TimerQueue::default();
        for i in 0..200u64 {
            let doomed = q.add(
                timer_tag(-1.0),
                Vec::new(),
                Duration::from_secs(3600),
                false,
            );
            q.add(
                timer_tag(i as f64),
                Vec::new(),
                Duration::from_millis(i),
                false,
            );
            q.clear(doomed);
        }

        let fired = drain_due(&mut q, Instant::now() + Duration::from_secs(60));
        assert_eq!(fired.len(), 200, "no live timer may be lost to compaction");
        assert_eq!(
            fired,
            (0..200).map(|i| i as f64).collect::<Vec<_>>(),
            "compaction must not disturb deadline order"
        );
    }

    #[test]
    fn next_deadline_skips_cleared_timers() {
        let mut q = TimerQueue::default();
        let soon = q.add(timer_tag(1.0), Vec::new(), Duration::ZERO, false);
        q.add(timer_tag(2.0), Vec::new(), Duration::from_secs(60), false);

        let with_soon = q.next_deadline().expect("a deadline is armed");
        q.clear(soon);
        let after_clear = q.next_deadline().expect("the later timer remains");

        assert!(
            after_clear > with_soon,
            "clearing the soonest timer must push the reported deadline out"
        );
    }

    #[test]
    fn timer_roots_cover_the_callback_and_its_bound_arguments() {
        let mut q = TimerQueue::default();
        q.add(
            timer_tag(1.0),
            vec![timer_tag(2.0), timer_tag(3.0)],
            Duration::ZERO,
            false,
        );

        let (callback, args) = q.iter_roots().next().expect("one armed timer");
        assert_eq!(callback.as_number(), Some(1.0));
        assert_eq!(
            args.len(),
            2,
            "bound arguments are rooted alongside the callback"
        );
    }

    #[test]
    fn timers_carry_their_bound_arguments_to_the_fire_site() {
        let mut q = TimerQueue::default();
        q.add(timer_tag(1.0), vec![timer_tag(9.0)], Duration::ZERO, false);

        let due = q.take_due(Instant::now() + Duration::from_millis(10));
        assert_eq!(due.len(), 1);
        let (_, args) = q.take_for_firing(due[0]).expect("timer is still armed");
        assert_eq!(args[0].as_number(), Some(9.0));
    }

    #[test]
    fn async_gen_yield_pending_round_trip() {
        let mut sched = JobScheduler::default();
        assert!(
            !sched.is_async_gen_yield_pending(),
            "yield-pending must default to false"
        );

        sched.set_async_gen_yield_pending(true);
        assert!(sched.is_async_gen_yield_pending());

        sched.set_async_gen_yield_pending(false);
        assert!(!sched.is_async_gen_yield_pending());
    }
}
