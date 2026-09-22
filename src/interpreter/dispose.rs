use super::*;

/// What a [`DisposeCursor`] needs from its driver next.
pub(crate) enum DisposeStep {
    /// Perform `Await(value)` and feed the outcome to the next
    /// [`DisposeCursor::step`] call.
    Await(JsValue),
    /// DisposeResources finished with this completion.
    Done(Completion),
}

enum Pending {
    None,
    /// The `Await` of an async disposer's result; a rejection is a throw
    /// completion for that resource.
    Disposer,
    /// DisposeResources step 3.d: the `Await(undefined)` owed before a
    /// synchronous disposer that follows a null/undefined resource.
    Barrier,
    /// DisposeResources step 4: the single trailing `Await(undefined)`.
    Trailing,
}

/// DisposeResources (proposal-explicit-resource-management, `sec-disposeresources`)
/// as a resumable state machine, so a caller can suspend at each `Await`
/// instead of draining the microtask queue inline.
pub(crate) struct DisposeCursor {
    /// Resources in registration order; disposal pops from the back.
    remaining: Vec<DisposableResource>,
    completion: Completion,
    current_error: Option<JsValue>,
    needs_await: bool,
    has_awaited: bool,
    pending: Pending,
}

impl DisposeCursor {
    pub(crate) fn new(stack: Vec<DisposableResource>, completion: Completion) -> Self {
        // A `Completion::Exit` (issue #242) makes the exit immediate: no
        // `Symbol.dispose`/`Symbol.asyncDispose` may run after `__host_exit`.
        let remaining = if matches!(completion, Completion::Exit(_)) {
            Vec::new()
        } else {
            stack
        };
        let current_error = match &completion {
            Completion::Throw(e) => Some(e.clone()),
            _ => None,
        };
        Self {
            remaining,
            completion,
            current_error,
            needs_await: false,
            has_awaited: false,
            pending: Pending::None,
        }
    }

    /// Advance the disposal. `awaited` carries the outcome of the `Await`
    /// requested by the previous [`DisposeStep::Await`] (`None` on the first
    /// call).
    pub(crate) fn step(
        &mut self,
        interp: &mut Interpreter,
        awaited: Option<Result<JsValue, JsValue>>,
    ) -> DisposeStep {
        interp.with_gc_root_scope(|interp| {
            self.for_each_value(|v| interp.gc_root_value(v));
            if let Some(Err(e)) = &awaited {
                interp.gc_root_value(e);
            }
            self.step_rooted(interp, awaited)
        })
    }

    fn step_rooted(
        &mut self,
        interp: &mut Interpreter,
        awaited: Option<Result<JsValue, JsValue>>,
    ) -> DisposeStep {
        match std::mem::replace(&mut self.pending, Pending::None) {
            Pending::None | Pending::Barrier => {}
            Pending::Disposer => {
                if let Some(Err(e)) = awaited {
                    self.record_error(interp, e);
                }
            }
            Pending::Trailing => return DisposeStep::Done(self.finish()),
        }

        while let Some(resource) = self.remaining.pop() {
            if resource.hint == DisposeHint::Sync && self.needs_await && !self.has_awaited {
                self.needs_await = false;
                self.remaining.push(resource);
                self.pending = Pending::Barrier;
                return DisposeStep::Await(JsValue::UNDEFINED);
            }

            if resource.dispose_method.is_undefined() {
                self.needs_await = true;
                continue;
            }

            let result = interp.call_function(&resource.dispose_method, &resource.value, &[]);
            // A disposer that called `__host_exit` makes the exit immediate:
            // skip the remaining disposers and `wrap_suppressed_error` (a
            // user-replaceable `SuppressedError` constructor).
            if let Completion::Exit(code) = result {
                self.remaining.clear();
                return DisposeStep::Done(Completion::Exit(code));
            }
            match result {
                Completion::Normal(v) if resource.hint == DisposeHint::Async => {
                    self.has_awaited = true;
                    self.pending = Pending::Disposer;
                    return DisposeStep::Await(v);
                }
                Completion::Throw(e) => self.record_error(interp, e),
                _ => {}
            }
        }

        if self.needs_await && !self.has_awaited {
            self.pending = Pending::Trailing;
            return DisposeStep::Await(JsValue::UNDEFINED);
        }
        DisposeStep::Done(self.finish())
    }

