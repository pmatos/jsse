//! Runtime half of the generator / async-generator implementation: the state
//! machines that `generator_transform.rs` produces bodies for, plus the
//! async-generator request queue. Entry points are the `Generator` and
//! `AsyncGenerator` prototype methods wired up in `builtins/iterators.rs`.

use super::*;

impl Interpreter {
    pub(crate) fn generator_next(&mut self, this: &JsValue, sent_value: JsValue) -> Completion {
        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            let err = self.create_type_error("Generator.prototype.next called on non-object");
            return Completion::Throw(err);
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            let err = self.create_type_error("Generator.prototype.next called on non-object");
            return Completion::Throw(err);
        };

        // Extract state (must release borrow before executing body)
        let state = obj_rc.borrow().iterator_state().cloned();
        let Some(IteratorState::Generator {
            body,
            func_env,
            is_strict,
            execution_state,
        }) = state
        else {
            let err = self.create_type_error("not a generator object");
            return Completion::Throw(err);
        };

        // Determine target_yield and previous sent values based on execution state
        let (target_yield, prev_sent, is_suspended_start) = match &execution_state {
            GeneratorExecutionState::Completed => {
                return Completion::Normal(
                    self.create_iter_result_object(JsValue::UNDEFINED, true),
                );
            }
            GeneratorExecutionState::Executing => {
                return Completion::Throw(self.create_type_error("Generator is already executing"));
            }
            GeneratorExecutionState::SuspendedStart => (0, Vec::new(), true),
            GeneratorExecutionState::SuspendedYield {
                target_yield,
                prev_sent,
            } => (*target_yield, prev_sent.clone(), false),
        };

        // Build the full prev_sent_values for this call by appending the current sent_value.
        // prev_sent_values[k] = the value that yield k evaluates to when fast-forwarded.
        // Yield (target_yield-1) evaluates to the current sent_value (since we're resuming from it).
        // NOTE: For SuspendedStart (first call), sent_value is irrelevant (no yield to resume from).
        let mut new_prev_sent = prev_sent.clone();
        if !is_suspended_start {
            new_prev_sent.push(sent_value.clone());
        }

        // Mark as executing
        obj_rc.borrow_mut().kind =
            crate::interpreter::types::ObjectKind::Iterator(IteratorState::Generator {
                body: body.clone(),
                func_env: func_env.clone(),
                is_strict,
                execution_state: GeneratorExecutionState::Executing,
            });

        // Set generator context - for yield* delegation and sent values
        self.generator_context = Some(GeneratorContext {
            target_yield,
            current_yield: 0,
            prev_sent_values: new_prev_sent.clone(),
            is_async: false,
            resume_kind: GeneratorResumeKind::Next,
        });

        let caller_realm = self.current_realm_id;
        if let Some(gen_realm) = obj_rc.borrow().generator_realm_id {
            self.current_realm_id = gen_realm;
        }

        func_env.borrow_mut().strict = is_strict;
        self.call_stack_envs.push(func_env.clone());
        let result = self.exec_body(&body, &func_env);
        self.call_stack_envs.pop();
        let _ctx = self.generator_context.take();

