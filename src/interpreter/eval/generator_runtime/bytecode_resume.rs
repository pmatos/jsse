//! `Generator.prototype.next`/`return`/`throw` for a generator whose body is a
//! bytecode chunk.

use super::*;

impl Interpreter {
    /// `Generator.prototype.next`/`return`/`throw` (sec-generatorresume,
    /// sec-generatorresumeabrupt) for a generator whose body is a bytecode
    /// chunk. `raw` asks for the `(value, done)` pair parked in
    /// `raw_iter_step` rather than a result object (see
    /// [`Self::generator_step_value`]).
    pub(crate) fn bytecode_generator_resume(
        &mut self,
        this: &JsValue,
        resume: Resume,
        raw: bool,
    ) -> Completion {
        enum Plan {
            Reject,
            Finished(Completion),
            Done(JsValue),
            Run {
                code: Rc<BytecodeGeneratorCode>,
                frame: Box<GeneratorFrame>,
                resume: Option<Resume>,
                realm: Option<usize>,
            },
        }
        let cell = this.as_object_id().and_then(|id| self.get_object(id));
        let Some(cell) = cell else {
            return Completion::Throw(self.create_type_error("not a generator object"));
        };
        let plan = {
            let mut obj = cell.borrow_mut();
            let realm = obj.generator_realm_id();
            let Some(IteratorState::BytecodeGenerator(generator)) = obj.iterator_state_mut() else {
                drop(obj);
                return Completion::Throw(self.create_type_error("not a generator object"));
            };
            let status = generator.status;
            match (status, resume) {
                (BytecodeGeneratorStatus::Executing, _) => Plan::Reject,
                (BytecodeGeneratorStatus::Completed, Resume::Next(_)) => {
                    Plan::Done(JsValue::UNDEFINED)
                }
                (BytecodeGeneratorStatus::Completed, Resume::Return(value)) => Plan::Done(value),
                (BytecodeGeneratorStatus::Completed, Resume::Throw(error)) => {
                    Plan::Finished(Completion::Throw(error))
                }
                (BytecodeGeneratorStatus::SuspendedStart, Resume::Return(value)) => {
                    generator.status = BytecodeGeneratorStatus::Completed;
                    Plan::Done(value)
                }
                (BytecodeGeneratorStatus::SuspendedStart, Resume::Throw(error)) => {
                    generator.status = BytecodeGeneratorStatus::Completed;
                    Plan::Finished(Completion::Throw(error))
                }
                (status, resume) => {
                    generator.status = BytecodeGeneratorStatus::Executing;
                    let frame = generator.frame.take().unwrap_or_else(|| {
                        Box::new(GeneratorFrame::for_chunk(self, &generator.code.chunk))
                    });
                    Plan::Run {
                        code: Rc::clone(&generator.code),
                        frame,
                        // The first `next(v)` has no `yield` to deliver `v` to.
                        resume: (status == BytecodeGeneratorStatus::SuspendedYield)
                            .then_some(resume),
                        realm,
                    }
                }
            }
        };
        let (code, mut frame, resume, realm) = match plan {
            Plan::Reject => {
                return Completion::Throw(self.create_type_error("Generator is already executing"));
            }
            Plan::Finished(completion) => return completion,
            Plan::Done(value) => return self.generator_step_result(raw, value, true),
            Plan::Run {
                code,
                frame,
                resume,
                realm,
            } => (code, frame, resume, realm),
        };
        let gen_id = this.as_object_id().unwrap_or_default();
        let result = self.with_realm(realm, |interp| {
            interp.call_stack_envs.push(code.func_env.clone());
            let framed = interp.node_enter_generator(gen_id);
            // Resumptions are executions of the body, not invocations (see
            // `exec_state_machine_body`); tree-walker hand-offs inside are
            // credited to the function that owns the chunk.
            #[cfg(feature = "perf-counters")]
            {
                interp.perf.body_non_function += 1;
                if !code.chunk.ast.is_empty() {
                    let (name, id) = code.perf_key.clone();
                    interp.perf.enter_ast_body(name, id, false);
                }
            }
            let result = interp.with_ic_body_in_frame(&code.body, &code.frame, |interp| {
                vm::run_generator_chunk(interp, &code.chunk, &code.func_env, &mut frame, resume)
            });
            #[cfg(feature = "perf-counters")]
            if !code.chunk.ast.is_empty() {
                let exclusive = interp.perf.leave_ast_body();
                interp.perf.ast_units_in_fallbacks += exclusive;
            }
            interp.node_leave_generator(framed);
            interp.call_stack_envs.pop();
            result
        });
        let finish = |frame: Option<Box<GeneratorFrame>>| {
            if let Some(IteratorState::BytecodeGenerator(generator)) =
                cell.borrow_mut().iterator_state_mut()
            {
                generator.status = if frame.is_some() {
                    BytecodeGeneratorStatus::SuspendedYield
                } else {
                    BytecodeGeneratorStatus::Completed
                };
                generator.frame = frame;
            } else {
                debug_assert!(false, "resumed bytecode generator lost its state");
            }
        };
        match result {
            Completion::Yield(value) => {
                finish(Some(frame));
                self.generator_step_result(raw, value, false)
            }
            Completion::Return(value) => {
                finish(None);
                frame.release(self);
                self.generator_step_result(raw, value, true)
            }
            Completion::Normal(_) | Completion::Empty => {
                finish(None);
                frame.release(self);
                self.generator_step_result(raw, JsValue::UNDEFINED, true)
            }
            other => {
                finish(None);
                frame.release(self);
                debug_assert!(
                    matches!(other, Completion::Throw(_) | Completion::Exit(_)),
                    "a generator body completed with {other:?}"
                );
                other
            }
        }
    }
}
