//! Complete Array/TypedArray indexOf, lastIndexOf and includes operations.

use super::super::*;
use super::{array, array_dense, typedarray};

#[derive(Clone, Copy)]
pub(super) enum Search {
    ArrayIndexOf,
    ArrayLastIndexOf,
    ArrayIncludes,
    TypedArrayIndexOf,
    TypedArrayLastIndexOf,
    TypedArrayIncludes,
}

impl Search {
    fn backwards(self) -> bool {
        matches!(self, Self::ArrayLastIndexOf | Self::TypedArrayLastIndexOf)
    }

    fn includes(self) -> bool {
        matches!(self, Self::ArrayIncludes | Self::TypedArrayIncludes)
    }

    fn equal(self, element: &JsValue, needle: &JsValue) -> bool {
        if self.includes() {
            same_value_zero(element, needle)
        } else {
            strict_equality(element, needle)
        }
    }

    fn result(self, found: Option<usize>) -> Completion {
        Completion::Normal(if self.includes() {
            JsValue::boolean(found.is_some())
        } else {
            JsValue::number(found.map_or(-1.0, |index| index as f64))
        })
    }
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
        method: Search,
        index: usize,
    ) -> Result<Option<JsValue>, Completion> {
        match self {
            Self::Array if method.includes() => array::get_index(interp, object, index).map(Some),
            Self::Array => array::get_if_present(interp, object, index),
            Self::TypedArray(ta) => {
                // Index-returning methods use HasProperty. includes reads invalid
                // indices as undefined after a detach/resize during coercion.
                if !method.includes() && !is_valid_integer_index(ta, index as f64) {
                    Ok(None)
                } else {
                    Ok(Some(typed_array_get_index(ta, index)))
                }
            }
        }
    }
}

