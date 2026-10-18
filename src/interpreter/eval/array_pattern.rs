use super::*;

/// A synchronous primitive of a lowered array binding/assignment pattern.
/// The driver evaluates Init's Terminator Operand before entering this seam.
pub(super) enum ArrayPatternAction<'a> {
    Init(JsValue),
    Step(Option<&'a str>),
    Drain(&'a str),
    Finish,
}

/// Existing driver state borrowed for one operation, never across suspension.
/// Both generator flavours share the GC-visible stack mirror; async functions
/// instead retain their loop stack in the scheduler's AsyncFunctionState.
pub(super) struct ArrayPatternContext<'a> {
    pub func_env: &'a EnvRef,
    pub term_env: &'a EnvRef,
    pub for_of_stack: &'a mut Vec<ForOfLoopState>,
    pub try_depth: usize,
    pub next_state: usize,
    pub generator_id: Option<u64>,
}

impl ArrayPatternContext<'_> {
    fn position(&self, name: &str) -> Option<usize> {
        self.for_of_stack
            .iter()
            .rposition(|state| &*state.iter_var == name)
    }

    fn iterator(&self, name: &str) -> JsValue {
        let env = self.func_env.borrow();
        if self.generator_id.is_some() {
            env.bindings.get(name).map(|binding| binding.value.clone())
        } else {
            env.get(name)
        }
        .unwrap_or(JsValue::UNDEFINED)
    }

    fn publish(&self, interp: &mut Interpreter) {
        if let Some(id) = self.generator_id {
            interp.sync_generator_for_of_stack(id, self.for_of_stack);
        }
    }

    fn discard(&mut self, interp: &mut Interpreter, pos: usize, iterator: &JsValue, failed: bool) {
        self.for_of_stack.remove(pos);
        interp.forget_for_of_iterator(iterator);
        if failed && let Some(id) = self.generator_id {
            interp.remove_generator_inline_iterator(id, iterator);
        }
        self.publish(interp);
    }
}

