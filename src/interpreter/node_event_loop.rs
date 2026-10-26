//! Node event-loop semantics for `--node` (issue #1229).
//!
//! Everything here is inert until the Node prelude installs its callbacks with
//! `__host_loop_install`; the default (test262) event loop in `event_loop.rs`
//! never reaches this file. The JS prelude owns the tick queue, the immediate
//! queue and the `process` lifecycle events; Rust owns what JS cannot: the
//! phase order of the loop, microtask-queue draining, the liveness test and
//! the unhandled-rejection list. See `docs/adr/2026-10-26-0854-node-host-mode.md`.

use super::super::*;

#[derive(Default)]
pub(crate) struct NodeLoopState {
    pub(crate) installed: bool,
    tick_cb: Option<JsValue>,
    immediate_cb: Option<JsValue>,
    uncaught_cb: Option<JsValue>,
    rejections_cb: Option<JsValue>,
    lifecycle_cb: Option<JsValue>,
    pub(crate) tick_scheduled: bool,
    pub(crate) immediate_count: u32,
    pub(crate) immediate_refed: u32,
    pending_rejections: Vec<JsValue>,
    running: bool,
    pub(crate) main_throw_handled: bool,
    /// JS function every host-async event (child processes, signals) goes to.
    pub(crate) host_dispatch: Option<JsValue>,
    /// Ref'd host handles JS reports through `__host_loop_ref`; each keeps the
    /// loop alive.
    pub(crate) external_refs: u32,
}

impl NodeLoopState {
    pub(crate) fn install(
        &mut self,
        tick: JsValue,
        uncaught: JsValue,
        rejections: JsValue,
        lifecycle: JsValue,
    ) {
        self.tick_cb = Some(tick);
        self.uncaught_cb = Some(uncaught);
        self.rejections_cb = Some(rejections);
        self.lifecycle_cb = Some(lifecycle);
        self.installed = true;
    }

    pub(crate) fn set_immediate_callback(&mut self, callback: JsValue) {
        self.immediate_cb = Some(callback);
    }

    pub(crate) fn for_each_root(&self, mut visit: impl FnMut(&JsValue)) {
        let callbacks = [
            &self.tick_cb,
            &self.immediate_cb,
            &self.uncaught_cb,
            &self.rejections_cb,
            &self.lifecycle_cb,
        ];
        for callback in callbacks.into_iter().flatten() {
            visit(callback);
        }
        for promise in &self.pending_rejections {
            visit(promise);
        }
        if let Some(dispatch) = &self.host_dispatch {
            visit(dispatch);
        }
    }
}

impl Interpreter {
    /// Promise reject hook (`reject_promise`): remember a promise that has no
    /// handler yet. It is re-checked when the rejections are processed, because
    /// a handler attached before then makes it handled.
    pub(crate) fn on_reject(
        &mut self,
        promise_id: u64,
        reactions: Vec<PromiseReaction>,
        reason: JsValue,
    ) {
        if self.scheduler.node_loop.installed && reactions.is_empty() {
            self.scheduler
                .node_loop
                .pending_rejections
                .push(JsValue::object(promise_id));
        }
        self.trigger_promise_reactions(reactions, reason);
    }

    /// True when a throwing main script was reported through
    /// `process.on('uncaughtException')`, so the CLI must not report it again.
    pub fn node_main_throw_handled(&self) -> bool {
        self.scheduler.node_loop.main_throw_handled
    }

    pub(crate) fn node_call(&mut self, callback: &JsValue, args: &[JsValue]) -> Completion {
        let frame = self.gc_root_frame();
        self.gc_root_value(callback);
        for arg in args {
            self.gc_root_value(arg);
        }
        let rooted_depth = self.gc_root_frame();
        let result = self.call_function(callback, &JsValue::UNDEFINED, args);
        self.gc_assert_root_depth(rooted_depth, "a node loop callback");
        self.gc_unroot_frame(frame);
        result
    }

    /// An exit unwinds the loop; a throw that got past the prelude's own
    /// handlers is routed to the uncaught-exception path.
    pub(crate) fn node_settle(&mut self, result: Completion) {
        match result {
            Completion::Exit(code) => self.pending_exit = Some(code),
            Completion::Throw(error) => self.node_uncaught(error),
            _ => {}
        }
    }

    pub(crate) fn node_uncaught(&mut self, error: JsValue) {
        let Some(callback) = self.scheduler.node_loop.uncaught_cb.clone() else {
            return;
        };
        let origin = JsValue::string(JsString::from_str("uncaughtException"));
        match self.node_call(&callback, &[error, origin]) {
            Completion::Exit(code) => self.pending_exit = Some(code),
            Completion::Throw(_) => self.pending_exit = Some(7),
            _ => {}
        }
    }

    /// Promises rejected without a handler that are still unhandled now, as a
    /// flat `[promise, reason, ...]` list.
    fn node_unhandled_rejections(&mut self) -> Vec<JsValue> {
        let pending = std::mem::take(&mut self.scheduler.node_loop.pending_rejections);
        let mut flat = Vec::new();
        for promise in pending {
            let Some(id) = promise.as_object_id() else {
                continue;
            };
            let Some(cell) = self.get_object_cell(id) else {
                continue;
            };
            let borrowed = cell.borrow();
            if let Some(data) = borrowed.promise_data()
                && !data.is_handled
                && let PromiseState::Rejected(reason) = &data.state
            {
                flat.push(promise.clone());
                flat.push(reason.clone());
            }
        }
        flat
    }

