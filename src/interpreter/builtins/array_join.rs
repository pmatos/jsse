//! Complete Array/TypedArray JoinIndexedProperties operations.

use super::super::*;
use super::{array, typedarray};

#[derive(Clone, Copy)]
pub(super) enum JoinMethod {
    ArrayJoin,
    ArrayLocaleString,
    TypedArrayJoin,
    TypedArrayLocaleString,
}

enum Elements {
    Array,
    TypedArray(TypedArrayInfo),
}

// Array.prototype.join/toLocaleString and their %TypedArray% counterparts.
pub(super) fn join(
    interp: &mut Interpreter,
    method: JoinMethod,
    this: &JsValue,
    args: &[JsValue],
) -> Completion {
    let locale = matches!(
        method,
        JoinMethod::ArrayLocaleString | JoinMethod::TypedArrayLocaleString
    );
    interp.with_gc_root_scope(|interp| {
        // Length access can collect before separator/locales/options are used.
        interp.gc_root_value(this);
        for value in args.iter().take(if locale { 2 } else { 1 }) {
            interp.gc_root_value(value);
        }
        let (object, length, elements) = match method {
            JoinMethod::ArrayJoin | JoinMethod::ArrayLocaleString => {
                let object = propagate!(array::to_object_val(interp, this));
                interp.gc_root_value(&object);
                let length = propagate!(array::length_of_array_like(interp, &object));
                (object, length, Elements::Array)
            }
            JoinMethod::TypedArrayJoin | JoinMethod::TypedArrayLocaleString => {
                let ta = propagate!(typedarray::validate_typed_array(interp, this));
                let length = typed_array_length(&ta);
                (this.clone(), length, Elements::TypedArray(ta))
            }
        };
        let separator = if !locale && args.first().is_some_and(|v| !v.is_undefined()) {
            propagate!(interp.to_js_string(&args[0]))
        } else {
            JsString::from_str(",")
        };
        let locale_args = locale.then(|| {
            [
                args.first().cloned().unwrap_or(JsValue::UNDEFINED),
                args.get(1).cloned().unwrap_or(JsValue::UNDEFINED),
            ]
        });

        // Preserve the host's Array-join cycle policy. Separator coercion is
        // outside this extent, even for an empty or already active receiver.
        let active_receiver = matches!(method, JoinMethod::ArrayJoin).then(|| {
            object
                .as_object_id()
                .expect("ToObject must return an object")
        });
        if let Some(id) = active_receiver {
            if interp.active_array_joins.contains(&id) {
                return Completion::Normal(JsValue::string(JsString::from_str("")));
            }
            interp.active_array_joins.push(id);
        }
        let result = join_indexed_properties(
            interp,
            &object,
            &elements,
            length,
            &separator,
            locale_args.as_ref(),
        );
        if let Some(id) = active_receiver {
            let popped = interp.active_array_joins.pop();
            debug_assert_eq!(popped, Some(id));
        }
        result
    })
}

fn join_indexed_properties(
    interp: &mut Interpreter,
    object: &JsValue,
    elements: &Elements,
    length: usize,
    separator: &JsString,
    locale_args: Option<&[JsValue; 2]>,
) -> Completion {
    let mut units = Vec::new();
    for index in 0..length {
        if index > 0 {
            propagate!(interp.append_code_units(&mut units, separator.as_units()));
        }
        // Release each visit's roots before reading the next element.
        let visit = interp.with_gc_root_scope(|interp| -> Result<(), Completion> {
            let element = match elements {
                Elements::TypedArray(ta) => typed_array_get_index(ta, index),
                Elements::Array => {
                    let fast = if locale_args.is_none() {
                        interp.numeric_index_fast_get(object, index as f64)
                    } else {
                        None
                    };
                    match fast {
                        Some(value) => value,
                        None => match interp.get_object_property(
                            object.as_object_id().expect("prepared object"),
                            &index.to_string(),
                            object,
                        ) {
                            Completion::Normal(value) => value,
                            other => return Err(other),
                        },
                    }
                }
            };
            if element.is_nullish() {
                return Ok(());
            }
            interp.gc_root_value(&element);
            if let Some(args) = locale_args {
                append_locale_string(interp, &mut units, &element, args)
            } else {
                interp
                    .append_js_string(&mut units, &element)
                    .map_err(Completion::Throw)
            }
        });
        propagate!(visit);
    }
    Completion::Normal(JsValue::string(JsString::from_vec(units)))
}

