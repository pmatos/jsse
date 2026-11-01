//! Call frames of generator bodies in `--node` stacks (issue #1529). V8 shows
//! a generator's body as a frame of its function while it runs, and an async
//! generator parked at an `await` as an `at async gen` frame. Neither engine
//! gives a generator body a call frame of its own, so the hooks do: the
//! function is recorded in a hidden slot of the generator object when it is
//! made (the collector traces it there), the body's drivers push a frame
//! around every step, and the position of the suspension is kept in a second
//! slot. Nothing here runs unless the prelude installed the hooks.

use super::async_frames::ParkedAwait;
use super::*;

const GENERATOR_FUNCTION: PrivateId = PrivateId(u64::MAX - 8);
const GENERATOR_POSITION: PrivateId = PrivateId(u64::MAX - 9);
const GENERATOR_OWNER: PrivateId = PrivateId(u64::MAX - 10);

impl Interpreter {
    /// Records `func` as the function of generator object `gen_id`.
    pub(crate) fn node_keep_generator_function(&mut self, gen_id: u64, func: u64) {
        if !self.node_host.stack.installed {
            return;
        }
        let Some(handle) = self.get_object_cell(gen_id).cloned() else {
            return;
        };
        let value = JsValue::object(func);
        handle
            .borrow_mut()
            .set_hidden_slot(GENERATOR_FUNCTION, value.clone());
        self.gc_write_barrier_value(&handle, &value);
    }

    fn generator_function(&self, gen_id: u64) -> Option<u64> {
        let object = self.get_object(gen_id)?;
        let function = object.borrow().hidden_slot(GENERATOR_FUNCTION)?;
        function.as_object_id()
    }

    /// The environment a generator body runs in, whichever implementation
    /// drives it.
    fn generator_env(&self, gen_id: u64) -> Option<EnvRef> {
        let object = self.get_object(gen_id)?;
        let object = object.borrow();
        match object.iterator_state()? {
            IteratorState::BytecodeGenerator(generator) => Some(generator.code.func_env.clone()),
            IteratorState::StateMachineGenerator { func_env, .. }
            | IteratorState::StateMachineAsyncGenerator { func_env, .. } => Some(func_env.clone()),
            _ => None,
        }
    }

    /// Gives the step of generator `gen_id` that is about to run its frame,
    /// above the environment already on `call_stack_envs`. Pair it with
    /// [`Self::node_leave_generator`].
    pub(crate) fn node_enter_generator(&mut self, gen_id: u64) -> bool {
        if !self.node_host.stack.installed {
            return false;
        }
        match self.generator_function(gen_id) {
            Some(func) => self.push_async_stack_frame(func),
            None => false,
        }
    }

    pub(crate) fn node_leave_generator(&mut self, framed: bool) {
        if framed {
            self.pop_call_frame();
        }
    }

    /// Runs `step`, a step of a state-machine generator, in the generator's
    /// frame: its environment and frame sit on the stacks around the step. A
    /// step that re-enters the driver (a finished `yield*` resumes the body)
    /// is already in the frame.
    pub(crate) fn node_in_generator_frame<T>(
        &mut self,
        gen_id: u64,
        step: impl FnOnce(&mut Self) -> T,
    ) -> T {
        if !self.node_host.stack.installed {
            return step(self);
        }
        let Some(env) = self.generator_env(gen_id) else {
            return step(self);
        };
        let running = self.call_stack_frames.last().is_some_and(|frame| {
            self.call_stack_envs
                .get(frame.env_index)
                .is_some_and(|top| Rc::ptr_eq(top, &env))
        });
        if running {
            return step(self);
        }
        let envs_base = self.call_stack_envs.len();
        self.call_stack_envs.push(env);
        let framed = self.node_enter_generator(gen_id);
        let result = step(self);
        self.node_leave_generator(framed);
        self.call_stack_envs.truncate(envs_base);
        result
    }

    /// Sets the position the next call reports: a for-of head steps its
    /// iterator at the head's own (0: unknown, the function's start).
    #[inline]
    pub(crate) fn node_set_call_pos(&mut self, at: u32) {
        if self.node_host.stack.installed {
            self.node_host.call_pos = at;
        }
    }

    /// The position V8 reports while a sync `yield*` delegates: the call's own
    /// if the operand is one, else the `yield` keyword (`at`).
    pub(crate) fn delegate_site(operand: &Option<Expression>, at: u32) -> u32 {
        match operand {
            Some(Expression::Call(_, _, site)) => site.1,
            _ => at,
        }
    }

