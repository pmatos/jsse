use super::LoopHeadContext;
use crate::ast::{ForInOfLeft, Pattern, Statement};
use crate::interpreter::Interpreter;
use crate::interpreter::atom::Atom;
use crate::interpreter::bytecode::chunk::HeadBinding;
use crate::interpreter::types::{BindingKind, Completion, Environment};
use crate::parser::Parser;
use crate::types::JsValue;

fn head(source: &str) -> ForInOfLeft {
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    match &program.body.as_slice()[0] {
        Statement::ForIn(loop_) => loop_.left.clone(),
        Statement::ForOf(loop_) => loop_.left.clone(),
        other => panic!("expected loop, got {other:?}"),
    }
}

fn evaluate(interp: &mut Interpreter, source: &str) -> JsValue {
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    match interp.run(&program) {
        Completion::Normal(value) => value,
        other => panic!("expected value from {source:?}, got {other:?}"),
    }
}

fn assert_success(completion: Completion) {
    assert!(
        matches!(completion, Completion::Normal(_) | Completion::Empty),
        "unexpected completion: {completion:?}"
    );
}

fn declaration_pattern(left: ForInOfLeft) -> Pattern {
    match left {
        ForInOfLeft::Variable(decl) => decl.declarations[0].pattern.clone(),
        other => panic!("expected declaration, got {other:?}"),
    }
}

// Defaults and expression targets execute IC Sites in their containing Body,
// just as every production loop driver does.
fn lowered_head(
    interp: &mut Interpreter,
    source: &str,
    value: JsValue,
    env: &crate::interpreter::types::EnvRef,
) -> Completion {
    let program = Parser::new(source).unwrap().parse_program().unwrap();
    let Statement::ForOf(loop_) = &program.body.as_slice()[0] else {
        panic!("expected for-of");
    };
    interp.with_ic_body(&program.body, |interp| {
        interp.bind_loop_head(
            &loop_.left,
            value,
            LoopHeadContext::Lowered {
                declaration: env,
                assignment: env,
            },
        )
    })
}

#[test]
fn ordinary_heads_bind_var_outside_and_lexicals_in_the_iteration() {
    let mut interp = Interpreter::new();
    for is_of in [false, true] {
        for keyword in ["var", "let", "const"] {
            let outer = Environment::new(None);
            let iteration = Environment::new(Some(outer.clone()));
            let source = if is_of {
                format!("for ({keyword} value of []) {{}}")
            } else {
                format!("for ({keyword} value in {{}}) {{}}")
            };
            let left = head(&source);
            let context = if is_of {
                LoopHeadContext::ForOf {
                    outer: &outer,
                    iteration: &iteration,
                    simple: None,
                }
            } else {
                LoopHeadContext::ForIn {
                    outer: &outer,
                    iteration: &iteration,
                    simple: None,
                }
            };
            let depth = interp.gc_root_frame();
            assert_success(interp.bind_loop_head(&left, JsValue::number(17.0), context));
            assert_eq!(interp.gc_root_frame(), depth);
            if keyword == "var" {
                assert_eq!(outer.borrow().get("value").unwrap().as_number(), Some(17.0));
                assert!(iteration.borrow().bindings.get("value").is_none());
            } else {
                assert!(outer.borrow().get("value").is_none());
                assert_eq!(
                    iteration.borrow().get("value").unwrap().as_number(),
                    Some(17.0)
                );
                assert_eq!(
                    iteration.borrow().bindings.get("value").unwrap().kind,
                    if keyword == "let" {
                        BindingKind::Let
                    } else {
                        BindingKind::Const
                    }
                );
            }
        }
    }
}