pub(super) fn search(
    interp: &mut Interpreter,
    method: Search,
    this: &JsValue,
    args: &[JsValue],
) -> Completion {
    interp.with_gc_root_scope(|interp| {
        // Preparation, fromIndex, and Array property access can all execute JS.
        interp.gc_root_value(this);
        for value in args.iter().take(2) {
            interp.gc_root_value(value);
        }
        let (object, length, elements) = match method {
            Search::ArrayIndexOf | Search::ArrayLastIndexOf | Search::ArrayIncludes => {
                let object = propagate!(array::to_object_val(interp, this));
                interp.gc_root_value(&object);
                let length = propagate!(array::length_of_array_like(interp, &object));
                (object, length, Elements::Array)
            }
            Search::TypedArrayIndexOf
            | Search::TypedArrayLastIndexOf
            | Search::TypedArrayIncludes => {
                let ta = propagate!(typedarray::validate_typed_array(interp, this));
                let length = typed_array_length(&ta);
                (this.clone(), length, Elements::TypedArray(ta))
            }
        };
        if length == 0 {
            return method.result(None);
        }
        let needle = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
        let backwards = method.backwards();
        // Only omission uses the reverse default; explicit undefined coerces to 0.
        let from = match args.get(1) {
            Some(value) => propagate!(interp.to_integer_or_infinity_value(value)),
            None if backwards => (length - 1) as f64,
            None => 0.0,
        };
        let start = if backwards {
            if from >= 0.0 {
                from.min((length - 1) as f64) as usize
            } else {
                let relative = length as f64 + from;
                if relative < 0.0 {
                    return method.result(None);
                }
                relative as usize
            }
        } else {
            if from >= length as f64 {
                return method.result(None);
            }
            if from >= 0.0 {
                from as usize
            } else {
                (length as f64 + from).max(0.0) as usize
            }
        };
        // Re-check the existing dense guard after observable coercion. Its
        // resume cursor leaves every hole and inherited index to the MOP.
        let (found, resume) = if matches!(elements, Elements::Array) {
            if backwards {
                array_dense::scan_backward(interp, &object, start, |v| method.equal(v, &needle))
            } else {
                array_dense::scan_forward(interp, &object, start, length, |v| {
                    method.equal(v, &needle)
                })
            }
        } else {
            (None, if backwards { start + 1 } else { start })
        };
        if found.is_some() {
            return method.result(found);
        }
        let mut indices = if backwards { 0..resume } else { resume..length };
        loop {
            let index = if backwards {
                indices.next_back()
            } else {
                indices.next()
            };
            let Some(index) = index else { break };
            if let Some(value) = propagate!(elements.read(interp, &object, method, index))
                && method.equal(&value, &needle)
            {
                return method.result(Some(index));
            }
        }
        method.result(None)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{Completion, Interpreter};
    use crate::types::JsValue;

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
    fn search_owns_preparation_direction_equality_and_results() {
        let mut interp = Interpreter::new();
        for (source, methods) in [
            (
                "[1, NaN, -0, 1]",
                [
                    Search::ArrayIndexOf,
                    Search::ArrayLastIndexOf,
                    Search::ArrayIncludes,
                ],
            ),
            (
                "new Float64Array([1, NaN, -0, 1])",
                [
                    Search::TypedArrayIndexOf,
                    Search::TypedArrayLastIndexOf,
                    Search::TypedArrayIncludes,
                ],
            ),
        ] {
            let receiver = evaluate(&mut interp, source);
            for (args, expected) in [
                (vec![JsValue::number(1.0)], [0.0, 3.0, 1.0]),
                (vec![JsValue::number(f64::NAN)], [-1.0, -1.0, 1.0]),
                (vec![JsValue::number(0.0)], [2.0, 2.0, 1.0]),
                (
                    vec![JsValue::number(1.0), JsValue::UNDEFINED],
                    [0.0, 0.0, 1.0],
                ),
                (
                    vec![JsValue::number(1.0), JsValue::number(-1.0)],
                    [3.0, 3.0, 1.0],
                ),
                (
                    vec![JsValue::number(1.0), JsValue::number(f64::INFINITY)],
                    [-1.0, 3.0, 0.0],
                ),
                (
                    vec![JsValue::number(1.0), JsValue::number(f64::NEG_INFINITY)],
                    [0.0, -1.0, 1.0],
                ),
            ] {
                for (index, method) in methods.iter().enumerate() {
                    let value = normal(search(&mut interp, *method, &receiver, &args));
                    if index == 2 {
                        assert_eq!(value.as_boolean(), Some(expected[index] != 0.0));
                    } else {
                        assert_eq!(value.as_number(), Some(expected[index]));
                    }
                }
            }
        }
    }

    #[test]
    fn search_roots_inputs_across_length_and_from_index_and_balances_abrupt_exits() {
        let mut interp = Interpreter::new();
        let receiver = evaluate(
            &mut interp,
            "({0: {tag: 1}, get length() { $262.gc(); return 1; }})",
        );
        interp.gc_root_value(&receiver);
        let target =
            normal(interp.get_object_property(receiver.as_object_id().unwrap(), "0", &receiver));
        let from = evaluate(&mut interp, "({valueOf() { $262.gc(); return 0; }})");
        interp.gc_unroot_value(&receiver);
        let depth = interp.gc_root_frame();
        let result = normal(search(
            &mut interp,
            Search::ArrayIndexOf,
            &receiver,
            &[target, from],
        ));
        assert_eq!(result.as_number(), Some(0.0));
        assert_eq!(interp.gc_root_frame(), depth);

        // The needle must survive even when it is not reachable from the receiver.
        let needle = evaluate(&mut interp, "({tag: 42})");
        interp.gc_root_value(&needle);
        let receiver = evaluate(
            &mut interp,
            "({get length() { $262.gc(); return 1; }, get 0() { return {}; }})",
        );
        interp.gc_unroot_value(&needle);
        let result = normal(search(
            &mut interp,
            Search::ArrayIndexOf,
            &receiver,
            std::slice::from_ref(&needle),
        ));
        assert_eq!(result.as_number(), Some(-1.0));
        let tag =
            normal(interp.get_object_property(needle.as_object_id().unwrap(), "tag", &needle));
        assert_eq!(tag.as_number(), Some(42.0));
        assert_eq!(interp.gc_root_frame(), depth);

        let receiver = evaluate(&mut interp, "({get length() { throw 42; }})");
        assert!(matches!(
            search(&mut interp, Search::ArrayIncludes, &receiver, &[]),
            Completion::Throw(value) if value.as_number() == Some(42.0)
        ));
        assert_eq!(interp.gc_root_frame(), depth);
    }
}