impl Interpreter {
    /// Own registration, stepping, rest retention and normal close for all
    /// three drivers. A protocol failure retires the record without closing
    /// it (IteratorStepValue sets [[Done]]); failures in a later default still
    /// unwind through the registered ForOfLoopState. Routing remains with the
    /// driver, as do its Terminator Operand and Completion Ownership rules.
    pub(super) fn run_array_pattern_iter(
        &mut self,
        mut context: ArrayPatternContext<'_>,
        name: &str,
        action: ArrayPatternAction<'_>,
    ) -> Result<(), Completion> {
        self.with_gc_root_scope(|interp| {
            match action {
                ArrayPatternAction::Init(iterable) => {
                    interp.gc_root_value(&iterable);
                    let iterator = interp
                        .for_of_init_iterator(&iterable, false)
                        .map_err(Completion::Throw)?;
                    if context.generator_id.is_some() {
                        context
                            .func_env
                            .borrow_mut()
                            .bindings
                            .insert(name, Binding::new(iterator, BindingKind::Let, true));
                    } else {
                        interp.env_set(context.func_env, name, iterator).ok();
                    }
                    context.for_of_stack.push(ForOfLoopState {
                        iter_var: Rc::from(name),
                        label_set: Rc::from(Vec::new()),
                        head_state: context.next_state,
                        after_state: context.next_state,
                        try_depth: context.try_depth,
                        outer_env: context.term_env.clone(),
                        iteration_env: None,
                        is_await: false,
                        iterator_closed: false,
                    });
                    context.publish(interp);
                }
                ArrayPatternAction::Step(dest) => {
                    let value = if let Some(pos) = context.position(name) {
                        let iterator = context.iterator(name);
                        interp.gc_root_value(&iterator);
                        match interp.iterator_step_value(&iterator) {
                            Ok(Some(value)) => value,
                            Ok(None) => {
                                context.discard(interp, pos, &iterator, false);
                                JsValue::UNDEFINED
                            }
                            Err(error) => {
                                context.discard(interp, pos, &iterator, true);
                                return Err(Completion::Throw(error));
                            }
                        }
                    } else {
                        JsValue::UNDEFINED
                    };
                    if let Some(dest) = dest {
                        interp.env_set(context.func_env, dest, value).ok();
                    }
                }
                ArrayPatternAction::Drain(dest) => {
                    let mut rest = Vec::new();
                    if let Some(pos) = context.position(name) {
                        let iterator = context.iterator(name);
                        interp.gc_root_value(&iterator);
                        loop {
                            match interp.iterator_step_value(&iterator) {
                                Ok(Some(value)) => {
                                    // Later next/done/value calls run user code;
                                    // a Rust Vec alone is invisible to the GC.
                                    interp.gc_root_value(&value);
                                    rest.push(value);
                                }
                                Ok(None) => break,
                                Err(error) => {
                                    context.discard(interp, pos, &iterator, true);
                                    return Err(Completion::Throw(error));
                                }
                            }
                        }
                        context.discard(interp, pos, &iterator, false);
                    }
                    let array = interp.create_array(rest);
                    interp.env_set(context.func_env, dest, array).ok();
                }
                ArrayPatternAction::Finish => {
                    if let Some(pos) = context.position(name) {
                        let state = context.for_of_stack.remove(pos);
                        // Publish removal before return() can re-enter or collect.
                        context.publish(interp);
                        if let abrupt @ (Completion::Throw(_) | Completion::Exit(_)) = interp
                            .close_for_of_loop(
                                state,
                                context.func_env,
                                Completion::Normal(JsValue::UNDEFINED),
                                context.generator_id,
                            )
                        {
                            return Err(abrupt);
                        }
                    }
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn setup(source: &str) -> (Interpreter, EnvRef) {
        let mut interp = Interpreter::new();
        let program = Parser::new(source).unwrap().parse_program().unwrap();
        assert!(matches!(
            interp.run(&program),
            Completion::Normal(_) | Completion::Empty
        ));
        let env = Environment::new_function_scope(Some(interp.realm().global_env.clone()));
        for name in ["iter", "inner", "value", "rest"] {
            env.borrow_mut().declare(name, BindingKind::Var);
        }
        (interp, env)
    }

    fn execute(
        interp: &mut Interpreter,
        env: &EnvRef,
        loops: &mut Vec<ForOfLoopState>,
        generator_id: Option<u64>,
        name: &str,
        action: ArrayPatternAction<'_>,
    ) -> Result<(), Completion> {
        interp.run_array_pattern_iter(
            ArrayPatternContext {
                func_env: env,
                term_env: env,
                for_of_stack: loops,
                try_depth: 2,
                next_state: 3,
                generator_id,
            },
            name,
            action,
        )
    }

    #[test]
    fn array_pattern_iterator_exhaustion_is_terminal_for_both_owners() {
        for owner in [None, Some(42)] {
            let (mut interp, env) = setup("var input = [11];");
            let input = interp.get_global_var_ref("input").unwrap();
            let mut loops = Vec::new();
            execute(
                &mut interp,
                &env,
                &mut loops,
                owner,
                "iter",
                ArrayPatternAction::Init(input),
            )
            .unwrap();
            assert_eq!(loops.len(), 1);
            for expected in [
                JsValue::number(11.0),
                JsValue::UNDEFINED,
                JsValue::UNDEFINED,
            ] {
                execute(
                    &mut interp,
                    &env,
                    &mut loops,
                    owner,
                    "iter",
                    ArrayPatternAction::Step(Some("value")),
                )
                .unwrap();
                assert!(same_value(&env.borrow().get("value").unwrap(), &expected));
            }
            assert!(loops.is_empty());
            execute(
                &mut interp,
                &env,
                &mut loops,
                owner,
                "iter",
                ArrayPatternAction::Drain("rest"),
            )
            .unwrap();
            let rest = env.borrow().get("rest").unwrap();
            assert_eq!(
                interp
                    .get_object_cell_expect(rest.as_object_id().unwrap())
                    .borrow()
                    .array_elements()
                    .unwrap()
                    .len(),
                0
            );
            execute(
                &mut interp,
                &env,
                &mut loops,
                owner,
                "iter",
                ArrayPatternAction::Finish,
            )
            .unwrap();
            if let Some(id) = owner {
                assert!(!interp.generator_for_of_stacks.contains_key(&id));
            }
        }
    }

    #[test]
    fn array_pattern_iterator_protocol_failure_retires_only_the_failed_record() {
        for owner in [None, Some(42)] {
            let (mut interp, env) = setup(
                r#"
                var closes = 0;
                var error = {};
                var input = {
                    [Symbol.iterator]() { return this; },
                    next() { throw error; },
                    return() { closes++; return {}; }
                };
            "#,
            );
            let input = interp.get_global_var_ref("input").unwrap();
            let error = interp.get_global_var_ref("error").unwrap();
            let mut loops = Vec::new();
            for name in ["iter", "inner"] {
                execute(
                    &mut interp,
                    &env,
                    &mut loops,
                    owner,
                    name,
                    ArrayPatternAction::Init(input.clone()),
                )
                .unwrap();
            }
            let result = execute(
                &mut interp,
                &env,
                &mut loops,
                owner,
                "inner",
                ArrayPatternAction::Step(None),
            );
            assert!(matches!(result, Err(Completion::Throw(e)) if same_value(&e, &error)));
            assert_eq!(loops.len(), 1);
            assert_eq!(&*loops[0].iter_var, "iter");
            assert_eq!(
                interp.get_global_var_ref("closes").unwrap().as_number(),
                Some(0.0)
            );
            execute(
                &mut interp,
                &env,
                &mut loops,
                owner,
                "inner",
                ArrayPatternAction::Finish,
            )
            .unwrap();
            execute(
                &mut interp,
                &env,
                &mut loops,
                owner,
                "iter",
                ArrayPatternAction::Finish,
            )
            .unwrap();
            execute(
                &mut interp,
                &env,
                &mut loops,
                owner,
                "iter",
                ArrayPatternAction::Finish,
            )
            .unwrap();
            assert_eq!(
                interp.get_global_var_ref("closes").unwrap().as_number(),
                Some(1.0)
            );
        }
    }
}
