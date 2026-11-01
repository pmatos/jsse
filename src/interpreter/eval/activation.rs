//! Synchronous Realm scopes and user-function activations.

use super::*;

impl Interpreter {
    /// Runs in the selected Realm and restores the caller on every returned
    /// completion. `None` keeps the current Realm. A suspension ends the scope;
    /// its continuation must select its Realm again when resumed.
    #[inline]
    pub(crate) fn with_realm<R>(
        &mut self,
        target: Option<usize>,
        run: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let caller = self.current_realm_id;
        if let Some(target) = target {
            self.current_realm_id = target;
        }
        let result = run(self);
        self.current_realm_id = caller;
        result
    }

    /// Runs `run` as the activation of `func_obj_id` in `exec_env`, visible to
    /// `fn.arguments`/`fn.caller` and the collector; resources declared in the
    /// body are disposed on exit.
    #[inline(always)]
    pub(super) fn run_activation(
        &mut self,
        func_obj_id: u64,
        arguments: CallFrameArguments,
        exec_env: &EnvRef,
        arrow_this: Option<&JsValue>,
        run: impl FnOnce(&mut Self) -> Completion,
    ) -> Completion {
        let caller_pos = self.take_call_pos();
        self.call_stack_frames.push(CallFrame {
            func_obj_id,
            arguments,
            is_eval: false,
            caller_pos,
            env_index: self.call_stack_envs.len(),
        });
        let frame_depth = self.call_stack_frames.len();
        let noted = match arrow_this {
            Some(this) if self.node_host.stack.installed => {
                self.node_note_arrow_receiver(this);
                true
            }
            _ => false,
        };
        self.call_stack_envs.push(exec_env.clone());
        self.in_tail_position = false;
        let saved_tco_suppress = std::mem::take(&mut self.tco_suppress_depth);
        let result = run(self);
        self.tco_suppress_depth = saved_tco_suppress;
        if noted {
            self.node_host.stack.arrow_receivers.pop();
        }
        self.call_stack_envs.pop();
        debug_assert_eq!(self.call_stack_frames.len(), frame_depth);
        self.pop_call_frame();
        if exec_env.borrow().has_dispose_stack() {
            self.dispose_resources(exec_env, result)
        } else {
            result
        }
    }
    /// [[Call]] of an ordinary compiled function: not a generator or async
    /// function, simple parameters, no `arguments` binding, and not a
    /// construction (so no NewTarget and no derived-constructor `this`). It
    /// does what the general path of `call_function_inner_impl` does for such
    /// a function, without rebuilding the call's invariants from the function
    /// object.
    pub(super) fn call_compiled_function(
        &mut self,
        func_obj_id: u64,
        frame: &Rc<LiteralFrame>,
        closure: EnvRef,
        function_realm: Option<usize>,
        this_val: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        let call = frame
            .compiled_call()
            .expect("compiled_call_target checked the record");
        #[cfg(feature = "perf-counters")]
        {
            self.perf.calls_to_user += 1;
        }
        let (result, func_env, may_trim) = self.with_realm(function_realm, |interp| {
            let func_env =
                interp.acquire_function_environment(closure, call.params.len().saturating_add(2));
            let sloppy_record = {
                let mut env = func_env.borrow_mut();
                let closure_strict = env.strict;
                if call.is_arrow {
                    env.is_arrow_scope = true;
                } else {
                    let effective_this =
                        interp.ordinary_call_bind_this(this_val, call.is_strict, closure_strict);
                    env.bind_this_for_call(effective_this, JsValue::UNDEFINED);
                }
                Self::bind_simple_parameters(frame, &call.params, args, &mut env);
                env.strict = call.is_strict;
                !call.is_arrow && !call.is_strict && !closure_strict
            };
            let call_frame_arguments = if sloppy_record {
                Self::deferred_arguments_record(args, &func_env, true)
            } else {
                CallFrameArguments::None
            };
            let arrow_this = call.is_arrow.then_some(this_val);
            let result = interp.run_activation(
                func_obj_id,
                call_frame_arguments,
                &func_env,
                arrow_this,
                |interp| {
                    interp.run_compiled_body(func_obj_id, &call.chunk, &call.body, frame, &func_env)
                },
            );
            (result, func_env, !sloppy_record)
        });
        if may_trim && Rc::strong_count(&func_env) > 1 {
            env_trim::trim_exited_environment(&func_env, frame, &call.params, &call.body);
        }
        self.recycle_environment(func_env);
        self.finish_user_call(result)
    }

