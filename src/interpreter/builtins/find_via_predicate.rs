//! The eight Array/TypedArray find natives share one FindViaPredicate.
//! Receiver preparation and result selection belong here so callers choose
//! only their method and receiver family, not a traversal protocol.

use super::super::*;
use super::{array, typedarray};

#[derive(Clone, Copy)]
pub(super) enum FindMethod {
    Find,
    FindIndex,
    FindLast,
    FindLastIndex,
}

enum ElementAccess {
    Array,
    TypedArray(TypedArrayInfo),
}

impl ElementAccess {
    fn read(
        &self,
        interp: &mut Interpreter,
        object: &JsValue,
        index: usize,
    ) -> Result<JsValue, Completion> {
        match self {
            Self::Array => array::get_index(interp, object, index),
            Self::TypedArray(ta) => Ok(typed_array_get_index(ta, index)),
        }
    }
}

impl FindMethod {
    pub(super) fn array(
        self,
        interp: &mut Interpreter,
        this: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        interp.with_gc_root_scope(|interp| {
            Self::root_inputs(interp, this, args);
            let object = propagate!(array::to_object_val(interp, this));
            // ToObject can create a wrapper whose length getter runs user code.
            interp.gc_root_value(&object);
            let length = propagate!(array::length_of_array_like(interp, &object));
            self.search(interp, &object, length, args, ElementAccess::Array)
        })
    }

    pub(super) fn typed_array(
        self,
        interp: &mut Interpreter,
        this: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        interp.with_gc_root_scope(|interp| {
            Self::root_inputs(interp, this, args);
            let ta = propagate!(typedarray::validate_typed_array(interp, this));
            let length = typed_array_length(&ta);
            self.search(interp, this, length, args, ElementAccess::TypedArray(ta))
        })
    }

    fn root_inputs(interp: &mut Interpreter, this: &JsValue, args: &[JsValue]) {
        interp.gc_root_value(this);
        for value in args.iter().take(2) {
            interp.gc_root_value(value);
        }
    }

    fn selects_index(self) -> bool {
        matches!(self, Self::FindIndex | Self::FindLastIndex)
    }

    fn missing(self) -> Completion {
        Completion::Normal(if self.selects_index() {
            JsValue::number(-1.0)
        } else {
            JsValue::UNDEFINED
        })
    }

    // §23.1.3.12.1 FindViaPredicate. Capture the length once, but read each
    // value at its visit time. Neither holes nor a later detached/OOB view
    // shorten the captured range, and no arena/buffer borrow crosses Call.
    fn search(
        self,
        interp: &mut Interpreter,
        object: &JsValue,
        length: usize,
        args: &[JsValue],
        access: ElementAccess,
    ) -> Completion {
        let predicate = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
        let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
        if !interp.is_callable(&predicate) {
            let message = match access {
                ElementAccess::TypedArray(_) => "callback is not a function",
                ElementAccess::Array => match self {
                    Self::Find => "find predicate is not a function",
                    Self::FindIndex => "findIndex predicate is not a function",
                    Self::FindLast => "findLast predicate is not a function",
                    Self::FindLastIndex => "findLastIndex predicate is not a function",
                },
            };
            return Completion::Throw(interp.create_type_error(message));
        }

        let descending = matches!(self, Self::FindLast | Self::FindLastIndex);
        for offset in 0..length {
            let index = if descending {
                length - 1 - offset
            } else {
                offset
            };
            let selected = interp.with_gc_root_scope(|interp| {
                let value = access.read(interp, object, index)?;
                interp.gc_root_value(&value);
                let tested = match interp.call_function(
                    &predicate,
                    &this_arg,
                    &[value.clone(), JsValue::number(index as f64), object.clone()],
                ) {
                    Completion::Normal(result) => interp.to_boolean_val(&result),
                    other => return Err(other),
                };
                Ok(tested.then_some(value))
            });
            match selected {
                Ok(Some(value)) => {
                    return Completion::Normal(if self.selects_index() {
                        JsValue::number(index as f64)
                    } else {
                        value
                    });
                }
                Ok(None) => {}
                Err(other) => return other,
            }
        }
        self.missing()
    }
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

