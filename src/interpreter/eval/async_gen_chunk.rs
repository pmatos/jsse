//! Async generator bodies as suspending bytecode chunks (#1105).
//!
//! The body is one chunk that suspends at `yield` and `await` (`Op::Yield`,
//! told apart by `GeneratorFrame::suspended_at_await`); the compiler has
//! already put the awaits the spec asks for into it: the operand of a `yield`
//! and of `return expr`, and the value of a `return(v)` resumption (see
//! `compiler/suspend.rs`).
//!
//! The request queue is the scheduler's (`async_gen_queues`): the request being
//! served stays at its front until its promise settles, so a request enqueued
//! while the body runs or awaits only waits. [`Interpreter::async_gen_drain`]
//! serves the front request according to the generator's state
//! (AsyncGeneratorResume, AsyncGeneratorDrainQueue) and keeps going until the
//! queue is empty or the body parks at an `await`; the reaction of that
//! `await` (`ReactionTarget::AsyncGen(id, Await)`) re-enters it.

use super::function_level_binding_kind;
use super::generator_runtime::AwaitReturnStart;
use super::*;
use crate::interpreter::bytecode::vm::{self, GeneratorFrame, Resume};
use crate::interpreter::literal_frame::LiteralFrame;
use crate::interpreter::types::{
    BytecodeGenerator, BytecodeGeneratorCode, BytecodeGeneratorStatus as Status,
};

/// What a run of the body starts from.
enum Action {
    Start,
    Resume(Resume),
}

/// What the drain loop does with the front request.
enum Plan {
    Run(Action),
    /// The request was settled without running the body; look at the next.
    Again,
    Stop,
}

/// How a run of the body ended.
enum Outcome {
    /// Parked at an `await`; its reaction continues the request.
    Parked,
    Yielded(JsValue),
    Finished(Result<JsValue, JsValue>),
    Exit(i32),
}

/// A bytecode generator state that belongs to an async generator.
fn is_chunk_async(state: Option<&IteratorState>) -> bool {
    matches!(state, Some(IteratorState::BytecodeGenerator(generator)) if generator.is_async)
}

impl Interpreter {
    /// Whether `id` is an async generator, driven by either implementation.
    pub(super) fn is_async_generator_object(&self, id: u64) -> bool {
        self.get_object_cell(id).is_some_and(|cell| {
            matches!(
                cell.borrow().iterator_state(),
                Some(IteratorState::StateMachineAsyncGenerator { .. })
            ) || is_chunk_async(cell.borrow().iterator_state())
        })
    }

    /// Whether `id` is an async generator whose body is a chunk.
    pub(super) fn is_bytecode_async_generator(&self, id: u64) -> bool {
        self.get_object_cell(id)
            .is_some_and(|cell| is_chunk_async(cell.borrow().iterator_state()))
    }

    /// The state of an async generator object for `body` when it can run as a
    /// bytecode chunk (declarations instantiated into `exec_env` now, the body
    /// starts at the first `next()`); `Ok(None)` means the state machine.
    pub(super) fn bytecode_async_generator_state(
        &mut self,
        func_obj_id: u64,
        body: &Body,
        frame: &Rc<LiteralFrame>,
        exec_env: &EnvRef,
    ) -> Result<Option<IteratorState>, JsValue> {
        use crate::interpreter::bytecode::{BytecodeCacheState, compiler};
        if !self.bytecode_enabled {
            return Ok(None);
        }
        let BytecodeCacheState::Compiled(chunk) = self.memoised_bytecode_verdict(
            func_obj_id,
            body,
            compiler::compile_async_generator_body,
        ) else {
            return Ok(None);
        };
        self.instantiate_hosted_body(&chunk, body, frame, exec_env)?;
        Ok(Some(IteratorState::BytecodeGenerator(Box::new(
            BytecodeGenerator {
                code: Rc::new(BytecodeGeneratorCode {
                    chunk,
                    body: body.clone(),
                    frame: Rc::clone(frame),
                    func_env: exec_env.clone(),
                    #[cfg(feature = "perf-counters")]
                    perf_key: self.perf_body_name(func_obj_id),
                }),
                status: Status::SuspendedStart,
                frame: None,
                is_async: true,
            },
        ))))
    }

