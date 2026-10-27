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

fn property(interp: &mut Interpreter, object: &JsValue, key: &str) -> JsValue {
    normal(interp.get_object_property(object.as_object_id().unwrap(), key, object))
}

const METHODS: [(Transform, bool, bool); 4] = [
    (Transform::ArrayMap, false, false),
    (Transform::ArrayFilter, false, true),
    (Transform::TypedArrayMap, true, false),
    (Transform::TypedArrayFilter, true, true),
];

#[test]
fn transformation_owns_callback_operands_and_output_selection() {
    for (method, typed, filter) in METHODS {
        let mut interp = Interpreter::new();
        let source = evaluate(
            &mut interp,
            if typed {
                "new Uint8Array([1, 2, 3])"
            } else {
                "[1, 2, 3]"
            },
        );
        let source_id = source.as_object_id();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = seen.clone();
        let callback = interp.create_function(JsFunction::native(
            "transform".to_string(),
            3,
            move |_, this, args| {
                assert_eq!(this.as_number(), Some(42.0));
                assert_eq!(args.len(), 3);
                let index = args[1].as_number().unwrap() as usize;
                assert_eq!(args[0].as_number(), Some((index + 1) as f64));
                assert_eq!(args[2].as_object_id(), source_id);
                log.borrow_mut().push(index);
                Completion::Normal(JsValue::number(if filter {
                    if index == 1 { 0.0 } else { -7.0 }
                } else {
                    (index + 11) as f64
                }))
            },
        ));
        let depth = interp.gc_root_frame();
        let output = normal(transform(
            &mut interp,
            method,
            &source,
            &[callback, JsValue::number(42.0)],
        ));
        assert_eq!(*seen.borrow(), vec![0, 1, 2]);
        let expected = if filter {
            vec![1.0, 3.0]
        } else {
            vec![11.0, 12.0, 13.0]
        };
        assert_eq!(
            property(&mut interp, &output, "length").as_number(),
            Some(expected.len() as f64)
        );
        for (index, value) in expected.into_iter().enumerate() {
            assert_eq!(
                property(&mut interp, &output, &index.to_string()).as_number(),
                Some(value)
            );
        }
        assert_eq!(interp.gc_root_frame(), depth);
    }
}

#[test]
fn transformation_preserves_species_schedule_and_array_holes() {
    for (method, typed, filter) in METHODS {
        let mut interp = Interpreter::new();
        let source = evaluate(
            &mut interp,
            &format!(
                "var trace = []; var source = {};\n\
             source.constructor = {{ get [Symbol.species]() {{\n\
               trace.push('species'); return function(n) {{\n\
                 trace.push('size:' + n); return {};\n\
               }};\n\
             }} }}; source",
                if typed {
                    "new Uint8Array([1, 2, 3])"
                } else {
                    "[1, , 3]"
                },
                if typed {
                    "new Uint8Array(n)"
                } else {
                    "new Array(n)"
                },
            ),
        );
        let callback = evaluate(
            &mut interp,
            "(function(value, index) { trace.push('call:' + index); return value; })",
        );
        let output = normal(transform(&mut interp, method, &source, &[callback]));
        let trace = evaluate(&mut interp, "trace.join(',')");
        let expected = if typed && filter {
            "call:0,call:1,call:2,species,size:3"
        } else if typed {
            "species,size:3,call:0,call:1,call:2"
        } else if filter {
            "species,size:0,call:0,call:2"
        } else {
            "species,size:3,call:0,call:2"
        };
        assert_eq!(trace.as_string().unwrap().to_rust_string(), expected);
        assert_eq!(
            property(&mut interp, &output, "length").as_number(),
            Some(if !typed && filter { 2.0 } else { 3.0 })
        );
        if !typed && !filter {
            assert!(property(&mut interp, &output, "1").is_undefined());
            assert_eq!(property(&mut interp, &output, "2").as_number(), Some(3.0));
        }
    }
}

#[test]
fn transformation_validates_empty_receivers_and_propagates_abrupt_completions() {
    for (method, typed, _) in METHODS {
        let mut interp = Interpreter::new();
        let source = evaluate(&mut interp, if typed { "new Uint8Array(0)" } else { "[]" });
        let depth = interp.gc_root_frame();
        assert!(matches!(
            transform(&mut interp, method, &source, &[]),
            Completion::Throw(_)
        ));
        assert!(matches!(
            transform(&mut interp, method, &JsValue::NULL, &[]),
            Completion::Throw(_)
        ));
        let never =
            interp.create_function(JsFunction::native("never".to_string(), 0, |_, _, _| {
                panic!("empty input invoked callback")
            }));
        let output = normal(transform(&mut interp, method, &source, &[never]));
        assert_eq!(
            property(&mut interp, &output, "length").as_number(),
            Some(0.0)
        );
        assert_eq!(interp.gc_root_frame(), depth);

        let source = evaluate(
            &mut interp,
            if typed { "new Uint8Array([1])" } else { "[1]" },
        );
        let abort =
            interp.create_function(JsFunction::native("abort".to_string(), 0, |_, _, _| {
                Completion::Exit(23)
            }));
        assert!(matches!(
            transform(&mut interp, method, &source, &[abort]),
            Completion::Exit(23)
        ));
        let throw =
            interp.create_function(JsFunction::native("throw".to_string(), 0, |_, _, _| {
                Completion::Throw(JsValue::number(73.0))
            }));
        assert!(
            matches!(transform(&mut interp, method, &source, &[throw]), Completion::Throw(v) if v.as_number() == Some(73.0))
        );
        assert_eq!(interp.gc_root_frame(), depth);
    }
}

#[test]
fn transformation_roots_inputs_and_original_filter_values_across_user_code() {
    for method in [Transform::ArrayMap, Transform::ArrayFilter] {
        let mut interp = Interpreter::new();
        let source = evaluate(
            &mut interp,
            "({get length() { $262.gc(); return 1; }, get 0() { return {tag:73}; }})",
        );
        let this_arg = evaluate(&mut interp, "({tag:19})");
        let callback = evaluate(
            &mut interp,
            "(function(v) { $262.gc(); if (this.tag !== 19) throw 'lost thisArg'; return {tag:v.tag}; })",
        );
        let parent = interp.create_array(vec![]);
        let depth = interp.gc_root_frame();
        interp.gc_root_value(&parent);
        let nested = interp.gc_root_frame();
        let output = normal(transform(
            &mut interp,
            method,
            &source,
            &[callback, this_arg],
        ));
        assert_eq!(interp.gc_root_frame(), nested);
        let value = property(&mut interp, &output, "0");
        assert_eq!(property(&mut interp, &value, "tag").as_number(), Some(73.0));
        assert!(
            interp
                .get_object_cell(parent.as_object_id().unwrap())
                .is_some()
        );
        interp.gc_unroot_frame(depth);
    }
}
