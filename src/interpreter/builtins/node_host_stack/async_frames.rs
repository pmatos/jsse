//! `at async` frames of `--node` stack traces (issue #1368). V8 appends them
//! when a stack is captured inside a promise reaction job: it follows the
//! promise the job settles (the result promise of the async function an
//! `await` resumes, else the reaction's derived promise) through pending
//! promises that have exactly one reaction, and prints a frame for each async
//! function parked at an `await` on the way. Nothing here runs a getter, a
//! `then` or any script, and nothing runs before the hooks are installed.

use super::combinator_frames::CombinatorHop;
use super::*;
use crate::interpreter::ModuleKey;

const FLAG_ASYNC: f64 = 4.0;
const MAX_HOPS: usize = 4096;

pub(crate) enum AsyncFrame {
    None,
    Module,
    Function { receiver_noted: bool },
}

/// An async arrow's call receiver, kept in its environment (where the
/// collector finds it) under a name no script can spell.
const RECEIVER_BINDING: &str = "%receiver";

/// One `at async` frame of the walk.
enum AsyncStep {
    Parked(ParkedAwait),
    Combinator(CombinatorHop),
}

/// An async function or module body parked at an `await`.
pub(crate) struct ParkedAwait {
    pub(crate) func: u64,
    pub(crate) receiver: Option<JsValue>,
    pub(crate) called_with: bool,
    pub(crate) pos: u32,
    pub(crate) result: Option<u64>,
    pub(crate) module: Option<ModuleKey>,
}

impl Interpreter {
    fn parked_await(&self, id: u64) -> Option<ParkedAwait> {
        if let Some(parked) = self.parked_hosted_await(id) {
            return Some(parked);
        }
        let state = self.scheduler.async_function_state(id)?;
        let kept = Self::kept_async_receiver(&state.func_env);
        Some(ParkedAwait {
            func: state.func_obj_id,
            called_with: kept.is_some(),
            receiver: kept.or_else(|| state.func_env.borrow().get_this()),
            pos: state.await_pos,
            result: match state.resolvers {
                PromiseResolvers::Internal(promise) => Some(promise),
                PromiseResolvers::Functions { .. } => None,
            },
            module: state.module_path.clone(),
        })
    }

    fn on_live_stack(&self, skip_top: usize, target: u64) -> bool {
        let mut frames = self.call_stack_frames.iter().rev().skip(skip_top);
        frames.any(|f| f.func_obj_id == target)
    }

    /// The frame an async state machine runs in while `--node` stacks are
    /// captured: a module body's top-level frame, or the function's own, which
    /// the bytecode VM's async activations have too (issue #1358).
    pub(crate) fn node_push_async_frame(&mut self, async_id: u64) -> AsyncFrame {
        if self.node_push_async_module_frame(async_id) {
            return AsyncFrame::Module;
        }
        if !self.node_host.stack.installed {
            return AsyncFrame::None;
        }
        let Some((func, env)) = self
            .scheduler
            .async_function_state(async_id)
            .map(|state| (state.func_obj_id, state.func_env.clone()))
            .filter(|(func, _)| *func != 0)
        else {
            return AsyncFrame::None;
        };
        self.push_function_frame(func, env)
    }

    fn push_function_frame(&mut self, func: u64, env: EnvRef) -> AsyncFrame {
        let receiver = Self::kept_async_receiver(&env);
        self.call_stack_envs.push(env);
        self.push_async_stack_frame(func);
        if let Some(receiver) = &receiver {
            self.node_note_arrow_receiver(receiver);
        }
        AsyncFrame::Function {
            receiver_noted: receiver.is_some(),
        }
    }

    /// Runs `run` (the parameter defaults of an async function) in the
    /// function's frame, as V8 does.
    pub(crate) fn node_in_async_frame<T>(
        &mut self,
        func: u64,
        env: &EnvRef,
        run: impl FnOnce(&mut Self) -> T,
    ) -> T {
        if !self.node_host.stack.installed {
            return run(self);
        }
        let frame = self.push_function_frame(func, env.clone());
        let result = run(self);
        self.node_pop_async_frame(frame);
        result
    }

