//! `%Iterator.prototype%.find` (§27.1.4.7).

use super::*;

pub(super) fn define_find(interp: &mut Interpreter, iter_proto_id: u64) {
    interp.define_method(iter_proto_id, "find", 1, |interp, this, args| {
        let (predicate, iter, next_method) = propagate!(require_callable_arg(
            interp,
            this,
            args,
            "predicate is not a function"
        ));
        let frame = interp.gc_root_frame();
        interp.gc_root_value(&iter);
        interp.gc_root_value(&next_method);
        let mut counter = 0.0;
        let outcome = loop {
            match interp.iterator_step_value_direct(&iter, &next_method) {
                Ok(Some(value)) => {
                    // A predicate that ignores its parameters leaves the
                    // value reachable only from here.
                    interp.gc_root_value(&value);
                    match interp.call_function(
                        &predicate,
                        &JsValue::UNDEFINED,
                        &[value.clone(), JsValue::number(counter)],
                    ) {
                        Completion::Normal(v) if interp.to_boolean_val(&v) => {
                            if let Err(e) = iterator_close_getter(interp, &iter) {
                                break Completion::Throw(e);
                            }
                            break Completion::Normal(value);
                        }
                        Completion::Throw(e) => {
                            let _ = iterator_close_with_completion(interp, &iter, Err(e.clone()));
                            break Completion::Throw(e);
                        }
                        _ => interp.gc_unroot_value(&value),
                    }
                    counter += 1.0;
                }
                Ok(None) => break Completion::Normal(JsValue::UNDEFINED),
                Err(e) => break Completion::Throw(e),
            }
        };
        interp.gc_unroot_frame(frame);
        outcome
    });
}