    /// An async generator reaching a `yield`: V8's last recorded position from
    /// here on, which a `yield*` delegation parks at and a disposal after the
    /// `yield` reports (issue #1548). An inline yield has none (0).
    pub(crate) fn node_note_yield_pos(&mut self, gen_id: u64, at: u32) {
        if at != 0 {
            self.node_note_generator_pos(gen_id, at);
        }
    }

    pub(crate) fn node_generator_pos(&self, gen_id: u64) -> u32 {
        if !self.node_host.stack.installed {
            return 0;
        }
        let slot = self
            .get_object(gen_id)
            .and_then(|object| object.borrow().hidden_slot(GENERATOR_POSITION));
        slot.and_then(|pos| pos.as_number()).unwrap_or(0.0) as u32
    }

    /// Notes where generator `gen_id` is about to park at an `await`.
    pub(crate) fn node_note_generator_pos(&mut self, gen_id: u64, at: u32) {
        if !self.node_host.stack.installed {
            return;
        }
        if let Some(object) = self.get_object(gen_id) {
            object
                .borrow_mut()
                .set_hidden_slot(GENERATOR_POSITION, JsValue::number(f64::from(at)));
        }
    }

    /// `node_note_generator_pos` for a suspension at the current call position,
    /// read before the generator's frame is left.
    pub(crate) fn node_note_generator_pos_here(&mut self, gen_id: u64) {
        let at = self.node_host.call_pos;
        self.node_note_generator_pos(gen_id, at);
    }

    /// The promise of the request an async generator is serving: what V8's
    /// walk continues from after the generator's own frame.
    pub(super) fn generator_request_promise(&self, gen_id: u64) -> Option<u64> {
        let queue = self.scheduler.async_gen_queue(gen_id)?;
        queue.front()?.promise.as_object_id()
    }

    /// An async generator parked at an `await`, for the `at async` walk.
    pub(super) fn parked_generator(&self, gen_id: u64) -> Option<ParkedAwait> {
        let func = self.generator_function(gen_id)?;
        let env = self.generator_env(gen_id)?;
        let pos = self.node_generator_pos(gen_id);
        let receiver = env.borrow().get_this();
        Some(ParkedAwait {
            func,
            receiver,
            called_with: false,
            pos,
            result: self.generator_request_promise(gen_id),
            module: None,
        })
    }

    /// How many reactions `awaited`, if a promise, has: taken before an
    /// `await_then` so the one it registers can be told from a foreign one.
    fn node_reaction_count(&self, awaited: &JsValue) -> usize {
        if !self.node_host.stack.installed {
            return 0;
        }
        awaited
            .as_object_id()
            .and_then(|id| self.get_object(id))
            .and_then(|promise| Some(promise.borrow().promise_data()?.fulfill_reactions.len()))
            .unwrap_or(0)
    }

    /// `await_then` for a closure-based `await` of async generator `gen_id`
    /// (`yield*`, a disposer's). The continuation registered on `awaited` is
    /// marked as the generator's, so the `at async` walk can see it, and the
    /// park keeps the generator's last noted position (the `yield*`, or the
    /// `yield` a disposal follows). PromiseResolve may have awaited another
    /// promise instead; then nothing is marked.
    pub(crate) fn await_then_in_generator(
        &mut self,
        awaited: &JsValue,
        gen_id: u64,
        resume: impl Fn(&mut Interpreter, Result<JsValue, JsValue>) -> Completion + 'static,
    ) {
        let before = self.node_reaction_count(awaited);
        self.await_then(awaited, resume);
        if !self.node_host.stack.installed {
            return;
        }
        if self.node_reaction_count(awaited) != before + 1 {
            return;
        }
        let handler = awaited
            .as_object_id()
            .and_then(|id| self.get_object(id))
            .and_then(|promise| {
                let promise = promise.borrow();
                promise
                    .promise_data()?
                    .fulfill_reactions
                    .last()?
                    .handler
                    .clone()
            })
            .and_then(|handler| self.get_object_cell(handler.as_object_id()?).cloned());
        if let Some(handler) = handler {
            let owner = JsValue::object(gen_id);
            handler
                .borrow_mut()
                .set_hidden_slot(GENERATOR_OWNER, owner.clone());
            self.gc_write_barrier_value(&handler, &owner);
        }
    }

    /// The async generator whose closure-based `await` `reaction` is.
    pub(super) fn awaiting_generator(&self, reaction: &PromiseReaction) -> Option<u64> {
        let handler = self.get_object(reaction.handler.as_ref()?.as_object_id()?)?;
        let owner = handler.borrow().hidden_slot(GENERATOR_OWNER)?;
        owner.as_object_id()
    }
}
