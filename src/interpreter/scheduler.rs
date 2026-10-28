use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rustc_hash::FxHashMap;

use crate::types::JsValue;

use super::AsyncDisposal;
use super::AsyncFunctionState;
use super::AsyncGenRequest;
use super::Completion;
use super::Interpreter;
use super::PendingDispose;
use super::PromiseReaction;
use super::TryContextInfo;
pub(crate) use super::event_loop::node_event_loop::NodeLoopState;
pub(crate) use super::timer_queue::TimerQueue;

pub(crate) type MicrotaskJob = Box<dyn FnOnce(&mut Interpreter) -> Completion>;

/// One entry of the microtask queue.
///
/// Promise jobs are typed values the collector reads its roots from directly;
/// `Closure` keeps the explicit-roots form for one-off producers (disposal,
/// async-generator and module plumbing).
pub(crate) enum Microtask {
    Closure {
        roots: Vec<JsValue>,
        job: MicrotaskJob,
    },
    Reaction {
        reaction: PromiseReaction,
        argument: JsValue,
    },
    ResolveThenable {
        promise_id: u64,
        thenable: JsValue,
        then_fn: JsValue,
    },
}

impl Microtask {
    pub(crate) fn for_each_root(&self, mut visit: impl FnMut(&JsValue)) {
        match self {
            Microtask::Closure { roots, .. } => roots.iter().for_each(visit),
            Microtask::Reaction { reaction, argument } => {
                visit(argument);
                if let Some(handler) = &reaction.handler {
                    visit(handler);
                }
                // Resolving closures only capture a promise id, which the
                // tracer cannot see, so the downstream promise is a root.
                reaction.target.for_each_value(&mut visit);
                if let Some(promise_id) = reaction.target.promise_id() {
                    visit(&JsValue::object(promise_id));
                }
            }
            Microtask::ResolveThenable {
                promise_id,
                thenable,
                then_fn,
            } => {
                visit(thenable);
                visit(then_fn);
                visit(&JsValue::object(*promise_id));
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct JobScheduler {
    microtask_queue: VecDeque<Microtask>,
    async_gen_queues: FxHashMap<u64, VecDeque<AsyncGenRequest>>,
    async_gen_yield_pending: bool,
    async_function_states: FxHashMap<u64, AsyncFunctionState>,
    async_disposals: FxHashMap<u64, AsyncDisposal>,
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
    /// Callbacks and pending rejections of the Node event loop (`--node`).
    pub(crate) node_loop: NodeLoopState,
}

impl JobScheduler {
    pub(crate) fn enqueue_microtask(&mut self, (roots, job): (Vec<JsValue>, MicrotaskJob)) {
        self.microtask_queue
            .push_back(Microtask::Closure { roots, job });
    }

    /// NewPromiseReactionJob (ECMA-262 §27.2.2.1).
    pub(crate) fn enqueue_reaction_job(&mut self, reaction: PromiseReaction, argument: JsValue) {
        self.microtask_queue
            .push_back(Microtask::Reaction { reaction, argument });
    }

    /// NewPromiseResolveThenableJob (ECMA-262 §27.2.2.2).
    pub(crate) fn enqueue_resolve_thenable_job(
        &mut self,
        promise_id: u64,
        thenable: JsValue,
        then_fn: JsValue,
    ) {
        self.microtask_queue.push_back(Microtask::ResolveThenable {
            promise_id,
            thenable,
            then_fn,
        });
    }

    pub(crate) fn pop_microtask(&mut self) -> Option<Microtask> {
        self.microtask_queue.pop_front()
    }

    /// Every value the scheduler keeps alive: roots held by queued microtasks,
    /// pending async-generator requests, and armed timers. Centralised here so
    /// a queue added later cannot be forgotten by the collector.
    ///
    /// A generator with a pending request is rooted too: its await/yield
    /// continuations capture it only inside native closures the tracer cannot
    /// walk, so nothing else keeps it alive while the request is outstanding.
    pub(crate) fn for_each_root(&self, mut visit: impl FnMut(&JsValue)) {
        for task in &self.microtask_queue {
            task.for_each_root(&mut visit);
        }
        for (gen_id, queue) in &self.async_gen_queues {
            if queue.is_empty() {
                continue;
            }
            visit(&JsValue::object(*gen_id));
            for request in queue {
                visit(&request.value);
                visit(&request.promise);
            }
        }
        for (callback, args) in self.timers.iter_roots() {
            visit(callback);
            for value in args {
                visit(value);
            }
        }
        self.node_loop.for_each_root(&mut visit);
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

    pub(crate) fn remove_async_gen_queue(&mut self, gen_id: u64) {
        if !self.async_gen_queues.is_empty() {
            self.async_gen_queues.remove(&gen_id);
        }
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

    pub(crate) fn async_function_state(&self, id: u64) -> Option<&AsyncFunctionState> {
        self.async_function_states.get(&id)
    }

    pub(crate) fn async_function_state_mut(&mut self, id: u64) -> Option<&mut AsyncFunctionState> {
        self.async_function_states.get_mut(&id)
    }

    pub(crate) fn park_async_function_dispose(&mut self, id: u64, pending: PendingDispose) {
        if let Some(state) = self.async_function_states.get_mut(&id) {
            state.pending_dispose = Some(pending);
        }
    }

    /// Write the driver's in-flight `try_stack` into the re-inserted
    /// `Executing` state in place, so a completion just parked on it (issue
    /// #719) is GC-rooted immediately rather than only once the driver's
    /// next suspension serializes it — in case the finally that owns it
    /// runs `$262.gc()` (or otherwise triggers a collection) before then.
    pub(crate) fn sync_async_function_try_stack(&mut self, id: u64, try_stack: &[TryContextInfo]) {
        if let Some(state) = self.async_function_states.get_mut(&id) {
            state.try_stack = try_stack.to_vec();
        }
    }

    pub(crate) fn iter_async_function_states(&self) -> impl Iterator<Item = &AsyncFunctionState> {
        self.async_function_states.values()
    }

    pub(crate) fn insert_async_disposal(&mut self, id: u64, disposal: AsyncDisposal) {
        self.async_disposals.insert(id, disposal);
    }

    pub(crate) fn remove_async_disposal(&mut self, id: u64) -> Option<AsyncDisposal> {
        self.async_disposals.remove(&id)
    }

    pub(crate) fn iter_async_disposals(&self) -> impl Iterator<Item = &AsyncDisposal> {
        self.async_disposals.values()
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

    pub(crate) fn set_timer_ref(&mut self, id: u64, refed: bool) {
        self.timers.set_ref(id, refed);
    }

    pub(crate) fn has_refed_timers(&self) -> bool {
        self.timers.has_refed()
    }

    pub(crate) fn has_timers(&self) -> bool {
        !self.timers.is_empty()
    }

    pub(crate) fn next_timer_deadline(&mut self) -> Option<Instant> {
        self.timers.next_deadline()
    }

    pub(crate) fn timers_mut(&mut self) -> &mut TimerQueue {
        &mut self.timers
    }

    pub(crate) fn take_due_timers(&mut self, now: Instant) -> Vec<u64> {
        self.timers.take_due(now)
    }

    pub(crate) fn take_timer_for_firing(&mut self, id: u64) -> Option<(JsValue, Vec<JsValue>)> {
        self.timers.take_for_firing(id)
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
#[path = "scheduler_tests.rs"]
mod tests;