    /// Every value the cursor keeps alive across a suspension, for GC rooting.
    pub(crate) fn for_each_value(&self, mut f: impl FnMut(&JsValue)) {
        for resource in &self.remaining {
            f(&resource.value);
            f(&resource.dispose_method);
        }
        match &self.completion {
            Completion::Return(v) | Completion::Throw(v) | Completion::Normal(v) => f(v),
            Completion::Break(_, Some(v)) | Completion::Continue(_, Some(v)) => f(v),
            _ => {}
        }
        if let Some(e) = &self.current_error {
            f(e);
        }
    }

    fn record_error(&mut self, interp: &mut Interpreter, error: JsValue) {
        let error = interp.wrap_suppressed_error(error, self.current_error.take());
        interp.gc_root_value(&error);
        self.current_error = Some(error);
    }

    fn finish(&mut self) -> Completion {
        match self.current_error.take() {
            Some(e) => Completion::Throw(e),
            None => std::mem::replace(&mut self.completion, Completion::Empty),
        }
    }
}

/// What an async function does once the disposal of its function-level
/// resources finishes, i.e. which completion the disposal was entered with.
#[derive(Clone, Copy)]
pub(crate) enum DisposeThen {
    /// A `return` left the body; a throw from disposal is routed as an
    /// exception.
    Return,
    /// An uncaught throw left the body.
    Throw,
    /// The body ran to its end.
    Complete,
    /// A block ending its state finished; its completion (after disposal)
    /// resumes the state's normal post-body handling.
    Block,
    /// `ExitScope`'s own (non-abrupt) disposal of a block scope; once done,
    /// continue at the carried state.
    ScopeExit(usize),
    /// A `return` crossing one or more open block scopes is disposing the
    /// innermost one; once done, `route_return!` is re-entered with the
    /// value carried by the cursor's own completion so it can continue
    /// unwinding whatever remains (further scopes, then for-of loops).
    ScopeCrossReturn,
    /// A `break`/`continue` crossing one or more open block scopes is
    /// disposing the innermost one; once done, `route_loop_control!` is
    /// re-entered with the carried target.
    ScopeCrossLoopControl(super::generator_transform::LoopControlTarget),
    /// An in-flight throw crossing one or more open block scopes is
    /// disposing the innermost one; the cursor was seeded with
    /// `Completion::Throw`, so it always finishes as a throw (the original
    /// exception, or a disposer's own error chained onto it), which becomes
    /// `pending_exception` and re-enters the driver's throw routing.
    ScopeCrossThrow,
    /// A `for-of` iteration's environment finished disposing; the `ForOfHead`
    /// state re-enters and finds `iteration_env` already cleared.
    ForOfIteration,
}

/// A function-level DisposeResources parked at one of its `Await`s.
pub(crate) struct PendingDispose {
    pub(crate) cursor: DisposeCursor,
    pub(crate) then: DisposeThen,
}

/// What an async generator does with the request at its front once the
/// disposal of its function-level resources finishes.
#[derive(Clone, Copy)]
pub(crate) enum GeneratorDisposeThen {
    /// Settle the request with the disposal's completion: reject with the
    /// (possibly chained) error on a throw, otherwise resolve
    /// `{ value, done: true }` (`undefined` when the body ran to its end).
    Settle,
    /// A `yield*` delegation that ended with a return completion whose value
    /// has not been Awaited by the unwinding: it is awaited after disposal, as
    /// for a generator without resources. (A `.return(v)` at a yield awaits `v`
    /// before the generator sees the return, so it settles with `Settle`.)
    ReturnAwait,
    /// A block scope left by a state transition finished disposing: the
    /// driver re-enters at the state it was about to run (a disposer's throw
    /// becomes an exception raised there).
    Reenter,
}

pub(crate) enum GeneratorDisposeState {
    /// Suspended at the `Await` of a `return` operand; the resources are still
    /// on the generator's function environment.
    ReturnOperand,
    Disposing {
        cursor: DisposeCursor,
        then: GeneratorDisposeThen,
    },
}

/// An async generator request parked at one of the `Await`s of its body's
/// DisposeResources. The request stays at the front of the generator's queue,
/// so a later request cannot start the generator early.
pub(crate) struct GeneratorDisposal {
    pub(crate) state: GeneratorDisposeState,
    pub(crate) promise: JsValue,
    pub(crate) resolve: JsValue,
    pub(crate) reject: JsValue,
}