// ToString(Invoke(element, "toLocaleString", « locales, options »)).
fn append_locale_string(
    interp: &mut Interpreter,
    out: &mut Vec<u16>,
    element: &JsValue,
    args: &[JsValue; 2],
) -> Result<(), Completion> {
    let element_obj = match interp.to_object(element) {
        Completion::Normal(value) => value,
        other => return Err(other),
    };
    interp.gc_root_value(&element_obj);
    let method = match interp.get_object_property(
        element_obj
            .as_object_id()
            .expect("ToObject must return an object"),
        "toLocaleString",
        element,
    ) {
        Completion::Normal(value) => value,
        other => return Err(other),
    };
    interp.gc_root_value(&method);
    if !interp.is_callable(&method) {
        return Err(Completion::Throw(
            interp.create_type_error("toLocaleString is not a function"),
        ));
    }
    let value = match interp.call_function(&method, element, args) {
        Completion::Normal(value) => value,
        other => return Err(other),
    };
    interp.gc_root_value(&value);
    interp
        .append_js_string(out, &value)
        .map_err(Completion::Throw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{Completion, Interpreter, JsFunction};
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

    fn text(completion: Completion) -> String {
        normal(completion).as_string().unwrap().to_rust_string()
    }

    #[test]
    fn joining_owns_preparation_range_reads_and_formatting() {
        let mut interp = Interpreter::new();
        evaluate(
            &mut interp,
            "Number.prototype.toLocaleString = function(l, o) { 'use strict'; if (typeof this !== 'number' || arguments.length !== 2 || l !== undefined || o !== undefined) throw 99; return 'n' + this; };",
        );
        for (method, source, expected) in [
            (JoinMethod::ArrayJoin, "[1,,null,undefined,3]", "1,,,,3"),
            (
                JoinMethod::ArrayLocaleString,
                "[1,,null,undefined,3]",
                "n1,,,,n3",
            ),
            (
                JoinMethod::TypedArrayJoin,
                "new BigInt64Array([1n,2n])",
                "1,2",
            ),
            (
                JoinMethod::TypedArrayLocaleString,
                "new Uint8Array([1,2])",
                "n1,n2",
            ),
        ] {
            let receiver = evaluate(&mut interp, source);
            let before = interp.gc_root_frame();
            assert_eq!(text(join(&mut interp, method, &receiver, &[])), expected);
            assert_eq!(interp.gc_root_frame(), before);
        }
        let receiver = evaluate(
            &mut interp,
            "({ get length() { return 3; }, get 0() { this[1] = 'changed'; return 'first'; }, 2: 'last' })",
        );
        let separator = evaluate(&mut interp, "({ toString() { return '|'; } })");
        assert_eq!(
            text(join(
                &mut interp,
                JoinMethod::ArrayJoin,
                &receiver,
                &[separator]
            )),
            "first|changed|last"
        );
    }

    #[test]
    fn joining_roots_inputs_before_length_and_each_collecting_conversion() {
        let mut interp = Interpreter::new();
        let separator = evaluate(
            &mut interp,
            "({ tag: '|', toString() { $262.gc(); return this.tag; } })",
        );
        let receiver = evaluate(
            &mut interp,
            "({ get length() { $262.gc(); return 2; }, get 0() { return { toString() { $262.gc(); return 'a'; } }; }, 1: 'b' })",
        );
        let before = interp.gc_root_frame();
        assert_eq!(
            text(join(
                &mut interp,
                JoinMethod::ArrayJoin,
                &receiver,
                &[separator]
            )),
            "a|b"
        );
        assert_eq!(interp.gc_root_frame(), before);

        let locales = evaluate(&mut interp, "({tag: 'left'})");
        let options = evaluate(&mut interp, "({tag: 'right'})");
        let receiver = evaluate(
            &mut interp,
            "({ get length() { $262.gc(); return 2; }, get 0() { return { tag: 'first', get toLocaleString() { $262.gc(); return function(l,o) { $262.gc(); return { tag: this.tag+'-'+l.tag+'-'+o.tag, toString() { $262.gc(); return this.tag; } }; }; } }; }, get 1() { $262.gc(); return { toLocaleString() { return 'b'; } }; } })",
        );
        assert_eq!(
            text(join(
                &mut interp,
                JoinMethod::ArrayLocaleString,
                &receiver,
                &[locales, options]
            )),
            "first-left-right,b"
        );
        assert_eq!(interp.gc_root_frame(), before);
    }

    #[test]
    fn joining_validates_before_separator_and_keeps_typed_reads_live() {
        let mut interp = Interpreter::new();
        for method in [
            JoinMethod::TypedArrayJoin,
            JoinMethod::TypedArrayLocaleString,
        ] {
            assert!(matches!(
                join(&mut interp, method, &JsValue::NULL, &[]),
                Completion::Throw(_)
            ));
        }
        for source in ["new Uint8Array(0)", "[]"] {
            let receiver = evaluate(&mut interp, source);
            let separator = evaluate(&mut interp, "({toString() { throw 42; }})");
            let method = if source == "[]" {
                JoinMethod::ArrayJoin
            } else {
                JoinMethod::TypedArrayJoin
            };
            assert!(
                matches!(join(&mut interp, method, &receiver, &[separator]), Completion::Throw(v) if v.as_number() == Some(42.0))
            );
        }
        let receiver = evaluate(&mut interp, "var view = new Uint8Array([1,2,3]); view");
        let separator = evaluate(
            &mut interp,
            "({ toString() { $262.detachArrayBuffer(view.buffer); return '|'; } })",
        );
        assert_eq!(
            text(join(
                &mut interp,
                JoinMethod::TypedArrayJoin,
                &receiver,
                &[separator]
            )),
            "||"
        );
        assert!(matches!(
            join(&mut interp, JoinMethod::TypedArrayJoin, &receiver, &[]),
            Completion::Throw(_)
        ));
    }

    #[test]
    fn joining_forwards_locale_abrupt_completions_and_restores_parent_roots() {
        for (method, source, holder) in [
            (
                JoinMethod::ArrayLocaleString,
                "var element = {}; [element]",
                "element",
            ),
            (
                JoinMethod::TypedArrayLocaleString,
                "new Uint8Array([1])",
                "Number.prototype",
            ),
        ] {
            let mut interp = Interpreter::new();
            let receiver = evaluate(&mut interp, source);
            let object = evaluate(&mut interp, holder);
            let abort =
                interp.create_function(JsFunction::native("abort".to_string(), 0, |_, _, args| {
                    assert_eq!(args.len(), 2);
                    Completion::Exit(29)
                }));
            interp
                .get_object_cell(object.as_object_id().unwrap())
                .unwrap()
                .borrow_mut()
                .insert_builtin("toLocaleString".to_string(), abort);
            interp.with_gc_root_scope(|interp| {
                interp.gc_root_value(&object);
                let before = interp.gc_root_frame();
                assert!(matches!(
                    join(interp, method, &receiver, &[]),
                    Completion::Exit(29)
                ));
                assert_eq!(interp.gc_root_frame(), before);
            });
        }
    }

    #[test]
    fn joining_owns_cycle_extent_and_releases_it_after_abrupt_reads() {
        let mut interp = Interpreter::new();
        let receiver = evaluate(
            &mut interp,
            "var cyclic = [1]; cyclic.push(cyclic,2); cyclic",
        );
        assert_eq!(
            text(join(&mut interp, JoinMethod::ArrayJoin, &receiver, &[])),
            "1,,2"
        );
        let separator = evaluate(&mut interp, "({ toString() { return cyclic.join('-'); } })");
        assert_eq!(
            text(join(
                &mut interp,
                JoinMethod::ArrayJoin,
                &receiver,
                &[separator]
            )),
            "11--21--22"
        );

        let receiver = evaluate(
            &mut interp,
            "var failing = { length: 1, get 0() { throw 73; } }; failing",
        );
        let before = interp.gc_root_frame();
        assert!(
            matches!(join(&mut interp, JoinMethod::ArrayJoin, &receiver, &[]), Completion::Throw(v) if v.as_number() == Some(73.0))
        );
        evaluate(
            &mut interp,
            "Object.defineProperty(failing,'0',{value:'recovered'})",
        );
        assert_eq!(
            text(join(&mut interp, JoinMethod::ArrayJoin, &receiver, &[])),
            "recovered"
        );
        assert_eq!(interp.gc_root_frame(), before);
    }
}