#[test]
fn shared_atom_head_creates_the_same_lexical_binding() {
    let mut interp = Interpreter::new();
    let outer = Environment::new(None);
    let iteration = Environment::new(Some(outer.clone()));
    let left = head("for (const value of []) {}");
    let simple = (Atom::intern("value"), BindingKind::Const);
    let depth = interp.gc_root_frame();
    assert_success(interp.bind_loop_head(
        &left,
        JsValue::number(31.0),
        LoopHeadContext::ForOf {
            outer: &outer,
            iteration: &iteration,
            simple: Some(&simple),
        },
    ));
    assert!(outer.borrow().get("value").is_none());
    assert_eq!(
        iteration.borrow().get("value").unwrap().as_number(),
        Some(31.0)
    );
    assert_eq!(interp.gc_root_frame(), depth);
    let assign = head("for (value of []) {}");
    assert!(matches!(
        interp.bind_loop_head(
            &assign,
            JsValue::number(32.0),
            LoopHeadContext::Lowered {
                declaration: &iteration,
                assignment: &iteration
            }
        ),
        Completion::Throw(_)
    ));
    assert_eq!(
        iteration.borrow().get("value").unwrap().as_number(),
        Some(31.0)
    );
    assert_eq!(interp.gc_root_frame(), depth);
}

#[test]
fn lowered_heads_preserve_separate_declaration_and_assignment_environments() {
    let mut interp = Interpreter::new();
    let declaration = Environment::new(None);
    let assignment = Environment::new(None);
    assignment.borrow_mut().declare("target", BindingKind::Let);
    assignment
        .borrow_mut()
        .initialize_binding("target", JsValue::number(1.0));
    let depth = interp.gc_root_frame();
    assert_success(interp.bind_loop_head(
        &head("for (let declared of []) {}"),
        JsValue::number(7.0),
        LoopHeadContext::Lowered {
            declaration: &declaration,
            assignment: &assignment,
        },
    ));
    assert_eq!(
        declaration.borrow().get("declared").unwrap().as_number(),
        Some(7.0)
    );
    assert!(assignment.borrow().get("declared").is_none());
    assert_success(interp.bind_loop_head(
        &head("for (target of []) {}"),
        JsValue::number(8.0),
        LoopHeadContext::Lowered {
            declaration: &declaration,
            assignment: &assignment,
        },
    ));
    assert_eq!(
        assignment.borrow().get("target").unwrap().as_number(),
        Some(8.0)
    );
    assert!(declaration.borrow().get("target").is_none());
    assert_eq!(interp.gc_root_frame(), depth);
}

#[test]
fn assignment_heads_propagate_const_and_tdz_errors_without_writing() {
    let mut interp = Interpreter::new();
    for kind in [BindingKind::Let, BindingKind::Const] {
        let env = Environment::new(None);
        env.borrow_mut().declare("target", kind);
        if kind == BindingKind::Const {
            env.borrow_mut()
                .initialize_binding("target", JsValue::number(3.0));
        }
        let depth = interp.gc_root_frame();
        assert!(matches!(
            interp.bind_loop_head(
                &head("for (target of []) {}"),
                JsValue::number(9.0),
                LoopHeadContext::Lowered {
                    declaration: &env,
                    assignment: &env
                }
            ),
            Completion::Throw(_)
        ));
        if kind == BindingKind::Let {
            assert!(env.borrow().is_in_tdz("target"));
        } else {
            assert_eq!(env.borrow().get("target").unwrap().as_number(), Some(3.0));
        }
        assert_eq!(interp.gc_root_frame(), depth);
    }
}

#[test]
fn invalid_expression_target_runs_its_effects_before_throwing() {
    let mut interp = Interpreter::new();
    evaluate(
        &mut interp,
        "var calls = 0; function mark() { calls++; return 0; } 0;",
    );
    let env = interp.realm().global_env.clone();
    let depth = interp.gc_root_frame();
    assert!(matches!(
        lowered_head(
            &mut interp,
            "for (mark() of []) {}",
            JsValue::number(1.0),
            &env,
        ),
        Completion::Throw(_)
    ));
    assert_eq!(interp.gc_root_frame(), depth);
    assert_eq!(evaluate(&mut interp, "calls;").as_number(), Some(1.0));
}

