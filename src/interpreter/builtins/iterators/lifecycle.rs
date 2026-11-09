//! Generator-style lifecycle shared by every lazy Iterator Helper adapter.

use crate::interpreter::{
    Interpreter,
    types::{Completion, IterHelperData, IteratorHelperPhase as Phase, JsFunction, ObjectKind},
};
use crate::types::JsValue;
use std::cell::Cell;
use std::rc::Rc;

pub(super) enum HelperStep {
    Yield(JsValue),
    // A final partial yield: resume once more without stepping or closing the source.
    FinalYield(JsValue),
    Done,
}

impl Interpreter {
    /// Validation and terminal results run in the invoked prototype method's
    /// Realm; active resumes enter the native adapter's creation Realm.
    pub(super) fn resume_iterator_helper(
        &mut self,
        this: &JsValue,
        args: &[JsValue],
        returning: bool,
    ) -> Completion {
        let method = if returning { "return" } else { "next" };
        let Some(id) = this.as_object_id() else {
            return Completion::Throw(
                self.create_type_error(&format!("Iterator Helper {method} called on non-object")),
            );
        };
        let (callback, phase) =
            {
                let Some(object) = self.get_object(id) else {
                    return Completion::Throw(self.create_type_error(&format!(
                        "Iterator Helper {method} called on invalid object"
                    )));
                };
                let object = object.borrow();
                let Some(IterHelperData::Helper {
                    next,
                    return_closure,
                    phase,
                }) = object.iter_helper()
                else {
                    return Completion::Throw(self.create_type_error(&format!(
                        "{method} method called on incompatible object"
                    )));
                };
                (
                    if returning {
                        return_closure.clone()
                    } else {
                        next.clone()
                    },
                    phase.get(),
                )
            };
        match phase {
            Phase::Executing => {
                Completion::Throw(self.create_type_error("Generator is already running"))
            }
            Phase::Completed => {
                Completion::Normal(self.create_iter_result_object(JsValue::UNDEFINED, true))
            }
            Phase::SuspendedStart | Phase::SuspendedYield | Phase::FinalYield => {
                self.call_function(&callback, this, args)
            }
        }
    }

    /// Adapters own stepping and source-close policy, but never lifecycle state
    /// or iterator-result construction. Fixed captures are pinned on the helper;
    /// replaceable captures remain in Rooted Slots supplied among `roots`.
    pub(super) fn create_iterator_helper(
        &mut self,
        roots: Vec<JsValue>,
        step: impl Fn(&mut Interpreter) -> Result<HelperStep, Completion> + 'static,
        close: impl Fn(&mut Interpreter) -> Result<(), JsValue> + 'static,
    ) -> JsValue {
        self.ensure_iterator_helper_prototype();
        let phase = Rc::new(Cell::new(Phase::SuspendedStart));
        let next_phase = phase.clone();
        let return_phase = phase.clone();
        let next_fn = self.create_function(JsFunction::native(
            "next".to_string(),
            0,
            move |interp, this, _args| {
                interp.with_gc_root_scope(|interp| {
                    interp.gc_root_value(this);
                    match next_phase.get() {
                        Phase::Executing => {
                            return Completion::Throw(
                                interp.create_type_error("Generator is already running"),
                            );
                        }
                        Phase::FinalYield => {
                            next_phase.set(Phase::Completed);
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            );
                        }
                        Phase::SuspendedStart | Phase::SuspendedYield => {}
                        Phase::Completed => {
                            unreachable!("completed helper bypasses its trampoline")
                        }
                    }
                    next_phase.set(Phase::Executing);
                    match step(interp) {
                        Ok(HelperStep::Yield(value)) => {
                            next_phase.set(Phase::SuspendedYield);
                            Completion::Normal(interp.create_iter_result_object(value, false))
                        }
                        Ok(HelperStep::FinalYield(value)) => {
                            next_phase.set(Phase::FinalYield);
                            Completion::Normal(interp.create_iter_result_object(value, false))
                        }
                        Ok(HelperStep::Done) => {
                            next_phase.set(Phase::Completed);
                            Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            )
                        }
                        Err(completion) => {
                            next_phase.set(Phase::Completed);
                            completion
                        }
                    }
                })
            },
        ));
        let return_fn = self.create_function(JsFunction::native(
            "return".to_string(),
            0,
            move |interp, this, _args| {
                interp.with_gc_root_scope(|interp| {
                    interp.gc_root_value(this);
                    match return_phase.get() {
                        Phase::Executing => {
                            return Completion::Throw(
                                interp.create_type_error("Generator is already running"),
                            );
                        }
                        Phase::FinalYield => {
                            return_phase.set(Phase::Completed);
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            );
                        }
                        // %IteratorHelperPrototype%.return completes before
                        // IteratorCloseAll if no body has started. Reentry from
                        // source close therefore observes completion.
                        Phase::SuspendedStart => return_phase.set(Phase::Completed),
                        Phase::SuspendedYield => return_phase.set(Phase::Executing),
                        Phase::Completed => {
                            unreachable!("completed helper bypasses its trampoline")
                        }
                    }
                    let result = close(interp);
                    return_phase.set(Phase::Completed);
                    match result {
                        Ok(()) => Completion::Normal(
                            interp.create_iter_result_object(JsValue::UNDEFINED, true),
                        ),
                        Err(error) => Completion::Throw(error),
                    }
                })
            },
        ));

        let id = self.create_object_id();
        {
            let mut object = self.get_object_cell_expect(id).borrow_mut();
            object.set_proto(self.realm().iterator_helper_prototype);
            object.set_class("Iterator Helper");
            object.kind = ObjectKind::IterHelper(Box::new(IterHelperData::Helper {
                next: next_fn.clone(),
                return_closure: return_fn.clone(),
                phase,
            }));
            object.gc_native_roots_mut().extend([next_fn, return_fn]);
        }
        let helper = JsValue::object(id);
        for root in &roots {
            self.pin_native_root(&helper, root);
        }
        helper
    }
}

#[cfg(test)]
mod tests;