    /// Serves the requests of async generator `gen_id`'s queue, front first,
    /// until it is empty or the body parks at an `await`.
    pub(super) fn async_gen_drain(&mut self, gen_id: u64) {
        self.with_generator_realm(gen_id, |interp| interp.drain_loop(gen_id, None));
    }

    /// The continuation of a body parked at an `await`: `value` is the settled
    /// value, a reason when `is_reject`.
    pub(super) fn async_gen_await_resume_chunk(
        &mut self,
        gen_id: u64,
        value: JsValue,
        is_reject: bool,
    ) {
        let resume = if is_reject {
            Resume::Throw(value)
        } else {
            Resume::Next(value)
        };
        self.with_generator_realm(gen_id, |interp| {
            interp.drain_loop(gen_id, Some(Action::Resume(resume)));
        });
    }

    fn drain_loop(&mut self, gen_id: u64, mut resumed: Option<Action>) {
        loop {
            let Some(request) = self.front_request(gen_id) else {
                return;
            };
            let action = match resumed.take() {
                Some(action) => action,
                None => match self.plan_request(gen_id, &request) {
                    Plan::Run(action) => action,
                    Plan::Again => continue,
                    Plan::Stop => return,
                },
            };
            match self.run_step(gen_id, action) {
                Outcome::Parked => return,
                Outcome::Exit(code) => {
                    self.pending_exit = Some(code);
                    return;
                }
                Outcome::Yielded(value) => self.settle_front(gen_id, Ok((value, false))),
                Outcome::Finished(result) => {
                    self.settle_front(gen_id, result.map(|value| (value, true)));
                }
            }
        }
    }

    fn front_request(&self, gen_id: u64) -> Option<AsyncGenRequest> {
        self.scheduler.async_gen_queue(gen_id)?.front().cloned()
    }

    fn status_of(&self, gen_id: u64) -> Option<Status> {
        let cell = self.get_object_cell(gen_id)?;
        match cell.borrow().iterator_state() {
            Some(IteratorState::BytecodeGenerator(generator)) if generator.is_async => {
                Some(generator.status)
            }
            _ => None,
        }
    }

    /// What to do with the front request in the generator's current state
    /// (AsyncGeneratorResume / AsyncGeneratorDrainQueue).
    fn plan_request(&mut self, gen_id: u64, request: &AsyncGenRequest) -> Plan {
        let Some(status) = self.status_of(gen_id) else {
            return Plan::Stop;
        };
        let value = request.value.clone();
        match (status, &request.kind) {
            (Status::Executing, _) => Plan::Stop,
            (Status::SuspendedStart, AsyncGenRequestKind::Next) => Plan::Run(Action::Start),
            (Status::SuspendedYield, AsyncGenRequestKind::Next) => {
                Plan::Run(Action::Resume(Resume::Next(value)))
            }
            (Status::SuspendedYield, AsyncGenRequestKind::Throw) => {
                Plan::Run(Action::Resume(Resume::Throw(value)))
            }
            (Status::SuspendedYield, AsyncGenRequestKind::Return) => {
                Plan::Run(Action::Resume(Resume::Return(value)))
            }
            (Status::SuspendedStart, _) => {
                self.complete_generator(gen_id);
                Plan::Again
            }
            (Status::Completed, AsyncGenRequestKind::Next) => {
                self.settle_front(gen_id, Ok((JsValue::UNDEFINED, true)));
                Plan::Again
            }
            (Status::Completed, AsyncGenRequestKind::Throw) => {
                self.settle_front(gen_id, Err(value));
                Plan::Again
            }
            (Status::Completed, AsyncGenRequestKind::Return) => {
                match self.async_gen_await_return(gen_id, value, &request.promise) {
                    AwaitReturnStart::Parked => Plan::Stop,
                    AwaitReturnStart::Settled => {
                        self.pop_request(gen_id);
                        Plan::Again
                    }
                }
            }
        }
    }

