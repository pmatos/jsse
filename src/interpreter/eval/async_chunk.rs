//! Async function bodies as suspending bytecode chunks (#1105).
//!
//! The body is one chunk that suspends at each `await` (`Op::Yield` with no
//! stub, see `compiler/suspend.rs`). The activation lives on the Rust stack
//! while the body runs and in `Interpreter::async_activations` between an
//! `await` and its continuation, which `ReactionTarget::AsyncResume(id)`
//! delivers through `async_function_resume`: the fulfilment value is pushed
//! as the `await`'s result, a rejection is raised at the op. The promise
//! machinery (PromiseResolve, one reaction job per await, resolving the
//! result promise with the return value) is the state machine's own, so tick
//! counts are unchanged.

use super::*;
use crate::interpreter::builtins::node_host_stack::ParkedAwait;
use crate::interpreter::bytecode::chunk::Chunk;
use crate::interpreter::bytecode::vm::{self, GeneratorFrame, Resume};
use crate::interpreter::literal_frame::LiteralFrame;

/// A suspended or running async function body.
pub(crate) struct AsyncActivation {
    func_obj_id: u64,
    chunk: Rc<Chunk>,
    body: Body,
    frame_info: Rc<LiteralFrame>,
    /// The environment the chunk runs in: the function environment, or the
    /// body environment when the parameters are not simple.
    env: EnvRef,
    resolvers: PromiseResolvers,
    frame: GeneratorFrame,
    realm_id: usize,
    /// The call receiver of an async arrow, which its `--node` stack frame
    /// names even though the body's `this` is lexical.
    receiver: Option<JsValue>,
    /// The offset plus one of the `await` it is parked at (`--node` only).
    await_pos: u32,
    #[cfg(feature = "perf-counters")]
    perf_key: crate::interpreter::perf_counters::BodyKey,
}

impl Interpreter {
    /// The environment a hosted body runs in: the function environment, or a
    /// fresh body environment when the parameters are not simple (the
    /// parameters' own bindings stay in `func_env`, sec-functiondeclarationinstantiation
    /// step 28).
    pub(super) fn hosted_body_env(
        &mut self,
        func_env: &EnvRef,
        params: &[Pattern],
        body: &Body,
        has_simple_params: bool,
    ) -> EnvRef {
        if has_simple_params {
            return func_env.clone();
        }
        let body_env = Environment::new_function_scope(Some(func_env.clone()));
        body_env.borrow_mut().strict = func_env.borrow().strict;
        body_env.borrow_mut().has_simple_params = false;
        let analysis = self.hoist_cache.analysis_for(body);
        for name in &analysis.var_names {
            body_env.borrow_mut().declare(name, BindingKind::Var);
            if name == "arguments" || params.iter().any(|p| p.binds_name(name)) {
                let val = func_env.borrow().get(name).unwrap_or(JsValue::UNDEFINED);
                let _ = self.env_set(&body_env, name, val);
            }
        }
        body_env
    }

    /// Instantiates the declarations of a hosted body (a generator or async
    /// function chunk) into `env` before its first run.
    pub(super) fn instantiate_hosted_body(
        &mut self,
        chunk: &Chunk,
        body: &Body,
        frame: &Rc<LiteralFrame>,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        let Some(analysis) = self.chunk_instantiation_analysis(chunk, body, frame) else {
            vm::declare_chunk_prologue(chunk, env);
            return Ok(());
        };
        let failed = self.with_ic_body_in_frame(body, frame, |interp| {
            interp.instantiate_chunk_declarations(chunk, body, env, analysis)
        });
        match failed {
            Some(Completion::Throw(error)) => Err(error),
            Some(other) => {
                unreachable!("function declaration instantiation completed with {other:?}")
            }
            None => Ok(()),
        }
    }

    /// Keeps the innermost scope the state-machine driver is running in alive
    /// through the step: lowered scopes entered during a run reach the
    /// scheduler's saved state only at the next suspension, so a collection
    /// inside a call made by the step would otherwise free what only they hold.
    /// One `call_stack_envs` slot at `base`, replaced at each step and
    /// truncated when the resumption ends.
    #[inline(never)]
    pub(super) fn pin_driver_env(&mut self, base: usize, env: &EnvRef) {
        match self.call_stack_envs.get_mut(base) {
            Some(slot) => *slot = env.clone(),
            None => self.call_stack_envs.push(env.clone()),
        }
    }