    pub(crate) fn node_keep_async_receiver(&self, env: &EnvRef, receiver: Option<&JsValue>) {
        if let Some(receiver) = receiver.filter(|_| self.node_host.stack.installed) {
            let mut env = env.borrow_mut();
            env.declare(RECEIVER_BINDING, BindingKind::Var);
            env.initialize_binding(RECEIVER_BINDING, receiver.clone());
        }
    }

    fn kept_async_receiver(env: &EnvRef) -> Option<JsValue> {
        let env = env.borrow();
        Some(env.bindings.get(RECEIVER_BINDING)?.value.clone())
    }

    pub(crate) fn node_pop_async_frame(&mut self, frame: AsyncFrame) {
        match frame {
            AsyncFrame::None => {}
            AsyncFrame::Module => self.node_pop_module_frame(),
            AsyncFrame::Function { receiver_noted } => {
                if receiver_noted {
                    self.node_host.stack.arrow_receivers.pop();
                }
                self.pop_call_frame();
                self.call_stack_envs.pop();
            }
        }
    }

    pub(crate) fn node_note_await_pos(&mut self, async_id: u64, at: u32) {
        if !self.node_host.stack.installed {
            return;
        }
        if let Some(state) = self.scheduler.async_function_state_mut(async_id) {
            state.await_pos = at;
        }
    }

    /// A function parked at a disposer's `await` is where V8 last recorded a
    /// position: the latest call, else its last `await`.
    pub(crate) fn node_note_dispose_pos(&mut self, async_id: u64) {
        match self.node_host.call_pos {
            0 => {}
            at => self.node_note_await_pos(async_id, at),
        }
    }

    /// Runs `job` as the settlement of `promise` for `at async` frames, as V8's
    /// module evaluation job is one of the promise `import()` returned.
    pub(crate) fn node_with_job_promise<T>(
        &mut self,
        promise: &JsValue,
        job: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let promise = promise
            .as_object_id()
            .filter(|_| self.node_host.stack.installed);
        let saved = std::mem::replace(&mut self.node_host.stack.job_promise, promise);
        let result = job(self);
        self.node_host.stack.job_promise = saved;
        result
    }

    /// Runs a reaction job with the promise it settles noted for the capture of
    /// any stack made while it runs.
    pub(crate) fn node_run_reaction_job(
        &mut self,
        reaction: PromiseReaction,
        argument: JsValue,
    ) -> Completion {
        if !self.node_host.stack.installed {
            return self.run_promise_reaction_job(reaction, argument);
        }
        let promise = match &reaction.target {
            ReactionTarget::AsyncResume(id) => self.parked_await(*id).and_then(|p| p.result),
            ReactionTarget::AsyncGen(id, _) => self.generator_request_promise(*id),
            target => target.promise_id(),
        };
        let saved = std::mem::replace(&mut self.node_host.stack.job_promise, promise);
        let result = self.run_promise_reaction_job(reaction, argument);
        self.node_host.stack.job_promise = saved;
        result
    }