impl GeneratorDisposal {
    pub(crate) fn new(
        state: GeneratorDisposeState,
        (promise, resolve, reject): (&JsValue, &JsValue, &JsValue),
    ) -> Self {
        Self {
            state,
            promise: promise.clone(),
            resolve: resolve.clone(),
            reject: reject.clone(),
        }
    }

    pub(crate) fn request(&self) -> (&JsValue, &JsValue, &JsValue) {
        (&self.promise, &self.resolve, &self.reject)
    }

    pub(crate) fn for_each_value(&self, mut f: impl FnMut(&JsValue)) {
        if let GeneratorDisposeState::Disposing { cursor, .. } = &self.state {
            cursor.for_each_value(&mut f);
        }
        f(&self.promise);
        f(&self.resolve);
        f(&self.reject);
    }
}

/// Outcome of starting a disposal that may suspend the async generator.
pub(crate) enum GeneratorDisposeStart {
    /// Disposal finished without an `Await` (or had nothing to dispose).
    Done(Completion),
    /// The request is parked; the driver must return without settling it.
    Parked,
}

/// A `disposeAsync()` call suspended at one of DisposeResources' `Await`s.
pub(crate) struct AsyncDisposal {
    pub(crate) cursor: DisposeCursor,
    pub(crate) promise: JsValue,
    pub(crate) resolve: JsValue,
    pub(crate) reject: JsValue,
}

impl AsyncDisposal {
    pub(crate) fn for_each_value(&self, mut f: impl FnMut(&JsValue)) {
        self.cursor.for_each_value(&mut f);
        f(&self.promise);
        f(&self.resolve);
        f(&self.reject);
    }
}

impl Interpreter {
    /// Takes `env`'s pending resources, or `None` when it has none (so no
    /// disposal, and no `Await`, is due).
    pub(crate) fn take_dispose_stack(&mut self, env: &EnvRef) -> Option<Vec<DisposableResource>> {
        env.borrow_mut()
            .dispose_stack
            .take()
            .filter(|stack| !stack.is_empty())
    }

    /// GetDisposeMethod for `async-dispose` falling back to `@@dispose`: the
    /// wrapper calls `method` and discards its result, so a promise returned
    /// by a synchronous disposer is never awaited.
    pub(crate) fn async_from_sync_dispose_method(&mut self, method: JsValue) -> JsValue {
        self.create_function(JsFunction::native(
            String::new(),
            0,
            move |interp, this, _args| match interp.call_function(&method, this, &[]) {
                Completion::Throw(e) => interp.create_rejected_promise(e),
                Completion::Exit(code) => Completion::Exit(code),
                _ => interp.create_resolved_promise(JsValue::UNDEFINED),
            },
        ))
    }

    /// Drive `cursor` to completion, draining the microtask queue inline at
    /// each `Await`. For callers that cannot suspend the running execution
    /// context.
    pub(crate) fn run_dispose_cursor_blocking(&mut self, cursor: DisposeCursor) -> Completion {
        self.run_dispose_cursor_holding(cursor, &[])
    }

    /// [`Self::run_dispose_cursor_blocking`] for a caller whose in-flight throw
    /// or return value (`held`) lives only in a Rust local. The jobs the drain
    /// runs may collect, so `held` and the cursor (between two `step`s) are
    /// rooted across it.
    pub(crate) fn run_dispose_cursor_holding(
        &mut self,
        mut cursor: DisposeCursor,
        held: &[Option<&JsValue>],
    ) -> Completion {
        let mut awaited = None;
        loop {
            match cursor.step(self, awaited.take()) {
                DisposeStep::Done(completion) => return completion,
                DisposeStep::Await(value) => {
                    let outcome = self.with_gc_root_scope(|interp| {
                        cursor.for_each_value(|v| interp.gc_root_value(v));
                        for v in held.iter().flatten() {
                            interp.gc_root_value(v);
                        }
                        interp.await_value(&value)
                    });
                    match outcome {
                        Completion::Normal(v) => awaited = Some(Ok(v)),
                        Completion::Throw(e) => awaited = Some(Err(e)),
                        // A job run by the drain called `__host_exit` (issue #242).
                        other => return other,
                    }
                }
            }
        }
    }