    /// Runs an async function whose body compiles to a chunk, up to its first
    /// `await`. `None` leaves the call to the state machine. Otherwise the
    /// completion is the call's own: `Exit` when the body ran `__host_exit`,
    /// the result `promise` in every other case.
    pub(super) fn host_async_function(
        &mut self,
        func_obj_id: u64,
        params: &[Pattern],
        body: &Body,
        frame: &Rc<LiteralFrame>,
        func_env: &EnvRef,
        resolvers: &PromiseResolvers,
        promise: &JsValue,
        has_simple_params: bool,
        receiver: Option<&JsValue>,
    ) -> Option<Completion> {
        use crate::interpreter::bytecode::{BytecodeCacheState, compiler};
        if !self.bytecode_enabled {
            return None;
        }
        let BytecodeCacheState::Compiled(chunk) =
            self.memoised_bytecode_verdict(func_obj_id, body, compiler::compile_async_body)
        else {
            return None;
        };
        let env = self.hosted_body_env(func_env, params, body, has_simple_params);
        if let Err(error) = self.instantiate_hosted_body(&chunk, body, frame, &env) {
            self.promise_resolvers_reject(resolvers, error);
            return Some(Completion::Normal(promise.clone()));
        }
        let activation = AsyncActivation {
            func_obj_id,
            frame: GeneratorFrame::for_chunk(self, &chunk),
            chunk,
            body: body.clone(),
            frame_info: Rc::clone(frame),
            env,
            resolvers: resolvers.clone(),
            realm_id: self.current_realm_id,
            receiver: receiver.filter(|_| self.node_host.stack.installed).cloned(),
            await_pos: 0,
            #[cfg(feature = "perf-counters")]
            perf_key: self.perf_body_name(func_obj_id),
        };
        Some(match self.run_async_activation(activation, None, None) {
            exit @ Completion::Exit(_) => exit,
            _ => Completion::Normal(promise.clone()),
        })
    }

    /// The continuation of a hosted async function awaiting: `value` is the
    /// settled value, an error when `is_error`. `Normal(undefined)` when `id`
    /// names no hosted activation.
    pub(super) fn resume_hosted_async(
        &mut self,
        id: u64,
        value: JsValue,
        is_error: bool,
    ) -> Completion {
        let Some(activation) = self.async_activations.remove(&id) else {
            return Completion::Normal(JsValue::UNDEFINED);
        };
        let resume = if is_error {
            Resume::Throw(value)
        } else {
            Resume::Next(value)
        };
        self.run_async_activation(activation, Some(resume), Some(id))
    }

    /// Runs `activation` to its next `await` or to its end, settling the
    /// result promise at the end. `Exit` is the only completion handed back
    /// that matters to the caller.
    fn run_async_activation(
        &mut self,
        mut activation: AsyncActivation,
        resume: Option<Resume>,
        id: Option<u64>,
    ) -> Completion {
        let (result, await_pos, gc_frame) = self.with_realm(Some(activation.realm_id), |interp| {
            // The result promise lives only in this stack-held activation.
            let gc_frame = interp.gc_root_frame();
            activation
                .resolvers
                .for_each_value(|value| interp.gc_root_value(value));
            interp.call_stack_envs.push(activation.env.clone());
            let stack_frame = interp.push_async_stack_frame(activation.func_obj_id);
            let noted = match &activation.receiver {
                Some(receiver) if stack_frame => {
                    interp.node_note_arrow_receiver(receiver);
                    true
                }
                _ => false,
            };
            #[cfg(feature = "perf-counters")]
            {
                interp.perf.body_non_function += 1;
                if !activation.chunk.ast.is_empty() {
                    let (name, key) = activation.perf_key.clone();
                    interp.perf.enter_ast_body(name, key, false);
                }
            }
            // Hand-offs cannot tail-call: this driver does not trampoline.
            let saved_tco = interp.tco_suppress_depth;
            interp.tco_suppress_depth = saved_tco + 1;
            let result =
                interp.with_ic_body_in_frame(&activation.body, &activation.frame_info, |interp| {
                    vm::run_generator_chunk(
                        interp,
                        &activation.chunk,
                        &activation.env,
                        &mut activation.frame,
                        resume,
                    )
                });
            interp.tco_suppress_depth = saved_tco;
            let await_pos = interp.node_host.call_pos;
            #[cfg(feature = "perf-counters")]
            if !activation.chunk.ast.is_empty() {
                let exclusive = interp.perf.leave_ast_body();
                interp.perf.ast_units_in_fallbacks += exclusive;
            }
            if noted {
                interp.node_host.stack.arrow_receivers.pop();
            }
            if stack_frame {
                interp.pop_call_frame();
            }
            interp.call_stack_envs.pop();
            (result, await_pos, gc_frame)
        });
        let completion = match result {
            Completion::Yield(awaited) => {
                self.gc_unroot_frame(gc_frame);
                activation.await_pos = await_pos;
                self.park_at_await(activation, id, awaited);
                return Completion::Normal(JsValue::UNDEFINED);
            }
            Completion::Return(value) => self.settle_async(&activation, Ok(value)),
            Completion::Normal(_) | Completion::Empty => {
                self.settle_async(&activation, Ok(JsValue::UNDEFINED))
            }
            Completion::Throw(error) => self.settle_async(&activation, Err(error)),
            Completion::Exit(code) => Completion::Exit(code),
            other => {
                debug_assert!(false, "an async function body completed with {other:?}");
                self.settle_async(&activation, Ok(JsValue::UNDEFINED))
            }
        };
        self.gc_unroot_frame(gc_frame);
        activation.frame.release(self);
        completion
    }

