use super::chunk::{Chunk, Constant};
use super::compiler::compile_body;
use super::op::Op;
use super::vm::run_chunk;
use crate::ast::{Expression, Literal, Statement};
use crate::interpreter::Interpreter;
use crate::interpreter::types::Completion;
use crate::types::JsValue;

fn run(chunk: Chunk) -> Completion {
    let mut interp = Interpreter::new();
    let env = interp.realm().global_env.clone();
    run_chunk(&mut interp, &chunk, &env, JsValue::Undefined)
}

#[test]
fn bytecode_enabled_defaults_to_false() {
    let interp = Interpreter::new();
    assert!(!interp.bytecode_enabled);
}

#[test]
fn bytecode_enabled_can_be_toggled() {
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = true;
    assert!(interp.bytecode_enabled);
}

fn eval_with_mode(source: &str, bytecode: bool) -> (JsValue, usize) {
    use crate::parser::Parser;
    let mut p = Parser::new(source).expect("parser init");
    let program = p.parse_program().expect("parse");
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = bytecode;
    let _ = interp.run(&program);
    let env = interp.realm().global_env.clone();
    let v = env.borrow().get("__r").unwrap_or(JsValue::Undefined);
    (v, interp.bytecode_chunks_executed)
}

#[test]
fn end_to_end_literal_return_takes_bytecode_path() {
    let source = "var __r = (function(){ return 42; })();";
    let (ast_v, ast_count) = eval_with_mode(source, false);
    let (bc_v, bc_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0, "AST mode should not execute any chunks");
    assert!(
        bc_count >= 1,
        "bytecode mode must execute at least one chunk"
    );
    assert!(matches!(ast_v, JsValue::Number(n) if n == 42.0));
    assert!(matches!(bc_v, JsValue::Number(n) if n == 42.0));
}

#[test]
fn end_to_end_addition_return_takes_bytecode_path() {
    let source = "var __r = (function(){ return 1 + 2; })();";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode mode must execute at least one chunk");
    assert!(matches!(v, JsValue::Number(n) if n == 3.0));
}

#[test]
fn end_to_end_ineligible_function_falls_back_to_ast() {
    // var declarations + identifier reads aren't compilable yet → must bail to AST
    let source = "var __r = (function(){ var x = 7; return x; })();";
    let (v, count) = eval_with_mode(source, true);
    assert_eq!(count, 0, "ineligible function must NOT use bytecode path");
    assert!(matches!(v, JsValue::Number(n) if n == 7.0));
}

#[test]
fn end_to_end_bytecode_off_unchanged() {
    let source = "var __r = (function(){ return 42; })();";
    let (v, count) = eval_with_mode(source, false);
    assert_eq!(count, 0);
    assert!(matches!(v, JsValue::Number(n) if n == 42.0));
}

#[test]
fn end_to_end_constructor_with_empty_body_returns_this() {
    // For `new f()` with an empty body, the spec returns the freshly
    // allocated `this` object — not undefined. Guards against future
    // regressions where the bytecode path could break construct semantics
    // for fall-through bodies.
    let source = "function f(){} var __r = new f();";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must execute the empty body");
    assert!(
        matches!(v, JsValue::Object(_)),
        "expected new f() to return the instance, got {v:?}"
    );
}

#[test]
fn load_const_and_return_yields_number_completion() {
    let chunk = Chunk {
        code: vec![Op::LoadConst as u8, 0, 0, Op::Return as u8],
        constants: vec![Constant::Number(42.0)],
        max_stack: 1,
        num_params: 0,
    };
    match run(chunk) {
        Completion::Return(JsValue::Number(n)) => assert_eq!(n, 42.0),
        other => panic!("expected Return(Number(42.0)), got {other:?}"),
    }
}

#[test]
fn return_undefined_completes_with_undefined() {
    let chunk = Chunk {
        code: vec![Op::ReturnUndefined as u8],
        constants: vec![],
        max_stack: 0,
        num_params: 0,
    };
    match run(chunk) {
        Completion::Return(JsValue::Undefined) => {}
        other => panic!("expected Return(Undefined), got {other:?}"),
    }
}

#[test]
fn add_two_numbers_via_eval_binary() {
    // Bytecode for `return 2 + 3;`
    let chunk = Chunk {
        code: vec![
            Op::LoadConst as u8,
            0,
            0,
            Op::LoadConst as u8,
            1,
            0,
            Op::Add as u8,
            Op::Return as u8,
        ],
        constants: vec![Constant::Number(2.0), Constant::Number(3.0)],
        max_stack: 2,
        num_params: 0,
    };
    match run(chunk) {
        Completion::Return(JsValue::Number(n)) => assert_eq!(n, 5.0),
        other => panic!("expected Return(Number(5.0)), got {other:?}"),
    }
}

#[test]
fn compile_body_return_number_literal() {
    let body = vec![Statement::Return(Some(Expression::Literal(
        Literal::Number(42.0),
    )))];
    let chunk = compile_body(&body).expect("compile");
    match run(chunk) {
        Completion::Return(JsValue::Number(n)) => assert_eq!(n, 42.0),
        other => panic!("expected Return(Number(42.0)), got {other:?}"),
    }
}

#[test]
fn compile_body_empty_returns_undefined() {
    let chunk = compile_body(&[]).expect("compile");
    match run(chunk) {
        Completion::Return(JsValue::Undefined) => {}
        other => panic!("expected Return(Undefined), got {other:?}"),
    }
}

#[test]
fn compile_body_bare_return_yields_undefined() {
    let body = vec![Statement::Return(None)];
    let chunk = compile_body(&body).expect("compile");
    match run(chunk) {
        Completion::Return(JsValue::Undefined) => {}
        other => panic!("expected Return(Undefined), got {other:?}"),
    }
}

#[test]
fn compile_body_return_addition_of_literals() {
    // return 2 + 3;
    let body = vec![Statement::Return(Some(Expression::Binary(
        crate::ast::BinaryOp::Add,
        Box::new(Expression::Literal(Literal::Number(2.0))),
        Box::new(Expression::Literal(Literal::Number(3.0))),
    )))];
    let chunk = compile_body(&body).expect("compile");
    match run(chunk) {
        Completion::Return(JsValue::Number(n)) => assert_eq!(n, 5.0),
        other => panic!("expected Return(Number(5.0)), got {other:?}"),
    }
}

#[test]
fn add_string_and_number_falls_through_to_string_concat() {
    // Bytecode for `return "x" + 1;`  → "x1"
    let chunk = Chunk {
        code: vec![
            Op::LoadConst as u8,
            0,
            0,
            Op::LoadConst as u8,
            1,
            0,
            Op::Add as u8,
            Op::Return as u8,
        ],
        constants: vec![Constant::String("x".into()), Constant::Number(1.0)],
        max_stack: 2,
        num_params: 0,
    };
    match run(chunk) {
        Completion::Return(JsValue::String(s)) => assert_eq!(s.to_string(), "x1"),
        other => panic!("expected Return(String(\"x1\")), got {other:?}"),
    }
}

#[test]
fn load_undefined_then_return_completes_with_undefined() {
    let chunk = Chunk {
        code: vec![Op::LoadUndefined as u8, Op::Return as u8],
        constants: vec![],
        max_stack: 1,
        num_params: 0,
    };
    match run(chunk) {
        Completion::Return(JsValue::Undefined) => {}
        other => panic!("expected Return(Undefined), got {other:?}"),
    }
}