    fn pop_request(&mut self, gen_id: u64) -> Option<AsyncGenRequest> {
        self.scheduler.async_gen_queue_mut(gen_id)?.pop_front()
    }

    /// AsyncGeneratorCompleteStep: retires the front request and settles its
    /// promise with `{ value, done }` or the reason. The generator's own state
    /// is already final, so what the settlement runs (a `then` read from the
    /// result object) can enqueue and be served.
    fn settle_front(&mut self, gen_id: u64, result: Result<(JsValue, bool), JsValue>) {
        let Some(request) = self.pop_request(gen_id) else {
            return;
        };
        let gc_frame = self.gc_root_frame();
        match result {
            Ok((value, done)) => {
                let iter_result = self.create_iter_result_object(value, done);
                self.gc_root_value(&iter_result);
                self.async_gen_resolve_request(&request.promise, iter_result);
            }
            Err(reason) => {
                self.gc_root_value(&reason);
                self.async_gen_reject_request(&request.promise, reason);
            }
        }
        self.gc_unroot_frame(gc_frame);
    }

    /// Latches the generator as completed and drops its frame.
    fn complete_generator(&mut self, gen_id: u64) {
        let frame = self.get_object_cell(gen_id).and_then(|cell| {
            match cell.borrow_mut().iterator_state_mut() {
                Some(IteratorState::BytecodeGenerator(generator)) if generator.is_async => {
                    generator.status = Status::Completed;
                    generator.frame.take()
                }
                _ => None,
            }
        });
        if let Some(frame) = frame {
            frame.release(self);
        }
    }

    /// Stores the frame back after a suspension.
    fn park_frame(&mut self, gen_id: u64, frame: Box<GeneratorFrame>, status: Status) {
        if let Some(cell) = self.get_object_cell(gen_id)
            && let Some(IteratorState::BytecodeGenerator(generator)) =
                cell.borrow_mut().iterator_state_mut()
            && generator.is_async
        {
            generator.status = status;
            generator.frame = Some(frame);
        }
    }

