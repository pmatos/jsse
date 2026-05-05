use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use rustc_hash::FxHashMap;

use crate::types::JsValue;

use super::AsyncFunctionState;
use super::AsyncGenRequest;
use super::Completion;
use super::Interpreter;

pub(crate) type MicrotaskJob = Box<dyn FnOnce(&mut Interpreter) -> Completion>;

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
    /// Host timer callbacks scheduled by setTimeout. These are intentionally
    /// tracked separately from promise-backed host async jobs so detached
    /// Atomics.waitAsync jobs do not keep the process alive.
    pending_timer_jobs: Arc<AtomicUsize>,
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

    pub(crate) fn pending_timer_jobs_handle(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.pending_timer_jobs)
    }

    pub(crate) fn pending_timer_jobs_count(&self) -> usize {
        self.pending_timer_jobs.load(Ordering::SeqCst)
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

        let job: MicrotaskJob = Box::new(|_interp| Completion::Normal(JsValue::Undefined));
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
            Box::new(|_interp| Completion::Normal(JsValue::Undefined))
        }
        fn tag(n: f64) -> Vec<JsValue> {
            vec![JsValue::Number(n)]
        }

        let mut sched = JobScheduler::default();
        sched.enqueue_microtask((tag(1.0), job()));
        sched.enqueue_microtask((tag(2.0), job()));
        sched.enqueue_microtask((tag(3.0), job()));

        let popped_tags: Vec<f64> = std::iter::from_fn(|| sched.pop_microtask())
            .map(|(roots, _)| match roots.as_slice() {
                [JsValue::Number(n)] => *n,
                _ => panic!("unexpected roots shape"),
            })
            .collect();

        assert_eq!(popped_tags, vec![1.0, 2.0, 3.0]);
    }

    fn next_request(tag: f64) -> super::super::AsyncGenRequest {
        super::super::AsyncGenRequest {
            kind: super::super::AsyncGenRequestKind::Next,
            value: JsValue::Number(tag),
            promise: JsValue::Undefined,
            resolve_fn: JsValue::Undefined,
            reject_fn: JsValue::Undefined,
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
            .map(|r| match r.value {
                JsValue::Number(n) => n,
                _ => panic!("unexpected value type"),
            })
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