    /// `skip_until` is a `captureStackTrace` function that is not on the live
    /// stack: V8 drops async frames up to and including its own, if it has one.
    fn async_awaiters(&self, room: usize, mut skip_until: Option<u64>) -> Vec<AsyncStep> {
        let mut out = Vec::new();
        let mut next = self.node_host.stack.job_promise;
        for _ in 0..MAX_HOPS {
            let Some(promise) = next.take().filter(|_| out.len() < room) else {
                break;
            };
            let Some(object) = self.get_object(promise) else {
                break;
            };
            let object = object.borrow();
            let target = match object.promise_data() {
                Some(data)
                    if matches!(data.state, PromiseState::Pending)
                        && data.fulfill_reactions.len() == 1 =>
                {
                    if let Some(hop) = self.combinator_hop(data) {
                        next = hop.next;
                        if !hop.name.is_empty() && skip_until.is_none() {
                            out.push(AsyncStep::Combinator(hop));
                        }
                        continue;
                    }
                    let reaction = &data.fulfill_reactions[0];
                    match self.awaiting_generator(reaction) {
                        Some(id) => ReactionTarget::AsyncGen(id, AsyncGenStep::Await),
                        None => reaction.target.clone(),
                    }
                }
                _ => break,
            };
            match target {
                ReactionTarget::AsyncResume(id) => {
                    let Some(parked) = self.parked_await(id) else {
                        break;
                    };
                    next = parked.result;
                    match skip_until {
                        Some(target) if parked.func == target => skip_until = None,
                        Some(_) => {}
                        None => out.push(AsyncStep::Parked(parked)),
                    }
                }
                ReactionTarget::AsyncGen(_, AsyncGenStep::AwaitReturn) => break,
                ReactionTarget::AsyncGen(id, _) => {
                    let Some(parked) = self.parked_generator(id) else {
                        break;
                    };
                    next = parked.result;
                    match skip_until {
                        Some(target) if parked.func == target => skip_until = None,
                        Some(_) => {}
                        None => out.push(AsyncStep::Parked(parked)),
                    }
                }
                other => next = other.promise_id(),
            }
        }
        out
    }

    /// The flat frame records of a capture: the live call stack, then, inside a
    /// reaction job, the `await`ing async functions.
    pub(super) fn capture_records(
        &mut self,
        skip_top: usize,
        until: Option<Until>,
        limit: usize,
    ) -> Vec<JsValue> {
        let raws = self.raw_frames(skip_top, until, limit);
        let mut record = Vec::with_capacity(raws.len() * RECORD_LEN);
        for raw in &raws {
            self.frame_record(raw, &mut record);
        }
        let skip_until = match until {
            Some(Until::Seen(target)) if !self.on_live_stack(skip_top, target) => Some(target),
            _ => None,
        };
        if self.node_host.stack.job_promise.is_some() && raws.len() < limit {
            for step in self.async_awaiters(limit - raws.len(), skip_until) {
                match step {
                    AsyncStep::Parked(parked) => self.async_record(parked, &mut record),
                    AsyncStep::Combinator(hop) => Self::combinator_record(&hop, &mut record),
                }
            }
        }
        record
    }

    fn async_record(&mut self, parked: ParkedAwait, record: &mut Vec<JsValue>) {
        if let Some(key) = &parked.module {
            let origin = self.node_host.stack.module_origins.get(key).cloned();
            let source = origin.map_or(-1.0, |origin| {
                self.node_host.stack.source_id(&origin) as f64
            });
            record.extend([
                JsValue::UNDEFINED,
                JsValue::UNDEFINED,
                JsValue::number(source),
                JsValue::number(f64::from(parked.pos.max(1))),
                JsValue::number(FLAG_TOPLEVEL + FLAG_ASYNC),
                JsValue::UNDEFINED,
            ]);
            return;
        }
        let raw = RawFrame {
            func: parked.func,
            pos: parked.pos,
            receiver: parked.receiver,
            constructor: false,
            called_with: parked.called_with,
            native: false,
        };
        self.frame_record(&raw, record);
        let flags_at = record.len() - 2;
        let flags = record[flags_at].as_number().unwrap_or(0.0);
        record[flags_at] = JsValue::number(flags + FLAG_ASYNC);
    }

    fn combinator_record(hop: &CombinatorHop, record: &mut Vec<JsValue>) {
        record.extend([
            string_value(hop.name),
            string_value("Promise"),
            JsValue::number(-1.0),
            JsValue::number(hop.index as f64),
            JsValue::number(FLAG_ASYNC + FLAG_COMBINATOR),
            string_value(hop.name),
        ]);
    }
}