    /// Resolves or rejects the result promise. `value` is rooted across the
    /// resolution, which may read `then` from it.
    fn settle_async(
        &mut self,
        activation: &AsyncActivation,
        outcome: Result<JsValue, JsValue>,
    ) -> Completion {
        let gc_frame = self.gc_root_frame();
        match outcome {
            Ok(value) => {
                self.gc_root_value(&value);
                self.promise_resolvers_resolve(&activation.resolvers, value);
            }
            Err(error) => {
                self.gc_root_value(&error);
                self.promise_resolvers_reject(&activation.resolvers, error);
            }
        }
        self.gc_unroot_frame(gc_frame);
        Completion::Normal(JsValue::UNDEFINED)
    }

    /// Parks the activation at an `await` of `awaited` and registers its
    /// continuation. The activation is stored first, so the collector traces
    /// it while PromiseResolve runs user code (`awaited.constructor`).
    fn park_at_await(&mut self, activation: AsyncActivation, id: Option<u64>, awaited: JsValue) {
        let id = id.unwrap_or_else(|| self.scheduler.alloc_async_function_id());
        self.async_activations.insert(id, activation);
        let gc_frame = self.gc_root_frame();
        self.gc_root_value(&awaited);
        let promise = self.await_promise_resolve(&awaited);
        self.await_then_internal(promise, &awaited, ReactionTarget::AsyncResume(id));
        self.gc_unroot_frame(gc_frame);
    }

    /// What `--node` stack traces show of the parked activation `id`.
    pub(crate) fn parked_hosted_await(&self, id: u64) -> Option<ParkedAwait> {
        let a = self.async_activations.get(&id)?;
        let called_with = a.receiver.is_some();
        Some(ParkedAwait {
            func: a.func_obj_id,
            receiver: a.receiver.clone().or_else(|| a.env.borrow().get_this()),
            called_with,
            pos: a.await_pos,
            result: match a.resolvers {
                PromiseResolvers::Internal(promise) => Some(promise),
                PromiseResolvers::Functions { .. } => None,
            },
            module: None,
        })
    }

    /// Roots of the parked async activations: the environments and operand
    /// values of each suspended frame, and its result promise.
    pub(in crate::interpreter) fn collect_async_activation_roots(
        &self,
        roots: &mut Vec<u64>,
        seen_envs: &mut crate::interpreter::types::EnvVisits,
    ) {
        for activation in self.async_activations.values() {
            Self::collect_env_roots(&activation.env, roots, seen_envs);
            activation
                .resolvers
                .for_each_value(|value| roots.extend(value.as_object_id()));
            roots.extend(
                activation
                    .frame
                    .stack
                    .iter()
                    .filter_map(JsValue::as_object_id),
            );
            for env in activation.frame.environments() {
                Self::collect_env_roots(env, roots, seen_envs);
            }
            roots.extend(activation.frame.with_objects());
            roots.extend(activation.receiver.iter().filter_map(JsValue::as_object_id));
        }
    }
}