    /// [`call_compiled_function`] for a non-arrow function that mentions
    /// `arguments`: the arguments object is created (mapped for a sloppy
    /// function) and bound ahead of the parameters, and it is the activation's
    /// `fn.arguments` record. Kept out of line so the plain path stays as it was.
    #[inline(never)]
    pub(super) fn call_compiled_function_with_arguments(
        &mut self,
        func_obj_id: u64,
        frame: &Rc<LiteralFrame>,
        closure: EnvRef,
        function_realm: Option<usize>,
        this_val: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        let call = frame
            .compiled_call()
            .expect("compiled_call_target checked the record");
        #[cfg(feature = "perf-counters")]
        {
            self.perf.calls_to_user += 1;
        }
        let (result, func_env, may_trim) = self.with_realm(function_realm, |interp| {
            let func_env =
                interp.acquire_function_environment(closure, call.params.len().saturating_add(2));
            let closure_strict = {
                let mut env = func_env.borrow_mut();
                let closure_strict = env.strict;
                let effective_this =
                    interp.ordinary_call_bind_this(this_val, call.is_strict, closure_strict);
                env.bind_this_for_call(effective_this, JsValue::UNDEFINED);
                closure_strict
            };
            let mapped = !call.is_strict && !closure_strict;
            let arguments_obj = interp.create_arguments_object(
                args,
                JsValue::object(func_obj_id),
                mapped.then_some((&func_env, call.params.as_slice(), &**frame)),
            );
            interp.bind_arguments_binding(&func_env, arguments_obj.clone());
            {
                let mut env = func_env.borrow_mut();
                if call.is_strict {
                    env.arguments_immutable = true;
                }
                Self::bind_simple_parameters(frame, &call.params, args, &mut env);
                env.strict = call.is_strict;
            }
            let result = interp.run_activation(
                func_obj_id,
                CallFrameArguments::Materialized(arguments_obj),
                &func_env,
                None,
                |interp| {
                    interp.run_compiled_body(func_obj_id, &call.chunk, &call.body, frame, &func_env)
                },
            );
            (result, func_env, !mapped)
        });
        if may_trim && Rc::strong_count(&func_env) > 1 {
            env_trim::trim_exited_environment(&func_env, frame, &call.params, &call.body);
        }
        self.recycle_environment(func_env);
        self.finish_user_call(result)
    }