        self.current_realm_id = caller_realm;
        match result {
            Completion::Yield(v) => {
                obj_rc.borrow_mut().kind =
                    crate::interpreter::types::ObjectKind::Iterator(IteratorState::Generator {
                        body: body.clone(),
                        func_env,
                        is_strict,
                        execution_state: GeneratorExecutionState::SuspendedYield {
                            target_yield: target_yield + 1,
                            prev_sent: new_prev_sent,
                        },
                    });
                Completion::Normal(self.create_iter_result_object(v, false))
            }
            Completion::Return(v) => {
                // §14.4.8: DisposeResources when generator completes
                let disp = self.dispose_resources(&func_env, Completion::Return(v));
                let v = match disp {
                    Completion::Return(v) => v,
                    Completion::Throw(e) => {
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::Generator {
                                body,
                                func_env,
                                is_strict,
                                execution_state: GeneratorExecutionState::Completed,
                            },
                        );
                        return Completion::Throw(e);
                    }
                    _ => JsValue::UNDEFINED,
                };
                obj_rc.borrow_mut().kind =
                    crate::interpreter::types::ObjectKind::Iterator(IteratorState::Generator {
                        body,
                        func_env,
                        is_strict,
                        execution_state: GeneratorExecutionState::Completed,
                    });
                Completion::Normal(self.create_iter_result_object(v, true))
            }
            Completion::Normal(_) | Completion::Empty => {
                // §14.4.8: DisposeResources when generator completes
                let disp =
                    self.dispose_resources(&func_env, Completion::Normal(JsValue::UNDEFINED));
                if let Completion::Throw(e) = disp {
                    obj_rc.borrow_mut().kind =
                        crate::interpreter::types::ObjectKind::Iterator(IteratorState::Generator {
                            body,
                            func_env,
                            is_strict,
                            execution_state: GeneratorExecutionState::Completed,
                        });
                    return Completion::Throw(e);
                }
                obj_rc.borrow_mut().kind =
                    crate::interpreter::types::ObjectKind::Iterator(IteratorState::Generator {
                        body,
                        func_env,
                        is_strict,
                        execution_state: GeneratorExecutionState::Completed,
                    });
                Completion::Normal(self.create_iter_result_object(JsValue::UNDEFINED, true))
            }
            Completion::Throw(e) => {
                let disp = self.dispose_resources(&func_env, Completion::Throw(e));
                obj_rc.borrow_mut().kind =
                    crate::interpreter::types::ObjectKind::Iterator(IteratorState::Generator {
                        body,
                        func_env,
                        is_strict,
                        execution_state: GeneratorExecutionState::Completed,
                    });
                match disp {
                    Completion::Throw(e) => Completion::Throw(e),
                    _ => {
                        Completion::Normal(self.create_iter_result_object(JsValue::UNDEFINED, true))
                    }
                }
            }
            other => other,
        }
    }

    pub(crate) fn generator_return(&mut self, this: &JsValue, value: JsValue) -> Completion {
        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            let err = self.create_type_error("Generator.prototype.return called on non-object");
            return Completion::Throw(err);
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            let err = self.create_type_error("Generator.prototype.return called on non-object");
            return Completion::Throw(err);
        };

        let state = obj_rc.borrow().iterator_state().cloned();
        let Some(IteratorState::Generator {
            body,
            func_env,
            is_strict,
            execution_state,
        }) = state
        else {
            return Completion::Throw(
                self.create_type_error(
                    "Generator.prototype.return called on incompatible receiver",
                ),
            );
        };

        match execution_state {
            GeneratorExecutionState::Completed => {
                Completion::Normal(self.create_iter_result_object(value, true))
            }
            GeneratorExecutionState::Executing => {
                Completion::Throw(self.create_type_error("Generator is already executing"))
            }
            GeneratorExecutionState::SuspendedStart => {
                obj_rc.borrow_mut().kind =
                    crate::interpreter::types::ObjectKind::Iterator(IteratorState::Generator {
                        body,
                        func_env,
                        is_strict,
                        execution_state: GeneratorExecutionState::Completed,
                    });
                Completion::Normal(self.create_iter_result_object(value, true))
            }
            GeneratorExecutionState::SuspendedYield {
                target_yield,
                prev_sent,
            } => {
                // target_yield in SuspendedYield is the NEXT yield index.
                // For return/throw, inject at the yield we were suspended at
                // (target_yield - 1), so fast-forward yields 0..target_yield-2.
                let inject_at = target_yield - 1;

                obj_rc.borrow_mut().kind =
                    crate::interpreter::types::ObjectKind::Iterator(IteratorState::Generator {
                        body: body.clone(),
                        func_env: func_env.clone(),
                        is_strict,
                        execution_state: GeneratorExecutionState::Executing,
                    });

                self.generator_context = Some(GeneratorContext {
                    target_yield: inject_at,
                    current_yield: 0,
                    prev_sent_values: prev_sent.clone(),
                    is_async: false,
                    resume_kind: GeneratorResumeKind::Return(value.clone()),
                });

                let caller_realm = self.current_realm_id;
                if let Some(gen_realm) = obj_rc.borrow().generator_realm_id {
                    self.current_realm_id = gen_realm;
                }

                func_env.borrow_mut().strict = is_strict;
                self.call_stack_envs.push(func_env.clone());
                let result = self.exec_body(&body, &func_env);
                self.call_stack_envs.pop();
                let _ctx = self.generator_context.take();

                self.current_realm_id = caller_realm;
                match result {
                    Completion::Yield(v) => {
                        // A yield in a finally block suspends the generator
                        let new_yield_index = inject_at + 1;
                        let mut new_prev_sent = prev_sent.clone();
                        // Pad prev_sent to cover the inject point
                        while new_prev_sent.len() < new_yield_index {
                            new_prev_sent.push(JsValue::UNDEFINED);
                        }
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::Generator {
                                body: body.clone(),
                                func_env,
                                is_strict,
                                execution_state: GeneratorExecutionState::SuspendedYield {
                                    target_yield: new_yield_index + 1,
                                    prev_sent: new_prev_sent,
                                },
                            },
                        );
                        Completion::Normal(self.create_iter_result_object(v, false))
                    }
                    Completion::Return(v) => {
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::Generator {
                                body,
                                func_env,
                                is_strict,
                                execution_state: GeneratorExecutionState::Completed,
                            },
                        );
                        Completion::Normal(self.create_iter_result_object(v, true))
                    }
                    Completion::Normal(_) | Completion::Empty => {
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::Generator {
                                body,
                                func_env,
                                is_strict,
                                execution_state: GeneratorExecutionState::Completed,
                            },
                        );
                        Completion::Normal(self.create_iter_result_object(JsValue::UNDEFINED, true))
                    }
                    Completion::Throw(e) => {
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::Generator {
                                body,
                                func_env,
                                is_strict,
                                execution_state: GeneratorExecutionState::Completed,
                            },
                        );
                        Completion::Throw(e)
                    }
                    other => other,
                }
            }
        }
    }

    pub(crate) fn generator_throw(&mut self, this: &JsValue, exception: JsValue) -> Completion {
        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            let err = self.create_type_error("Generator.prototype.throw called on non-object");
            return Completion::Throw(err);
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            let err = self.create_type_error("Generator.prototype.throw called on non-object");
            return Completion::Throw(err);
        };

        let state = obj_rc.borrow().iterator_state().cloned();
        let Some(IteratorState::Generator {
            body,
            func_env,
            is_strict,
            execution_state,
        }) = state
        else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.throw called on incompatible receiver"),
            );
        };

        match execution_state {
            GeneratorExecutionState::Completed | GeneratorExecutionState::SuspendedStart => {
                obj_rc.borrow_mut().kind =
                    crate::interpreter::types::ObjectKind::Iterator(IteratorState::Generator {
                        body,
                        func_env,
                        is_strict,
                        execution_state: GeneratorExecutionState::Completed,
                    });
                Completion::Throw(exception)
            }
            GeneratorExecutionState::Executing => {
                Completion::Throw(self.create_type_error("Generator is already executing"))
            }
            GeneratorExecutionState::SuspendedYield {
                target_yield,
                prev_sent,
            } => {
                let inject_at = target_yield - 1;

                obj_rc.borrow_mut().kind =
                    crate::interpreter::types::ObjectKind::Iterator(IteratorState::Generator {
                        body: body.clone(),
                        func_env: func_env.clone(),
                        is_strict,
                        execution_state: GeneratorExecutionState::Executing,
                    });

                self.generator_context = Some(GeneratorContext {
                    target_yield: inject_at,
                    current_yield: 0,
                    prev_sent_values: prev_sent.clone(),
                    is_async: false,
                    resume_kind: GeneratorResumeKind::Throw(exception),
                });

                let caller_realm = self.current_realm_id;
                if let Some(gen_realm) = obj_rc.borrow().generator_realm_id {
                    self.current_realm_id = gen_realm;
                }

                func_env.borrow_mut().strict = is_strict;
                self.call_stack_envs.push(func_env.clone());
                let result = self.exec_body(&body, &func_env);
                self.call_stack_envs.pop();
                let _ctx = self.generator_context.take();

                self.current_realm_id = caller_realm;
                match result {
                    Completion::Yield(v) => {
                        let new_yield_index = inject_at + 1;
                        let mut new_prev_sent = prev_sent.clone();
                        while new_prev_sent.len() < new_yield_index {
                            new_prev_sent.push(JsValue::UNDEFINED);
                        }
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::Generator {
                                body: body.clone(),
                                func_env,
                                is_strict,
                                execution_state: GeneratorExecutionState::SuspendedYield {
                                    target_yield: new_yield_index + 1,
                                    prev_sent: new_prev_sent,
                                },
                            },
                        );
                        Completion::Normal(self.create_iter_result_object(v, false))
                    }
                    Completion::Return(v) => {
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::Generator {
                                body,
                                func_env,
                                is_strict,
                                execution_state: GeneratorExecutionState::Completed,
                            },
                        );
                        Completion::Normal(self.create_iter_result_object(v, true))
                    }
                    Completion::Normal(_) | Completion::Empty => {
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::Generator {
                                body,
                                func_env,
                                is_strict,
                                execution_state: GeneratorExecutionState::Completed,
                            },
                        );
                        Completion::Normal(self.create_iter_result_object(JsValue::UNDEFINED, true))
                    }
                    Completion::Throw(e) => {
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::Generator {
                                body,
                                func_env,
                                is_strict,
                                execution_state: GeneratorExecutionState::Completed,
                            },
                        );
                        Completion::Throw(e)
                    }
                    other => other,
                }
            }
        }
    }

    pub(crate) fn generator_next_state_machine(
        &mut self,
        this: &JsValue,
        sent_value: JsValue,
    ) -> Completion {
        let caller_realm = self.current_realm_id;
        if let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(obj_rc) = self.get_object(o.id)
            && let Some(realm_id) = obj_rc.borrow().generator_realm_id
        {
            self.current_realm_id = realm_id;
        }
        let result = self.generator_next_state_machine_impl(this, sent_value);
        self.current_realm_id = caller_realm;
        result
    }

    fn generator_next_state_machine_impl(
        &mut self,
        this: &JsValue,
        sent_value: JsValue,
    ) -> Completion {
        use crate::interpreter::generator_transform::{LoopControlTarget, StateTerminator};

        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.next called on non-object"),
            );
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.next called on non-object"),
            );
        };

        let state = obj_rc.borrow().iterator_state().cloned();
        let Some(IteratorState::StateMachineGenerator {
            state_machine,
            func_env,
            is_strict,
            execution_state,
            try_stack,
            pending_binding,
            delegated_iterator,
            pending_exception: stored_pending_exception,
            pending_return: stored_pending_return,
            ..
        }) = state
        else {
            return Completion::Throw(self.create_type_error("not a state machine generator"));
        };

        if let Some(ref deleg_info) = delegated_iterator {
            let iterator = deleg_info.iterator.clone();
            let next_method = deleg_info.next_method.clone();
            let resume_state = deleg_info.resume_state;
            let binding = deleg_info.sent_value_binding.clone();

            let result = match self.call_function(
                &next_method,
                &iterator,
                std::slice::from_ref(&sent_value),
            ) {
                Completion::Normal(v) if (v).is_object() => Ok(v),
                Completion::Normal(_) => {
                    Err(self.create_type_error("Iterator result is not an object"))
                }
                Completion::Throw(e) => Err(e),
                _ => Err(self.create_type_error("Iterator next failed")),
            };
            match result {
                Ok(iter_result) => {
                    let done = match self.iterator_complete(&iter_result) {
                        Ok(d) => d,
                        Err(e) => return Completion::Throw(e),
                    };
                    if done {
                        let value = match self.iterator_value(&iter_result) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        if let Some(ref bind) = binding {
                            use crate::interpreter::generator_transform::SentValueBindingKind;
                            match &bind.kind {
                                SentValueBindingKind::Variable(name) => {
                                    let mut env = func_env.borrow_mut();
                                    let needs_init = env
                                        .bindings
                                        .get(name.as_str())
                                        .is_some_and(|b| !b.initialized);
                                    if needs_init {
                                        env.initialize_binding(name, value.clone());
                                    } else {
                                        env.set(name, value.clone()).ok();
                                    }
                                }
                                SentValueBindingKind::Pattern(pattern) => {
                                    let _ = self.bind_pattern(
                                        pattern,
                                        value.clone(),
                                        BindingKind::Var,
                                        &func_env,
                                    );
                                }
                                SentValueBindingKind::Discard
                                | SentValueBindingKind::InlineYield { .. } => {}
                            }
                        }
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        return self.generator_next_state_machine(this, JsValue::UNDEFINED);
                    } else {
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::UNDEFINED,
                                try_stack,
                                pending_binding: None,
                                delegated_iterator: Some(
                                    crate::interpreter::types::DelegatedIteratorInfo {
                                        iterator,
                                        next_method: next_method.clone(),
                                        resume_state,
                                        sent_value_binding: binding,
                                    },
                                ),
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        // Per spec §14.4.14: yield innerResult directly
                        return Completion::Normal(iter_result);
                    }
                }
                Err(e) => {
                    // Clear delegation and propagate error through generator's
                    // try-stack so the generator's own catch/finally can handle it
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineGenerator {
                            state_machine: state_machine.clone(),
                            func_env: func_env.clone(),
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: resume_state,
                            },
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: try_stack.clone(),
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    return self.generator_throw_state_machine(this, e);
                }
            }
        }

        let current_state_id = match &execution_state {
            StateMachineExecutionState::Completed => {
                return Completion::Normal(
                    self.create_iter_result_object(JsValue::UNDEFINED, true),
                );
            }
            StateMachineExecutionState::Executing => {
                return Completion::Throw(self.create_type_error("Generator is already executing"));
            }
            StateMachineExecutionState::SuspendedStart => 0,
            StateMachineExecutionState::SuspendedAtState { state_id } => *state_id,
        };

        obj_rc.borrow_mut().kind =
            crate::interpreter::types::ObjectKind::Iterator(IteratorState::StateMachineGenerator {
                state_machine: state_machine.clone(),
                func_env: func_env.clone(),
                is_strict,
                execution_state: StateMachineExecutionState::Executing,
                _sent_value: sent_value.clone(),
                try_stack: try_stack.clone(),
                pending_binding: None,
                delegated_iterator: None,
                pending_exception: None,
                pending_return: None,
            });

        use crate::interpreter::generator_transform::SentValueBindingKind;
        let mut initial_inline_yield_target: Option<usize> = None;
        let mut initial_inline_yield_sent: Option<JsValue> = None;
        let mut initial_inline_yield_prev_sent: Option<Vec<JsValue>> = None;
        if let Some(binding) = pending_binding {
            match &binding.kind {
                SentValueBindingKind::Variable(name) => {
                    let mut env = func_env.borrow_mut();
                    let needs_init = env
                        .bindings
                        .get(name.as_str())
                        .is_some_and(|b| !b.initialized);
                    if needs_init {
                        env.initialize_binding(name, sent_value.clone());
                    } else {
                        env.set(name, sent_value.clone()).ok();
                    }
                }
                SentValueBindingKind::Pattern(pattern) => {
                    let _ =
                        self.bind_pattern(pattern, sent_value.clone(), BindingKind::Var, &func_env);
                }
                SentValueBindingKind::Discard => {}
                SentValueBindingKind::InlineYield {
                    yield_target,
                    prev_sent,
                } => {
                    initial_inline_yield_target = Some(*yield_target);
                    initial_inline_yield_sent = Some(sent_value.clone());
                    let mut new_prev = prev_sent.clone();
                    new_prev.push(sent_value.clone());
                    initial_inline_yield_prev_sent = Some(new_prev);
                }
            }
        }

        func_env.borrow_mut().strict = is_strict;
        let saved_in_state_machine = self.in_state_machine;
        self.in_state_machine = true;
        let mut current_id = current_state_id;
        let mut current_try_stack = try_stack;
        let mut pending_exception: Option<JsValue> = stored_pending_exception;
        let mut pending_return: Option<JsValue> = stored_pending_return;
        let mut inline_yield_target: Option<usize> = initial_inline_yield_target;
        let mut inline_yield_sent: Option<JsValue> = initial_inline_yield_sent;
        let mut inline_yield_prev_sent: Option<Vec<JsValue>> = initial_inline_yield_prev_sent;
        let mut for_of_stack = self
            .generator_for_of_stacks
            .get(&o.id)
            .cloned()
            .unwrap_or_default();

        loop {
            let terminator = state_machine.states[current_id].terminator.clone();

            let is_inline_replay = inline_yield_target.is_some();
            if let Some(target) = inline_yield_target.take() {
                let _sv = inline_yield_sent.take().unwrap_or(JsValue::UNDEFINED);
                let prev = inline_yield_prev_sent.take().unwrap_or_default();
                self.generator_context = Some(GeneratorContext {
                    target_yield: target,
                    current_yield: 0,
                    prev_sent_values: prev,
                    is_async: false,
                    resume_kind: GeneratorResumeKind::Next,
                });
            }

            self.in_state_machine = true;
            let term_env = for_of_stack
                .last()
                .map_or(&func_env, ForOfLoopState::effective_env)
                .clone();
            let mut stmt_result = self.exec_body(&state_machine.states[current_id].body, &term_env);
            self.in_state_machine = saved_in_state_machine;
            while let Completion::TailCall { func, this, args } = stmt_result {
                stmt_result = self.call_function(&func, &this, &args);
            }
            let ctx_after = if is_inline_replay {
                self.generator_context.take()
            } else {
                None
            };

            if let Completion::Yield(yield_val) = stmt_result {
                self.destructuring_yield = false;
                let yield_count = ctx_after.as_ref().map(|c| c.current_yield).unwrap_or(1);
                let inline_prev = ctx_after.map(|c| c.prev_sent_values).unwrap_or_default();
                // Save any iterators that need IteratorClose if generator.return() is called
                let pending = std::mem::take(&mut self.pending_iter_close);
                if pending.is_empty() {
                    self.generator_inline_iters.remove(&o.id);
                } else {
                    self.generator_inline_iters.insert(o.id, pending);
                }
                self.sync_generator_for_of_stack(o.id, &for_of_stack);
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineGenerator {
                        state_machine: state_machine.clone(),
                        func_env: func_env.clone(),
                        is_strict,
                        execution_state: StateMachineExecutionState::SuspendedAtState {
                            state_id: current_id,
                        },
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: current_try_stack.clone(),
                        pending_binding: Some(
                            crate::interpreter::generator_transform::SentValueBinding {
                                kind: SentValueBindingKind::InlineYield {
                                    yield_target: yield_count,
                                    prev_sent: inline_prev,
                                },
                            },
                        ),
                        delegated_iterator: None,
                        pending_exception: pending_exception.take(),
                        pending_return: pending_return.take(),
                    },
                );
                return Completion::Normal(self.create_iter_result_object(yield_val, false));
            }
            if let Completion::Exit(code) = stmt_result {
                // `__host_exit` (issue #242) is uncatchable and immediate:
                // complete the generator without routing through its
                // catch/finally states or disposing, and propagate the exit.
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    },
                );
                self.generator_inline_iters.remove(&o.id);
                return Completion::Exit(code);
            }
            if let Completion::Throw(e) = stmt_result {
                // Route genuine throws through the generator body's
                // catch/finally states (a `Completion::Exit` was handled
                // above and never reaches here).
                if let Some(try_info) = current_try_stack.pop() {
                    if let Some(catch_state) = try_info.catch_state {
                        pending_exception = Some(e);
                        current_id = catch_state;
                        continue;
                    } else if let Some(finally_state) = try_info.finally_state {
                        current_id = finally_state;
                        continue;
                    }
                }
                // §27.5.3.3: DisposeResources when generator throws
                let disp = self.dispose_resources(&func_env, Completion::Throw(e));
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    },
                );
                self.generator_inline_iters.remove(&o.id);
                return disp;
            }
            if let Completion::Return(v) = stmt_result {
                // Check try_stack for enclosing finally blocks before completing
                let mut ret_val_opt = Some(v);
                for i in (0..current_try_stack.len()).rev() {
                    if !current_try_stack[i].entered_finally
                        && current_try_stack[i].finally_state.is_some()
                    {
                        pending_return = ret_val_opt.take();
                        let finally_state = current_try_stack[i].finally_state.unwrap();
                        current_try_stack = current_try_stack[..=i].to_vec();
                        current_id = finally_state;
                        break;
                    }
                }
                if ret_val_opt.is_none() {
                    continue;
                }
                let v = ret_val_opt.unwrap();
                // §27.5.3.3: DisposeResources when generator returns
                let disp = self.dispose_resources(&func_env, Completion::Return(v));
                let ret_val = match disp {
                    Completion::Return(v) => v,
                    Completion::Throw(e) => {
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        self.generator_inline_iters.remove(&o.id);
                        return Completion::Throw(e);
                    }
                    _ => JsValue::UNDEFINED,
                };
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    },
                );
                self.generator_inline_iters.remove(&o.id);
                return Completion::Normal(self.create_iter_result_object(ret_val, true));
            }

            match &terminator {
                StateTerminator::Yield {
                    value,
                    is_delegate,
                    resume_state,
                    sent_value_binding,
                } => {
                    let yield_val = if let Some(expr) = value {
                        let mut _result = self.eval_expr(expr, &term_env);
                        while let Completion::TailCall { func, this, args } = _result {
                            _result = self.call_function(&func, &this, &args);
                        }
                        match _result {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                // Route genuine throws through the try-stack for
                                // catch/finally handling (a `Completion::Exit`
                                // takes the `other` arm below and never reaches
                                // here — issue #242).
                                if let Some(try_info) = current_try_stack.pop() {
                                    if let Some(catch_state) = try_info.catch_state {
                                        pending_exception = Some(e);
                                        current_id = catch_state;
                                        continue;
                                    } else if let Some(finally_state) = try_info.finally_state {
                                        current_id = finally_state;
                                        continue;
                                    }
                                }
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                return Completion::Throw(e);
                            }
                            other => return other,
                        }
                    } else {
                        JsValue::UNDEFINED
                    };

                    if *is_delegate {
                        let iterator = match self.get_iterator(&yield_val) {
                            Ok(it) => it,
                            Err(e) => {
                                if let Some(try_info) = current_try_stack.last()
                                    && let Some(catch_state) = try_info.catch_state
                                {
                                    let new_try_stack =
                                        current_try_stack[..current_try_stack.len() - 1].to_vec();
                                    obj_rc.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::StateMachineGenerator {
                                                state_machine: state_machine.clone(),
                                                func_env: func_env.clone(),
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::SuspendedAtState {
                                                        state_id: catch_state,
                                                    },
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: new_try_stack,
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: Some(e),
                                                pending_return: None,
                                            },
                                        );
                                    return self
                                        .generator_next_state_machine(this, JsValue::UNDEFINED);
                                }
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                return Completion::Throw(e);
                            }
                        };

                        let next_method = if let Some(io) = iterator
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                        {
                            if let Some(cached) = self.iterator_next_cache.get(&io.id).cloned() {
                                cached
                            } else {
                                match self.get_object_property(io.id, "next", &iterator) {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => {
                                        // Route through try-stack
                                        if let Some(try_info) = current_try_stack.last()
                                            && let Some(catch_state) = try_info.catch_state
                                        {
                                            let new_try_stack = current_try_stack
                                                [..current_try_stack.len() - 1]
                                                .to_vec();
                                            obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(IteratorState::StateMachineGenerator {
                                                state_machine: state_machine.clone(),
                                                func_env: func_env.clone(),
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::SuspendedAtState {
                                                        state_id: catch_state,
                                                    },
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: new_try_stack,
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: Some(e),
                                                pending_return: None,
                                            });
                                            return self.generator_next_state_machine(
                                                this,
                                                JsValue::UNDEFINED,
                                            );
                                        }
                                        return Completion::Throw(e);
                                    }
                                    _ => JsValue::UNDEFINED,
                                }
                            }
                        } else {
                            JsValue::UNDEFINED
                        };

                        let iter_result = match self.call_function(
                            &next_method,
                            &iterator,
                            &[JsValue::UNDEFINED],
                        ) {
                            Completion::Normal(v) if (v).is_object() => Ok(v),
                            Completion::Normal(_) => {
                                Err(self.create_type_error("Iterator result is not an object"))
                            }
                            Completion::Throw(e) => Err(e),
                            _ => Err(self.create_type_error("Iterator next failed")),
                        };
                        let iter_result = match iter_result {
                            Ok(r) => r,
                            Err(e) => {
                                // Propagate through generator's try-stack
                                if let Some(try_info) = current_try_stack.last()
                                    && let Some(catch_state) = try_info.catch_state
                                {
                                    pending_exception = Some(e);
                                    let new_try_stack =
                                        current_try_stack[..current_try_stack.len() - 1].to_vec();
                                    obj_rc.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::StateMachineGenerator {
                                                state_machine: state_machine.clone(),
                                                func_env: func_env.clone(),
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::SuspendedAtState {
                                                        state_id: catch_state,
                                                    },
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: new_try_stack,
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: pending_exception.take(),
                                                pending_return: None,
                                            },
                                        );
                                    return self
                                        .generator_next_state_machine(this, JsValue::UNDEFINED);
                                }
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                return Completion::Throw(e);
                            }
                        };

                        let done = match self.iterator_complete(&iter_result) {
                            Ok(d) => d,
                            Err(e) => {
                                if let Some(try_info) = current_try_stack.last()
                                    && let Some(catch_state) = try_info.catch_state
                                {
                                    let new_try_stack =
                                        current_try_stack[..current_try_stack.len() - 1].to_vec();
                                    obj_rc.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::StateMachineGenerator {
                                                state_machine: state_machine.clone(),
                                                func_env: func_env.clone(),
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::SuspendedAtState {
                                                        state_id: catch_state,
                                                    },
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: new_try_stack,
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: Some(e),
                                                pending_return: None,
                                            },
                                        );
                                    return self
                                        .generator_next_state_machine(this, JsValue::UNDEFINED);
                                }
                                return Completion::Throw(e);
                            }
                        };

                        if done {
                            let value = match self.iterator_value(&iter_result) {
                                Ok(v) => v,
                                Err(e) => {
                                    if let Some(try_info) = current_try_stack.last()
                                        && let Some(catch_state) = try_info.catch_state
                                    {
                                        let new_try_stack = current_try_stack
                                            [..current_try_stack.len() - 1]
                                            .to_vec();
                                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(IteratorState::StateMachineGenerator {
                                                state_machine: state_machine.clone(),
                                                func_env: func_env.clone(),
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::SuspendedAtState {
                                                        state_id: catch_state,
                                                    },
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: new_try_stack,
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: Some(e),
                                                pending_return: None,
                                            });
                                        return self.generator_next_state_machine(
                                            this,
                                            JsValue::UNDEFINED,
                                        );
                                    }
                                    return Completion::Throw(e);
                                }
                            };
                            use crate::interpreter::generator_transform::SentValueBindingKind;
                            if let Some(binding) = sent_value_binding {
                                match &binding.kind {
                                    SentValueBindingKind::Variable(name) => {
                                        self.env_set(&term_env, name, value.clone()).ok();
                                    }
                                    SentValueBindingKind::Pattern(pattern) => {
                                        let _ = self.bind_pattern(
                                            pattern,
                                            value.clone(),
                                            BindingKind::Var,
                                            &term_env,
                                        );
                                    }
                                    SentValueBindingKind::Discard
                                    | SentValueBindingKind::InlineYield { .. } => {}
                                }
                            }
                            current_id = *resume_state;
                            continue;
                        } else {
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state:
                                            StateMachineExecutionState::SuspendedAtState {
                                                state_id: *resume_state,
                                            },
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: current_try_stack,
                                        pending_binding: None,
                                        delegated_iterator: Some(
                                            crate::interpreter::types::DelegatedIteratorInfo {
                                                iterator,
                                                next_method: next_method.clone(),
                                                resume_state: *resume_state,
                                                sent_value_binding: sent_value_binding.clone(),
                                            },
                                        ),
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            // Per spec §14.4.14: yield innerResult directly (don't extract value)
                            return Completion::Normal(iter_result);
                        }
                    }

                    // Save any iterators that need IteratorClose if generator.return() is called
                    let pending = std::mem::take(&mut self.pending_iter_close);
                    if pending.is_empty() {
                        self.generator_inline_iters.remove(&o.id);
                    } else {
                        self.generator_inline_iters.insert(o.id, pending);
                    }
                    self.sync_generator_for_of_stack(o.id, &for_of_stack);
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: *resume_state,
                            },
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: current_try_stack,
                            pending_binding: sent_value_binding.clone(),
                            delegated_iterator: None,
                            pending_exception: pending_exception.take(),
                            pending_return: pending_return.take(),
                        },
                    );
                    return Completion::Normal(self.create_iter_result_object(yield_val, false));
                }

                StateTerminator::Return(expr) => {
                    let ret_val = if let Some(e) = expr {
                        let mut result = self.eval_expr(e, &term_env);
                        while let Completion::TailCall { func, this, args } = result {
                            result = self.call_function(&func, &this, &args);
                        }
                        match result {
                            Completion::Normal(v) => v,
                            Completion::Throw(err) => {
                                let disp =
                                    self.dispose_resources(&func_env, Completion::Throw(err));
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                self.generator_inline_iters.remove(&o.id);
                                return disp;
                            }
                            other => return other,
                        }
                    } else {
                        JsValue::UNDEFINED
                    };

                    // Check try_stack for enclosing finally blocks before completing
                    let mut ret_val_opt = Some(ret_val);
                    for i in (0..current_try_stack.len()).rev() {
                        if !current_try_stack[i].entered_finally
                            && current_try_stack[i].finally_state.is_some()
                        {
                            pending_return = ret_val_opt.take();
                            let finally_state = current_try_stack[i].finally_state.unwrap();
                            current_try_stack = current_try_stack[..=i].to_vec();
                            current_id = finally_state;
                            break;
                        }
                    }
                    if ret_val_opt.is_none() {
                        continue;
                    }
                    let ret_val = ret_val_opt.unwrap();

                    // §27.5.3.3: DisposeResources when generator completes via return
                    let disp = self.dispose_resources(&func_env, Completion::Return(ret_val));
                    let ret_val = match disp {
                        Completion::Return(v) => v,
                        Completion::Throw(e) => {
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            self.generator_inline_iters.remove(&o.id);
                            return Completion::Throw(e);
                        }
                        _ => JsValue::UNDEFINED,
                    };

                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    self.generator_inline_iters.remove(&o.id);
                    return Completion::Normal(self.create_iter_result_object(ret_val, true));
                }

                StateTerminator::Throw(expr) => {
                    let throw_val = {
                        let mut result = self.eval_expr(expr, &term_env);
                        while let Completion::TailCall { func, this, args } = result {
                            result = self.call_function(&func, &this, &args);
                        }
                        match result {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => e,
                            other => return other,
                        }
                    };

                    if let Some(try_info) = current_try_stack.pop()
                        && let Some(catch_state) = try_info.catch_state
                    {
                        current_id = catch_state;
                        continue;
                    }

                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    self.generator_inline_iters.remove(&o.id);
                    return Completion::Throw(throw_val);
                }

                // Async-function transforms are currently the only machines
                // that emit LoopControl. If a shared transform emits one for a
                // generator, abrupt loop cleanup is identical to Goto.
                StateTerminator::Goto(next_state)
                | StateTerminator::LoopControl(LoopControlTarget {
                    target_state: next_state,
                    ..
                }) => {
                    if let Err(completion) = self.align_generator_for_of_stack(
                        o.id,
                        &mut for_of_stack,
                        &mut current_try_stack,
                        &func_env,
                        *next_state,
                    ) {
                        match completion {
                            Completion::Throw(error) => {
                                // The driver marked the generator Executing at
                                // entry. Restore a resumable snapshot before
                                // handing the close failure to the ordinary
                                // throw-resume path, which owns catch/finally
                                // routing and terminal state transitions.
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state:
                                                StateMachineExecutionState::SuspendedAtState {
                                                    state_id: current_id,
                                                },
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: current_try_stack,
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                return self.generator_throw_state_machine(this, error);
                            }
                            Completion::Exit(code) => {
                                self.generator_inline_iters.remove(&o.id);
                                self.generator_for_of_stacks.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                return Completion::Exit(code);
                            }
                            _ => unreachable!("for-of unwind returned a non-abrupt completion"),
                        }
                    }
                    current_id = *next_state;
                }

                StateTerminator::ConditionalGoto {
                    condition,
                    true_state,
                    false_state,
                } => {
                    let cond_val = match self.eval_expr(condition, &term_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            return Completion::Throw(e);
                        }
                        other => return other,
                    };
                    current_id = if self.to_boolean_val(&cond_val) {
                        *true_state
                    } else {
                        *false_state
                    };
                }

                StateTerminator::TryEnter {
                    try_state,
                    catch_state,
                    finally_state,
                    after_state,
                } => {
                    current_try_stack.push(TryContextInfo {
                        catch_state: catch_state.as_ref().map(|c| c.state),
                        finally_state: *finally_state,
                        _after_state: *after_state,
                        entered_catch: false,
                        entered_finally: false,
                    });
                    current_id = *try_state;
                }

                StateTerminator::TryExit { after_state } => {
                    current_try_stack.pop();
                    if let Some(exc) = pending_exception.take() {
                        // Re-throw pending exception after finally completes
                        if let Some(try_info) = current_try_stack.pop() {
                            if let Some(catch_state) = try_info.catch_state {
                                pending_exception = Some(exc);
                                current_id = catch_state;
                                continue;
                            } else if let Some(finally_state) = try_info.finally_state {
                                pending_exception = Some(exc);
                                current_id = finally_state;
                                continue;
                            }
                        }
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        return Completion::Throw(exc);
                    }
                    if let Some(ret_val) = pending_return.take() {
                        // Re-enter the abrupt-return driver after each finally.
                        // It uses loop `try_depth` boundaries to decide which
                        // iteration environments close before the next outer
                        // finally, and which remain until an inner finally has
                        // finished.
                        self.sync_generator_for_of_stack(o.id, &for_of_stack);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: *after_state,
                                },
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: current_try_stack,
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        return self.generator_return_state_machine(this, ret_val);
                    }
                    current_id = *after_state;
                }

                StateTerminator::EnterCatch { body_state, param } => {
                    if let Some(ctx) = current_try_stack.last_mut() {
                        ctx.entered_catch = true;
                    }
                    let exception_val = pending_exception.take().unwrap_or(JsValue::UNDEFINED);
                    if let Some(pattern) = param {
                        let _ =
                            self.bind_pattern(pattern, exception_val, BindingKind::Let, &term_env);
                    }
                    current_id = *body_state;
                }

                StateTerminator::EnterFinally { body_state } => {
                    if let Some(ctx) = current_try_stack.last_mut() {
                        ctx.entered_finally = true;
                    }
                    current_id = *body_state;
                }

                StateTerminator::SwitchDispatch {
                    discriminant,
                    cases,
                    default_state,
                    after_state,
                } => {
                    let disc_val = match self.eval_expr(discriminant, &term_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            return Completion::Throw(e);
                        }
                        other => return other,
                    };

                    let mut matched = false;
                    for case in cases {
                        let case_val = match self.eval_expr(&case.test, &term_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                return Completion::Throw(e);
                            }
                            other => return other,
                        };
                        if strict_equality(&disc_val, &case_val) {
                            current_id = case.state;
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        current_id = default_state.unwrap_or(*after_state);
                    }
                }

                StateTerminator::ForOfInit {
                    iterable,
                    iter_var,
                    label_set,
                    next_var: _,
                    left,
                    head_state,
                    after_state: forinit_after,
                    is_await: _,
                } => {
                    // §14.7.5.5: lexical head names are in TDZ while the RHS
                    // is evaluated, but that temporary environment is not the
                    // environment used by any loop iteration.
                    let iterable_env = Self::for_of_head_tdz_env(left, &term_env);

                    let iterable_val = match self.eval_expr(iterable, &iterable_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            if let Some(try_info) = current_try_stack.pop() {
                                if let Some(catch_state) = try_info.catch_state {
                                    pending_exception = Some(e);
                                    current_id = catch_state;
                                    continue;
                                } else if let Some(finally_state) = try_info.finally_state {
                                    current_id = finally_state;
                                    continue;
                                }
                            }
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            return Completion::Throw(e);
                        }
                        other => return other,
                    };
                    let iterator = match self.get_iterator(&iterable_val) {
                        Ok(iter) => iter,
                        Err(e) => {
                            if let Some(try_info) = current_try_stack.pop() {
                                if let Some(catch_state) = try_info.catch_state {
                                    pending_exception = Some(e);
                                    current_id = catch_state;
                                    continue;
                                } else if let Some(finally_state) = try_info.finally_state {
                                    current_id = finally_state;
                                    continue;
                                }
                            }
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            return Completion::Throw(e);
                        }
                    };
                    self.gc_root_value(&iterator);
                    func_env.borrow_mut().bindings.insert(
                        iter_var.clone(),
                        crate::interpreter::types::Binding {
                            value: iterator,
                            kind: crate::interpreter::types::BindingKind::Let,
                            initialized: true,
                            deletable: false,
                        },
                    );
                    for_of_stack.push(ForOfLoopState {
                        iter_var: iter_var.clone(),
                        label_set: label_set.clone(),
                        head_state: *head_state,
                        after_state: *forinit_after,
                        try_depth: current_try_stack.len(),
                        outer_env: term_env.clone(),
                        iteration_env: None,
                    });
                    self.sync_generator_for_of_stack(o.id, &for_of_stack);
                    current_id = *head_state;
                }

                StateTerminator::ForOfHead {
                    iter_var,
                    next_var: _,
                    left,
                    body_state,
                    after_state,
                    is_await: _,
                } => {
                    let loop_pos = match for_of_stack
                        .iter()
                        .rposition(|loop_state| loop_state.iter_var == *iter_var)
                    {
                        Some(pos) => pos,
                        None => {
                            debug_assert!(false, "for-of head without an active loop state");
                            for_of_stack.push(ForOfLoopState {
                                iter_var: iter_var.clone(),
                                label_set: vec![],
                                head_state: current_id,
                                after_state: *after_state,
                                try_depth: current_try_stack.len(),
                                outer_env: term_env.clone(),
                                iteration_env: None,
                            });
                            for_of_stack.len() - 1
                        }
                    };

                    let iterator = func_env
                        .borrow()
                        .bindings
                        .get(iter_var)
                        .map(|b| b.value.clone())
                        .unwrap_or(JsValue::UNDEFINED);

                    // §14.7.5.6 step 7.h: a throwing disposer ends the loop
                    // with a throw completion, so the iterator still closes
                    // and the generator's own handlers still see the error.
                    if let Some(iteration_env) = for_of_stack[loop_pos].iteration_env.take()
                        && let Completion::Throw(e) =
                            self.dispose_resources(&iteration_env, Completion::Empty)
                    {
                        self.iterator_close(&iterator, e.clone());
                        self.gc_unroot_value(&iterator);
                        for_of_stack.remove(loop_pos);
                        self.sync_generator_for_of_stack(o.id, &for_of_stack);
                        if let Some(try_info) = current_try_stack.pop() {
                            if let Some(catch_state) = try_info.catch_state {
                                pending_exception = Some(e);
                                current_id = catch_state;
                                continue;
                            } else if let Some(finally_state) = try_info.finally_state {
                                // TryExit re-throws whatever `pending_exception`
                                // still holds once the finally body completes.
                                pending_exception = Some(e);
                                current_id = finally_state;
                                continue;
                            }
                        }
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        return Completion::Throw(e);
                    }

                    let step_result = match self.iterator_next(&iterator) {
                        Ok(v) => v,
                        Err(e) => {
                            self.discard_failed_generator_for_of_loop(
                                o.id,
                                &mut for_of_stack,
                                loop_pos,
                                &iterator,
                            );
                            if Self::enter_generator_exception_handler(
                                &mut current_try_stack,
                                &mut pending_exception,
                                &mut current_id,
                                e.clone(),
                            ) {
                                continue;
                            }
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            return Completion::Throw(e);
                        }
                    };
                    match self.iterator_complete(&step_result) {
                        Ok(true) => {
                            self.gc_unroot_value(&iterator);
                            if let Some(o) = iterator
                                .as_object_id()
                                .map(|id| crate::types::JsObject { id })
                            {
                                let id = o.id;
                                self.pending_iter_close.retain(|v| {
                                    if let Some(ov) =
                                        (v).as_object_id().map(|id| crate::types::JsObject { id })
                                    {
                                        ov.id != id
                                    } else {
                                        true
                                    }
                                });
                            }
                            for_of_stack.remove(loop_pos);
                            self.sync_generator_for_of_stack(o.id, &for_of_stack);
                            current_id = *after_state;
                        }
                        Ok(false) => {
                            let val = match self.iterator_value(&step_result) {
                                Ok(v) => v,
                                Err(e) => {
                                    self.discard_failed_generator_for_of_loop(
                                        o.id,
                                        &mut for_of_stack,
                                        loop_pos,
                                        &iterator,
                                    );
                                    if Self::enter_generator_exception_handler(
                                        &mut current_try_stack,
                                        &mut pending_exception,
                                        &mut current_id,
                                        e.clone(),
                                    ) {
                                        continue;
                                    }
                                    obj_rc.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::StateMachineGenerator {
                                                state_machine,
                                                func_env,
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::Completed,
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: vec![],
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: None,
                                                pending_return: None,
                                            },
                                        );
                                    return Completion::Throw(e);
                                }
                            };
                            let needs_iteration_env = Self::for_of_head_lexical(left).is_some();
                            let outer_env = for_of_stack[loop_pos].outer_env.clone();
                            let bind_env = if needs_iteration_env {
                                let iteration_env = Environment::new(Some(outer_env));
                                for_of_stack[loop_pos].iteration_env = Some(iteration_env.clone());
                                self.sync_generator_for_of_stack(o.id, &for_of_stack);
                                iteration_env
                            } else {
                                outer_env
                            };

                            match left {
                                ForInOfLeft::Variable(decl) => {
                                    let kind = match decl.kind {
                                        VarKind::Var => crate::interpreter::types::BindingKind::Var,
                                        VarKind::Let => crate::interpreter::types::BindingKind::Let,
                                        VarKind::Const | VarKind::Using | VarKind::AwaitUsing => {
                                            crate::interpreter::types::BindingKind::Const
                                        }
                                    };
                                    if matches!(decl.kind, VarKind::Using | VarKind::AwaitUsing) {
                                        let hint = DisposeHint::for_var_kind(decl.kind);
                                        if let Err(e) =
                                            self.add_disposable_resource(&bind_env, &val, hint)
                                        {
                                            self.iterator_close(&iterator, e.clone());
                                            self.discard_failed_generator_for_of_loop(
                                                o.id,
                                                &mut for_of_stack,
                                                loop_pos,
                                                &iterator,
                                            );
                                            if Self::enter_generator_exception_handler(
                                                &mut current_try_stack,
                                                &mut pending_exception,
                                                &mut current_id,
                                                e.clone(),
                                            ) {
                                                continue;
                                            }
                                            obj_rc.borrow_mut().kind =
                                                crate::interpreter::types::ObjectKind::Iterator(
                                                    IteratorState::StateMachineGenerator {
                                                        state_machine,
                                                        func_env,
                                                        is_strict,
                                                        execution_state:
                                                            StateMachineExecutionState::Completed,
                                                        _sent_value: JsValue::UNDEFINED,
                                                        try_stack: vec![],
                                                        pending_binding: None,
                                                        delegated_iterator: None,
                                                        pending_exception: None,
                                                        pending_return: None,
                                                    },
                                                );
                                            return Completion::Throw(e);
                                        }
                                    }
                                    if let Some(d) = decl.declarations.first()
                                        && let Err(e) =
                                            self.bind_pattern(&d.pattern, val, kind, &bind_env)
                                    {
                                        self.iterator_close(&iterator, e.clone());
                                        self.discard_failed_generator_for_of_loop(
                                            o.id,
                                            &mut for_of_stack,
                                            loop_pos,
                                            &iterator,
                                        );
                                        if Self::enter_generator_exception_handler(
                                            &mut current_try_stack,
                                            &mut pending_exception,
                                            &mut current_id,
                                            e.clone(),
                                        ) {
                                            continue;
                                        }
                                        obj_rc.borrow_mut().kind =
                                            crate::interpreter::types::ObjectKind::Iterator(
                                                IteratorState::StateMachineGenerator {
                                                    state_machine,
                                                    func_env,
                                                    is_strict,
                                                    execution_state:
                                                        StateMachineExecutionState::Completed,
                                                    _sent_value: JsValue::UNDEFINED,
                                                    try_stack: vec![],
                                                    pending_binding: None,
                                                    delegated_iterator: None,
                                                    pending_exception: None,
                                                    pending_return: None,
                                                },
                                            );
                                        return Completion::Throw(e);
                                    }
                                }
                                ForInOfLeft::Pattern(pat) => {
                                    match self.assign_to_for_pattern(pat, val, &term_env) {
                                        Completion::Normal(_) | Completion::Empty => {}
                                        Completion::Throw(e) => {
                                            self.iterator_close(&iterator, e.clone());
                                            self.discard_failed_generator_for_of_loop(
                                                o.id,
                                                &mut for_of_stack,
                                                loop_pos,
                                                &iterator,
                                            );
                                            if Self::enter_generator_exception_handler(
                                                &mut current_try_stack,
                                                &mut pending_exception,
                                                &mut current_id,
                                                e.clone(),
                                            ) {
                                                continue;
                                            }
                                            obj_rc.borrow_mut().kind =
                                                crate::interpreter::types::ObjectKind::Iterator(
                                                    IteratorState::StateMachineGenerator {
                                                        state_machine,
                                                        func_env,
                                                        is_strict,
                                                        execution_state:
                                                            StateMachineExecutionState::Completed,
                                                        _sent_value: JsValue::UNDEFINED,
                                                        try_stack: vec![],
                                                        pending_binding: None,
                                                        delegated_iterator: None,
                                                        pending_exception: None,
                                                        pending_return: None,
                                                    },
                                                );
                                            return Completion::Throw(e);
                                        }
                                        _other => {}
                                    }
                                }
                                ForInOfLeft::Expression(_) => {
                                    // for-of with expression LHS is handled via assignment
                                }
                            }
                            // Add iterator to pending_iter_close so generator.return() can close it
                            let already_pending = if let Some(o) = iterator
                                .as_object_id()
                                .map(|id| crate::types::JsObject { id })
                            {
                                let id = o.id;
                                self.pending_iter_close.iter().any(|v| {
                                    if let Some(ov) =
                                        (v).as_object_id().map(|id| crate::types::JsObject { id })
                                    {
                                        ov.id == id
                                    } else {
                                        false
                                    }
                                })
                            } else {
                                false
                            };
                            if !already_pending {
                                self.pending_iter_close.push(iterator);
                            }
                            current_id = *body_state;
                        }
                        Err(e) => {
                            self.discard_failed_generator_for_of_loop(
                                o.id,
                                &mut for_of_stack,
                                loop_pos,
                                &iterator,
                            );
                            if Self::enter_generator_exception_handler(
                                &mut current_try_stack,
                                &mut pending_exception,
                                &mut current_id,
                                e.clone(),
                            ) {
                                continue;
                            }
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            return Completion::Throw(e);
                        }
                    }
                }

                StateTerminator::Completed => {
                    let has_pending_return = pending_return.is_some();
                    let ret_val = pending_return.take().unwrap_or(JsValue::UNDEFINED);
                    // §27.5.3.3 GeneratorStart: DisposeResources when generator completes
                    let disp = if has_pending_return {
                        self.dispose_resources(&func_env, Completion::Return(ret_val.clone()))
                    } else {
                        self.dispose_resources(&func_env, Completion::Normal(JsValue::UNDEFINED))
                    };
                    let final_val = match disp {
                        Completion::Return(v) => v,
                        Completion::Throw(e) => {
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            return Completion::Throw(e);
                        }
                        _ => ret_val,
                    };
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    return Completion::Normal(self.create_iter_result_object(final_val, true));
                }

                StateTerminator::Await { .. } => {
                    unreachable!("Await terminator in sync generator")
                }
            }
        }
    }

    pub(crate) fn generator_return_state_machine(
        &mut self,
        this: &JsValue,
        value: JsValue,
    ) -> Completion {
        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.return called on non-object"),
            );
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.return called on non-object"),
            );
        };

        let state = obj_rc.borrow().iterator_state().cloned();
        if let Some(IteratorState::StateMachineGenerator {
            state_machine,
            func_env,
            is_strict,
            execution_state,
            mut try_stack,
            delegated_iterator,
            ..
        }) = state
        {
            let suspended_state_id = match execution_state {
                StateMachineExecutionState::Executing => {
                    return Completion::Throw(
                        self.create_type_error("Generator is already running"),
                    );
                }
                StateMachineExecutionState::Completed => {
                    return Completion::Normal(self.create_iter_result_object(value, true));
                }
                StateMachineExecutionState::SuspendedStart => {
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    return Completion::Normal(self.create_iter_result_object(value, true));
                }
                StateMachineExecutionState::SuspendedAtState { state_id } => state_id,
            };

            if let Some(ref deleg_info) = delegated_iterator {
                let iterator = deleg_info.iterator.clone();
                let next_method = deleg_info.next_method.clone();
                let resume_state = deleg_info.resume_state;
                let binding = deleg_info.sent_value_binding.clone();

                match self.iterator_return(&iterator, &value) {
                    Ok(Some(iter_result)) => {
                        let done = match self.iterator_complete(&iter_result) {
                            Ok(d) => d,
                            Err(e) => {
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineGenerator {
                                            state_machine: state_machine.clone(),
                                            func_env: func_env.clone(),
                                            is_strict,
                                            execution_state:
                                                StateMachineExecutionState::SuspendedAtState {
                                                    state_id: resume_state,
                                                },
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: try_stack.clone(),
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                return self.generator_throw_state_machine(this, e);
                            }
                        };
                        if done {
                            let result_value = match self.iterator_value(&iter_result) {
                                Ok(v) => v,
                                Err(e) => {
                                    obj_rc.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::StateMachineGenerator {
                                                state_machine: state_machine.clone(),
                                                func_env: func_env.clone(),
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::SuspendedAtState {
                                                        state_id: resume_state,
                                                    },
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: try_stack.clone(),
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: None,
                                                pending_return: None,
                                            },
                                        );
                                    return self.generator_throw_state_machine(this, e);
                                }
                            };
                            // Clear delegation and propagate return through
                            // generator's try-finally stack
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine: state_machine.clone(),
                                        func_env: func_env.clone(),
                                        is_strict,
                                        execution_state:
                                            StateMachineExecutionState::SuspendedAtState {
                                                state_id: resume_state,
                                            },
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: try_stack.clone(),
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            return self.generator_return_state_machine(this, result_value);
                        } else {
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state:
                                            StateMachineExecutionState::SuspendedAtState {
                                                state_id: resume_state,
                                            },
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: try_stack.clone(),
                                        pending_binding: None,
                                        delegated_iterator: Some(
                                            crate::interpreter::types::DelegatedIteratorInfo {
                                                iterator,
                                                next_method: next_method.clone(),
                                                resume_state,
                                                sent_value_binding: binding,
                                            },
                                        ),
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            // Per spec §14.4.14: yield innerReturnResult directly
                            return Completion::Normal(iter_result);
                        }
                    }
                    Ok(None) => {
                        // Per spec 14.4.14 step 5.c.iii: "If return is undefined,
                        // return Completion(received)." — clear the delegation and
                        // propagate the return through the generator's own body
                        // (which may have try-finally).
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        return self.generator_return_state_machine(this, value);
                    }
                    Err(e) => {
                        // Propagate error through generator's try-catch
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        return self.generator_throw_state_machine(this, e);
                    }
                }
            }

            obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                IteratorState::StateMachineGenerator {
                    state_machine: state_machine.clone(),
                    func_env: func_env.clone(),
                    is_strict,
                    execution_state: StateMachineExecutionState::Executing,
                    _sent_value: JsValue::UNDEFINED,
                    try_stack: try_stack.clone(),
                    pending_binding: None,
                    delegated_iterator: None,
                    pending_exception: None,
                    pending_return: None,
                },
            );

            // A return injected at a suspended yield is the loop body's abrupt
            // completion. Finalizers lexically inside the loop run before its
            // iteration environment is disposed; finalizers surrounding the
            // loop run after disposal and IteratorClose.
            let finally_idx = (0..try_stack.len())
                .rev()
                .find(|&i| !try_stack[i].entered_finally && try_stack[i].finally_state.is_some());
            let mut for_of_stack = self
                .generator_for_of_stacks
                .get(&o.id)
                .cloned()
                .unwrap_or_default();
            let unwind_from = finally_idx.map_or(0, |handler_depth| {
                for_of_stack
                    .iter()
                    .position(|loop_state| loop_state.try_depth > handler_depth)
                    .unwrap_or(for_of_stack.len())
            });
            let return_completion = self.unwind_generator_for_of_loops(
                o.id,
                &mut for_of_stack,
                &mut try_stack,
                &func_env,
                unwind_from,
                Completion::Return(value.clone()),
            );
            let return_value = match return_completion {
                Completion::Return(return_value) => return_value,
                Completion::Throw(error) => {
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: suspended_state_id,
                            },
                            _sent_value: JsValue::UNDEFINED,
                            try_stack,
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    return self.generator_throw_state_machine(this, error);
                }
                Completion::Exit(code) => {
                    self.generator_inline_iters.remove(&o.id);
                    self.generator_for_of_stacks.remove(&o.id);
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    return Completion::Exit(code);
                }
                _ => value.clone(),
            };

            if let Some(idx) = finally_idx {
                let finally_state = try_stack[idx].finally_state.unwrap();
                // Keep the selected entry until TryExit so EnterFinally marks
                // and pops that context, not the surrounding one.
                let remaining_stack = try_stack[..=idx].to_vec();
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::SuspendedAtState {
                            state_id: finally_state,
                        },
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: remaining_stack,
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: Some(return_value.clone()),
                    },
                );
                return self.generator_next_state_machine(this, JsValue::UNDEFINED);
            }

            obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                IteratorState::StateMachineGenerator {
                    state_machine,
                    func_env,
                    is_strict,
                    execution_state: StateMachineExecutionState::Completed,
                    _sent_value: JsValue::UNDEFINED,
                    try_stack: vec![],
                    pending_binding: None,
                    delegated_iterator: None,
                    pending_exception: None,
                    pending_return: None,
                },
            );
            // Close any iterators that were open when generator was suspended via InlineYield
            if let Some(iters) = self.generator_inline_iters.remove(&o.id) {
                for iter in iters {
                    if let Err(e) = self.iterator_close_result(&iter) {
                        return Completion::Throw(e);
                    }
                }
            }
        }
        Completion::Normal(self.create_iter_result_object(value, true))
    }

    pub(crate) fn generator_throw_state_machine(
        &mut self,
        this: &JsValue,
        exception: JsValue,
    ) -> Completion {
        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.throw called on non-object"),
            );
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.throw called on non-object"),
            );
        };

        let state = obj_rc.borrow().iterator_state().cloned();
        if let Some(IteratorState::StateMachineGenerator {
            state_machine,
            func_env,
            is_strict,
            execution_state,
            try_stack,
            delegated_iterator,
            pending_return: stored_pending_return,
            ..
        }) = state
        {
            match execution_state {
                StateMachineExecutionState::Executing => {
                    return Completion::Throw(
                        self.create_type_error("Generator is already running"),
                    );
                }
                StateMachineExecutionState::Completed
                | StateMachineExecutionState::SuspendedStart => {
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    return Completion::Throw(exception);
                }
                StateMachineExecutionState::SuspendedAtState { .. } => {}
            }

            if let Some(ref deleg_info) = delegated_iterator {
                let iterator = deleg_info.iterator.clone();
                let next_method = deleg_info.next_method.clone();
                let resume_state = deleg_info.resume_state;
                let binding = deleg_info.sent_value_binding.clone();

                match self.iterator_throw(&iterator, &exception) {
                    Ok(Some(iter_result)) => {
                        let done = match self.iterator_complete(&iter_result) {
                            Ok(d) => d,
                            Err(e) => {
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineGenerator {
                                            state_machine: state_machine.clone(),
                                            func_env: func_env.clone(),
                                            is_strict,
                                            execution_state:
                                                StateMachineExecutionState::SuspendedAtState {
                                                    state_id: resume_state,
                                                },
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: try_stack.clone(),
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                return self.generator_throw_state_machine(this, e);
                            }
                        };
                        if done {
                            let result_value = match self.iterator_value(&iter_result) {
                                Ok(v) => v,
                                Err(e) => {
                                    obj_rc.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::StateMachineGenerator {
                                                state_machine: state_machine.clone(),
                                                func_env: func_env.clone(),
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::SuspendedAtState {
                                                        state_id: resume_state,
                                                    },
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: try_stack.clone(),
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: None,
                                                pending_return: None,
                                            },
                                        );
                                    return self.generator_throw_state_machine(this, e);
                                }
                            };
                            use crate::interpreter::generator_transform::SentValueBindingKind;
                            if let Some(ref bind) = binding {
                                match &bind.kind {
                                    SentValueBindingKind::Variable(name) => {
                                        self.env_set(&func_env, name, result_value.clone()).ok();
                                    }
                                    SentValueBindingKind::Pattern(pattern) => {
                                        let _ = self.bind_pattern(
                                            pattern,
                                            result_value.clone(),
                                            BindingKind::Var,
                                            &func_env,
                                        );
                                    }
                                    SentValueBindingKind::Discard
                                    | SentValueBindingKind::InlineYield { .. } => {}
                                }
                            }
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine: state_machine.clone(),
                                        func_env: func_env.clone(),
                                        is_strict,
                                        execution_state:
                                            StateMachineExecutionState::SuspendedAtState {
                                                state_id: resume_state,
                                            },
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: try_stack.clone(),
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            return self.generator_next_state_machine(this, JsValue::UNDEFINED);
                        } else {
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state:
                                            StateMachineExecutionState::SuspendedAtState {
                                                state_id: resume_state,
                                            },
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: try_stack.clone(),
                                        pending_binding: None,
                                        delegated_iterator: Some(
                                            crate::interpreter::types::DelegatedIteratorInfo {
                                                iterator,
                                                next_method: next_method.clone(),
                                                resume_state,
                                                sent_value_binding: binding,
                                            },
                                        ),
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            // Per spec §14.4.14: yield innerResult directly
                            return Completion::Normal(iter_result);
                        }
                    }
                    Ok(None) => {
                        // Per §14.4.14 step 5.b.iii: close iterator with normal
                        // completion, then throw TypeError (yield* protocol violation)
                        if let Err(e) = self.iterator_close_result(&iterator) {
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine: state_machine.clone(),
                                        func_env: func_env.clone(),
                                        is_strict,
                                        execution_state:
                                            StateMachineExecutionState::SuspendedAtState {
                                                state_id: resume_state,
                                            },
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: try_stack.clone(),
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            return self.generator_throw_state_machine(this, e);
                        }
                        let type_err = self
                            .create_type_error("The iterator does not provide a 'throw' method");
                        // Clear delegation and propagate throw through generator body
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        return self.generator_throw_state_machine(this, type_err);
                    }
                    Err(e) => {
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        return self.generator_throw_state_machine(this, e);
                    }
                }
            }

            // Walk try_stack from innermost to outermost to find a handler
            for i in (0..try_stack.len()).rev() {
                let try_info = &try_stack[i];
                if !try_info.entered_catch
                    && !try_info.entered_finally
                    && let Some(catch_state) = try_info.catch_state
                {
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: catch_state,
                            },
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: try_stack[..i].to_vec(),
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: Some(exception.clone()),
                            pending_return: stored_pending_return,
                        },
                    );
                    return self.generator_next_state_machine(this, JsValue::UNDEFINED);
                }
                if !try_info.entered_finally
                    && let Some(finally_state) = try_info.finally_state
                {
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: finally_state,
                            },
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: try_stack[..i].to_vec(),
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: Some(exception.clone()),
                            pending_return: stored_pending_return,
                        },
                    );
                    return self.generator_next_state_machine(this, JsValue::UNDEFINED);
                }
            }

            obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                IteratorState::StateMachineGenerator {
                    state_machine,
                    func_env,
                    is_strict,
                    execution_state: StateMachineExecutionState::Completed,
                    _sent_value: JsValue::UNDEFINED,
                    try_stack: vec![],
                    pending_binding: None,
                    delegated_iterator: None,
                    pending_exception: None,
                    pending_return: None,
                },
            );
        }
        Completion::Throw(exception)
    }

    fn reject_with_type_error(&mut self, msg: &str) -> Completion {
        let promise = self.create_promise_object();
        let promise_id = if let Some(po) = (promise)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            po.id
        } else {
            0
        };
        let (_resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);
        let err = self.create_type_error(msg);
        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[err]);
        self.drain_microtasks();
        Completion::Normal(promise)
    }

    fn async_gen_enqueue(
        &mut self,
        this: &JsValue,
        value: JsValue,
        kind: super::AsyncGenRequestKind,
    ) -> Completion {
        let gen_id = if let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            o.id
        } else {
            return self.reject_with_type_error("not an async generator");
        };

        let promise = self.create_promise_object();
        let promise_id = if let Some(po) = (promise)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            po.id
        } else {
            0
        };
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);

        let request = super::AsyncGenRequest {
            kind,
            value,
            promise: promise.clone(),
            resolve_fn,
            reject_fn,
        };

        // Check generator state before mutating the queue
        let is_executing = if let Some(obj_rc) = self.get_object_cell(gen_id) {
            matches!(
                obj_rc.borrow().iterator_state(),
                Some(IteratorState::StateMachineAsyncGenerator {
                    execution_state: StateMachineExecutionState::Executing,
                    ..
                })
            )
        } else {
            false
        };

        let queue = self.scheduler.async_gen_queue_or_default(gen_id);
        queue.push_back(request);
        let queue_len = queue.len();

        // Per spec §27.6.3.7 step 5: if the generator is not executing,
        // call AsyncGeneratorResume immediately (not via microtask)
        if !is_executing && queue_len == 1 {
            let this_clone = this.clone();
            self.async_gen_process_queue(&this_clone);
        }

        Completion::Normal(promise)
    }

    fn async_gen_process_queue(&mut self, this: &JsValue) {
        let gen_id = if let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            o.id
        } else {
            return;
        };

        let request = {
            let queue = self.scheduler.async_gen_queue(gen_id);
            match queue.and_then(|q| q.front().cloned()) {
                Some(r) => r,
                None => return,
            }
        };
        self.scheduler.set_async_gen_yield_pending(false);
        let result = match request.kind {
            super::AsyncGenRequestKind::Next => self
                .async_generator_next_state_machine_with_promise(
                    this,
                    request.value.clone(),
                    request.promise.clone(),
                    request.resolve_fn.clone(),
                    request.reject_fn.clone(),
                ),
            super::AsyncGenRequestKind::Return => self
                .async_generator_return_state_machine_with_promise(
                    this,
                    request.value.clone(),
                    request.promise.clone(),
                    request.resolve_fn.clone(),
                    request.reject_fn.clone(),
                ),
            super::AsyncGenRequestKind::Throw => self
                .async_generator_throw_state_machine_with_promise(
                    this,
                    request.value.clone(),
                    request.promise.clone(),
                    request.resolve_fn.clone(),
                    request.reject_fn.clone(),
                ),
        };

        // If the yield suspended asynchronously (pending promise), don't pop — the
        // fulfill/reject handler will pop and schedule the next request
        if self.scheduler.is_async_gen_yield_pending() {
            self.scheduler.set_async_gen_yield_pending(false);
            let _ = result;
            return;
        }

        if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
            queue.pop_front();
        }

        // Process next request inline per spec (AsyncGeneratorDrainQueue)
        let this_clone = this.clone();
        self.async_gen_process_queue(&this_clone);

        let _ = result;
    }

    /// Called when Await(innerResult) resolves during yield* delegation in an async generator.
    /// Implements yield* step 8.a.iii-vi + AsyncGeneratorYield inline.
    fn yield_star_await_inner_result_resume(
        &mut self,
        gen_this: &JsValue,
        gen_id: u64,
        awaited_result: JsValue,
        promise: &JsValue,
        resolve_fn: &JsValue,
        reject_fn: &JsValue,
        is_rejection: bool,
    ) {
        let obj_rc = match self.get_object(gen_id) {
            Some(o) => o,
            None => return,
        };

        let state = obj_rc.borrow().iterator_state().cloned();
        let Some(IteratorState::StateMachineAsyncGenerator {
            state_machine,
            func_env,
            is_strict,
            try_stack,
            delegated_iterator,
            pending_binding,
            ..
        }) = state
        else {
            return;
        };

        let deleg_info = match delegated_iterator {
            Some(d) => d,
            None => return,
        };

        if is_rejection {
            self.generator_inline_iters.remove(&gen_id);
            obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                IteratorState::StateMachineAsyncGenerator {
                    state_machine,
                    func_env,
                    is_strict,
                    execution_state: StateMachineExecutionState::Completed,
                    _sent_value: JsValue::UNDEFINED,
                    try_stack: vec![],
                    pending_binding: None,
                    delegated_iterator: None,
                    pending_exception: None,
                    pending_return: None,
                },
            );
            let _ = self.call_function(reject_fn, &JsValue::UNDEFINED, &[awaited_result]);
            if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                queue.pop_front();
            }
            self.async_gen_process_queue(gen_this);
            return;
        }

        // §15.5.5 step 8.a.iii: If innerResult is not an Object, throw TypeError
        if !(awaited_result).is_object() {
            let err = self.create_type_error("Iterator result is not an object");
            self.generator_inline_iters.remove(&gen_id);
            obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                IteratorState::StateMachineAsyncGenerator {
                    state_machine,
                    func_env,
                    is_strict,
                    execution_state: StateMachineExecutionState::Completed,
                    _sent_value: JsValue::UNDEFINED,
                    try_stack: vec![],
                    pending_binding: None,
                    delegated_iterator: None,
                    pending_exception: None,
                    pending_return: None,
                },
            );
            let _ = self.call_function(reject_fn, &JsValue::UNDEFINED, &[err]);
            if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                queue.pop_front();
            }
            self.async_gen_process_queue(gen_this);
            return;
        }

        // §15.5.5 step 8.a.iv: done = IteratorComplete(innerResult)
        let done = match self.iterator_complete(&awaited_result) {
            Ok(d) => d,
            Err(e) => {
                self.generator_inline_iters.remove(&gen_id);
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    },
                );
                let _ = self.call_function(reject_fn, &JsValue::UNDEFINED, &[e]);
                if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                    queue.pop_front();
                }
                self.async_gen_process_queue(gen_this);
                return;
            }
        };

        // §15.5.5 step 8.a.v-vi
        let value = match self.iterator_value(&awaited_result) {
            Ok(v) => v,
            Err(e) => {
                let has_catch = try_stack
                    .iter()
                    .rev()
                    .any(|tc| !tc.entered_catch && !tc.entered_finally && tc.catch_state.is_some());
                if has_catch {
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineAsyncGenerator {
                            state_machine: state_machine.clone(),
                            func_env: func_env.clone(),
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: deleg_info.resume_state,
                            },
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: try_stack.clone(),
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: Some(e),
                            pending_return: None,
                        },
                    );
                    self.scheduler.set_async_gen_yield_pending(false);
                    let _ = self.async_generator_next_state_machine_with_promise(
                        gen_this,
                        JsValue::UNDEFINED,
                        promise.clone(),
                        resolve_fn.clone(),
                        reject_fn.clone(),
                    );
                    if self.scheduler.is_async_gen_yield_pending() {
                        self.scheduler.set_async_gen_yield_pending(false);
                        return;
                    }
                    if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                        queue.pop_front();
                    }
                    self.async_gen_process_queue(gen_this);
                    return;
                }
                self.generator_inline_iters.remove(&gen_id);
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    },
                );
                let _ = self.call_function(reject_fn, &JsValue::UNDEFINED, &[e]);
                if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                    queue.pop_front();
                }
                self.async_gen_process_queue(gen_this);
                return;
            }
        };

        if done {
            // §15.5.5 step 8.a.v: return IteratorValue(innerResult)
            // Bind the yield* result and resume the state machine
            use crate::interpreter::generator_transform::SentValueBindingKind;
            if let Some(ref binding) = pending_binding {
                match &binding.kind {
                    SentValueBindingKind::Variable(name) => {
                        self.env_set(&func_env, name, value.clone()).ok();
                    }
                    SentValueBindingKind::Pattern(pattern) => {
                        let _ =
                            self.bind_pattern(pattern, value.clone(), BindingKind::Var, &func_env);
                    }
                    SentValueBindingKind::Discard | SentValueBindingKind::InlineYield { .. } => {}
                }
            }
            obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                IteratorState::StateMachineAsyncGenerator {
                    state_machine,
                    func_env,
                    is_strict,
                    execution_state: StateMachineExecutionState::SuspendedAtState {
                        state_id: deleg_info.resume_state,
                    },
                    _sent_value: JsValue::UNDEFINED,
                    try_stack,
                    pending_binding: None,
                    delegated_iterator: None,
                    pending_exception: None,
                    pending_return: None,
                },
            );
            self.scheduler.set_async_gen_yield_pending(false);
            let _ = self.async_generator_next_state_machine_with_promise(
                gen_this,
                value,
                promise.clone(),
                resolve_fn.clone(),
                reject_fn.clone(),
            );
            if self.scheduler.is_async_gen_yield_pending() {
                self.scheduler.set_async_gen_yield_pending(false);
                return;
            }
            if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                queue.pop_front();
            }
            self.async_gen_process_queue(gen_this);
            return;
        }

        // done=false: §27.6.3.8 AsyncGeneratorYield
        // Step 9: AsyncGeneratorCompleteStep — resolve the .next() promise
        let iter_result = self.create_iter_result_object(value, false);
        let _ = self.call_function(resolve_fn, &JsValue::UNDEFINED, &[iter_result]);

        // Pop the current (Next) request from the queue
        if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
            queue.pop_front();
        }

        // Step 10-11: Check if queue has more requests (e.g. .return()/.throw())
        // If so, process via AsyncGeneratorUnwrapYieldResumption inline
        let next_request = self
            .scheduler
            .async_gen_queue(gen_id)
            .and_then(|q| q.front().cloned());

        if let Some(request) = next_request {
            match request.kind {
                super::AsyncGenRequestKind::Return => {
                    // §27.6.3.7 AsyncGeneratorUnwrapYieldResumption for return
                    // Await(returnValue) then handle yield* return protocol
                    let ret_val = request.value.clone();
                    let ret_promise = request.promise.clone();
                    let ret_resolve = request.resolve_fn.clone();
                    let ret_reject = request.reject_fn.clone();

                    // Save state keeping delegated_iterator for the return handler
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: deleg_info.resume_state,
                            },
                            _sent_value: JsValue::UNDEFINED,
                            try_stack,
                            pending_binding: None,
                            delegated_iterator: Some(deleg_info),
                            pending_exception: None,
                            pending_return: None,
                        },
                    );

                    // §27.6.3.7 step 2: Await(resumptionValue.[[Value]])
                    let unwrap_promise = self.promise_resolve_value(&ret_val);
                    let unwrap_id = if let Some(uo) = (unwrap_promise)
                        .as_object_id()
                        .map(|id| crate::types::JsObject { id })
                    {
                        uo.id
                    } else {
                        0
                    };

                    let gen_this_r = gen_this.clone();
                    let gen_id_r = gen_id;
                    let ret_promise_c = ret_promise.clone();
                    let ret_resolve_c = ret_resolve.clone();
                    let ret_reject_c = ret_reject.clone();

                    let on_fulfilled = self.create_function(JsFunction::native(
                        "yieldStarUnwrapReturnFulfill".to_string(),
                        1,
                        move |interp, _this, args| {
                            let awaited_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                            interp.yield_star_return_after_unwrap(
                                &gen_this_r,
                                gen_id_r,
                                awaited_val,
                                &ret_promise_c,
                                &ret_resolve_c,
                                &ret_reject_c,
                            );
                            Completion::Normal(JsValue::UNDEFINED)
                        },
                    ));

                    let gen_this_r2 = gen_this.clone();
                    let gen_id_r2 = gen_id;
                    let ret_reject_c2 = ret_reject.clone();
                    let on_rejected = self.create_function(JsFunction::native(
                        "yieldStarUnwrapReturnReject".to_string(),
                        1,
                        move |interp, _this, args| {
                            let reason = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                            if let Some(obj) = interp.get_object(gen_id_r2) {
                                let mut b = obj.borrow_mut();
                                if let Some(IteratorState::StateMachineAsyncGenerator {
                                    execution_state,
                                    delegated_iterator,
                                    try_stack,
                                    ..
                                }) = b.iterator_state_mut()
                                {
                                    interp.generator_inline_iters.remove(&gen_id_r2);
                                    *execution_state = StateMachineExecutionState::Completed;
                                    *delegated_iterator = None;
                                    try_stack.clear();
                                }
                            }
                            let _ = interp.call_function(
                                &ret_reject_c2,
                                &JsValue::UNDEFINED,
                                &[reason],
                            );
                            if let Some(queue) = interp.scheduler.async_gen_queue_mut(gen_id_r2) {
                                queue.pop_front();
                            }
                            interp.async_gen_process_queue(&gen_this_r2);
                            Completion::Normal(JsValue::UNDEFINED)
                        },
                    ));

                    let unwrap_state = self.get_promise_state(unwrap_id);
                    match unwrap_state {
                        Some(PromiseState::Fulfilled(v)) => {
                            let handler = on_fulfilled;
                            let val = v.clone();
                            self.scheduler.enqueue_microtask((
                                vec![val.clone(), handler.clone()],
                                Box::new(move |interp| {
                                    let _ =
                                        interp.call_function(&handler, &JsValue::UNDEFINED, &[val]);
                                    Completion::Normal(JsValue::UNDEFINED)
                                }),
                            ));
                        }
                        Some(PromiseState::Rejected(r)) => {
                            let handler = on_rejected;
                            let reason = r.clone();
                            self.scheduler.enqueue_microtask((
                                vec![reason.clone(), handler.clone()],
                                Box::new(move |interp| {
                                    let _ = interp.call_function(
                                        &handler,
                                        &JsValue::UNDEFINED,
                                        &[reason],
                                    );
                                    Completion::Normal(JsValue::UNDEFINED)
                                }),
                            ));
                        }
                        Some(PromiseState::Pending) => {
                            if let Some(obj) = self.get_object_cell(unwrap_id) {
                                let mut ob = obj.borrow_mut();
                                if let Some(pd) = ob.promise_data_mut() {
                                    pd.is_handled = true;
                                    pd.fulfill_reactions.push(PromiseReaction {
                                        handler: Some(on_fulfilled),
                                        promise_id: None,
                                        resolve: JsValue::UNDEFINED,
                                        reject: JsValue::UNDEFINED,
                                        reaction_type: PromiseReactionType::Fulfill,
                                    });
                                    pd.reject_reactions.push(PromiseReaction {
                                        handler: Some(on_rejected),
                                        promise_id: None,
                                        resolve: JsValue::UNDEFINED,
                                        reject: JsValue::UNDEFINED,
                                        reaction_type: PromiseReactionType::Reject,
                                    });
                                }
                            }
                        }
                        None => {
                            let handler = on_fulfilled;
                            let val = ret_val.clone();
                            self.scheduler.enqueue_microtask((
                                vec![val.clone(), handler.clone()],
                                Box::new(move |interp| {
                                    let _ =
                                        interp.call_function(&handler, &JsValue::UNDEFINED, &[val]);
                                    Completion::Normal(JsValue::UNDEFINED)
                                }),
                            ));
                        }
                    }
                    self.scheduler.set_async_gen_yield_pending(true);
                }
                _ => {
                    // Normal/Throw: save state and let process_queue handle it
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: deleg_info.resume_state,
                            },
                            _sent_value: JsValue::UNDEFINED,
                            try_stack,
                            pending_binding: None,
                            delegated_iterator: Some(deleg_info),
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    self.async_gen_process_queue(gen_this);
                }
            }
        } else {
            // Queue is empty — suspend the generator at yield
            obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                IteratorState::StateMachineAsyncGenerator {
                    state_machine,
                    func_env,
                    is_strict,
                    execution_state: StateMachineExecutionState::SuspendedAtState {
                        state_id: deleg_info.resume_state,
                    },
                    _sent_value: JsValue::UNDEFINED,
                    try_stack,
                    pending_binding: None,
                    delegated_iterator: Some(deleg_info),
                    pending_exception: None,
                    pending_return: None,
                },
            );
        }
    }

    /// Called when the Await in AsyncGeneratorUnwrapYieldResumption for a return
    /// completion resolves during yield* delegation.
    fn yield_star_return_after_unwrap(
        &mut self,
        gen_this: &JsValue,
        gen_id: u64,
        awaited_val: JsValue,
        ret_promise: &JsValue,
        ret_resolve: &JsValue,
        ret_reject: &JsValue,
    ) {
        let obj_rc = match self.get_object(gen_id) {
            Some(o) => o,
            None => return,
        };

        let state = obj_rc.borrow().iterator_state().cloned();
        let Some(IteratorState::StateMachineAsyncGenerator {
            state_machine,
            func_env,
            is_strict,
            try_stack,
            delegated_iterator,
            ..
        }) = state
        else {
            return;
        };

        let deleg_info = match delegated_iterator {
            Some(d) => d,
            None => return,
        };

        let iterator = deleg_info.iterator.clone();

        // yield* step 8.c: received.[[Type]] is return, received.[[Value]] = awaited_val
        // Step 8.c.ii: GetMethod(iterator, "return")
        match self.iterator_return(&iterator, &awaited_val) {
            Ok(Some(inner_return_result)) => {
                // Step 8.c.v: Await(innerReturnResult)
                let iawait_result = match self.await_value(&inner_return_result) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        self.generator_inline_iters.remove(&gen_id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let _ = self.call_function(ret_reject, &JsValue::UNDEFINED, &[e]);
                        if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                            queue.pop_front();
                        }
                        self.async_gen_process_queue(gen_this);
                        return;
                    }
                    _ => inner_return_result,
                };
                let done = match self.iterator_complete(&iawait_result) {
                    Ok(d) => d,
                    Err(e) => {
                        self.generator_inline_iters.remove(&gen_id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let _ = self.call_function(ret_reject, &JsValue::UNDEFINED, &[e]);
                        if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                            queue.pop_front();
                        }
                        self.async_gen_process_queue(gen_this);
                        return;
                    }
                };
                let value = match self.iterator_value(&iawait_result) {
                    Ok(v) => v,
                    Err(e) => {
                        self.generator_inline_iters.remove(&gen_id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let _ = self.call_function(ret_reject, &JsValue::UNDEFINED, &[e]);
                        if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                            queue.pop_front();
                        }
                        self.async_gen_process_queue(gen_this);
                        return;
                    }
                };
                if done {
                    self.generator_inline_iters.remove(&gen_id);
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    let ret_promise_id = if let Some(po) = (ret_promise)
                        .as_object_id()
                        .map(|id| crate::types::JsObject { id })
                    {
                        po.id
                    } else {
                        0
                    };
                    let _ = self.async_generator_await_return(value, ret_promise_id);
                    if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                        queue.pop_front();
                    }
                    self.async_gen_process_queue(gen_this);
                } else {
                    // Not done — yield the value and continue delegation
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: deleg_info.resume_state,
                            },
                            _sent_value: JsValue::UNDEFINED,
                            try_stack,
                            pending_binding: None,
                            delegated_iterator: Some(deleg_info),
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    let iter_result = self.create_iter_result_object(value, false);
                    let _ = self.call_function(ret_resolve, &JsValue::UNDEFINED, &[iter_result]);
                    if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                        queue.pop_front();
                    }
                    self.async_gen_process_queue(gen_this);
                }
            }
            Ok(None) => {
                // No .return() method — §15.5.5 step 8.c.iii: Await(received.[[Value]])
                self.generator_inline_iters.remove(&gen_id);
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    },
                );
                let ret_promise_id = if let Some(po) = (ret_promise)
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                {
                    po.id
                } else {
                    0
                };
                let _ = self.async_generator_await_return(awaited_val, ret_promise_id);
                if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                    queue.pop_front();
                }
                self.async_gen_process_queue(gen_this);
            }
            Err(e) => {
                self.generator_inline_iters.remove(&gen_id);
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    },
                );
                let _ = self.call_function(ret_reject, &JsValue::UNDEFINED, &[e]);
                if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
                    queue.pop_front();
                }
                self.async_gen_process_queue(gen_this);
            }
        }
    }

    fn async_generator_next_state_machine_with_promise(
        &mut self,
        this: &JsValue,
        sent_value: JsValue,
        promise: JsValue,
        resolve_fn: JsValue,
        reject_fn: JsValue,
    ) -> Completion {
        let caller_realm = self.current_realm_id;
        if let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(obj_rc) = self.get_object(o.id)
            && let Some(realm_id) = obj_rc.borrow().generator_realm_id
        {
            self.current_realm_id = realm_id;
        }
        let result = self.async_generator_next_state_machine_impl(
            this, sent_value, promise, resolve_fn, reject_fn,
        );
        self.current_realm_id = caller_realm;
        result
    }

    fn async_generator_next_state_machine_impl(
        &mut self,
        this: &JsValue,
        sent_value: JsValue,
        promise: JsValue,
        resolve_fn: JsValue,
        reject_fn: JsValue,
    ) -> Completion {
        use crate::interpreter::generator_transform::{LoopControlTarget, StateTerminator};

        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.next called on non-object");
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.next called on non-object");
        };

        let state = obj_rc.borrow().iterator_state().cloned();
        let Some(IteratorState::StateMachineAsyncGenerator {
            state_machine,
            func_env,
            is_strict,
            execution_state,
            try_stack,
            pending_binding,
            delegated_iterator,
            pending_exception: stored_pending_exception,
            pending_return: stored_pending_return,
            ..
        }) = state
        else {
            return self.reject_with_type_error("not a state machine async generator");
        };

        if let Some(ref deleg_info) = delegated_iterator {
            let iterator = deleg_info.iterator.clone();
            let next_method = deleg_info.next_method.clone();
            let resume_state = deleg_info.resume_state;
            let binding = deleg_info.sent_value_binding.clone();

            // Handle .return() during yield* delegation
            if let Some(ret_val) = stored_pending_return {
                match self.iterator_return(&iterator, &ret_val) {
                    Ok(Some(iter_result)) => {
                        let awaited_result = match self.await_value(&iter_result) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            _ => iter_result,
                        };
                        let done = match self.iterator_complete(&awaited_result) {
                            Ok(d) => d,
                            Err(e) => {
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        };
                        let value = match self.iterator_value(&awaited_result) {
                            Ok(v) => v,
                            Err(e) => {
                                // IteratorValue threw — route through try/catch stack
                                // per spec §15.5.5 step 7.c.ix: ? IteratorValue(innerReturnResult)
                                // Route through the state machine's try/catch handling
                                let has_catch = try_stack.iter().rev().any(|tc| {
                                    !tc.entered_catch
                                        && !tc.entered_finally
                                        && tc.catch_state.is_some()
                                });
                                if has_catch {
                                    obj_rc.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::StateMachineAsyncGenerator {
                                                state_machine: state_machine.clone(),
                                                func_env: func_env.clone(),
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::SuspendedAtState {
                                                        state_id: resume_state,
                                                    },
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: try_stack.clone(),
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: Some(e),
                                                pending_return: None,
                                            },
                                        );
                                    return self.async_generator_next_state_machine_with_promise(
                                        this,
                                        JsValue::UNDEFINED,
                                        promise,
                                        resolve_fn,
                                        reject_fn,
                                    );
                                }
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        };
                        if done {
                            self.generator_inline_iters.remove(&o.id);
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let promise_id = if let Some(po) = (promise)
                                .as_object_id()
                                .map(|id| crate::types::JsObject { id })
                            {
                                po.id
                            } else {
                                0
                            };
                            return self.async_generator_await_return(value, promise_id);
                        } else {
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state:
                                            StateMachineExecutionState::SuspendedAtState {
                                                state_id: resume_state,
                                            },
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack,
                                        pending_binding: None,
                                        delegated_iterator: Some(
                                            crate::interpreter::types::DelegatedIteratorInfo {
                                                iterator,
                                                next_method: next_method.clone(),
                                                resume_state,
                                                sent_value_binding: binding,
                                            },
                                        ),
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let iter_result = self.create_iter_result_object(value, false);
                            let _ = self.call_function(
                                &resolve_fn,
                                &JsValue::UNDEFINED,
                                &[iter_result],
                            );
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                    }
                    Ok(None) => {
                        // No .return() method — complete the generator
                        // §15.5.5 step 7.c.iii.1: Await(received.[[Value]])
                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let promise_id = if let Some(po) = (promise)
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                        {
                            po.id
                        } else {
                            0
                        };
                        return self.async_generator_await_return(ret_val, promise_id);
                    }
                    Err(e) => {
                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                }
            }

            // Handle .throw() during yield* delegation
            if let Some(exc) = stored_pending_exception {
                match self.iterator_throw(&iterator, &exc) {
                    Ok(Some(iter_result)) => {
                        let awaited_result = match self.await_value(&iter_result) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            _ => iter_result,
                        };
                        let done = match self.iterator_complete(&awaited_result) {
                            Ok(d) => d,
                            Err(e) => {
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        };
                        let value = match self.iterator_value(&awaited_result) {
                            Ok(v) => v,
                            Err(e) => {
                                // IteratorValue threw — route through try/catch stack
                                // per spec §15.5.5 step 7.b.ii.7: ? IteratorValue(innerResult)
                                let has_catch = try_stack.iter().rev().any(|tc| {
                                    !tc.entered_catch
                                        && !tc.entered_finally
                                        && tc.catch_state.is_some()
                                });
                                if has_catch {
                                    obj_rc.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::StateMachineAsyncGenerator {
                                                state_machine: state_machine.clone(),
                                                func_env: func_env.clone(),
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::SuspendedAtState {
                                                        state_id: resume_state,
                                                    },
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: try_stack.clone(),
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: Some(e),
                                                pending_return: None,
                                            },
                                        );
                                    return self.async_generator_next_state_machine_with_promise(
                                        this,
                                        JsValue::UNDEFINED,
                                        promise,
                                        resolve_fn,
                                        reject_fn,
                                    );
                                }
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        };
                        if done {
                            if let Some(ref bind) = binding {
                                use crate::interpreter::generator_transform::SentValueBindingKind;
                                match &bind.kind {
                                    SentValueBindingKind::Variable(name) => {
                                        self.env_set(&func_env, name, value.clone()).ok();
                                    }
                                    SentValueBindingKind::Pattern(pattern) => {
                                        let _ = self.bind_pattern(
                                            pattern,
                                            value.clone(),
                                            BindingKind::Var,
                                            &func_env,
                                        );
                                    }
                                    SentValueBindingKind::Discard
                                    | SentValueBindingKind::InlineYield { .. } => {}
                                }
                            }
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine: state_machine.clone(),
                                        func_env: func_env.clone(),
                                        is_strict,
                                        execution_state:
                                            StateMachineExecutionState::SuspendedAtState {
                                                state_id: resume_state,
                                            },
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: try_stack.clone(),
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            return self.async_generator_next_state_machine_with_promise(
                                this,
                                JsValue::UNDEFINED,
                                promise,
                                resolve_fn,
                                reject_fn,
                            );
                        } else {
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state:
                                            StateMachineExecutionState::SuspendedAtState {
                                                state_id: resume_state,
                                            },
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack,
                                        pending_binding: None,
                                        delegated_iterator: Some(
                                            crate::interpreter::types::DelegatedIteratorInfo {
                                                iterator,
                                                next_method: next_method.clone(),
                                                resume_state,
                                                sent_value_binding: binding,
                                            },
                                        ),
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let iter_result = self.create_iter_result_object(value, false);
                            let _ = self.call_function(
                                &resolve_fn,
                                &JsValue::UNDEFINED,
                                &[iter_result],
                            );
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                    }
                    Ok(None) => {
                        // No .throw() method — close iterator and throw TypeError
                        let _ = self.iterator_close(&iterator, exc.clone());
                        let type_err =
                            self.create_type_error("The iterator does not provide a throw method");
                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[type_err]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    Err(e) => {
                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                }
            }

            let result = match self.call_function(
                &next_method,
                &iterator,
                std::slice::from_ref(&sent_value),
            ) {
                Completion::Normal(v) if (v).is_object() => Ok(v),
                Completion::Normal(_) => {
                    Err(self.create_type_error("Iterator result is not an object"))
                }
                Completion::Throw(e) => Err(e),
                _ => Err(self.create_type_error("Iterator next failed")),
            };
            match result {
                Ok(iter_result) => {
                    // Await the iterator result (inner async iterators return promises)
                    let awaited_result = match self.await_value(&iter_result) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            self.generator_inline_iters.remove(&o.id);
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                        _ => iter_result,
                    };
                    let done = match self.iterator_complete(&awaited_result) {
                        Ok(d) => d,
                        Err(e) => {
                            self.generator_inline_iters.remove(&o.id);
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                    };
                    let value = match self.iterator_value(&awaited_result) {
                        Ok(v) => v,
                        Err(e) => {
                            // Propagate through generator's try/catch stack
                            let mut ts = try_stack.clone();
                            for i in (0..ts.len()).rev() {
                                if !ts[i].entered_catch
                                    && !ts[i].entered_finally
                                    && let Some(catch_state) = ts[i].catch_state
                                {
                                    ts.truncate(i);
                                    obj_rc.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::StateMachineAsyncGenerator {
                                                state_machine: state_machine.clone(),
                                                func_env: func_env.clone(),
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::SuspendedAtState {
                                                        state_id: catch_state,
                                                    },
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: ts,
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: Some(e),
                                                pending_return: None,
                                            },
                                        );
                                    return self.async_generator_next_state_machine_with_promise(
                                        this,
                                        JsValue::UNDEFINED,
                                        promise,
                                        resolve_fn,
                                        reject_fn,
                                    );
                                }
                            }
                            self.generator_inline_iters.remove(&o.id);
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                    };
                    if done {
                        if let Some(ref bind) = binding {
                            use crate::interpreter::generator_transform::SentValueBindingKind;
                            match &bind.kind {
                                SentValueBindingKind::Variable(name) => {
                                    let mut env = func_env.borrow_mut();
                                    let needs_init = env
                                        .bindings
                                        .get(name.as_str())
                                        .is_some_and(|b| !b.initialized);
                                    if needs_init {
                                        env.initialize_binding(name, value.clone());
                                    } else {
                                        env.set(name, value.clone()).ok();
                                    }
                                }
                                SentValueBindingKind::Pattern(pattern) => {
                                    let _ = self.bind_pattern(
                                        pattern,
                                        value.clone(),
                                        BindingKind::Var,
                                        &func_env,
                                    );
                                }
                                SentValueBindingKind::Discard
                                | SentValueBindingKind::InlineYield { .. } => {}
                            }
                        }
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        // Reuse the same promise — don't create a new one
                        return self.async_generator_next_state_machine_with_promise(
                            this,
                            JsValue::UNDEFINED,
                            promise,
                            resolve_fn,
                            reject_fn,
                        );
                    } else {
                        // Per spec §14.4.13 step 7.a.vi: for async generators,
                        // yield the value directly without awaiting (AsyncGeneratorYield)
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::UNDEFINED,
                                try_stack,
                                pending_binding: None,
                                delegated_iterator: Some(
                                    crate::interpreter::types::DelegatedIteratorInfo {
                                        iterator,
                                        next_method: next_method.clone(),
                                        resume_state,
                                        sent_value_binding: binding,
                                    },
                                ),
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let iter_result = self.create_iter_result_object(value, false);
                        let _ =
                            self.call_function(&resolve_fn, &JsValue::UNDEFINED, &[iter_result]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                }
                Err(e) => {
                    self.generator_inline_iters.remove(&o.id);
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                    self.drain_microtasks();
                    return Completion::Normal(promise);
                }
            }
        }

        let current_state_id = match &execution_state {
            StateMachineExecutionState::Completed => {
                let result = self.create_iter_result_object(JsValue::UNDEFINED, true);
                let _ = self.call_function(&resolve_fn, &JsValue::UNDEFINED, &[result]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            StateMachineExecutionState::Executing => {
                let err = self.create_type_error("AsyncGenerator is already executing");
                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[err]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            StateMachineExecutionState::SuspendedStart => 0,
            StateMachineExecutionState::SuspendedAtState { state_id } => *state_id,
        };

        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
            IteratorState::StateMachineAsyncGenerator {
                state_machine: state_machine.clone(),
                func_env: func_env.clone(),
                is_strict,
                execution_state: StateMachineExecutionState::Executing,
                _sent_value: sent_value.clone(),
                try_stack: try_stack.clone(),
                pending_binding: None,
                delegated_iterator: None,
                pending_exception: None,
                pending_return: None,
            },
        );

        use crate::interpreter::generator_transform::SentValueBindingKind;
        let mut initial_inline_yield_target: Option<usize> = None;
        let mut initial_inline_yield_sent: Option<JsValue> = None;
        let mut initial_inline_yield_prev_sent: Option<Vec<JsValue>> = None;
        if let Some(binding) = pending_binding {
            match &binding.kind {
                SentValueBindingKind::Variable(name) => {
                    let mut env = func_env.borrow_mut();
                    let needs_init = env
                        .bindings
                        .get(name.as_str())
                        .is_some_and(|b| !b.initialized);
                    if needs_init {
                        env.initialize_binding(name, sent_value.clone());
                    } else {
                        env.set(name, sent_value.clone()).ok();
                    }
                }
                SentValueBindingKind::Pattern(pattern) => {
                    let _ =
                        self.bind_pattern(pattern, sent_value.clone(), BindingKind::Var, &func_env);
                }
                SentValueBindingKind::Discard => {}
                SentValueBindingKind::InlineYield {
                    yield_target,
                    prev_sent,
                } => {
                    initial_inline_yield_target = Some(*yield_target);
                    initial_inline_yield_sent = Some(sent_value.clone());
                    let mut new_prev = prev_sent.clone();
                    new_prev.push(sent_value.clone());
                    initial_inline_yield_prev_sent = Some(new_prev);
                }
            }
        }

        func_env.borrow_mut().strict = is_strict;
        let saved_in_state_machine = self.in_state_machine;
        self.in_state_machine = true;
        let mut current_id = current_state_id;
        let mut current_try_stack = try_stack;
        let check_abrupt_on_resume =
            stored_pending_exception.is_some() || stored_pending_return.is_some();
        let mut pending_exception: Option<JsValue> = stored_pending_exception;
        let mut pending_return: Option<JsValue> = stored_pending_return;
        let mut inline_yield_target: Option<usize> = initial_inline_yield_target;
        let mut inline_yield_sent: Option<JsValue> = initial_inline_yield_sent;
        let mut inline_yield_prev_sent: Option<Vec<JsValue>> = initial_inline_yield_prev_sent;
        let mut check_abrupt_on_resume = check_abrupt_on_resume;
        let mut for_of_stack = self
            .generator_for_of_stacks
            .get(&o.id)
            .cloned()
            .unwrap_or_default();
        loop {
            if check_abrupt_on_resume {
                check_abrupt_on_resume = false;
                // Check pending_exception before executing state (handles .throw() with no try/catch)
                if let Some(exc) = pending_exception.take() {
                    let mut handled = false;
                    for i in (0..current_try_stack.len()).rev() {
                        if !current_try_stack[i].entered_catch
                            && !current_try_stack[i].entered_finally
                        {
                            if let Some(catch_state) = current_try_stack[i].catch_state {
                                pending_exception = Some(exc.clone());
                                current_id = catch_state;
                                handled = true;
                                break;
                            } else if let Some(finally_state) = current_try_stack[i].finally_state {
                                pending_exception = Some(exc.clone());
                                current_id = finally_state;
                                handled = true;
                                break;
                            }
                        }
                    }
                    if handled {
                        continue;
                    }
                    let disp = self.dispose_resources(&func_env, Completion::Throw(exc));
                    let exc = match disp {
                        Completion::Throw(e) => e,
                        _ => unreachable!(),
                    };
                    self.generator_inline_iters.remove(&o.id);
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[exc]);
                    self.drain_microtasks();
                    return Completion::Normal(promise);
                }
                // Check pending_return before executing state (handles .return() with no try/catch)
                if let Some(ret_val) = pending_return.take() {
                    if current_try_stack.is_empty() {
                        if let Some(iters) = self.generator_inline_iters.remove(&o.id) {
                            for iter in iters {
                                if let Err(e) = self.iterator_close_result(&iter) {
                                    self.generator_inline_iters.remove(&o.id);
                                    obj_rc.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::StateMachineAsyncGenerator {
                                                state_machine,
                                                func_env,
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::Completed,
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: vec![],
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: None,
                                                pending_return: None,
                                            },
                                        );
                                    let _ =
                                        self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                    self.drain_microtasks();
                                    return Completion::Normal(promise);
                                }
                            }
                        }
                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let promise_id = if let Some(po) = (promise)
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                        {
                            po.id
                        } else {
                            0
                        };
                        return self.async_generator_await_return(ret_val, promise_id);
                    }
                    // Has try/finally — route to finally handler
                    let mut return_handled = false;
                    for i in (0..current_try_stack.len()).rev() {
                        if !current_try_stack[i].entered_finally
                            && let Some(finally_state) = current_try_stack[i].finally_state
                        {
                            pending_return = Some(ret_val.clone());
                            current_id = finally_state;
                            return_handled = true;
                            break;
                        }
                    }
                    if return_handled {
                        continue;
                    }
                    // No finally — just propagate
                    pending_return = Some(ret_val);
                }
            } // end if check_abrupt_on_resume

            let terminator = state_machine.states[current_id].terminator.clone();

            let is_inline_replay = inline_yield_target.is_some();
            if let Some(target) = inline_yield_target.take() {
                let _sv = inline_yield_sent.take().unwrap_or(JsValue::UNDEFINED);
                let prev = inline_yield_prev_sent.take().unwrap_or_default();
                self.generator_context = Some(GeneratorContext {
                    target_yield: target,
                    current_yield: 0,
                    prev_sent_values: prev,
                    is_async: true,
                    resume_kind: GeneratorResumeKind::Next,
                });
            }

            self.in_state_machine = true;
            let term_env = for_of_stack
                .last()
                .map_or(&func_env, ForOfLoopState::effective_env)
                .clone();
            let mut stmt_result = self.exec_body(&state_machine.states[current_id].body, &term_env);
            self.in_state_machine = saved_in_state_machine;
            while let Completion::TailCall { func, this, args } = stmt_result {
                stmt_result = self.call_function(&func, &this, &args);
            }
            let ctx_after = if is_inline_replay {
                self.generator_context.take()
            } else {
                None
            };

            if let Completion::Exit(code) = stmt_result {
                // `__host_exit` (issue #242) is uncatchable and immediate:
                // complete the async generator without routing to its
                // catch/finally states, disposing, or settling the result
                // promise, and propagate the exit.
                self.generator_inline_iters.remove(&o.id);
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    },
                );
                return Completion::Exit(code);
            }
            if let Completion::Throw(e) = stmt_result {
                // Route genuine throws through the async generator body's
                // catch/finally states (a `Completion::Exit` was handled above
                // and never reaches here).
                if let Some(try_info) = current_try_stack.pop() {
                    if let Some(catch_state) = try_info.catch_state {
                        pending_exception = Some(e);
                        current_id = catch_state;
                        continue;
                    } else if let Some(finally_state) = try_info.finally_state {
                        current_id = finally_state;
                        continue;
                    }
                }
                // §27.6.3.3: DisposeResources when async generator throws
                let disp = self.dispose_resources(&func_env, Completion::Throw(e));
                let e = match disp {
                    Completion::Throw(e) => e,
                    _ => unreachable!(),
                };
                self.generator_inline_iters.remove(&o.id);
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    },
                );
                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            if let Completion::Return(v) = stmt_result {
                // §27.6.3.3: DisposeResources when async generator returns
                let disp = self.dispose_resources(&func_env, Completion::Return(v));
                let v = match disp {
                    Completion::Return(v) => v,
                    Completion::Throw(e) => {
                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    _ => JsValue::UNDEFINED,
                };
                let awaited = match self.await_value(&v) {
                    Completion::Normal(av) => av,
                    Completion::Throw(e) => {
                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    _ => JsValue::UNDEFINED,
                };
                self.generator_inline_iters.remove(&o.id);
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    },
                );
                let iter_result = self.create_iter_result_object(awaited, true);
                let _ = self.call_function(&resolve_fn, &JsValue::UNDEFINED, &[iter_result]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            if let Completion::Yield(yield_val) = stmt_result {
                let _is_destructuring = self.destructuring_yield;
                self.destructuring_yield = false;
                let awaited_val = match self.await_value(&yield_val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    _ => yield_val,
                };
                let pending = std::mem::take(&mut self.pending_iter_close);
                if pending.is_empty() {
                    self.generator_inline_iters.remove(&o.id);
                } else {
                    self.generator_inline_iters.insert(o.id, pending);
                }
                self.sync_generator_for_of_stack(o.id, &for_of_stack);
                // Any Completion::Yield from exec_statements is an inline yield:
                // it came from a loop body or complex control flow that isn't
                // decomposed by the state machine transformer. Use InlineYield
                // to re-enter the same state and fast-forward past previous yields.
                {
                    let yield_count = ctx_after.as_ref().map(|c| c.current_yield).unwrap_or(1);
                    let inline_prev = ctx_after.map(|c| c.prev_sent_values).unwrap_or_default();
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineAsyncGenerator {
                            state_machine: state_machine.clone(),
                            func_env: func_env.clone(),
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: current_id,
                            },
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: current_try_stack.clone(),
                            pending_binding: Some(
                                crate::interpreter::generator_transform::SentValueBinding {
                                    kind: SentValueBindingKind::InlineYield {
                                        yield_target: yield_count,
                                        prev_sent: inline_prev,
                                    },
                                },
                            ),
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                }
                let iter_result = self.create_iter_result_object(awaited_val, false);
                let _ = self.call_function(&resolve_fn, &JsValue::UNDEFINED, &[iter_result]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }

            match &terminator {
                StateTerminator::Yield {
                    value,
                    is_delegate,
                    resume_state,
                    sent_value_binding,
                } => {
                    let yield_val = if let Some(expr) = value {
                        match self.eval_expr(expr, &term_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                // Route genuine throws through the try-stack for
                                // catch/finally handling (a `Completion::Exit`
                                // takes the `other` arm below and never reaches
                                // here — issue #242).
                                if let Some(try_info) = current_try_stack.pop() {
                                    if let Some(catch_state) = try_info.catch_state {
                                        pending_exception = Some(e);
                                        current_id = catch_state;
                                        continue;
                                    } else if let Some(finally_state) = try_info.finally_state {
                                        current_id = finally_state;
                                        continue;
                                    }
                                }
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            other => {
                                if let Completion::Yield(yv) = other {
                                    yv
                                } else {
                                    JsValue::UNDEFINED
                                }
                            }
                        }
                    } else {
                        JsValue::UNDEFINED
                    };

                    if *is_delegate {
                        let iterator = match self.get_async_iterator(&yield_val) {
                            Ok(it) => it,
                            Err(e) => match self.get_iterator(&yield_val) {
                                Ok(it) => it,
                                Err(_) => {
                                    self.generator_inline_iters.remove(&o.id);
                                    obj_rc.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::StateMachineAsyncGenerator {
                                                state_machine,
                                                func_env,
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::Completed,
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: vec![],
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: None,
                                                pending_return: None,
                                            },
                                        );
                                    let _ =
                                        self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                    self.drain_microtasks();
                                    return Completion::Normal(promise);
                                }
                            },
                        };

                        let next_method = if let Some(io) = iterator
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                        {
                            if let Some(cached) = self.iterator_next_cache.get(&io.id).cloned() {
                                cached
                            } else {
                                match self.get_object_property(io.id, "next", &iterator) {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => {
                                        self.generator_inline_iters.remove(&o.id);
                                        obj_rc.borrow_mut().kind =
                                            crate::interpreter::types::ObjectKind::Iterator(
                                                IteratorState::StateMachineAsyncGenerator {
                                                    state_machine,
                                                    func_env,
                                                    is_strict,
                                                    execution_state:
                                                        StateMachineExecutionState::Completed,
                                                    _sent_value: JsValue::UNDEFINED,
                                                    try_stack: vec![],
                                                    pending_binding: None,
                                                    delegated_iterator: None,
                                                    pending_exception: None,
                                                    pending_return: None,
                                                },
                                            );
                                        let _ = self.call_function(
                                            &reject_fn,
                                            &JsValue::UNDEFINED,
                                            &[e],
                                        );
                                        self.drain_microtasks();
                                        return Completion::Normal(promise);
                                    }
                                    _ => JsValue::UNDEFINED,
                                }
                            }
                        } else {
                            JsValue::UNDEFINED
                        };

                        let iter_result = match self.call_function(
                            &next_method,
                            &iterator,
                            &[JsValue::UNDEFINED],
                        ) {
                            Completion::Normal(v) if (v).is_object() => Ok(v),
                            Completion::Normal(_) => {
                                Err(self.create_type_error("Iterator result is not an object"))
                            }
                            Completion::Throw(e) => Err(e),
                            _ => Err(self.create_type_error("Iterator next failed")),
                        };
                        let iter_result = match iter_result {
                            Ok(r) => r,
                            Err(e) => {
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        };

                        // §15.5.5 step 8.a.ii: Await(innerResult)
                        // Must suspend the generator properly (not drain microtasks)
                        // so that microtasks enqueued before it.next() get a chance
                        // to fire before the generator resumes.
                        let wrapped = self.promise_resolve_value(&iter_result);
                        let wrapped_id = if let Some(wo) = (wrapped)
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                        {
                            wo.id
                        } else {
                            0
                        };

                        // Save state with delegated_iterator for resumption
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: *resume_state,
                                },
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: current_try_stack,
                                pending_binding: sent_value_binding.clone(),
                                delegated_iterator: Some(
                                    crate::interpreter::types::DelegatedIteratorInfo {
                                        iterator: iterator.clone(),
                                        next_method: next_method.clone(),
                                        resume_state: *resume_state,
                                        sent_value_binding: sent_value_binding.clone(),
                                    },
                                ),
                                pending_exception: None,
                                pending_return: None,
                            },
                        );

                        let promise_c = promise.clone();
                        let resolve_fn_c = resolve_fn.clone();
                        let reject_fn_c = reject_fn.clone();
                        let gen_this = this.clone();
                        let gen_id = o.id;

                        // Fulfillment handler: called when Await(innerResult) resolves.
                        // Implements yield* step 8.a.iii-vi + AsyncGeneratorYield.
                        let fulfill_handler = self.create_function(JsFunction::native(
                            "yieldStarAwaitFulfill".to_string(),
                            1,
                            move |interp, _this, args| {
                                let awaited_result =
                                    args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                                interp.yield_star_await_inner_result_resume(
                                    &gen_this,
                                    gen_id,
                                    awaited_result,
                                    &promise_c,
                                    &resolve_fn_c,
                                    &reject_fn_c,
                                    false,
                                );
                                Completion::Normal(JsValue::UNDEFINED)
                            },
                        ));

                        let promise_c2 = promise.clone();
                        let reject_fn_c2 = reject_fn.clone();
                        let gen_this2 = this.clone();
                        let gen_id2 = o.id;
                        let reject_handler = self.create_function(JsFunction::native(
                            "yieldStarAwaitReject".to_string(),
                            1,
                            move |interp, _this, args| {
                                let reason = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                                interp.yield_star_await_inner_result_resume(
                                    &gen_this2,
                                    gen_id2,
                                    reason,
                                    &promise_c2,
                                    &JsValue::UNDEFINED,
                                    &reject_fn_c2,
                                    true,
                                );
                                Completion::Normal(JsValue::UNDEFINED)
                            },
                        ));

                        let wrapped_state = self.get_promise_state(wrapped_id);
                        match wrapped_state {
                            Some(PromiseState::Fulfilled(v)) => {
                                let handler = fulfill_handler;
                                let val = v.clone();
                                self.scheduler.enqueue_microtask((
                                    vec![val.clone(), handler.clone()],
                                    Box::new(move |interp| {
                                        let _ = interp.call_function(
                                            &handler,
                                            &JsValue::UNDEFINED,
                                            &[val],
                                        );
                                        Completion::Normal(JsValue::UNDEFINED)
                                    }),
                                ));
                            }
                            Some(PromiseState::Rejected(r)) => {
                                let handler = reject_handler;
                                let reason = r.clone();
                                self.scheduler.enqueue_microtask((
                                    vec![reason.clone(), handler.clone()],
                                    Box::new(move |interp| {
                                        let _ = interp.call_function(
                                            &handler,
                                            &JsValue::UNDEFINED,
                                            &[reason],
                                        );
                                        Completion::Normal(JsValue::UNDEFINED)
                                    }),
                                ));
                            }
                            Some(PromiseState::Pending) => {
                                if let Some(obj) = self.get_object_cell(wrapped_id) {
                                    let mut ob = obj.borrow_mut();
                                    if let Some(pd) = ob.promise_data_mut() {
                                        pd.is_handled = true;
                                        pd.fulfill_reactions.push(PromiseReaction {
                                            handler: Some(fulfill_handler),
                                            promise_id: None,
                                            resolve: JsValue::UNDEFINED,
                                            reject: JsValue::UNDEFINED,
                                            reaction_type: PromiseReactionType::Fulfill,
                                        });
                                        pd.reject_reactions.push(PromiseReaction {
                                            handler: Some(reject_handler),
                                            promise_id: None,
                                            resolve: JsValue::UNDEFINED,
                                            reject: JsValue::UNDEFINED,
                                            reaction_type: PromiseReactionType::Reject,
                                        });
                                    }
                                }
                            }
                            None => {
                                // Not a promise — treat as immediately fulfilled
                                let handler = fulfill_handler;
                                let val = iter_result.clone();
                                self.scheduler.enqueue_microtask((
                                    vec![val.clone(), handler.clone()],
                                    Box::new(move |interp| {
                                        let _ = interp.call_function(
                                            &handler,
                                            &JsValue::UNDEFINED,
                                            &[val],
                                        );
                                        Completion::Normal(JsValue::UNDEFINED)
                                    }),
                                ));
                            }
                        }

                        self.scheduler.set_async_gen_yield_pending(true);
                        return Completion::Normal(promise);
                    }

                    // Check if yield value is a pending promise — need async suspension
                    let wrapped = self.promise_resolve_value(&yield_val);
                    let wrapped_id = if let Some(wo) = (wrapped)
                        .as_object_id()
                        .map(|id| crate::types::JsObject { id })
                    {
                        wo.id
                    } else {
                        0
                    };
                    let wrapped_state = self.get_promise_state(wrapped_id);

                    if matches!(wrapped_state, Some(PromiseState::Pending)) {
                        let pending = std::mem::take(&mut self.pending_iter_close);
                        if pending.is_empty() {
                            self.generator_inline_iters.remove(&o.id);
                        } else {
                            self.generator_inline_iters.insert(o.id, pending);
                        }
                        // Suspend generator and register callbacks for when promise resolves
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: *resume_state,
                                },
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: current_try_stack,
                                pending_binding: sent_value_binding.clone(),
                                delegated_iterator: None,
                                pending_exception: pending_exception.take(),
                                pending_return: pending_return.take(),
                            },
                        );

                        let resolve_fn_c = resolve_fn.clone();
                        let reject_fn_c = reject_fn.clone();
                        let gen_this = this.clone();
                        let gen_id = o.id;

                        let fulfill_handler = self.create_function(JsFunction::native(
                            "asyncGenYieldFulfill".to_string(),
                            1,
                            move |interp, _this, args| {
                                let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                                let iter_result = interp.create_iter_result_object(v, false);
                                let _ = interp.call_function(
                                    &resolve_fn_c,
                                    &JsValue::UNDEFINED,
                                    &[iter_result],
                                );
                                if let Some(queue) = interp.scheduler.async_gen_queue_mut(gen_id) {
                                    queue.pop_front();
                                }
                                interp.async_gen_process_queue(&gen_this);
                                Completion::Normal(JsValue::UNDEFINED)
                            },
                        ));

                        let gen_this2 = this.clone();
                        let gen_id2 = o.id;
                        let reject_handler = self.create_function(JsFunction::native(
                            "asyncGenYieldReject".to_string(),
                            1,
                            move |interp, _this, args| {
                                let e = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                                let _ =
                                    interp.call_function(&reject_fn_c, &JsValue::UNDEFINED, &[e]);
                                if let Some(queue) = interp.scheduler.async_gen_queue_mut(gen_id2) {
                                    queue.pop_front();
                                }
                                interp.async_gen_process_queue(&gen_this2);
                                Completion::Normal(JsValue::UNDEFINED)
                            },
                        ));

                        if let Some(obj) = self.get_object_cell(wrapped_id) {
                            let mut ob = obj.borrow_mut();
                            if let Some(pd) = ob.promise_data_mut() {
                                pd.is_handled = true;
                                pd.fulfill_reactions.push(PromiseReaction {
                                    handler: Some(fulfill_handler),
                                    promise_id: None,
                                    resolve: JsValue::UNDEFINED,
                                    reject: JsValue::UNDEFINED,
                                    reaction_type: PromiseReactionType::Fulfill,
                                });
                                pd.reject_reactions.push(PromiseReaction {
                                    handler: Some(reject_handler),
                                    promise_id: None,
                                    resolve: JsValue::UNDEFINED,
                                    reject: JsValue::UNDEFINED,
                                    reaction_type: PromiseReactionType::Reject,
                                });
                            }
                        }

                        self.scheduler.set_async_gen_yield_pending(true);
                        return Completion::Normal(promise);
                    }

                    // Already-resolved path: value is not a pending promise.
                    // Per spec §27.6.3.8 AsyncGeneratorYield, we must still go through
                    // a microtask boundary (Await always creates a PromiseReactionJob).
                    let awaited_val = if let Some(PromiseState::Fulfilled(v)) = wrapped_state {
                        v
                    } else if let Some(PromiseState::Rejected(e)) = wrapped_state {
                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let reject_fn_c2 = reject_fn.clone();
                        let gen_this3 = this.clone();
                        let gen_id3 = o.id;
                        self.scheduler.enqueue_microtask((
                            vec![e.clone(), reject_fn_c2.clone(), gen_this3.clone()],
                            Box::new(move |interp| {
                                let _ =
                                    interp.call_function(&reject_fn_c2, &JsValue::UNDEFINED, &[e]);
                                if let Some(queue) = interp.scheduler.async_gen_queue_mut(gen_id3) {
                                    queue.pop_front();
                                }
                                interp.async_gen_process_queue(&gen_this3);
                                Completion::Normal(JsValue::UNDEFINED)
                            }),
                        ));
                        self.scheduler.set_async_gen_yield_pending(true);
                        return Completion::Normal(promise);
                    } else {
                        yield_val
                    };

                    let pending = std::mem::take(&mut self.pending_iter_close);
                    if pending.is_empty() {
                        self.generator_inline_iters.remove(&o.id);
                    } else {
                        self.generator_inline_iters.insert(o.id, pending);
                    }
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: *resume_state,
                            },
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: current_try_stack,
                            pending_binding: sent_value_binding.clone(),
                            delegated_iterator: None,
                            pending_exception: pending_exception.take(),
                            pending_return: pending_return.take(),
                        },
                    );

                    // Schedule resolution via microtask to ensure proper interleaving
                    let resolve_fn_c2 = resolve_fn.clone();
                    let gen_this3 = this.clone();
                    let gen_id3 = o.id;
                    self.scheduler.enqueue_microtask((
                        vec![
                            awaited_val.clone(),
                            resolve_fn_c2.clone(),
                            gen_this3.clone(),
                        ],
                        Box::new(move |interp| {
                            let iter_result = interp.create_iter_result_object(awaited_val, false);
                            let _ = interp.call_function(
                                &resolve_fn_c2,
                                &JsValue::UNDEFINED,
                                &[iter_result],
                            );
                            if let Some(queue) = interp.scheduler.async_gen_queue_mut(gen_id3) {
                                queue.pop_front();
                            }
                            // Process next queue item inline (not via microtask) per spec
                            interp.async_gen_process_queue(&gen_this3);
                            Completion::Normal(JsValue::UNDEFINED)
                        }),
                    ));
                    self.scheduler.set_async_gen_yield_pending(true);
                    return Completion::Normal(promise);
                }

                StateTerminator::Return(expr) => {
                    if let Some(e) = expr {
                        // return expr; — §13.10.1 step 3: Await(exprValue)
                        let mut result = self.eval_expr(e, &term_env);
                        while let Completion::TailCall { func, this, args } = result {
                            result = self.call_function(&func, &this, &args);
                        }
                        let ret_val = match result {
                            Completion::Normal(v) => v,
                            Completion::Throw(err) => {
                                let disp =
                                    self.dispose_resources(&func_env, Completion::Throw(err));
                                let err = match disp {
                                    Completion::Throw(e) => e,
                                    _ => unreachable!(),
                                };
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[err]);
                                return Completion::Normal(promise);
                            }
                            other => {
                                if let Completion::Yield(yv) = other {
                                    yv
                                } else {
                                    JsValue::UNDEFINED
                                }
                            }
                        };

                        // §27.6.3.3: DisposeResources
                        let disp =
                            self.dispose_resources(&func_env, Completion::Return(ret_val.clone()));
                        match disp {
                            Completion::Return(_) => {}
                            Completion::Throw(e) => {
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                return Completion::Normal(promise);
                            }
                            _ => {}
                        }

                        // Microtask-based Await: wrap in PromiseResolve, schedule via PerformPromiseThen
                        let wrapper = self.promise_resolve_value(&ret_val);

                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );

                        let gen_id = o.id;
                        let gen_this_f = this.clone();
                        let gen_this_r = this.clone();
                        let resolve_fn_c = resolve_fn.clone();
                        let reject_fn_c = reject_fn.clone();

                        let on_fulfilled =
                            self.create_function(JsFunction::native("".to_string(), 1, {
                                move |interp, _this, args| {
                                    let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                                    let iter_result = interp.create_iter_result_object(v, true);
                                    let _ = interp.call_function(
                                        &resolve_fn_c,
                                        &JsValue::UNDEFINED,
                                        &[iter_result],
                                    );
                                    if let Some(queue) =
                                        interp.scheduler.async_gen_queue_mut(gen_id)
                                    {
                                        queue.pop_front();
                                    }
                                    interp.async_gen_process_queue(&gen_this_f);
                                    Completion::Normal(JsValue::UNDEFINED)
                                }
                            }));

                        let on_rejected =
                            self.create_function(JsFunction::native("".to_string(), 1, {
                                let gen_id2 = gen_id;
                                move |interp, _this, args| {
                                    let e = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                                    let _ = interp.call_function(
                                        &reject_fn_c,
                                        &JsValue::UNDEFINED,
                                        &[e],
                                    );
                                    if let Some(queue) =
                                        interp.scheduler.async_gen_queue_mut(gen_id2)
                                    {
                                        queue.pop_front();
                                    }
                                    interp.async_gen_process_queue(&gen_this_r);
                                    Completion::Normal(JsValue::UNDEFINED)
                                }
                            }));

                        let chain_promise = self.create_promise_object();
                        let cp_id = if let Some(po) = (chain_promise)
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                        {
                            po.id
                        } else {
                            0
                        };
                        let (cp_resolve, cp_reject) = self.create_resolving_functions(cp_id);
                        let _ = self.perform_promise_then(
                            &wrapper,
                            &on_fulfilled,
                            &on_rejected,
                            chain_promise,
                            cp_resolve,
                            cp_reject,
                        );

                        self.scheduler.set_async_gen_yield_pending(true);
                        return Completion::Normal(promise);
                    } else {
                        // return; — no expression, no Await per §13.10.1
                        let disp = self
                            .dispose_resources(&func_env, Completion::Return(JsValue::UNDEFINED));
                        match disp {
                            Completion::Return(_) => {}
                            Completion::Throw(e) => {
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                return Completion::Normal(promise);
                            }
                            _ => {}
                        }

                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let iter_result = self.create_iter_result_object(JsValue::UNDEFINED, true);
                        let _ =
                            self.call_function(&resolve_fn, &JsValue::UNDEFINED, &[iter_result]);
                        return Completion::Normal(promise);
                    }
                }

                StateTerminator::Throw(expr) => {
                    let throw_val = {
                        let mut result = self.eval_expr(expr, &term_env);
                        while let Completion::TailCall { func, this, args } = result {
                            result = self.call_function(&func, &this, &args);
                        }
                        match result {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => e,
                            other => {
                                if let Completion::Yield(yv) = other {
                                    yv
                                } else {
                                    JsValue::UNDEFINED
                                }
                            }
                        }
                    };

                    if let Some(try_info) = current_try_stack.pop()
                        && let Some(catch_state) = try_info.catch_state
                    {
                        pending_exception = Some(throw_val);
                        current_id = catch_state;
                        continue;
                    }

                    self.generator_inline_iters.remove(&o.id);
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[throw_val]);
                    return Completion::Normal(promise);
                }

                // Async-function transforms are currently the only machines
                // that emit LoopControl. If a shared transform emits one for
                // an async generator, cleanup is identical to Goto.
                StateTerminator::Goto(next_state)
                | StateTerminator::LoopControl(LoopControlTarget {
                    target_state: next_state,
                    ..
                }) => {
                    if let Err(completion) = self.align_generator_for_of_stack(
                        o.id,
                        &mut for_of_stack,
                        &mut current_try_stack,
                        &func_env,
                        *next_state,
                    ) {
                        self.generator_inline_iters.remove(&o.id);
                        self.generator_for_of_stacks.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        return match completion {
                            Completion::Throw(error) => {
                                let _ =
                                    self.call_function(&reject_fn, &JsValue::UNDEFINED, &[error]);
                                self.drain_microtasks();
                                Completion::Normal(promise)
                            }
                            Completion::Exit(code) => Completion::Exit(code),
                            _ => unreachable!("for-of unwind returned a non-abrupt completion"),
                        };
                    }
                    current_id = *next_state;
                }

                StateTerminator::ConditionalGoto {
                    condition,
                    true_state,
                    false_state,
                } => {
                    let cond_val = match self.eval_expr(condition, &term_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            self.generator_inline_iters.remove(&o.id);
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                        other => {
                            if let Completion::Yield(yv) = other {
                                yv
                            } else {
                                JsValue::UNDEFINED
                            }
                        }
                    };
                    current_id = if self.to_boolean_val(&cond_val) {
                        *true_state
                    } else {
                        *false_state
                    };
                }

                StateTerminator::TryEnter {
                    try_state,
                    catch_state,
                    finally_state,
                    after_state,
                } => {
                    current_try_stack.push(TryContextInfo {
                        catch_state: catch_state.as_ref().map(|c| c.state),
                        finally_state: *finally_state,
                        _after_state: *after_state,
                        entered_catch: false,
                        entered_finally: false,
                    });
                    current_id = *try_state;
                }

                StateTerminator::TryExit { after_state } => {
                    current_try_stack.pop();
                    if let Some(exc) = pending_exception.take() {
                        // Re-throw pending exception after finally completes
                        if let Some(try_info) = current_try_stack.pop() {
                            if let Some(catch_state) = try_info.catch_state {
                                pending_exception = Some(exc);
                                current_id = catch_state;
                                continue;
                            } else if let Some(finally_state) = try_info.finally_state {
                                pending_exception = Some(exc);
                                current_id = finally_state;
                                continue;
                            }
                        }
                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[exc]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    if let Some(ret_val) = pending_return.take() {
                        if current_try_stack.is_empty() {
                            self.generator_inline_iters.remove(&o.id);
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let iter_result = self.create_iter_result_object(ret_val, true);
                            let _ = self.call_function(
                                &resolve_fn,
                                &JsValue::UNDEFINED,
                                &[iter_result],
                            );
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                        pending_return = Some(ret_val);
                    }
                    current_id = *after_state;
                }

                StateTerminator::EnterCatch { body_state, param } => {
                    if let Some(ctx) = current_try_stack.last_mut() {
                        ctx.entered_catch = true;
                    }
                    let exception_val = pending_exception.take().unwrap_or(JsValue::UNDEFINED);
                    if let Some(pattern) = param {
                        let _ =
                            self.bind_pattern(pattern, exception_val, BindingKind::Let, &term_env);
                    }
                    current_id = *body_state;
                }

                StateTerminator::EnterFinally { body_state } => {
                    if let Some(ctx) = current_try_stack.last_mut() {
                        ctx.entered_finally = true;
                    }
                    current_id = *body_state;
                }

                StateTerminator::SwitchDispatch {
                    discriminant,
                    cases,
                    default_state,
                    after_state,
                } => {
                    let disc_val = match self.eval_expr(discriminant, &term_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            self.generator_inline_iters.remove(&o.id);
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                        other => {
                            if let Completion::Yield(yv) = other {
                                yv
                            } else {
                                JsValue::UNDEFINED
                            }
                        }
                    };

                    let mut matched = false;
                    for case in cases {
                        let case_val = match self.eval_expr(&case.test, &term_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            other => {
                                if let Completion::Yield(yv) = other {
                                    yv
                                } else {
                                    JsValue::UNDEFINED
                                }
                            }
                        };
                        if strict_equality(&disc_val, &case_val) {
                            current_id = case.state;
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        current_id = default_state.unwrap_or(*after_state);
                    }
                }

                StateTerminator::ForOfInit {
                    iterable,
                    iter_var,
                    label_set,
                    next_var: _,
                    left,
                    head_state,
                    after_state: forinit_after,
                    is_await,
                } => {
                    let iterable_env = Self::for_of_head_tdz_env(left, &term_env);

                    let iterable_val = match self.eval_expr(iterable, &iterable_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            if let Some(try_info) = current_try_stack.pop() {
                                if let Some(catch_state) = try_info.catch_state {
                                    pending_exception = Some(e);
                                    current_id = catch_state;
                                    continue;
                                } else if let Some(finally_state) = try_info.finally_state {
                                    current_id = finally_state;
                                    continue;
                                }
                            }
                            self.generator_inline_iters.remove(&o.id);
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                        other => {
                            if let Completion::Yield(yv) = other {
                                yv
                            } else {
                                JsValue::UNDEFINED
                            }
                        }
                    };
                    let iterator = if *is_await {
                        match self.get_async_iterator(&iterable_val) {
                            Ok(iter) => iter,
                            Err(e) => {
                                if let Some(try_info) = current_try_stack.pop() {
                                    if let Some(catch_state) = try_info.catch_state {
                                        pending_exception = Some(e);
                                        current_id = catch_state;
                                        continue;
                                    } else if let Some(finally_state) = try_info.finally_state {
                                        current_id = finally_state;
                                        continue;
                                    }
                                }
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        }
                    } else {
                        match self.get_iterator(&iterable_val) {
                            Ok(iter) => iter,
                            Err(e) => {
                                if let Some(try_info) = current_try_stack.pop() {
                                    if let Some(catch_state) = try_info.catch_state {
                                        pending_exception = Some(e);
                                        current_id = catch_state;
                                        continue;
                                    } else if let Some(finally_state) = try_info.finally_state {
                                        current_id = finally_state;
                                        continue;
                                    }
                                }
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        }
                    };
                    self.gc_root_value(&iterator);
                    func_env.borrow_mut().bindings.insert(
                        iter_var.clone(),
                        crate::interpreter::types::Binding {
                            value: iterator,
                            kind: crate::interpreter::types::BindingKind::Let,
                            initialized: true,
                            deletable: false,
                        },
                    );
                    for_of_stack.push(ForOfLoopState {
                        iter_var: iter_var.clone(),
                        label_set: label_set.clone(),
                        head_state: *head_state,
                        after_state: *forinit_after,
                        try_depth: current_try_stack.len(),
                        outer_env: term_env.clone(),
                        iteration_env: None,
                    });
                    self.sync_generator_for_of_stack(o.id, &for_of_stack);
                    current_id = *head_state;
                }

                StateTerminator::ForOfHead {
                    iter_var,
                    next_var: _,
                    left,
                    body_state,
                    after_state,
                    is_await,
                } => {
                    let loop_pos = match for_of_stack
                        .iter()
                        .rposition(|loop_state| loop_state.iter_var == *iter_var)
                    {
                        Some(pos) => pos,
                        None => {
                            debug_assert!(false, "for-of head without an active loop state");
                            for_of_stack.push(ForOfLoopState {
                                iter_var: iter_var.clone(),
                                label_set: vec![],
                                head_state: current_id,
                                after_state: *after_state,
                                try_depth: current_try_stack.len(),
                                outer_env: term_env.clone(),
                                iteration_env: None,
                            });
                            for_of_stack.len() - 1
                        }
                    };

                    let iterator = func_env
                        .borrow()
                        .bindings
                        .get(iter_var)
                        .map(|b| b.value.clone())
                        .unwrap_or(JsValue::UNDEFINED);

                    // §14.7.5.6 step 7.h: a throwing disposer ends the loop
                    // with a throw completion, so the iterator still closes
                    // and the generator's own handlers still see the error.
                    if let Some(iteration_env) = for_of_stack[loop_pos].iteration_env.take()
                        && let Completion::Throw(e) =
                            self.dispose_resources(&iteration_env, Completion::Empty)
                    {
                        self.iterator_close(&iterator, e.clone());
                        self.gc_unroot_value(&iterator);
                        for_of_stack.remove(loop_pos);
                        self.sync_generator_for_of_stack(o.id, &for_of_stack);
                        if let Some(try_info) = current_try_stack.pop() {
                            if let Some(catch_state) = try_info.catch_state {
                                pending_exception = Some(e);
                                current_id = catch_state;
                                continue;
                            } else if let Some(finally_state) = try_info.finally_state {
                                // TryExit re-throws whatever `pending_exception`
                                // still holds once the finally body completes.
                                pending_exception = Some(e);
                                current_id = finally_state;
                                continue;
                            }
                        }
                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }

                    let step_result = match self.iterator_next(&iterator) {
                        Ok(v) => v,
                        Err(e) => {
                            self.discard_failed_generator_for_of_loop(
                                o.id,
                                &mut for_of_stack,
                                loop_pos,
                                &iterator,
                            );
                            if Self::enter_generator_exception_handler(
                                &mut current_try_stack,
                                &mut pending_exception,
                                &mut current_id,
                                e.clone(),
                            ) {
                                continue;
                            }
                            self.generator_inline_iters.remove(&o.id);
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                    };
                    let step_result = if *is_await {
                        match self.await_value(&step_result) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                self.discard_failed_generator_for_of_loop(
                                    o.id,
                                    &mut for_of_stack,
                                    loop_pos,
                                    &iterator,
                                );
                                if Self::enter_generator_exception_handler(
                                    &mut current_try_stack,
                                    &mut pending_exception,
                                    &mut current_id,
                                    e.clone(),
                                ) {
                                    continue;
                                }
                                self.generator_inline_iters.remove(&o.id);
                                obj_rc.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::UNDEFINED,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        },
                                    );
                                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            other => {
                                if let Completion::Yield(yv) = other {
                                    yv
                                } else {
                                    JsValue::UNDEFINED
                                }
                            }
                        }
                    } else {
                        step_result
                    };
                    match self.iterator_complete(&step_result) {
                        Ok(true) => {
                            self.gc_unroot_value(&iterator);
                            if let Some(o) = iterator
                                .as_object_id()
                                .map(|id| crate::types::JsObject { id })
                            {
                                let id = o.id;
                                self.pending_iter_close.retain(|v| {
                                    if let Some(ov) =
                                        (v).as_object_id().map(|id| crate::types::JsObject { id })
                                    {
                                        ov.id != id
                                    } else {
                                        true
                                    }
                                });
                            }
                            for_of_stack.remove(loop_pos);
                            self.sync_generator_for_of_stack(o.id, &for_of_stack);
                            current_id = *after_state;
                        }
                        Ok(false) => {
                            let val = match self.iterator_value(&step_result) {
                                Ok(v) => v,
                                Err(e) => {
                                    self.discard_failed_generator_for_of_loop(
                                        o.id,
                                        &mut for_of_stack,
                                        loop_pos,
                                        &iterator,
                                    );
                                    if Self::enter_generator_exception_handler(
                                        &mut current_try_stack,
                                        &mut pending_exception,
                                        &mut current_id,
                                        e.clone(),
                                    ) {
                                        continue;
                                    }
                                    self.generator_inline_iters.remove(&o.id);
                                    obj_rc.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::StateMachineAsyncGenerator {
                                                state_machine,
                                                func_env,
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::Completed,
                                                _sent_value: JsValue::UNDEFINED,
                                                try_stack: vec![],
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: None,
                                                pending_return: None,
                                            },
                                        );
                                    let _ =
                                        self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                    self.drain_microtasks();
                                    return Completion::Normal(promise);
                                }
                            };
                            let needs_iteration_env = Self::for_of_head_lexical(left).is_some();
                            let outer_env = for_of_stack[loop_pos].outer_env.clone();
                            let bind_env = if needs_iteration_env {
                                let iteration_env = Environment::new(Some(outer_env));
                                for_of_stack[loop_pos].iteration_env = Some(iteration_env.clone());
                                self.sync_generator_for_of_stack(o.id, &for_of_stack);
                                iteration_env
                            } else {
                                outer_env
                            };

                            match left {
                                ForInOfLeft::Variable(decl) => {
                                    let kind = match decl.kind {
                                        VarKind::Var => crate::interpreter::types::BindingKind::Var,
                                        VarKind::Let => crate::interpreter::types::BindingKind::Let,
                                        VarKind::Const | VarKind::Using | VarKind::AwaitUsing => {
                                            crate::interpreter::types::BindingKind::Const
                                        }
                                    };
                                    if matches!(decl.kind, VarKind::Using | VarKind::AwaitUsing) {
                                        let hint = DisposeHint::for_var_kind(decl.kind);
                                        if let Err(e) =
                                            self.add_disposable_resource(&bind_env, &val, hint)
                                        {
                                            self.iterator_close(&iterator, e.clone());
                                            self.discard_failed_generator_for_of_loop(
                                                o.id,
                                                &mut for_of_stack,
                                                loop_pos,
                                                &iterator,
                                            );
                                            if Self::enter_generator_exception_handler(
                                                &mut current_try_stack,
                                                &mut pending_exception,
                                                &mut current_id,
                                                e.clone(),
                                            ) {
                                                continue;
                                            }
                                            self.generator_inline_iters.remove(&o.id);
                                            obj_rc.borrow_mut().kind =
                                                crate::interpreter::types::ObjectKind::Iterator(
                                                    IteratorState::StateMachineAsyncGenerator {
                                                        state_machine,
                                                        func_env,
                                                        is_strict,
                                                        execution_state:
                                                            StateMachineExecutionState::Completed,
                                                        _sent_value: JsValue::UNDEFINED,
                                                        try_stack: vec![],
                                                        pending_binding: None,
                                                        delegated_iterator: None,
                                                        pending_exception: None,
                                                        pending_return: None,
                                                    },
                                                );
                                            let _ = self.call_function(
                                                &reject_fn,
                                                &JsValue::UNDEFINED,
                                                &[e],
                                            );
                                            self.drain_microtasks();
                                            return Completion::Normal(promise);
                                        }
                                    }
                                    if let Some(d) = decl.declarations.first()
                                        && let Err(e) =
                                            self.bind_pattern(&d.pattern, val, kind, &bind_env)
                                    {
                                        self.iterator_close(&iterator, e.clone());
                                        self.discard_failed_generator_for_of_loop(
                                            o.id,
                                            &mut for_of_stack,
                                            loop_pos,
                                            &iterator,
                                        );
                                        if Self::enter_generator_exception_handler(
                                            &mut current_try_stack,
                                            &mut pending_exception,
                                            &mut current_id,
                                            e.clone(),
                                        ) {
                                            continue;
                                        }
                                        self.generator_inline_iters.remove(&o.id);
                                        obj_rc.borrow_mut().kind =
                                            crate::interpreter::types::ObjectKind::Iterator(
                                                IteratorState::StateMachineAsyncGenerator {
                                                    state_machine,
                                                    func_env,
                                                    is_strict,
                                                    execution_state:
                                                        StateMachineExecutionState::Completed,
                                                    _sent_value: JsValue::UNDEFINED,
                                                    try_stack: vec![],
                                                    pending_binding: None,
                                                    delegated_iterator: None,
                                                    pending_exception: None,
                                                    pending_return: None,
                                                },
                                            );
                                        let _ = self.call_function(
                                            &reject_fn,
                                            &JsValue::UNDEFINED,
                                            &[e],
                                        );
                                        self.drain_microtasks();
                                        return Completion::Normal(promise);
                                    }
                                }
                                ForInOfLeft::Pattern(pat) => {
                                    match self.assign_to_for_pattern(pat, val, &term_env) {
                                        Completion::Normal(_) | Completion::Empty => {}
                                        Completion::Throw(e) => {
                                            self.iterator_close(&iterator, e.clone());
                                            self.discard_failed_generator_for_of_loop(
                                                o.id,
                                                &mut for_of_stack,
                                                loop_pos,
                                                &iterator,
                                            );
                                            if Self::enter_generator_exception_handler(
                                                &mut current_try_stack,
                                                &mut pending_exception,
                                                &mut current_id,
                                                e.clone(),
                                            ) {
                                                continue;
                                            }
                                            self.generator_inline_iters.remove(&o.id);
                                            obj_rc.borrow_mut().kind =
                                                crate::interpreter::types::ObjectKind::Iterator(
                                                    IteratorState::StateMachineAsyncGenerator {
                                                        state_machine,
                                                        func_env,
                                                        is_strict,
                                                        execution_state:
                                                            StateMachineExecutionState::Completed,
                                                        _sent_value: JsValue::UNDEFINED,
                                                        try_stack: vec![],
                                                        pending_binding: None,
                                                        delegated_iterator: None,
                                                        pending_exception: None,
                                                        pending_return: None,
                                                    },
                                                );
                                            let _ = self.call_function(
                                                &reject_fn,
                                                &JsValue::UNDEFINED,
                                                &[e],
                                            );
                                            self.drain_microtasks();
                                            return Completion::Normal(promise);
                                        }
                                        _other => {}
                                    }
                                }
                                ForInOfLeft::Expression(_) => {}
                            }
                            let already_pending = if let Some(o) = iterator
                                .as_object_id()
                                .map(|id| crate::types::JsObject { id })
                            {
                                let id = o.id;
                                self.pending_iter_close.iter().any(|v| {
                                    if let Some(ov) =
                                        (v).as_object_id().map(|id| crate::types::JsObject { id })
                                    {
                                        ov.id == id
                                    } else {
                                        false
                                    }
                                })
                            } else {
                                false
                            };
                            if !already_pending {
                                self.pending_iter_close.push(iterator);
                            }
                            current_id = *body_state;
                        }
                        Err(e) => {
                            self.discard_failed_generator_for_of_loop(
                                o.id,
                                &mut for_of_stack,
                                loop_pos,
                                &iterator,
                            );
                            if Self::enter_generator_exception_handler(
                                &mut current_try_stack,
                                &mut pending_exception,
                                &mut current_id,
                                e.clone(),
                            ) {
                                continue;
                            }
                            self.generator_inline_iters.remove(&o.id);
                            obj_rc.borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                    }
                }

                StateTerminator::Completed => {
                    // §27.6.3.3: DisposeResources when async generator completes
                    let disp =
                        self.dispose_resources(&func_env, Completion::Normal(JsValue::UNDEFINED));
                    if let Completion::Throw(e) = disp {
                        self.generator_inline_iters.remove(&o.id);
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            },
                        );
                        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                        return Completion::Normal(promise);
                    }
                    self.generator_inline_iters.remove(&o.id);
                    obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::UNDEFINED,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        },
                    );
                    let iter_result = self.create_iter_result_object(JsValue::UNDEFINED, true);
                    let _ = self.call_function(&resolve_fn, &JsValue::UNDEFINED, &[iter_result]);
                    return Completion::Normal(promise);
                }

                StateTerminator::Await {
                    value,
                    resume_state,
                    sent_value_binding,
                } => {
                    let await_val = match self.eval_expr(value, &term_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            pending_exception = Some(e);
                            check_abrupt_on_resume = true;
                            current_id = *resume_state;
                            continue;
                        }
                        _ => JsValue::UNDEFINED,
                    };

                    // §27.7.5.3 Await: always suspend and schedule continuation
                    // via PerformPromiseThen, even for already-resolved promises
                    let p = self.promise_resolve_value(&await_val);
                    let _p_id = if let Some(o) =
                        (p).as_object_id().map(|id| crate::types::JsObject { id })
                    {
                        o.id
                    } else {
                        0
                    };

                    {
                        let binding_clone = sent_value_binding.clone();
                        let resume_id = *resume_state;
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StateMachineAsyncGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_id,
                                },
                                _sent_value: JsValue::UNDEFINED,
                                try_stack: current_try_stack.clone(),
                                pending_binding: binding_clone.clone(),
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: pending_return.take(),
                            },
                        );

                        let this_clone = this.clone();
                        let promise_c = promise.clone();
                        let resolve_c = resolve_fn.clone();
                        let reject_c = reject_fn.clone();
                        let gen_id = if let Some(o) = (this)
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                        {
                            o.id
                        } else {
                            0
                        };

                        let this_f = this_clone.clone();
                        let promise_f = promise_c.clone();
                        let resolve_f = resolve_c.clone();
                        let reject_f = reject_c.clone();
                        let fulfill_handler = self.create_function(JsFunction::native(
                            "asyncGenAwaitFulfill".to_string(),
                            1,
                            move |interp, _this, args| {
                                let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                                interp.async_gen_await_resume(
                                    &this_f, v, false, &promise_f, &resolve_f, &reject_f, gen_id,
                                );
                                Completion::Normal(JsValue::UNDEFINED)
                            },
                        ));

                        let this_r = this_clone.clone();
                        let promise_r = promise_c;
                        let resolve_r = resolve_c;
                        let reject_r = reject_c;
                        let reject_handler = self.create_function(JsFunction::native(
                            "asyncGenAwaitReject".to_string(),
                            1,
                            move |interp, _this, args| {
                                let e = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                                interp.async_gen_await_resume(
                                    &this_r, e, true, &promise_r, &resolve_r, &reject_r, gen_id,
                                );
                                Completion::Normal(JsValue::UNDEFINED)
                            },
                        ));

                        let _ = self.promise_then(&p, &fulfill_handler, &reject_handler);

                        self.scheduler.set_async_gen_yield_pending(true);
                        return Completion::Normal(promise);
                    }
                }
            }
        }
    }

    fn apply_sent_value_binding(
        &mut self,
        binding: &crate::interpreter::generator_transform::SentValueBinding,
        value: &JsValue,
        env: &EnvRef,
    ) {
        use crate::interpreter::generator_transform::SentValueBindingKind;
        match &binding.kind {
            SentValueBindingKind::Variable(name) => {
                self.env_set(env, name, value.clone()).ok();
            }
            SentValueBindingKind::Pattern(pattern) => {
                let _ = self.bind_pattern(pattern, value.clone(), BindingKind::Var, env);
            }
            SentValueBindingKind::Discard | SentValueBindingKind::InlineYield { .. } => {}
        }
    }

    fn async_gen_await_resume(
        &mut self,
        this: &JsValue,
        value: JsValue,
        is_reject: bool,
        promise: &JsValue,
        resolve_fn: &JsValue,
        reject_fn: &JsValue,
        gen_id: u64,
    ) {
        let Some(obj_rc) = self.get_object_cell(gen_id) else {
            return;
        };

        let state = obj_rc.borrow().iterator_state().cloned();
        let Some(IteratorState::StateMachineAsyncGenerator {
            func_env,
            pending_binding,
            ..
        }) = &state
        else {
            return;
        };

        if is_reject {
            // Set pending_exception so the executor routes through try_stack
            if let Some(obj) = self.get_object_cell(gen_id) {
                let mut o = obj.borrow_mut();
                if let Some(IteratorState::StateMachineAsyncGenerator {
                    pending_exception, ..
                }) = o.iterator_state_mut()
                {
                    *pending_exception = Some(value);
                }
            }
        } else if let Some(b) = pending_binding {
            self.apply_sent_value_binding(b, &value, func_env);
            // Clear the pending_binding
            if let Some(obj) = self.get_object(gen_id) {
                let mut o = obj.borrow_mut();
                if let Some(IteratorState::StateMachineAsyncGenerator {
                    pending_binding, ..
                }) = o.iterator_state_mut()
                {
                    *pending_binding = None;
                }
            }
        }

        self.scheduler.set_async_gen_yield_pending(false);
        let _ = self.async_generator_next_state_machine_with_promise(
            this,
            JsValue::UNDEFINED,
            promise.clone(),
            resolve_fn.clone(),
            reject_fn.clone(),
        );
        if self.scheduler.is_async_gen_yield_pending() {
            self.scheduler.set_async_gen_yield_pending(false);
            return;
        }

        // Pop the queue entry and process next
        if let Some(queue) = self.scheduler.async_gen_queue_mut(gen_id) {
            queue.pop_front();
        }
        let this_clone = this.clone();
        if self
            .scheduler
            .async_gen_queue(gen_id)
            .is_some_and(|q| !q.is_empty())
        {
            self.scheduler.enqueue_microtask((
                vec![this_clone.clone()],
                Box::new(move |interp| {
                    interp.async_gen_process_queue(&this_clone);
                    Completion::Normal(JsValue::UNDEFINED)
                }),
            ));
        }
    }

    pub(crate) fn async_generator_next(
        &mut self,
        this: &JsValue,
        sent_value: JsValue,
    ) -> Completion {
        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.next called on non-object");
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.next called on non-object");
        };

        let state = obj_rc.borrow().iterator_state().cloned();
        if let Some(IteratorState::StateMachineAsyncGenerator { .. }) = &state {
            return self.async_gen_enqueue(this, sent_value, super::AsyncGenRequestKind::Next);
        }
        let Some(IteratorState::AsyncGenerator {
            body,
            func_env,
            is_strict,
            execution_state,
        }) = state
        else {
            return self.reject_with_type_error("not an async generator object");
        };

        let promise = self.create_promise_object();
        let promise_id = if let Some(po) = (promise)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            po.id
        } else {
            0
        };
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);

        // Determine target_yield and previous sent values based on execution state
        let (target_yield, prev_sent, is_suspended_start) = match &execution_state {
            GeneratorExecutionState::Completed => {
                let result = self.create_iter_result_object(JsValue::UNDEFINED, true);
                let _ = self.call_function(&resolve_fn, &JsValue::UNDEFINED, &[result]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            GeneratorExecutionState::Executing => {
                let err = self.create_type_error("AsyncGenerator is already executing");
                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[err]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            GeneratorExecutionState::SuspendedStart => (0, Vec::new(), true),
            GeneratorExecutionState::SuspendedYield {
                target_yield,
                prev_sent,
            } => (*target_yield, prev_sent.clone(), false),
        };

        // Build the full prev_sent_values for this call by appending the current sent_value.
        // For SuspendedStart (first call), sent_value is irrelevant (no yield to resume from).
        let mut new_prev_sent = prev_sent.clone();
        if !is_suspended_start {
            new_prev_sent.push(sent_value.clone());
        }

        // Mark as executing
        obj_rc.borrow_mut().kind =
            crate::interpreter::types::ObjectKind::Iterator(IteratorState::AsyncGenerator {
                body: body.clone(),
                func_env: func_env.clone(),
                is_strict,
                execution_state: GeneratorExecutionState::Executing,
            });

        self.generator_context = Some(GeneratorContext {
            target_yield,
            current_yield: 0,
            prev_sent_values: new_prev_sent.clone(),
            is_async: true,
            resume_kind: GeneratorResumeKind::Next,
        });

        let caller_realm = self.current_realm_id;
        if let Some(gen_realm) = obj_rc.borrow().generator_realm_id {
            self.current_realm_id = gen_realm;
        }

        func_env.borrow_mut().strict = is_strict;
        self.call_stack_envs.push(func_env.clone());
        let result = self.exec_body(&body, &func_env);
        self.call_stack_envs.pop();
        let _ctx = self.generator_context.take();

        self.current_realm_id = caller_realm;
        match result {
            Completion::Yield(v) => {
                let awaited = match self.await_value(&v) {
                    Completion::Normal(av) => av,
                    Completion::Throw(e) => {
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::AsyncGenerator {
                                body,
                                func_env,
                                is_strict,
                                execution_state: GeneratorExecutionState::Completed,
                            },
                        );
                        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    other => {
                        if let Completion::Yield(yv) = other {
                            yv
                        } else {
                            JsValue::UNDEFINED
                        }
                    }
                };
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::AsyncGenerator {
                        body,
                        func_env,
                        is_strict,
                        execution_state: GeneratorExecutionState::SuspendedYield {
                            target_yield: target_yield + 1,
                            prev_sent: new_prev_sent,
                        },
                    },
                );
                let iter_result = self.create_iter_result_object(awaited, false);
                let _ = self.call_function(&resolve_fn, &JsValue::UNDEFINED, &[iter_result]);
            }
            Completion::Return(v) => {
                let awaited = match self.await_value(&v) {
                    Completion::Normal(av) => av,
                    Completion::Throw(e) => {
                        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::AsyncGenerator {
                                body,
                                func_env,
                                is_strict,
                                execution_state: GeneratorExecutionState::Completed,
                            },
                        );
                        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    other => {
                        if let Completion::Yield(yv) = other {
                            yv
                        } else {
                            JsValue::UNDEFINED
                        }
                    }
                };
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::AsyncGenerator {
                        body,
                        func_env,
                        is_strict,
                        execution_state: GeneratorExecutionState::Completed,
                    },
                );
                let iter_result = self.create_iter_result_object(awaited, true);
                let _ = self.call_function(&resolve_fn, &JsValue::UNDEFINED, &[iter_result]);
            }
            Completion::Normal(_) => {
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::AsyncGenerator {
                        body,
                        func_env,
                        is_strict,
                        execution_state: GeneratorExecutionState::Completed,
                    },
                );
                let iter_result = self.create_iter_result_object(JsValue::UNDEFINED, true);
                let _ = self.call_function(&resolve_fn, &JsValue::UNDEFINED, &[iter_result]);
            }
            Completion::Throw(e) => {
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::AsyncGenerator {
                        body,
                        func_env,
                        is_strict,
                        execution_state: GeneratorExecutionState::Completed,
                    },
                );
                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
            }
            _ => {}
        }
        self.drain_microtasks();
        Completion::Normal(promise)
    }

    /// Per spec §27.6.3.9 step 10.a: AsyncGenerator awaiting-return
    /// Wraps the return value in Promise.resolve(value).then(onFulfilled, onRejected)
    /// where onFulfilled resolves the response promise with {value: v, done: true}
    /// and onRejected rejects the response promise.
    fn async_generator_await_return(
        &mut self,
        value: JsValue,
        response_promise_id: u64,
    ) -> Completion {
        let response_promise = JsValue::object(response_promise_id);

        // Create Promise.resolve(value) — wraps value in a promise
        let wrapper_promise = self.create_promise_object();
        let wrapper_id = if let Some(o) = (wrapper_promise)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            o.id
        } else {
            0
        };
        let (wrapper_resolve, _wrapper_reject) = self.create_resolving_functions(wrapper_id);
        let _ = self.call_function(&wrapper_resolve, &JsValue::UNDEFINED, &[value]);
        self.drain_microtasks();

        let (resp_resolve, resp_reject) = self.create_resolving_functions(response_promise_id);

        let on_fulfilled = self.create_function(JsFunction::native("".to_string(), 1, {
            let resolve = resp_resolve;
            move |interp, _this, args| {
                let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let iter_result = interp.create_iter_result_object(v, true);
                let _ = interp.call_function(&resolve, &JsValue::UNDEFINED, &[iter_result]);
                Completion::Normal(JsValue::UNDEFINED)
            }
        }));

        let on_rejected = self.create_function(JsFunction::native("".to_string(), 1, {
            let reject = resp_reject;
            move |interp, _this, args| {
                let e = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let _ = interp.call_function(&reject, &JsValue::UNDEFINED, &[e]);
                Completion::Normal(JsValue::UNDEFINED)
            }
        }));

        // Chain: PerformPromiseThen(wrapper_promise, onFulfilled, onRejected, responseCap)
        let (rp_resolve, rp_reject) = self.create_resolving_functions(response_promise_id);
        let _ = self.perform_promise_then(
            &wrapper_promise,
            &on_fulfilled,
            &on_rejected,
            response_promise.clone(),
            rp_resolve,
            rp_reject,
        );
        self.drain_microtasks();

        Completion::Normal(response_promise)
    }

    pub(crate) fn async_generator_return(&mut self, this: &JsValue, value: JsValue) -> Completion {
        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.return called on non-object");
        };

        let Some(obj_rc) = self.get_object(o.id) else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.return called on non-object");
        };
        let state = obj_rc.borrow().iterator_state().cloned();

        if let Some(IteratorState::StateMachineAsyncGenerator { .. }) = &state {
            return self.async_gen_enqueue(this, value, super::AsyncGenRequestKind::Return);
        }

        // Non-state-machine IteratorState::AsyncGenerator path is below
        self.async_generator_return_legacy(this, value)
    }

    fn async_generator_return_state_machine_with_promise(
        &mut self,
        this: &JsValue,
        value: JsValue,
        promise: JsValue,
        resolve_fn: JsValue,
        reject_fn: JsValue,
    ) -> Completion {
        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            return Completion::Normal(promise);
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return Completion::Normal(promise);
        };
        let state = obj_rc.borrow().iterator_state().cloned();
        let Some(IteratorState::StateMachineAsyncGenerator {
            state_machine,
            func_env,
            is_strict,
            execution_state,
            try_stack,
            delegated_iterator,
            ..
        }) = state
        else {
            return Completion::Normal(promise);
        };

        let promise_id = if let Some(po) = (promise)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            po.id
        } else {
            0
        };

        match execution_state {
            StateMachineExecutionState::Executing => {
                let err = self.create_type_error("AsyncGenerator is already executing");
                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[err]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            StateMachineExecutionState::SuspendedStart | StateMachineExecutionState::Completed => {
                self.generator_inline_iters.remove(&o.id);
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    },
                );
                return self.async_generator_await_return(value, promise_id);
            }
            StateMachineExecutionState::SuspendedAtState { .. } => {}
        }

        // For Promise values, check if PromiseResolve would throw (e.g. broken .constructor)
        // Skip for non-promise values to avoid spurious "then" getter access
        if self.is_promise(&value) {
            let promise_ctor = self.get_global_var("Promise").unwrap_or(JsValue::UNDEFINED);
            if let Err(e) = self.promise_resolve_with_constructor(&promise_ctor, &value) {
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack,
                        pending_binding: None,
                        delegated_iterator,
                        pending_exception: Some(e),
                        pending_return: None,
                    },
                );
                return self.async_generator_next_state_machine_with_promise(
                    this,
                    JsValue::UNDEFINED,
                    promise,
                    resolve_fn,
                    reject_fn,
                );
            }
        }

        // Route through the existing next_state_machine with pending_return
        // The Await (which calls PromiseResolve) happens inside the yield handler
        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
            IteratorState::StateMachineAsyncGenerator {
                state_machine,
                func_env,
                is_strict,
                execution_state,
                _sent_value: JsValue::UNDEFINED,
                try_stack,
                pending_binding: None,
                delegated_iterator,
                pending_exception: None,
                pending_return: Some(value),
            },
        );
        self.async_generator_next_state_machine_with_promise(
            this,
            JsValue::UNDEFINED,
            promise,
            resolve_fn,
            reject_fn,
        )
    }

    fn async_generator_throw_state_machine_with_promise(
        &mut self,
        this: &JsValue,
        exception: JsValue,
        promise: JsValue,
        resolve_fn: JsValue,
        reject_fn: JsValue,
    ) -> Completion {
        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            return Completion::Normal(promise);
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return Completion::Normal(promise);
        };
        let state = obj_rc.borrow().iterator_state().cloned();
        let Some(IteratorState::StateMachineAsyncGenerator {
            state_machine,
            func_env,
            is_strict,
            execution_state,
            try_stack,
            delegated_iterator,
            ..
        }) = state
        else {
            return Completion::Normal(promise);
        };

        match execution_state {
            StateMachineExecutionState::Executing => {
                let err = self.create_type_error("AsyncGenerator is already executing");
                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[err]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            StateMachineExecutionState::SuspendedStart | StateMachineExecutionState::Completed => {
                self.generator_inline_iters.remove(&o.id);
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::UNDEFINED,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    },
                );
                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[exception]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            StateMachineExecutionState::SuspendedAtState { .. } => {}
        }

        // Route through next_state_machine with pending_exception set
        // This handles delegation and try/catch stack
        obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
            IteratorState::StateMachineAsyncGenerator {
                state_machine,
                func_env,
                is_strict,
                execution_state,
                _sent_value: JsValue::UNDEFINED,
                try_stack,
                pending_binding: None,
                delegated_iterator,
                pending_exception: Some(exception),
                pending_return: None,
            },
        );
        self.async_generator_next_state_machine_with_promise(
            this,
            JsValue::UNDEFINED,
            promise,
            resolve_fn,
            reject_fn,
        )
    }

    fn async_generator_return_legacy(&mut self, this: &JsValue, value: JsValue) -> Completion {
        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.return called on non-object");
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.return called on non-object");
        };
        let state = obj_rc.borrow().iterator_state().cloned();

        // NOTE: The old state machine path has been removed since it now routes through the queue.
        // Only the legacy IteratorState::AsyncGenerator path remains here.

        let Some(IteratorState::AsyncGenerator {
            body,
            func_env,
            is_strict,
            execution_state,
        }) = state
        else {
            return self.reject_with_type_error("not an async generator object");
        };

        let promise = self.create_promise_object();
        let promise_id = if let Some(po) = (promise)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            po.id
        } else {
            0
        };
        let (_resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);

        match &execution_state {
            GeneratorExecutionState::SuspendedStart | GeneratorExecutionState::Completed => {
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::AsyncGenerator {
                        body,
                        func_env,
                        is_strict,
                        execution_state: GeneratorExecutionState::Completed,
                    },
                );
                self.async_generator_await_return(value, promise_id)
            }
            GeneratorExecutionState::Executing => {
                let err = self.create_type_error("AsyncGenerator is already executing");
                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[err]);
                self.drain_microtasks();
                Completion::Normal(promise)
            }
            GeneratorExecutionState::SuspendedYield { .. } => {
                obj_rc.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                    IteratorState::AsyncGenerator {
                        body,
                        func_env,
                        is_strict,
                        execution_state: GeneratorExecutionState::Completed,
                    },
                );
                self.async_generator_await_return(value, promise_id)
            }
        }
    }

    pub(crate) fn async_generator_throw(
        &mut self,
        this: &JsValue,
        exception: JsValue,
    ) -> Completion {
        let Some(o) = (this)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.throw called on non-object");
        };

        let Some(obj_rc) = self.get_object(o.id) else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.throw called on non-object");
        };
        let state = obj_rc.borrow().iterator_state().cloned();

        if let Some(IteratorState::StateMachineAsyncGenerator { .. }) = &state {
            return self.async_gen_enqueue(this, exception, super::AsyncGenRequestKind::Throw);
        }

        // Non-state-machine path (legacy)
        let Some(IteratorState::AsyncGenerator {
            body,
            func_env,
            is_strict,
            ..
        }) = state
        else {
            return self.reject_with_type_error("not an async generator object");
        };

        let promise = self.create_promise_object();
        let promise_id = if let Some(po) = (promise)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            po.id
        } else {
            0
        };
        let (_, reject_fn) = self.create_resolving_functions(promise_id);

        obj_rc.borrow_mut().kind =
            crate::interpreter::types::ObjectKind::Iterator(IteratorState::AsyncGenerator {
                body,
                func_env,
                is_strict,
                execution_state: GeneratorExecutionState::Completed,
            });
        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[exception]);
        self.drain_microtasks();
        Completion::Normal(promise)
    }

    fn align_generator_for_of_stack(
        &mut self,
        generator_id: u64,
        for_of_stack: &mut Vec<ForOfLoopState>,
        try_stack: &mut Vec<TryContextInfo>,
        func_env: &EnvRef,
        target_state: usize,
    ) -> Result<(), Completion> {
        // Jumping to a loop's `after_state` leaves that loop, so it closes;
        // jumping to its `head_state` is the next iteration, so it stays.
        let keep_len = if let Some(pos) = for_of_stack
            .iter()
            .rposition(|loop_state| loop_state.after_state == target_state)
        {
            pos
        } else if let Some(pos) = for_of_stack
            .iter()
            .rposition(|loop_state| loop_state.head_state == target_state)
        {
            pos + 1
        } else {
            for_of_stack.len()
        };

        match self.unwind_generator_for_of_loops(
            generator_id,
            for_of_stack,
            try_stack,
            func_env,
            keep_len,
            Completion::Empty,
        ) {
            completion @ (Completion::Throw(_) | Completion::Exit(_)) => Err(completion),
            _ => Ok(()),
        }
    }

    /// Remove both saved representations of a failed loop after its caller
    /// has applied the required close behavior. Iterator protocol failures
    /// call this directly; binding failures call IteratorClose first.
    fn discard_failed_generator_for_of_loop(
        &mut self,
        generator_id: u64,
        for_of_stack: &mut Vec<ForOfLoopState>,
        loop_pos: usize,
        iterator: &JsValue,
    ) {
        for_of_stack.remove(loop_pos);
        self.unroot_for_of_iterator(iterator);
        self.remove_generator_inline_iterator(generator_id, iterator);
        self.sync_generator_for_of_stack(generator_id, for_of_stack);
    }

    fn enter_generator_exception_handler(
        try_stack: &mut Vec<TryContextInfo>,
        pending_exception: &mut Option<JsValue>,
        current_id: &mut usize,
        error: JsValue,
    ) -> bool {
        if let Some(try_info) = try_stack.pop() {
            if let Some(catch_state) = try_info.catch_state {
                *pending_exception = Some(error);
                *current_id = catch_state;
                return true;
            }
            if let Some(finally_state) = try_info.finally_state {
                *pending_exception = Some(error);
                *current_id = finally_state;
                return true;
            }
        }
        false
    }

    /// Close transformed generator for-of loops from the inside out while
    /// carrying the current completion through per-iteration disposal and
    /// IteratorClose. Handlers lexically inside a loop have finished before
    /// that loop closes, so discard them at the loop's recorded boundary.
    fn unwind_generator_for_of_loops(
        &mut self,
        generator_id: u64,
        for_of_stack: &mut Vec<ForOfLoopState>,
        try_stack: &mut Vec<TryContextInfo>,
        func_env: &EnvRef,
        keep_len: usize,
        mut completion: Completion,
    ) -> Completion {
        while for_of_stack.len() > keep_len {
            let loop_state = for_of_stack.pop().expect("loop stack is non-empty");
            try_stack.truncate(loop_state.try_depth);
            completion =
                self.close_for_of_loop(loop_state, func_env, completion, Some(generator_id));
            if matches!(completion, Completion::Exit(_)) {
                break;
            }
        }
        self.sync_generator_for_of_stack(generator_id, for_of_stack);
        completion
    }

    /// Mirrors the driver's local loop stack into the GC-visible map, so a
    /// collection triggered by user code sees exactly the environments the
    /// driver still holds.
    fn sync_generator_for_of_stack(&mut self, generator_id: u64, for_of_stack: &[ForOfLoopState]) {
        if for_of_stack.is_empty() {
            // Generators with no for-of hit this on every state transition;
            // skip hashing the id when the table holds nothing to remove.
            if !self.generator_for_of_stacks.is_empty() {
                self.generator_for_of_stacks.remove(&generator_id);
            }
        } else {
            let slot = self
                .generator_for_of_stacks
                .entry(generator_id)
                .or_default();
            slot.clear();
            slot.extend_from_slice(for_of_stack);
        }
    }
}