    /// Node's post-macrotask checkpoint: the whole `nextTick` queue, then every
    /// promise job, repeated while either refills, then the rejections that are
    /// still unhandled.
    pub(crate) fn node_checkpoint(&mut self) {
        loop {
            if self.pending_exit.is_some() {
                return;
            }
            if self.scheduler.node_loop.tick_scheduled {
                match self.scheduler.node_loop.tick_cb.clone() {
                    Some(callback) => {
                        let result = self.node_call(&callback, &[]);
                        let threw = matches!(result, Completion::Throw(_));
                        self.node_settle(result);
                        if threw {
                            // Node abandons the checkpoint on a throwing tick;
                            // the queue resumes at the next one.
                            return;
                        }
                    }
                    None => self.scheduler.node_loop.tick_scheduled = false,
                }
                continue;
            }
            self.drain_microtasks();
            if self.pending_exit.is_some() || self.scheduler.node_loop.tick_scheduled {
                continue;
            }
            let unhandled = self.node_unhandled_rejections();
            if unhandled.is_empty() {
                return;
            }
            let Some(callback) = self.scheduler.node_loop.rejections_cb.clone() else {
                return;
            };
            let list = self.create_array(unhandled);
            let result = self.node_call(&callback, &[list]);
            self.node_settle(result);
        }
    }

    fn node_loop_alive(&self) -> bool {
        self.scheduler.has_refed_timers()
            || self.scheduler.node_loop.immediate_refed > 0
            || self.scheduler.node_loop.external_refs > 0
            || self.has_awaited_pending_async()
    }

    fn node_run_immediates(&mut self) {
        if self.scheduler.node_loop.immediate_count == 0 {
            return;
        }
        let Some(callback) = self.scheduler.node_loop.immediate_cb.clone() else {
            return;
        };
        let result = self.node_call(&callback, &[]);
        self.node_settle(result);
        self.node_checkpoint();
    }

    /// The poll phase: sleep until the earliest timer or a host-async
    /// completion, but not at all while a ref'd immediate is waiting or once
    /// nothing ref'd is left alive (libuv's zero poll timeout).
    fn node_poll(&mut self) {
        let block = self.scheduler.node_loop.immediate_refed == 0
            && !self.host_events_pending()
            && self.node_loop_alive();
        let wait = self.completion_wait(std::time::Duration::from_millis(100));
        let arrived = {
            let (mtx, cvar) = &*self.agent_async_completions;
            let mut lock = mtx.lock().unwrap();
            if block && lock.is_empty() {
                lock = cvar.wait_timeout(lock, wait).unwrap().0;
            }
            !lock.is_empty()
        };
        if arrived {
            self.node_checkpoint();
        }
        self.pump_host_events();
    }

    #[cfg(unix)]
    fn host_events_pending(&self) -> bool {
        self.node_host.child.has_events()
    }

    #[cfg(not(unix))]
    fn host_events_pending(&self) -> bool {
        false
    }

    #[cfg(unix)]
    fn pump_host_events(&mut self) {
        self.node_pump_host_events();
    }

    #[cfg(not(unix))]
    fn pump_host_events(&mut self) {}

    /// Timers, poll, check, repeated while a ref'd handle keeps the loop alive.
    /// Callbacks run their own tick/microtask checkpoints (the prelude calls
    /// `__host_checkpoint` after each one), as Node does.
    pub(crate) fn node_run_loop(&mut self) {
        if self.scheduler.node_loop.running {
            return;
        }
        self.scheduler.node_loop.running = true;
        while self.pending_exit.is_none() && self.node_loop_alive() {
            self.run_due_timers();
            if self.pending_exit.is_some() {
                break;
            }
            self.node_poll();
            self.node_run_immediates();
        }
        self.scheduler.node_loop.running = false;
    }

    /// The Node branch of `drain_after_run`: report a throwing main script
    /// through the uncaught-exception path, run the checkpoint, then the loop.
    pub(crate) fn node_drain_after_run(&mut self, result: &Completion) {
        self.scheduler.node_loop.main_throw_handled = false;
        self.node_esm_main_ready();
        if self.pending_exit.is_some() {
            return;
        }
        if let Completion::Throw(error) = result {
            self.node_uncaught(error.clone());
            self.scheduler.node_loop.main_throw_handled = self.pending_exit.is_none();
        }
        self.node_checkpoint();
        self.node_run_loop();
    }

    fn node_lifecycle(&mut self, event: &str) {
        let Some(callback) = self.scheduler.node_loop.lifecycle_cb.clone() else {
            return;
        };
        let name = JsValue::string(JsString::from_str(event));
        let result = self.node_call(&callback, &[name]);
        self.node_settle(result);
        self.node_checkpoint();
    }

    /// Called by the CLI once the main script and the loop have gone idle:
    /// `beforeExit` (which may start more work, so the loop is re-entered),
    /// then `exit`. A no-op unless the Node event loop is installed.
    pub fn node_host_finish(&mut self) {
        if !self.scheduler.node_loop.installed {
            return;
        }
        while self.pending_exit.is_none() {
            self.node_lifecycle("beforeExit");
            if self.pending_exit.is_some() || !self.node_loop_alive() {
                break;
            }
            self.node_run_loop();
        }
        if self.pending_exit.is_none() {
            let unsettled = self.node_esm_unsettled_main();
            if unsettled {
                eprintln!("Warning: Detected unsettled top-level await");
            }
            self.node_lifecycle(if unsettled { "unsettled" } else { "exit" });
        }
    }
}
