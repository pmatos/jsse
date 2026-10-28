//! Event-loop servicing: draining jobs, firing timers and waiting on host-async completions.

use super::*;

#[path = "node_event_loop.rs"]
pub(crate) mod node_event_loop;

impl Interpreter {
    /// How long a wait on the host-async completion queue may block.
    ///
    /// Capped at the next timer deadline: an in-process timer never signals
    /// that queue, so an uncapped wait would sleep straight past it. Call
    /// before taking the queue lock — resolving the deadline needs `&mut self`.
    ///
    /// A drain nested inside a timer callback is not servicing timers, so their
    /// deadlines must not shorten its wait: capping on an overdue deadline this
    /// level cannot act on would return zero every time and spin.
    pub(crate) fn completion_wait(
        &mut self,
        remaining: std::time::Duration,
    ) -> std::time::Duration {
        let wait = remaining.min(std::time::Duration::from_millis(100));
        if self.dispatching_timers {
            return wait;
        }
        match self.scheduler.next_timer_deadline() {
            Some(at) => wait.min(at.saturating_duration_since(std::time::Instant::now())),
            None => wait,
        }
    }

    /// Fire every timer that has come due, in deadline then arming order, and
    /// report whether any ran.
    ///
    /// Timer callbacks are macrotasks: they run from an event-loop boundary
    /// rather than from inside a microtask drain, and the microtask queue is
    /// drained after each one so a promise settled by one callback is observed
    /// before the next callback runs. A callback that throws is ignored (as
    /// when each timer had its own thread, and as for an unhandled rejection);
    /// a `__host_exit` latches the terminal sink and abandons the rest.
    ///
    /// The batch is collected as ids and each timer is taken from the queue
    /// only as it is about to run, so a callback can still cancel a sibling
    /// that has not fired yet, and the queue goes on rooting the ones waiting.
    pub(crate) fn run_due_timers(&mut self) -> bool {
        // One task must never re-enter another. A callback that nests a whole
        // interpreter run (`$262.evalScript`, say) reaches this again, and
        // would find the rest of its own batch — or a zero-delay interval
        // already re-armed for the next turn — due, and recurse until the
        // drain deadline.
        if self.dispatching_timers {
            return false;
        }
        let now = self.scheduler.timers_mut().now();
        let due = self.scheduler.take_due_timers(now);
        if due.is_empty() {
            return false;
        }
        self.dispatching_timers = true;
        for id in due {
            let Some((callback, args)) = self.scheduler.take_timer_for_firing(id) else {
                continue; // cancelled by an earlier callback in this batch
            };
            // A one-shot has just left the queue; root it across its own call.
            let frame = self.gc_root_frame();
            self.gc_root_value(&callback);
            for arg in &args {
                self.gc_root_value(arg);
            }
            let rooted_depth = self.gc_root_frame();
            self.job_depth += 1;
            let result = self.call_budgeted_timer(&callback, &args);
            self.finish_job();
            self.gc_assert_root_depth(rooted_depth, "a timer callback");
            self.gc_unroot_frame(frame);
            if let Completion::Exit(code) = result {
                self.pending_exit = Some(code);
                break;
            }
            if self.pending_exit.is_some() {
                break;
            }
            // Microtask checkpoint between tasks, as on the HTML/Node loop.
            self.drain_microtasks();
            if self.pending_exit.is_some() {
                break;
            }
        }
        self.dispatching_timers = false;
        true
    }

    /// Drain the job queues after a program run, keeping its completion value
    /// reachable: a collection inside a job must not free the pending result.
    pub(crate) fn drain_after_run(&mut self, result: &Completion) {
        let frame = self.gc_root_frame();
        if let Completion::Normal(value) | Completion::Throw(value) = result {
            self.gc_root_value(value);
        }
        if self.scheduler.node_loop.installed {
            self.node_drain_after_run(result);
        } else {
            self.drain_microtasks_until_idle();
        }
        self.gc_unroot_frame(frame);
    }