    /// Spec `Await(value)` for native code: `resume` runs in a later job with
    /// the fulfilment value or rejection reason.
    pub(crate) fn await_then(
        &mut self,
        value: &JsValue,
        resume: impl Fn(&mut Interpreter, Result<JsValue, JsValue>) -> Completion + 'static,
    ) {
        self.with_gc_root_scope(|interp| {
            // `promise_resolve_value` reads `value.constructor`, which can run
            // user code and collect.
            interp.gc_root_value(value);
            interp.schedule_await_resume(value, resume);
        });
    }

    fn schedule_await_resume(
        &mut self,
        value: &JsValue,
        resume: impl Fn(&mut Interpreter, Result<JsValue, JsValue>) -> Completion + 'static,
    ) {
        let resume = Rc::new(resume);
        let promise = self.promise_resolve_value(value);
        let promise_id = promise.as_object_id().unwrap_or_default();
        match self.get_promise_state(promise_id) {
            Some(PromiseState::Fulfilled(v)) => {
                self.scheduler.enqueue_microtask((
                    vec![v.clone()],
                    Box::new(move |interp| resume(interp, Ok(v))),
                ));
            }
            Some(PromiseState::Rejected(e)) => {
                self.scheduler.enqueue_microtask((
                    vec![e.clone()],
                    Box::new(move |interp| resume(interp, Err(e))),
                ));
            }
            Some(PromiseState::Pending) => {
                let on_fulfilled = {
                    let resume = resume.clone();
                    self.create_function(JsFunction::native(
                        "awaitFulfill".to_string(),
                        1,
                        move |interp, _this, args| {
                            let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                            resume(interp, Ok(v))
                        },
                    ))
                };
                let on_rejected = self.create_function(JsFunction::native(
                    "awaitReject".to_string(),
                    1,
                    move |interp, _this, args| {
                        let e = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        resume(interp, Err(e))
                    },
                ));
                self.perform_promise_then(
                    &promise,
                    &on_fulfilled,
                    &on_rejected,
                    JsValue::UNDEFINED,
                    JsValue::UNDEFINED,
                    JsValue::UNDEFINED,
                );
            }
            None => {
                let v = value.clone();
                self.scheduler.enqueue_microtask((
                    vec![v.clone()],
                    Box::new(move |interp| resume(interp, Ok(v))),
                ));
            }
        }
    }

