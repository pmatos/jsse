//! Complete Array/TypedArray reductions behind one native operation.

use super::super::*;
use super::{array, typedarray};

#[derive(Clone, Copy)]
pub(super) enum Reduction {
    Array,
    ArrayRight,
    TypedArray,
    TypedArrayRight,
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

pub(super) fn reduce(
    interp: &mut Interpreter,
    method: Reduction,
    this: &JsValue,
    args: &[JsValue],
) -> Completion {
    interp.with_gc_root_scope(|interp| {
        // Length access can run JS before the callback or initial value is used.
        interp.gc_root_value(this);
        for value in args.iter().take(2) {
            interp.gc_root_value(value);
        }
        let (object, length, elements) = match method {
            Reduction::Array | Reduction::ArrayRight => {
                let object = propagate!(array::to_object_val(interp, this));
                interp.gc_root_value(&object);
                let length = propagate!(array::length_of_array_like(interp, &object));
                (object, length, Elements::Array)
            }
            Reduction::TypedArray | Reduction::TypedArrayRight => {
                let ta = propagate!(typedarray::validate_typed_array(interp, this));
                let length = typed_array_length(&ta);
                (this.clone(), length, Elements::TypedArray(ta))
            }
        };
        let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
        if !interp.is_callable(&callback) {
            let message = match method {
                Reduction::Array => "reduce callback is not a function",
                Reduction::ArrayRight => "reduceRight callback is not a function",
                Reduction::TypedArray | Reduction::TypedArrayRight => "callback is not a function",
            };
            return Completion::Throw(interp.callback_error(&callback, message));
        }

        // Some(undefined) is an explicit initial value; None still needs a seed.
        let mut accumulator = args.get(1).cloned();
        let backwards = matches!(method, Reduction::ArrayRight | Reduction::TypedArrayRight);
        for offset in 0..length {
            let index = if backwards {
                length - 1 - offset
            } else {
                offset
            };
            let visited = interp.with_gc_root_scope(|interp| -> Result<(), Completion> {
                // A callback-created accumulator may have no other owner while
                // the next HasProperty/Get runs JS. Release it after this visit
                // so replaced accumulators do not build an ever-growing root set.
                if let Some(acc) = &accumulator {
                    interp.gc_root_value(acc);
                }
                let Some(value) = elements.read(interp, &object, index)? else {
                    return Ok(());
                };
                interp.gc_root_value(&value);
                accumulator = Some(if let Some(acc) = &accumulator {
                    match interp.call_function(
                        &callback,
                        &JsValue::UNDEFINED,
                        &[
                            acc.clone(),
                            value,
                            JsValue::number(index as f64),
                            object.clone(),
                        ],
                    ) {
                        Completion::Normal(next) => next,
                        other => return Err(other),
                    }
                } else {
                    value
                });
                Ok(())
            });
            propagate!(visited);
            // No JS runs between releasing this visit and rooting the new
            // accumulator in the next visit (or returning it to the caller).
        }
        match accumulator {
            Some(value) => Completion::Normal(value),
            None => Completion::Throw(
                interp.create_type_error("Reduce of empty array with no initial value"),
            ),
        }
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

    const METHODS: [Reduction; 4] = [
        Reduction::Array,
        Reduction::ArrayRight,
        Reduction::TypedArray,
        Reduction::TypedArrayRight,
    ];

    #[test]
    fn reduction_owns_direction_initial_value_and_callback_operands() {
        let mut interp = Interpreter::new();
        let array = evaluate(&mut interp, "[1, 2, 3]");
        let typed = evaluate(&mut interp, "new Uint8Array([1, 2, 3])");
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = seen.clone();
        let callback = interp.create_function(JsFunction::native(
            "fold".to_string(),
            4,
            move |_, this, args| {
                assert!(this.is_undefined());
                assert_eq!(args.len(), 4);
                log.borrow_mut().push((
                    args[0].as_number().unwrap(),
                    args[1].as_number().unwrap(),
                    args[2].as_number().unwrap(),
                    args[3].as_object_id().unwrap(),
                ));
                Completion::Normal(JsValue::number(
                    args[0].as_number().unwrap() * 10.0 + args[1].as_number().unwrap(),
                ))
            },
        ));
        for (method, receiver, backwards) in [
            (Reduction::Array, &array, false),
            (Reduction::ArrayRight, &array, true),
            (Reduction::TypedArray, &typed, false),
            (Reduction::TypedArrayRight, &typed, true),
        ] {
            for initial in [false, true] {
                seen.borrow_mut().clear();
                let mut args = vec![callback.clone()];
                if initial {
                    args.push(JsValue::number(9.0));
                }
                let before = interp.gc_root_frame();
                let result = normal(reduce(&mut interp, method, receiver, &args));
                let expected = match (backwards, initial) {
                    (false, false) => 123.0,
                    (false, true) => 9123.0,
                    (true, false) => 321.0,
                    (true, true) => 9321.0,
                };
                assert_eq!(result.as_number(), Some(expected));
                assert_eq!(interp.gc_root_frame(), before);
                let seen = seen.borrow();
                let order = match (backwards, initial) {
                    (false, false) => vec![1.0, 2.0],
                    (false, true) => vec![0.0, 1.0, 2.0],
                    (true, false) => vec![1.0, 0.0],
                    (true, true) => vec![2.0, 1.0, 0.0],
                };
                assert_eq!(seen.iter().map(|v| v.2).collect::<Vec<_>>(), order);
                assert!(seen.iter().all(|v| v.1 == v.2 + 1.0));
                assert!(seen.iter().all(|v| Some(v.3) == receiver.as_object_id()));
                assert_eq!(
                    seen[0].0,
                    if initial {
                        9.0
                    } else if backwards {
                        3.0
                    } else {
                        1.0
                    }
                );
            }
        }
    }

    #[test]
    fn reduction_distinguishes_omission_and_restores_roots_on_abrupt_exits() {
        let mut interp = Interpreter::new();
        let array = evaluate(&mut interp, "[]");
        let typed = evaluate(&mut interp, "new Uint8Array(0)");
        let callback = interp.create_function(JsFunction::native(
            "must not run".to_string(),
            0,
            |_, _, _| panic!("empty input must not invoke callback"),
        ));
        for (method, receiver) in METHODS.into_iter().zip([&array, &array, &typed, &typed]) {
            let before = interp.gc_root_frame();
            for args in [vec![], vec![callback.clone()]] {
                assert!(matches!(
                    reduce(&mut interp, method, receiver, &args),
                    Completion::Throw(_)
                ));
                assert_eq!(interp.gc_root_frame(), before);
            }
            assert!(
                normal(reduce(
                    &mut interp,
                    method,
                    receiver,
                    &[callback.clone(), JsValue::UNDEFINED],
                ))
                .is_undefined()
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
        for (method, receiver) in METHODS.into_iter().zip([&array, &array, &typed, &typed]) {
            let before = interp.gc_root_frame();
            assert!(matches!(
                reduce(
                    &mut interp,
                    method,
                    receiver,
                    &[abort.clone(), JsValue::UNDEFINED]
                ),
                Completion::Exit(17)
            ));
            assert!(
                matches!(reduce(&mut interp, method, receiver, &[throw.clone(), JsValue::UNDEFINED]), Completion::Throw(v) if v.as_number() == Some(42.0))
            );
            assert_eq!(interp.gc_root_frame(), before);
        }
    }
}
