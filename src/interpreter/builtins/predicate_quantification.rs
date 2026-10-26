//! Complete Array/TypedArray some/every operations behind one entry point.

use super::super::*;
use super::{array, typedarray};

#[derive(Clone, Copy)]
pub(super) enum Quantifier {
    ArraySome,
    ArrayEvery,
    TypedArraySome,
    TypedArrayEvery,
}

enum Elements {
    Array,
    TypedArray(TypedArrayInfo),
}

impl Elements {
    fn read(
        &self,
        interp: &mut Interpreter,
        object: &JsValue,
        index: usize,
    ) -> Result<Option<JsValue>, Completion> {
        match self {
            Self::Array => array::get_if_present(interp, object, index),
            Self::TypedArray(ta) => Ok(Some(typed_array_get_index(ta, index))),
        }
    }
}

// Array.prototype.some/every and %TypedArray%.prototype.some/every.
pub(super) fn quantify(
    interp: &mut Interpreter,
    method: Quantifier,
    this: &JsValue,
    args: &[JsValue],
) -> Completion {
    interp.with_gc_root_scope(|interp| {
        // Length access can run JS before the predicate or thisArg is used.
        interp.gc_root_value(this);
        for value in args.iter().take(2) {
            interp.gc_root_value(value);
        }
        let (object, length, elements) = match method {
            Quantifier::ArraySome | Quantifier::ArrayEvery => {
                let object = propagate!(array::to_object_val(interp, this));
                interp.gc_root_value(&object);
                let length = propagate!(array::length_of_array_like(interp, &object));
                (object, length, Elements::Array)
            }
            Quantifier::TypedArraySome | Quantifier::TypedArrayEvery => {
                let ta = propagate!(typedarray::validate_typed_array(interp, this));
                let length = typed_array_length(&ta);
                (this.clone(), length, Elements::TypedArray(ta))
            }
        };
        let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
        if !interp.is_callable(&callback) {
            let message = match method {
                Quantifier::ArraySome => "some callback is not a function",
                Quantifier::ArrayEvery => "every callback is not a function",
                Quantifier::TypedArraySome | Quantifier::TypedArrayEvery => {
                    "callback is not a function"
                }
            };
            return Completion::Throw(interp.create_type_error(message));
        }
        let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
        let stop = matches!(method, Quantifier::ArraySome | Quantifier::TypedArraySome);
        for index in 0..length {
            // Array skips missing indices; TypedArray still reads every index
            // after a detach/resize. Neither read changes the captured range.
            let tested = interp.with_gc_root_scope(|interp| -> Result<Option<bool>, Completion> {
                let Some(value) = elements.read(interp, &object, index)? else {
                    return Ok(None);
                };
                interp.gc_root_value(&value);
                match interp.call_function(
                    &callback,
                    &this_arg,
                    &[value, JsValue::number(index as f64), object.clone()],
                ) {
                    Completion::Normal(result) => Ok(Some(interp.to_boolean_val(&result))),
                    other => Err(other),
                }
            });
            if propagate!(tested) == Some(stop) {
                return Completion::Normal(JsValue::boolean(stop));
            }
        }
        Completion::Normal(JsValue::boolean(!stop))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{Completion, Interpreter, JsFunction};
    use crate::types::JsValue;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn normal(completion: Completion) -> JsValue {
        match completion {
            Completion::Normal(value) => value,
            other => panic!("expected normal completion, got {other:?}"),
        }
    }

    fn evaluate(interp: &mut Interpreter, source: &str) -> JsValue {
        let program = crate::parser::Parser::new(source)
            .unwrap()
            .parse_program()
            .unwrap();
        normal(interp.run(&program))
    }

    const METHODS: [(Quantifier, bool, bool); 4] = [
        (Quantifier::ArraySome, false, true),
        (Quantifier::ArrayEvery, false, false),
        (Quantifier::TypedArraySome, true, true),
        (Quantifier::TypedArrayEvery, true, false),
    ];

    #[test]
    fn quantification_owns_callback_operands_truthiness_and_short_circuiting() {
        let mut interp = Interpreter::new();
        let array = evaluate(&mut interp, "[1, 2, 3]");
        let typed = evaluate(&mut interp, "new Uint8Array([1, 2, 3])");
        for (method, is_typed, stop) in METHODS {
            let receiver = if is_typed { &typed } else { &array };
            for stop_at in [None, Some(1)] {
                let seen = Rc::new(RefCell::new(Vec::new()));
                let log = seen.clone();
                let receiver_id = receiver.as_object_id();
                let callback = interp.create_function(JsFunction::native(
                    "predicate".to_string(),
                    3,
                    move |_, this, args| {
                        assert_eq!(this.as_number(), Some(9.0));
                        assert_eq!(args.len(), 3);
                        let index = args[1].as_number().unwrap() as usize;
                        assert_eq!(args[0].as_number(), Some((index + 1) as f64));
                        assert_eq!(args[2].as_object_id(), receiver_id);
                        log.borrow_mut().push(index);
                        let truthy = if Some(index) == stop_at { stop } else { !stop };
                        // Non-boolean results exercise ToBoolean at the seam.
                        Completion::Normal(JsValue::number(if truthy { -7.0 } else { 0.0 }))
                    },
                ));
                let before = interp.gc_root_frame();
                let result = normal(quantify(
                    &mut interp,
                    method,
                    receiver,
                    &[callback, JsValue::number(9.0)],
                ));
                assert_eq!(
                    result.as_boolean(),
                    Some(if stop_at.is_some() { stop } else { !stop })
                );
                assert_eq!(
                    *seen.borrow(),
                    if stop_at.is_some() {
                        vec![0, 1]
                    } else {
                        vec![0, 1, 2]
                    }
                );
                assert_eq!(interp.gc_root_frame(), before);
            }
        }
    }

    #[test]
    fn quantification_validates_empty_inputs_and_propagates_abrupt_completions() {
        let mut interp = Interpreter::new();
        let array = evaluate(&mut interp, "[]");
        let typed = evaluate(&mut interp, "new Uint8Array(0)");
        let callback = interp.create_function(JsFunction::native(
            "must not run".to_string(),
            0,
            |_, _, _| panic!("empty input must not invoke callback"),
        ));
        for (method, is_typed, stop) in METHODS {
            let receiver = if is_typed { &typed } else { &array };
            let before = interp.gc_root_frame();
            assert!(matches!(
                quantify(&mut interp, method, receiver, &[]),
                Completion::Throw(_)
            ));
            assert_eq!(
                normal(quantify(
                    &mut interp,
                    method,
                    receiver,
                    std::slice::from_ref(&callback)
                ))
                .as_boolean(),
                Some(!stop)
            );
            assert_eq!(interp.gc_root_frame(), before);
        }

        let array = evaluate(&mut interp, "[1]");
        let typed = evaluate(&mut interp, "new Uint8Array([1])");
        let abort =
            interp.create_function(JsFunction::native("abort".to_string(), 0, |_, _, _| {
                Completion::Exit(17)
            }));
        let throw =
            interp.create_function(JsFunction::native("throw".to_string(), 0, |_, _, _| {
                Completion::Throw(JsValue::number(42.0))
            }));
        for (method, is_typed, _) in METHODS {
            let receiver = if is_typed { &typed } else { &array };
            let before = interp.gc_root_frame();
            assert!(matches!(
                quantify(&mut interp, method, receiver, std::slice::from_ref(&abort)),
                Completion::Exit(17)
            ));
            assert!(
                matches!(quantify(&mut interp, method, receiver, std::slice::from_ref(&throw)), Completion::Throw(v) if v.as_number() == Some(42.0))
            );
            assert_eq!(interp.gc_root_frame(), before);
        }
    }

    #[test]
    fn quantification_roots_inputs_before_observable_preparation() {
        for (method, stop) in [
            (Quantifier::ArraySome, true),
            (Quantifier::ArrayEvery, false),
        ] {
            let mut interp = Interpreter::new();
            let this_arg = evaluate(&mut interp, "({tag: 73})");
            let callback = evaluate(
                &mut interp,
                "(function(value, index, object) { $262.gc(); if (this.tag !== 73 || value.tag !== index + 1 || object.length !== 2) throw 99; return index === 1; })",
            );
            let receiver = evaluate(
                &mut interp,
                "({ get length() { $262.gc(); return 2; }, get 0() { return {tag: 1}; }, get 1() { return {tag: 2}; } })",
            );
            let before = interp.gc_root_frame();
            assert_eq!(
                normal(quantify(
                    &mut interp,
                    method,
                    &receiver,
                    &[callback, this_arg]
                ))
                .as_boolean(),
                Some(stop)
            );
            assert_eq!(interp.gc_root_frame(), before);
        }
    }
}