    /// Advance the suspended `disposeAsync()` `id` and either park it at its
    /// next `Await` or settle its promise.
    pub(crate) fn async_disposal_step(
        &mut self,
        id: u64,
        awaited: Option<Result<JsValue, JsValue>>,
    ) -> Completion {
        let Some(mut disposal) = self.scheduler.remove_async_disposal(id) else {
            return Completion::Normal(JsValue::UNDEFINED);
        };
        let step = self.with_gc_root_scope(|interp| {
            disposal.for_each_value(|v| interp.gc_root_value(v));
            disposal.cursor.step(interp, awaited)
        });
        match step {
            DisposeStep::Await(value) => {
                self.scheduler.insert_async_disposal(id, disposal);
                self.await_then(&value, move |interp, outcome| {
                    interp.async_disposal_step(id, Some(outcome))
                });
                Completion::Normal(JsValue::UNDEFINED)
            }
            DisposeStep::Done(Completion::Exit(code)) => Completion::Exit(code),
            DisposeStep::Done(Completion::Throw(e)) => {
                let _ = self.call_function(&disposal.reject, &JsValue::UNDEFINED, &[e]);
                Completion::Normal(JsValue::UNDEFINED)
            }
            DisposeStep::Done(_) => {
                let _ = self.call_function(
                    &disposal.resolve,
                    &JsValue::UNDEFINED,
                    &[JsValue::UNDEFINED],
                );
                Completion::Normal(JsValue::UNDEFINED)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native_method(interp: &mut Interpreter, exit_code: Option<i32>) -> JsValue {
        interp.create_function(JsFunction::native(
            "disposer".to_string(),
            0,
            move |_interp, _this, _args| match exit_code {
                Some(code) => Completion::Exit(code),
                None => Completion::Normal(JsValue::UNDEFINED),
            },
        ))
    }

    fn resource(hint: DisposeHint, method: JsValue) -> DisposableResource {
        DisposableResource {
            value: JsValue::UNDEFINED,
            hint,
            dispose_method: method,
        }
    }

    /// Steps the cursor to completion feeding fulfilled awaits; returns the
    /// number of `Await` requests and the final completion.
    fn drive(interp: &mut Interpreter, stack: Vec<DisposableResource>) -> (usize, Completion) {
        let mut cursor = DisposeCursor::new(stack, Completion::Normal(JsValue::UNDEFINED));
        let mut awaits = 0;
        let mut awaited = None;
        loop {
            match cursor.step(interp, awaited.take()) {
                DisposeStep::Await(_) => {
                    awaits += 1;
                    awaited = Some(Ok(JsValue::UNDEFINED));
                }
                DisposeStep::Done(c) => return (awaits, c),
            }
        }
    }

    #[test]
    fn empty_stack_does_not_await() {
        let mut interp = Interpreter::new();
        let (awaits, c) = drive(&mut interp, Vec::new());
        assert_eq!(awaits, 0);
        assert!(matches!(c, Completion::Normal(_)));
    }

    #[test]
    fn null_resources_share_one_trailing_await() {
        let mut interp = Interpreter::new();
        for count in 1..=3 {
            let stack = (0..count)
                .map(|_| resource(DisposeHint::Async, JsValue::UNDEFINED))
                .collect();
            let (awaits, _) = drive(&mut interp, stack);
            assert_eq!(awaits, 1, "{count} null resources");
        }
    }

    #[test]
    fn async_disposer_awaits_once_and_suppresses_trailing_await() {
        let mut interp = Interpreter::new();
        let method = native_method(&mut interp, None);
        let stack = vec![
            resource(DisposeHint::Async, JsValue::UNDEFINED),
            resource(DisposeHint::Async, method.clone()),
        ];
        assert_eq!(drive(&mut interp, stack).0, 1);

        let stack = vec![
            resource(DisposeHint::Async, method),
            resource(DisposeHint::Async, JsValue::UNDEFINED),
        ];
        assert_eq!(drive(&mut interp, stack).0, 1);
    }

    #[test]
    fn sync_disposer_after_null_awaits_before_running() {
        let mut interp = Interpreter::new();
        let method = native_method(&mut interp, None);
        // Disposal order is reverse registration order: the null resource is
        // disposed first, then the synchronous one.
        let stack = vec![
            resource(DisposeHint::Sync, method),
            resource(DisposeHint::Async, JsValue::UNDEFINED),
        ];
        assert_eq!(drive(&mut interp, stack).0, 1);
    }

    #[test]
    fn rejected_async_disposer_becomes_throw_completion() {
        let mut interp = Interpreter::new();
        let method = native_method(&mut interp, None);
        let mut cursor = DisposeCursor::new(
            vec![resource(DisposeHint::Async, method)],
            Completion::Normal(JsValue::UNDEFINED),
        );
        assert!(matches!(
            cursor.step(&mut interp, None),
            DisposeStep::Await(_)
        ));
        let reason = JsValue::number(7.0);
        match cursor.step(&mut interp, Some(Err(reason))) {
            DisposeStep::Done(Completion::Throw(e)) => assert_eq!(e.as_number(), Some(7.0)),
            _ => panic!("expected a throw completion"),
        }
    }

    #[test]
    fn exit_completion_skips_every_disposer() {
        let mut interp = Interpreter::new();
        let method = native_method(&mut interp, None);
        let mut cursor = DisposeCursor::new(
            vec![resource(DisposeHint::Async, method)],
            Completion::Exit(3),
        );
        assert!(matches!(
            cursor.step(&mut interp, None),
            DisposeStep::Done(Completion::Exit(3))
        ));
    }

    #[test]
    fn disposer_exit_stops_disposal() {
        let mut interp = Interpreter::new();
        let exiting = native_method(&mut interp, Some(9));
        let later = native_method(&mut interp, None);
        let mut cursor = DisposeCursor::new(
            vec![
                resource(DisposeHint::Async, later),
                resource(DisposeHint::Async, exiting),
            ],
            Completion::Normal(JsValue::UNDEFINED),
        );
        assert!(matches!(
            cursor.step(&mut interp, None),
            DisposeStep::Done(Completion::Exit(9))
        ));
    }
}
