//! IteratorClose helpers shared by the `%IteratorPrototype%` methods. Each one
//! roots the completion it carries while `return()` (arbitrary user code that
//! can collect) runs.

use super::super::super::*;

// IteratorClose that uses get_object_property for .return (invokes getters)
pub(super) fn iterator_close_getter(
    interp: &mut Interpreter,
    iterator: &JsValue,
) -> Result<(), JsValue> {
    if let Some(io_id) = iterator.as_object_id() {
        let return_method = match interp.get_object_property(io_id, "return", iterator) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Ok(()),
        };
        if return_method.is_nullish() {
            return Ok(());
        }
        match interp.call_function(&return_method, iterator, &[]) {
            Completion::Normal(inner_result) => {
                if !inner_result.is_object() {
                    return Err(interp.create_type_error("Iterator result is not an object"));
                }
                Ok(())
            }
            Completion::Throw(e) => Err(e),
            _ => Ok(()),
        }
    } else {
        Ok(())
    }
}

pub(super) fn close_iterator_for_error(
    interp: &mut Interpreter,
    iterator: &JsValue,
    error: JsValue,
) -> JsValue {
    interp.gc_root_value(&error);
    let _ = iterator_close_getter(interp, iterator);
    interp.gc_unroot_value(&error);
    error
}

// IteratorClose per spec, taking a completion and returning updated completion.
// If completion is Err (throw), the original error is preserved even if .return() throws.
pub(super) fn iterator_close_with_completion(
    interp: &mut Interpreter,
    iterator: &JsValue,
    completion: Result<(), JsValue>,
) -> Result<(), JsValue> {
    interp.with_gc_root_scope(|interp| {
        // The thrown value must survive the `return` lookup and call, which run
        // arbitrary user code and can collect.
        if let Err(orig) = &completion {
            interp.gc_root_value(orig);
        }
        iterator_close_with_completion_rooted(interp, iterator, completion)
    })
}

fn iterator_close_with_completion_rooted(
    interp: &mut Interpreter,
    iterator: &JsValue,
    completion: Result<(), JsValue>,
) -> Result<(), JsValue> {
    if let Some(io_id) = iterator.as_object_id() {
        let return_method = match interp.get_object_property(io_id, "return", iterator) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                // Step 5: If completion is a throw completion, return ? completion.
                if let Err(orig) = completion {
                    return Err(orig);
                }
                return Err(e);
            }
            _ => JsValue::UNDEFINED,
        };
        if return_method.is_nullish() {
            return completion;
        }
        let inner_result = interp.call_function(&return_method, iterator, &[]);
        match inner_result {
            Completion::Normal(v) => {
                // Step 5: If completion is throw, return completion
                if let Err(e) = completion {
                    return Err(e);
                }
                // Step 7: If innerResult.[[Value]] is not an Object, throw TypeError
                if !v.is_object() {
                    return Err(interp.create_type_error("Iterator result is not an object"));
                }
                // Step 8: Return completion
                completion
            }
            Completion::Throw(e) => {
                // Step 5: If completion is throw, return original completion
                if let Err(orig) = completion {
                    return Err(orig);
                }
                // Step 6: innerResult is throw, return it
                Err(e)
            }
            _ => completion,
        }
    } else {
        completion
    }
}

// IteratorCloseAll per spec: close iterators in reverse order, accumulating errors
pub(super) fn iterator_close_all(
    interp: &mut Interpreter,
    open_iters: &[(JsValue, JsValue)],
    initial_completion: Result<(), JsValue>,
) -> Result<(), JsValue> {
    let mut completion = initial_completion;
    for (iter, _) in open_iters.iter().rev() {
        completion = iterator_close_with_completion(interp, iter, completion);
    }
    completion
}