    /// Drain already-queued jobs, then wait for host async jobs such as
    /// Atomics.waitAsync/getReportAsync when JS code is still awaiting them.
    ///
    /// A pending host-async Promise is considered "awaited" if it has any
    /// fulfill/reject reactions registered (from `.then` / `await`). Detached
    /// fire-and-forget host async work (e.g. `Atomics.waitAsync(ta, 0, 0, Infinity)`
    /// with no `.then` / `await`) does not block process exit.
    pub(crate) fn drain_microtasks_until_idle(&mut self) {
        self.drain_microtasks();
        // Terminal-sink read for `__host_exit` (issue #242): the drain above
        // latches `pending_exit` when a job exits. Stop immediately; don't block
        // up to 120s waiting on timers/host-async work.
        if self.pending_exit.is_some() {
            return;
        }
        // Timers belong to the outermost drain. A drain nested inside a timer
        // callback neither fires them nor waits on them.
        let servicing_timers = !self.dispatching_timers;
        let waiting_on_timers = |interp: &Self| servicing_timers && interp.scheduler.has_timers();
        if !self.has_awaited_pending_async() && !waiting_on_timers(self) {
            return;
        }

        // An armed timer keeps the loop alive, so an uncleared `setInterval`
        // runs until this cap rather than forever as it would under Node.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        loop {
            self.drain_microtasks();
            // See above (issue #242): a job drained above may have latched the
            // exit sink — stop immediately instead of waiting up to 120s on any
            // still-pending timer/async job. Inert unless the floor is on.
            if self.pending_exit.is_some() {
                return;
            }
            if !self.has_awaited_pending_async() && !waiting_on_timers(self) {
                break;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }

            // Run due timers, then loop so the microtask drain above handles
            // whatever their callbacks queued. Timers armed by a callback wait
            // for the next turn, so a recursive `setTimeout(fn, 0)` cannot
            // starve the host-async completions below.
            if self.run_due_timers() {
                if self.pending_exit.is_some() {
                    return;
                }
                continue;
            }

            if self.scheduler.timers_mut().is_virtual() {
                if !self.fast_forward_to_next_timer() {
                    break;
                }
                continue;
            }

            // Nothing is due: sleep until a host async completion arrives or
            // the earliest timer comes due, whichever is sooner.
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            let wait = self.completion_wait(remaining);
            let (ref mtx, ref cvar) = *self.agent_async_completions;
            let lock = mtx.lock().unwrap();
            if !lock.is_empty() {
                drop(lock);
                continue;
            }
            let (_guard, _timeout_result) = cvar.wait_timeout(lock, wait).unwrap();
            drop(_guard);
        }

        self.drain_microtasks();
    }

    /// True if any tracked pending host-async Promise has reactions registered,
    /// meaning JS code is awaiting / handling it. Unrelated suspended async
    /// functions or queued microtasks do not count — only per-promise evidence
    /// tied to an in-flight host worker.
    pub(crate) fn has_awaited_pending_async(&self) -> bool {
        if self.scheduler.pending_async_jobs_count() == 0 {
            return false;
        }
        let ids = self.scheduler.pending_async_promise_ids_lock();
        for &id in ids.iter() {
            if let Some(obj) = self.get_object_cell(id)
                && let Some(pd) = obj.borrow().promise_data()
                && (!pd.fulfill_reactions.is_empty() || !pd.reject_reactions.is_empty())
            {
                return true;
            }
        }
        false
    }

    /// Like drain_microtasks but blocks waiting for async completions via Condvar.
    /// Used by agent threads that need to wait for waitAsync resolutions.
    pub(crate) fn drain_microtasks_blocking(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        loop {
            while let Some(task) = self.scheduler.pop_microtask() {
                let job_result = self.run_budgeted_microtask(task);
                // A `__host_exit` inside the job (issue #242) latches the
                // terminal sink and stops draining.
                if let Completion::Exit(code) = job_result {
                    self.pending_exit = Some(code);
                    return;
                }
            }
            if self.pending_exit.is_some() {
                return;
            }
            let completions: Vec<_> = {
                let mut lock = self.agent_async_completions.0.lock().unwrap();
                lock.drain(..).collect()
            };
            if !completions.is_empty() {
                for f in completions {
                    f(self);
                }
                continue;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            // An agent thread services its own timers (#254); nothing arrives
            // cross-thread on its behalf.
            if self.run_due_timers() {
                if self.pending_exit.is_some() {
                    return;
                }
                continue;
            }
            // Wait for async completions with a timeout.
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            let wait = self.completion_wait(remaining);
            let (ref mtx, ref cvar) = *self.agent_async_completions;
            let lock = mtx.lock().unwrap();
            if !lock.is_empty() {
                drop(lock);
                continue;
            }
            let (_guard, _timeout_result) = cvar.wait_timeout(lock, wait).unwrap();
            drop(_guard);
        }
    }
}