    /// Runs the body until its next suspension or its end.
    fn run_step(&mut self, gen_id: u64, action: Action) -> Outcome {
        let Some(cell) = self.get_object(gen_id) else {
            return Outcome::Finished(Ok(JsValue::UNDEFINED));
        };
        let (code, mut frame) = {
            let mut obj = cell.borrow_mut();
            let Some(IteratorState::BytecodeGenerator(generator)) = obj.iterator_state_mut() else {
                return Outcome::Finished(Ok(JsValue::UNDEFINED));
            };
            generator.status = Status::Executing;
            let frame = generator.frame.take().unwrap_or_else(|| {
                Box::new(GeneratorFrame::for_chunk(self, &generator.code.chunk))
            });
            (Rc::clone(&generator.code), frame)
        };
        let resume = match action {
            Action::Start => None,
            Action::Resume(resume) => Some(resume),
        };
        self.call_stack_envs.push(code.func_env.clone());
        let framed = self.node_enter_generator(gen_id);
        #[cfg(feature = "perf-counters")]
        {
            self.perf.body_non_function += 1;
            if !code.chunk.ast.is_empty() {
                let (name, key) = code.perf_key.clone();
                self.perf.enter_ast_body(name, key, false);
            }
        }
        let saved_tco = self.tco_suppress_depth;
        self.tco_suppress_depth = saved_tco + 1;
        let result = self.with_ic_body_in_frame(&code.body, &code.frame, |interp| {
            vm::run_generator_chunk(interp, &code.chunk, &code.func_env, &mut frame, resume)
        });
        self.tco_suppress_depth = saved_tco;
        #[cfg(feature = "perf-counters")]
        if !code.chunk.ast.is_empty() {
            let exclusive = self.perf.leave_ast_body();
            self.perf.ast_units_in_fallbacks += exclusive;
        }
        if matches!(result, Completion::Yield(_)) && frame.suspended_at_await(&code.chunk) {
            self.node_note_generator_pos_here(gen_id);
        }
        self.node_leave_generator(framed);
        self.call_stack_envs.pop();
        let finish = |interp: &mut Self, frame: Box<GeneratorFrame>| {
            interp.complete_generator(gen_id);
            frame.release(interp);
        };
        match result {
            Completion::Yield(value) if frame.suspended_at_await(&code.chunk) => {
                self.park_frame(gen_id, frame, Status::Executing);
                self.park_async_gen_at_await(gen_id, &value);
                Outcome::Parked
            }
            Completion::Yield(value) => {
                self.park_frame(gen_id, frame, Status::SuspendedYield);
                Outcome::Yielded(value)
            }
            Completion::Return(value) => {
                finish(self, frame);
                Outcome::Finished(Ok(value))
            }
            Completion::Normal(_) | Completion::Empty => {
                finish(self, frame);
                Outcome::Finished(Ok(JsValue::UNDEFINED))
            }
            Completion::Throw(error) => {
                finish(self, frame);
                Outcome::Finished(Err(error))
            }
            Completion::Exit(code) => {
                finish(self, frame);
                Outcome::Exit(code)
            }
            other => {
                debug_assert!(false, "an async generator body completed with {other:?}");
                finish(self, frame);
                Outcome::Finished(Ok(JsValue::UNDEFINED))
            }
        }
    }

    /// Registers the continuation of an `await` of `awaited`. The frame is
    /// already stored on the generator, so the collector traces it while
    /// PromiseResolve runs user code (`awaited.constructor`).
    fn park_async_gen_at_await(&mut self, gen_id: u64, awaited: &JsValue) {
        let gc_frame = self.gc_root_frame();
        self.gc_root_value(awaited);
        let promise = self.await_promise_resolve(awaited);
        let target = ReactionTarget::AsyncGen(gen_id, AsyncGenStep::Await);
        self.await_then_internal(promise, awaited, target);
        self.gc_unroot_frame(gc_frame);
    }

    /// The state-machine state of an async generator object: the one a body
    /// the compiler cannot host runs on.
    pub(super) fn state_machine_async_generator_state(
        &mut self,
        func_obj_id: u64,
        body: &Body,
        params: &[Pattern],
        func_env: &EnvRef,
        exec_env: EnvRef,
        is_strict: bool,
    ) -> IteratorState {
        let state_machine =
            self.state_machine_for(StateMachineKind::AsyncGenerator, body, params, func_obj_id);
        for temp_var in &state_machine.temp_vars {
            exec_env.borrow_mut().declare(temp_var, BindingKind::Var);
        }
        for lv in &state_machine.local_vars {
            let Some(bk) = function_level_binding_kind(lv) else {
                continue;
            };
            if !exec_env.borrow().bindings.contains_key(&lv.name)
                && !func_env.borrow().bindings.contains_key(&lv.name)
            {
                exec_env.borrow_mut().declare(&lv.name, bk);
            }
        }
        self.hoist_annexb_at_state_machine_entry(body, &exec_env);
        IteratorState::StateMachineAsyncGenerator {
            state_machine,
            func_env: exec_env,
            is_strict,
            execution_state: StateMachineExecutionState::SuspendedStart,
            _sent_value: JsValue::UNDEFINED,
            try_stack: vec![],
            pending_binding: None,
            delegated_iterator: None,
            pending_exception: None,
            pending_return: None,
        }
    }
}