    /// [`call_compiled_function`] for a function whose parameters are plain
    /// names with defaults or a rest element: the parameters bind through the
    /// literal's atoms, and the body runs in the parameter environment when
    /// nothing can observe the difference (`LiteralFrame::shares_param_env`),
    /// else in a pooled body environment of its own. Kept out of line so the
    /// simple-parameter path stays as it was.
    #[inline(never)]
    pub(super) fn call_compiled_function_with_defaults(
        &mut self,
        func_obj_id: u64,
        frame: &Rc<LiteralFrame>,
        closure: EnvRef,
        function_realm: Option<usize>,
        this_val: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        let call = frame
            .compiled_call()
            .expect("compiled_call_target checked the record");
        #[cfg(feature = "perf-counters")]
        {
            self.perf.calls_to_user += 1;
        }
        let pending = self.with_realm(function_realm, |interp| {
            let func_env =
                interp.acquire_function_environment(closure, call.params.len().saturating_add(2));
            let sloppy_record = {
                let mut env = func_env.borrow_mut();
                let closure_strict = env.strict;
                if call.is_arrow {
                    env.is_arrow_scope = true;
                } else {
                    let effective_this =
                        interp.ordinary_call_bind_this(this_val, call.is_strict, closure_strict);
                    env.bind_this_for_call(effective_this, JsValue::UNDEFINED);
                }
                env.has_parameter_expressions = true;
                !call.is_arrow && !call.is_strict && !closure_strict
            };
            let plan = frame
                .named_param_plan(&call.params)
                .expect("a compiled call with parameter expressions has a named parameter plan");
            let bound = interp.node_in_async_frame(func_obj_id, &func_env, |interp| {
                interp.bind_named_parameters(plan, &call.params, args, &func_env)
            });
            if let Err(error) = bound {
                return Err((error, func_env));
            }
            let separate_body_env = !frame.shares_param_env(&call.params, &call.body);
            let exec_env = if separate_body_env {
                let body_env =
                    interp.acquire_body_environment(&func_env, &call.params, &call.body, frame);
                body_env.borrow_mut().strict = call.is_strict;
                body_env
            } else {
                func_env.borrow_mut().strict = call.is_strict;
                func_env.clone()
            };
            let call_frame_arguments = if sloppy_record {
                Self::deferred_arguments_record(args, &func_env, false)
            } else {
                CallFrameArguments::None
            };
            let arrow_this = call.is_arrow.then_some(this_val);
            let result = interp.run_activation(
                func_obj_id,
                call_frame_arguments,
                &exec_env,
                arrow_this,
                |interp| {
                    interp.run_compiled_body(func_obj_id, &call.chunk, &call.body, frame, &exec_env)
                },
            );
            Ok((result, func_env, exec_env, separate_body_env))
        });
        let (result, func_env, exec_env, separate_body_env) = match pending {
            Ok(call) => call,
            Err((error, func_env)) => {
                self.recycle_environment(func_env);
                return Completion::Throw(error);
            }
        };
        env_trim::trim_activation_scopes(
            &func_env,
            &exec_env,
            separate_body_env,
            frame,
            &call.params,
            &call.body,
        );
        if separate_body_env {
            self.recycle_environment(exec_env);
        } else {
            drop(exec_env);
        }
        self.recycle_environment(func_env);
        self.finish_user_call(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realm_scope_nests_and_returns_typed_results_after_restoration() {
        let mut interp = Interpreter::new();
        let caller = interp.current_realm_id;
        let first = interp.create_new_realm();
        let second = interp.create_new_realm();
        assert_eq!(interp.current_realm_id, caller);

        let result = interp.with_realm(Some(first), |interp| {
            assert_eq!(interp.current_realm_id, first);
            let result = interp.with_realm(Some(second), |interp| {
                assert_eq!(interp.current_realm_id, second);
                interp.with_realm(None, |interp| {
                    assert_eq!(interp.current_realm_id, second);
                    Err::<(), _>("nested failure")
                })?;
                Ok::<_, &str>(17)
            });
            assert_eq!(result, Err("nested failure"));
            assert_eq!(interp.current_realm_id, first);
            // Realm construction itself also temporarily enters a new Realm.
            let created = interp.create_new_realm();
            assert_eq!(interp.current_realm_id, first);
            (created, 23)
        });
        assert_eq!(result, (3, 23));
        assert_eq!(interp.current_realm_id, caller);
        assert_eq!(
            interp.with_realm(None, |interp| interp.current_realm_id),
            caller
        );
    }

    #[test]
    fn realm_scope_preserves_completions_and_ends_at_suspension() {
        let mut interp = Interpreter::new();
        let caller = interp.current_realm_id;
        let foreign = interp.create_new_realm();
        let value = JsValue::number(31.0);
        let completions = [
            Completion::Normal(value.clone()),
            Completion::Return(value.clone()),
            Completion::Throw(value.clone()),
            Completion::Yield(value.clone()),
            Completion::Break(Some("outer".into()), Some(value.clone())),
            Completion::Continue(None, None),
            Completion::TailCall {
                func: JsValue::UNDEFINED,
                this: JsValue::NULL,
                args: vec![value],
            },
            Completion::Exit(9),
            Completion::Empty,
        ];
        for completion in completions {
            let expected = format!("{completion:?}");
            let returned = interp.with_realm(Some(foreign), |interp| {
                assert_eq!(interp.current_realm_id, foreign);
                completion
            });
            assert_eq!(format!("{returned:?}"), expected);
            assert_eq!(interp.current_realm_id, caller);
        }
    }

    #[test]
    fn realm_scope_selects_error_intrinsics_without_leaking_into_caller() {
        let mut interp = Interpreter::new();
        let caller = interp.current_realm_id;
        let foreign = interp.create_new_realm();
        let foreign_error =
            interp.with_realm(Some(foreign), |interp| interp.create_type_error("foreign"));
        let local_error = interp.create_type_error("caller");
        for (error, realm) in [(foreign_error, foreign), (local_error, caller)] {
            assert_eq!(
                interp
                    .get_object_cell(error.as_object_id().unwrap())
                    .unwrap()
                    .borrow()
                    .prototype_id,
                interp.realms[realm].type_error_prototype
            );
        }
        assert_ne!(
            interp.realms[caller].type_error_prototype,
            interp.realms[foreign].type_error_prototype
        );
    }
}
