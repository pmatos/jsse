//! The blocking `await` of the tree-walker (sec-await).

use super::*;

impl Interpreter {
    pub(crate) fn await_value(&mut self, val: &JsValue) -> Completion {
        use std::cell::Cell;

        // §27.7.5.3 Await — every await goes through PromiseResolve and schedules
        // its continuation as a microtask, ensuring proper interleaving.
        let promise = self.promise_resolve_value(val);
        let promise_id = if let Some(o) = (promise)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            o.id
        } else {
            0
        };

        let gc_frame = self.gc_root_frame();
        self.gc_root_value(&promise);

        let done = Rc::new(Cell::new(false));
        let result: Rc<RefCell<Option<Result<JsValue, JsValue>>>> = Rc::new(RefCell::new(None));

        let state = self.get_promise_state(promise_id);
        match state {
            Some(PromiseState::Fulfilled(v)) => {
                let done_c = done.clone();
                let result_c = result.clone();
                let value = v.clone();
                self.scheduler.enqueue_microtask((
                    vec![value.clone()],
                    Box::new(move |_interp| {
                        done_c.set(true);
                        *result_c.borrow_mut() = Some(Ok(value));
                        Completion::Normal(JsValue::UNDEFINED)
                    }),
                ));
            }
            Some(PromiseState::Rejected(r)) => {
                let done_c = done.clone();
                let result_c = result.clone();
                let reason = r.clone();
                self.scheduler.enqueue_microtask((
                    vec![reason.clone()],
                    Box::new(move |_interp| {
                        done_c.set(true);
                        *result_c.borrow_mut() = Some(Err(reason));
                        Completion::Normal(JsValue::UNDEFINED)
                    }),
                ));
            }
            Some(PromiseState::Pending) => {
                let done_f = done.clone();
                let result_f = result.clone();
                let done_r = done.clone();
                let result_r = result.clone();

                let fulfill_handler = self.create_function(JsFunction::native(
                    "awaitFulfill".to_string(),
                    1,
                    move |_interp, _this, args| {
                        let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        done_f.set(true);
                        *result_f.borrow_mut() = Some(Ok(v));
                        Completion::Normal(JsValue::UNDEFINED)
                    },
                ));
                let reject_handler = self.create_function(JsFunction::native(
                    "awaitReject".to_string(),
                    1,
                    move |_interp, _this, args| {
                        let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        done_r.set(true);
                        *result_r.borrow_mut() = Some(Err(v));
                        Completion::Normal(JsValue::UNDEFINED)
                    },
                ));

                if let Some(obj) = self.get_object_cell(promise_id) {
                    let mut o = obj.borrow_mut();
                    if let Some(pd) = o.promise_data_mut() {
                        pd.is_handled = true;
                        pd.fulfill_reactions.push(PromiseReaction {
                            handler: Some(fulfill_handler),
                            target: ReactionTarget::None,
                            reaction_type: PromiseReactionType::Fulfill,
                        });
                        pd.reject_reactions.push(PromiseReaction {
                            handler: Some(reject_handler),
                            target: ReactionTarget::None,
                            reaction_type: PromiseReactionType::Reject,
                        });
                    }
                }
            }
            None => {
                self.gc_unroot_frame(gc_frame);
                return Completion::Normal(val.clone());
            }
        }

        let await_deadline = if self.is_agent_thread {
            Some(std::time::Instant::now() + std::time::Duration::from_secs(120))
        } else {
            None
        };
        loop {
            if done.get() {
                break;
            }
            // `__host_exit` latched (issue #242): run no further queued job.
            if self.pending_exit.is_some() {
                break;
            }
            if let Some(task) = self.scheduler.pop_microtask() {
                let job_result = self.run_budgeted_microtask(task);
                // A `__host_exit` inside the job (issue #242) latches the
                // terminal sink; the post-loop check turns it into a
                // `Completion::Exit` returned from this `await`.
                if let Completion::Exit(code) = job_result {
                    self.pending_exit = Some(code);
                    break;
                }
                continue;
            }
            // Check agent async completions
            let completions: Vec<_> = {
                let mut lock = self.agent_async_completions.0.lock().unwrap();
                lock.drain(..).collect()
            };
            if !completions.is_empty() {
                for f in completions {
                    f(self);
                    // Stop before the rest of the batch if a callback exited.
                    if self.pending_exit.is_some() {
                        break;
                    }
                }
                continue;
            }
            // For agent threads, block-wait for async completions
            if let Some(deadline) = await_deadline {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }
                // Timers are in-process (#254), so this loop has to fire them
                // itself rather than wait for a completion that never comes.
                if self.run_due_timers() {
                    continue;
                }
                let wait = self.completion_wait(remaining);
                let (ref mtx, ref cvar) = *self.agent_async_completions;
                let lock = mtx.lock().unwrap();
                if !lock.is_empty() {
                    drop(lock);
                    continue;
                }
                let _ = cvar.wait_timeout(lock, wait).unwrap();
                continue;
            }
            break;
        }

        self.gc_unroot_frame(gc_frame);

        // If a drained job requested `__host_exit` (issue #242), propagate the
        // exit out of this `await` uncatchably instead of yielding the awaited
        // value; the terminal sink stays set for `main`.
        if let Some(code) = self.pending_exit {
            return Completion::Exit(code);
        }
        match result.borrow_mut().take() {
            Some(Ok(v)) => Completion::Normal(v),
            Some(Err(e)) => Completion::Throw(e),
            None => Completion::Normal(JsValue::UNDEFINED),
        }
    }
}
