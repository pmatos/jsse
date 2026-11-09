use super::HelperStep;
use crate::interpreter::{Interpreter, types::Completion};
use crate::types::JsValue;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn call(interp: &mut Interpreter, helper: &JsValue, method: &str) -> Completion {
    let id = helper.as_object_id().unwrap();
    let Completion::Normal(function) = interp.get_object_property(id, method, helper) else {
        panic!("missing helper method");
    };
    interp.call_function(&function, helper, &[JsValue::number(99.0)])
}

fn result(interp: &mut Interpreter, completion: Completion, done: bool) -> JsValue {
    let Completion::Normal(value) = completion else {
        panic!("expected iterator result: {completion:?}");
    };
    let id = value.as_object_id().unwrap();
    let Completion::Normal(actual) = interp.get_object_property(id, "done", &value) else {
        panic!("missing done");
    };
    assert_eq!(actual.as_boolean(), Some(done));
    let Completion::Normal(value) = interp.get_object_property(id, "value", &value) else {
        panic!("missing value");
    };
    value
}

#[test]
fn yields_then_completes_without_reentering_the_adapter() {
    let mut interp = Interpreter::new();
    let steps = Rc::new(Cell::new(0));
    let calls = steps.clone();
    let helper = interp.create_iterator_helper(
        vec![],
        move |_| {
            calls.set(calls.get() + 1);
            Ok(if calls.get() == 1 {
                HelperStep::Yield(JsValue::number(7.0))
            } else {
                HelperStep::Done
            })
        },
        |_| panic!("exhausted source must not be closed"),
    );
    let first = call(&mut interp, &helper, "next");
    assert_eq!(result(&mut interp, first, false).as_number(), Some(7.0));
    for method in ["next", "next", "return"] {
        let completion = call(&mut interp, &helper, method);
        assert!(result(&mut interp, completion, true).is_undefined());
    }
    assert_eq!(steps.get(), 2);
}

#[test]
fn final_yield_skips_both_step_and_close_afterward() {
    let mut interp = Interpreter::new();
    let helper = interp.create_iterator_helper(
        vec![],
        |_| Ok(HelperStep::FinalYield(JsValue::number(3.0))),
        |_| panic!("source already exhausted"),
    );
    let first = call(&mut interp, &helper, "next");
    assert_eq!(result(&mut interp, first, false).as_number(), Some(3.0));
    for method in ["return", "next"] {
        let completion = call(&mut interp, &helper, method);
        result(&mut interp, completion, true);
    }
}

#[test]
fn abrupt_step_is_forwarded_and_latches_completion() {
    let mut interp = Interpreter::new();
    for exit in [false, true] {
        let helper = interp.create_iterator_helper(
            vec![],
            move |_| {
                Err(if exit {
                    Completion::Exit(23)
                } else {
                    Completion::Throw(JsValue::number(17.0))
                })
            },
            |_| panic!("abrupt step already owns close policy"),
        );
        let completion = call(&mut interp, &helper, "next");
        match (completion, exit) {
            (Completion::Throw(value), false) => {
                assert_eq!(value.as_number(), Some(17.0));
            }
            (Completion::Exit(23), true) => {}
            other => panic!("changed completion: {other:?}"),
        }
        let completion = call(&mut interp, &helper, "return");
        result(&mut interp, completion, true);
    }
}

#[test]
fn return_closes_once_even_when_close_throws() {
    let mut interp = Interpreter::new();
    for started in [false, true] {
        let closes = Rc::new(Cell::new(0));
        let count = closes.clone();
        let helper = interp.create_iterator_helper(
            vec![],
            |_| Ok(HelperStep::Yield(JsValue::UNDEFINED)),
            move |_| {
                count.set(count.get() + 1);
                Err(JsValue::number(11.0))
            },
        );
        if started {
            let completion = call(&mut interp, &helper, "next");
            result(&mut interp, completion, false);
        }
        assert!(
            matches!(call(&mut interp, &helper, "return"), Completion::Throw(v) if v.as_number() == Some(11.0))
        );
        for method in ["return", "next"] {
            let completion = call(&mut interp, &helper, method);
            result(&mut interp, completion, true);
        }
        assert_eq!(closes.get(), 1);
    }
}

#[test]
fn next_rejects_recursive_next_and_return_without_finishing_the_outer_step() {
    let mut interp = Interpreter::new();
    let slot = Rc::new(RefCell::new(JsValue::UNDEFINED));
    let recursive = slot.clone();
    let helper = interp.create_iterator_helper(
        vec![],
        move |interp| {
            for method in ["next", "return"] {
                assert!(matches!(
                    call(interp, &recursive.borrow().clone(), method),
                    Completion::Throw(_)
                ));
            }
            Ok(HelperStep::Yield(JsValue::number(1.0)))
        },
        |_| Ok(()),
    );
    *slot.borrow_mut() = helper.clone();
    let completion = call(&mut interp, &helper, "next");
    result(&mut interp, completion, false);
    let completion = call(&mut interp, &helper, "next");
    result(&mut interp, completion, false);
}

#[test]
fn close_reentrancy_distinguishes_start_from_suspended_yield() {
    let mut interp = Interpreter::new();
    for started in [false, true] {
        let slot = Rc::new(RefCell::new(JsValue::UNDEFINED));
        let recursive = slot.clone();
        let helper = interp.create_iterator_helper(
            vec![],
            |_| Ok(HelperStep::Yield(JsValue::UNDEFINED)),
            move |interp| {
                for method in ["next", "return"] {
                    let completion = call(interp, &recursive.borrow().clone(), method);
                    if started {
                        assert!(matches!(completion, Completion::Throw(_)));
                    } else {
                        result(interp, completion, true);
                    }
                }
                Ok(())
            },
        );
        *slot.borrow_mut() = helper.clone();
        if started {
            let completion = call(&mut interp, &helper, "next");
            result(&mut interp, completion, false);
        }
        let completion = call(&mut interp, &helper, "return");
        result(&mut interp, completion, true);
    }
}

#[test]
fn pinned_captures_survive_collection_during_a_step() {
    let mut interp = Interpreter::new();
    let capture = interp.create_array(vec![JsValue::number(42.0)]);
    let id = capture.as_object_id().unwrap();
    let helper = interp.create_iterator_helper(
        vec![capture.clone()],
        move |interp| {
            interp.gc.request();
            interp.gc_safepoint();
            assert!(interp.get_object(id).is_some());
            Ok(HelperStep::Yield(capture.clone()))
        },
        |_| Ok(()),
    );
    let depth = interp.gc_root_frame();
    let completion = call(&mut interp, &helper, "next");
    assert_eq!(
        result(&mut interp, completion, false).as_object_id(),
        Some(id)
    );
    assert_eq!(interp.gc_root_frame(), depth);
}