#[test]
fn using_head_registers_before_binding_and_leaves_disposal_to_the_caller() {
    let mut interp = Interpreter::new();
    let resource = evaluate(
        &mut interp,
        r#"
        var events = [];
        ({ tag: 23, get [Symbol.dispose]() {
            events.push("get");
            $262.gc();
            return function () { events.push("dispose:" + this.tag); };
        } });
    "#,
    );
    let outer = interp.realm().global_env.clone();
    let iteration = Environment::new(Some(outer.clone()));
    interp.call_stack_envs.push(iteration.clone());
    let depth = interp.gc_root_frame();
    assert_success(interp.bind_loop_head(
        &head("for (using held of []) {}"),
        resource.clone(),
        LoopHeadContext::ForOf {
            outer: &outer,
            iteration: &iteration,
            simple: None,
        },
    ));
    assert_eq!(interp.gc_root_frame(), depth);
    assert_eq!(
        iteration.borrow().get("held").unwrap().as_object_id(),
        resource.as_object_id()
    );
    assert_eq!(
        evaluate(&mut interp, "events.join(',');")
            .as_string()
            .unwrap()
            .to_rust_string(),
        "get"
    );
    assert_success(interp.dispose_resources(&iteration, Completion::Normal(JsValue::UNDEFINED)));
    assert_eq!(
        evaluate(&mut interp, "events.join(',');")
            .as_string()
            .unwrap()
            .to_rust_string(),
        "get,dispose:23"
    );
    assert_eq!(interp.gc_root_frame(), depth);
    interp.call_stack_envs.pop();

    let resource = evaluate(&mut interp, "({ get [Symbol.dispose]() { throw 73; } });");
    let iteration = Environment::new(Some(outer.clone()));
    assert!(matches!(
        interp.bind_loop_head(
            &head("for (using rejected of []) {}"),
            resource,
            LoopHeadContext::ForOf { outer: &outer, iteration: &iteration, simple: None }
        ),
        Completion::Throw(value) if value.as_number() == Some(73.0)
    ));
    assert!(iteration.borrow().get("rejected").is_none());
    assert_eq!(interp.gc_root_frame(), depth);
}

#[test]
fn destructuring_errors_are_returned_without_routing_or_leaking_roots() {
    let mut interp = Interpreter::new();
    let env = interp.realm().global_env.clone();
    let value = evaluate(&mut interp, "({});");
    let depth = interp.gc_root_frame();
    assert!(matches!(
        lowered_head(
            &mut interp,
            "for (let {x = (() => { throw 42; })()} of []) {}",
            value,
            &env,
        ),
        Completion::Throw(value) if value.as_number() == Some(42.0)
    ));
    assert_eq!(interp.gc_root_frame(), depth);
}

#[test]
fn vm_pattern_adapter_preserves_declaration_assignment_and_catch_semantics() {
    let mut interp = Interpreter::new();
    let outer = Environment::new(None);
    outer.borrow_mut().declare("caught", BindingKind::Const);
    outer
        .borrow_mut()
        .initialize_binding("caught", JsValue::number(90.0));
    let env = Environment::new(Some(outer.clone()));
    env.borrow_mut().declare("assigned", BindingKind::Let);
    env.borrow_mut()
        .initialize_binding("assigned", JsValue::number(0.0));
    let depth = interp.gc_root_frame();
    let value = evaluate(&mut interp, "[4, 5];");
    let declare = HeadBinding::Declare {
        pattern: declaration_pattern(head("for (let [first, second] of []) {}")),
        kind: BindingKind::Let,
    };
    assert_success(interp.bind_head_binding(&declare, value, &env));
    assert_eq!(env.borrow().get("first").unwrap().as_number(), Some(4.0));
    assert_eq!(env.borrow().get("second").unwrap().as_number(), Some(5.0));
    let assign = HeadBinding::Assign {
        pattern: Pattern::Identifier("assigned".into()),
    };
    assert_success(interp.bind_head_binding(&assign, JsValue::number(6.0), &env));
    assert_eq!(env.borrow().get("assigned").unwrap().as_number(), Some(6.0));
    let catch = HeadBinding::Catch {
        pattern: Pattern::Identifier("caught".into()),
    };
    assert_success(interp.bind_head_binding(&catch, JsValue::number(7.0), &env));
    assert_eq!(env.borrow().get("caught").unwrap().as_number(), Some(7.0));
    assert_eq!(
        outer.borrow().get("caught").unwrap().as_number(),
        Some(90.0)
    );
    assert_eq!(interp.gc_root_frame(), depth);
}