    #[test]
    fn direction_projection_and_miss_are_shared_by_both_families() {
        let mut interp = Interpreter::new();
        let array = evaluate(&mut interp, "[10, 20, 30]");
        let typed = evaluate(&mut interp, "new Uint8Array([10, 20, 30])");
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = seen.clone();
        let predicate = interp.create_function(JsFunction::native(
            "predicate".to_string(),
            3,
            move |_, this, args| {
                assert_eq!(this.as_number(), Some(77.0));
                assert_eq!(args.len(), 3);
                log.borrow_mut().push((
                    args[0].as_number().unwrap(),
                    args[1].as_number().unwrap(),
                    args[2].as_object_id().unwrap(),
                ));
                Completion::Normal(JsValue::boolean(args[0].as_number() == Some(20.0)))
            },
        ));
        let args = [predicate, JsValue::number(77.0)];
        for (method, expected, indices) in [
            (FindMethod::Find, 20.0, vec![0.0, 1.0]),
            (FindMethod::FindIndex, 1.0, vec![0.0, 1.0]),
            (FindMethod::FindLast, 20.0, vec![2.0, 1.0]),
            (FindMethod::FindLastIndex, 1.0, vec![2.0, 1.0]),
        ] {
            for (receiver, typed_family) in [(&array, false), (&typed, true)] {
                seen.borrow_mut().clear();
                let result = if typed_family {
                    method.typed_array(&mut interp, receiver, &args)
                } else {
                    method.array(&mut interp, receiver, &args)
                };
                assert_eq!(normal(result).as_number(), Some(expected));
                let seen = seen.borrow();
                assert_eq!(seen.iter().map(|v| v.1).collect::<Vec<_>>(), indices);
                assert!(seen.iter().all(|v| v.0 == (v.1 + 1.0) * 10.0));
                assert!(seen.iter().all(|v| Some(v.2) == receiver.as_object_id()));
            }
        }

        let never =
            interp.create_function(JsFunction::native("never".to_string(), 0, |_, _, _| {
                Completion::Normal(JsValue::boolean(false))
            }));
        for method in [FindMethod::Find, FindMethod::FindLast] {
            assert!(normal(method.array(&mut interp, &array, &[never.clone()])).is_undefined());
            assert!(
                normal(method.typed_array(&mut interp, &typed, &[never.clone()])).is_undefined()
            );
        }
        for method in [FindMethod::FindIndex, FindMethod::FindLastIndex] {
            assert_eq!(
                normal(method.array(&mut interp, &array, &[never.clone()])).as_number(),
                Some(-1.0)
            );
            assert_eq!(
                normal(method.typed_array(&mut interp, &typed, &[never.clone()])).as_number(),
                Some(-1.0)
            );
        }
    }

    #[test]
    fn empty_inputs_validate_predicates_and_scopes_restore_on_abrupt_exit() {
        let mut interp = Interpreter::new();
        let array = evaluate(&mut interp, "[]");
        let typed = evaluate(&mut interp, "new Uint8Array(0)");
        let before = interp.gc_root_frame();
        for method in [
            FindMethod::Find,
            FindMethod::FindIndex,
            FindMethod::FindLast,
            FindMethod::FindLastIndex,
        ] {
            assert!(matches!(
                method.array(&mut interp, &array, &[]),
                Completion::Throw(_)
            ));
            assert_eq!(interp.gc_root_frame(), before);
            assert!(matches!(
                method.typed_array(&mut interp, &typed, &[]),
                Completion::Throw(_)
            ));
            assert_eq!(interp.gc_root_frame(), before);
        }

        let array = evaluate(&mut interp, "[1, 2]");
        let typed = evaluate(&mut interp, "new Uint8Array([1, 2])");
        let abrupt =
            interp.create_function(JsFunction::native("abrupt".to_string(), 0, |_, _, _| {
                Completion::Throw(JsValue::number(42.0))
            }));
        for result in [
            FindMethod::Find.array(&mut interp, &array, &[abrupt.clone()]),
            FindMethod::FindLast.typed_array(&mut interp, &typed, &[abrupt]),
        ] {
            assert!(matches!(result, Completion::Throw(value) if value.as_number() == Some(42.0)));
        }
        assert_eq!(interp.gc_root_frame(), before);
    }
}
