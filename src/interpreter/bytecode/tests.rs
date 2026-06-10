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
    let v = interp
        .get_global_var_ref("__r")
        .unwrap_or(JsValue::Undefined);
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
        names: vec![],
        max_stack: 1,
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
        names: vec![],
        max_stack: 0,
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
        names: vec![],
        max_stack: 2,
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
fn end_to_end_sub_mul_div_mod_pow_via_bytecode() {
    let cases = [
        ("(function(){ return 5 - 3; })()", 2.0),
        ("(function(){ return 2 * 3; })()", 6.0),
        ("(function(){ return 10 / 4; })()", 2.5),
        ("(function(){ return 7 % 3; })()", 1.0),
        ("(function(){ return 2 ** 8; })()", 256.0),
    ];
    for (expr, expected) in cases {
        let source = format!("var __r = {expr};");
        let (v, count) = eval_with_mode(&source, true);
        assert!(count >= 1, "{expr}: bytecode path must run");
        match v {
            JsValue::Number(n) => assert_eq!(n, expected, "{expr}"),
            other => panic!("{expr}: expected Number({expected}), got {other:?}"),
        }
    }
}

#[test]
fn end_to_end_comparison_and_equality_ops_via_bytecode() {
    let cases = [
        ("(function(){ return 1 < 2; })()", true),
        ("(function(){ return 2 < 1; })()", false),
        ("(function(){ return 2 > 1; })()", true),
        ("(function(){ return 1 <= 1; })()", true),
        ("(function(){ return 1 >= 2; })()", false),
        ("(function(){ return 1 == 1; })()", true),
        ("(function(){ return 1 != 2; })()", true),
        ("(function(){ return 1 === 1; })()", true),
        ("(function(){ return 1 !== 2; })()", true),
        ("(function(){ return '1' == 1; })()", true),
        ("(function(){ return '1' === 1; })()", false),
    ];
    for (expr, expected) in cases {
        let source = format!("var __r = {expr};");
        let (v, count) = eval_with_mode(&source, true);
        assert!(count >= 1, "{expr}: bytecode path must run");
        match v {
            JsValue::Boolean(b) => assert_eq!(b, expected, "{expr}"),
            other => panic!("{expr}: expected Boolean({expected}), got {other:?}"),
        }
    }
}

#[test]
fn end_to_end_bitwise_ops_via_bytecode() {
    let cases = [
        ("(function(){ return 0xff & 0x0f; })()", 0x0f),
        ("(function(){ return 0xf0 | 0x0f; })()", 0xff),
        ("(function(){ return 0xff ^ 0x0f; })()", 0xf0),
        ("(function(){ return 1 << 4; })()", 16),
        ("(function(){ return 32 >> 2; })()", 8),
        ("(function(){ return 4294967295 >>> 28; })()", 15),
    ];
    for (expr, expected) in cases {
        let source = format!("var __r = {expr};");
        let (v, count) = eval_with_mode(&source, true);
        assert!(count >= 1, "{expr}: bytecode path must run");
        match v {
            JsValue::Number(n) => assert_eq!(n as i64, expected as i64, "{expr}"),
            other => panic!("{expr}: expected Number, got {other:?}"),
        }
    }
}

#[test]
fn end_to_end_unary_ops_via_bytecode() {
    let cases: &[(&str, JsValue)] = &[
        ("(function(){ return -5; })()", JsValue::Number(-5.0)),
        ("(function(){ return +'3'; })()", JsValue::Number(3.0)),
        ("(function(){ return !true; })()", JsValue::Boolean(false)),
        ("(function(){ return !0; })()", JsValue::Boolean(true)),
        ("(function(){ return ~5; })()", JsValue::Number(-6.0)),
        ("(function(){ return void 0; })()", JsValue::Undefined),
        (
            "(function(){ return void 'anything'; })()",
            JsValue::Undefined,
        ),
    ];
    for (expr, expected) in cases {
        let source = format!("var __r = {expr};");
        let (v, count) = eval_with_mode(&source, true);
        assert!(count >= 1, "{expr}: bytecode path must run");
        match (&v, expected) {
            (JsValue::Number(n), JsValue::Number(e)) => assert_eq!(n, e, "{expr}"),
            (JsValue::Boolean(b), JsValue::Boolean(e)) => assert_eq!(b, e, "{expr}"),
            (JsValue::Undefined, JsValue::Undefined) => {}
            _ => panic!("{expr}: expected {expected:?}, got {v:?}"),
        }
    }
}

#[test]
fn end_to_end_ternary_conditional_via_bytecode() {
    let cases: &[(&str, JsValue)] = &[
        (
            "(function(){ return true ? 1 : 2; })()",
            JsValue::Number(1.0),
        ),
        (
            "(function(){ return false ? 1 : 2; })()",
            JsValue::Number(2.0),
        ),
        ("(function(){ return 0 ? 1 : 2; })()", JsValue::Number(2.0)),
        (
            "(function(){ return 1 ? 'yes' : 'no'; })()",
            JsValue::Number(0.0), /* placeholder */
        ),
        (
            "(function(){ return null ? 1 : 2; })()",
            JsValue::Number(2.0),
        ),
    ];
    for (i, (expr, expected)) in cases.iter().enumerate() {
        let source = format!("var __r = {expr};");
        let (v, count) = eval_with_mode(&source, true);
        assert!(count >= 1, "{expr}: bytecode path must run");
        if i == 3 {
            // String case — check separately
            match v {
                JsValue::String(ref s) => assert_eq!(s.to_rust_string(), "yes", "{expr}"),
                _ => panic!("{expr}: expected String 'yes', got {v:?}"),
            }
            continue;
        }
        match (&v, expected) {
            (JsValue::Number(n), JsValue::Number(e)) => assert_eq!(n, e, "{expr}"),
            _ => panic!("{expr}: expected {expected:?}, got {v:?}"),
        }
    }
}

#[test]
fn end_to_end_logical_short_circuit_via_bytecode() {
    // && returns lhs if falsy, else rhs
    // || returns lhs if truthy, else rhs
    // ?? returns lhs if non-nullish, else rhs
    let cases: &[(&str, JsValue)] = &[
        ("(function(){ return true && 5; })()", JsValue::Number(5.0)),
        (
            "(function(){ return false && 5; })()",
            JsValue::Boolean(false),
        ),
        ("(function(){ return 0 && 5; })()", JsValue::Number(0.0)),
        ("(function(){ return 1 && 2; })()", JsValue::Number(2.0)),
        ("(function(){ return false || 5; })()", JsValue::Number(5.0)),
        ("(function(){ return 7 || 5; })()", JsValue::Number(7.0)),
        ("(function(){ return 0 || 5; })()", JsValue::Number(5.0)),
        ("(function(){ return null ?? 5; })()", JsValue::Number(5.0)),
        ("(function(){ return 0 ?? 5; })()", JsValue::Number(0.0)),
        (
            "(function(){ return 'x' ?? 5; })()",
            JsValue::Number(0.0), /* placeholder */
        ),
    ];
    for (i, (expr, expected)) in cases.iter().enumerate() {
        let source = format!("var __r = {expr};");
        let (v, count) = eval_with_mode(&source, true);
        assert!(count >= 1, "{expr}: bytecode path must run");
        if i == 9 {
            // 'x' ?? 5 → 'x'
            match v {
                JsValue::String(ref s) => assert_eq!(s.to_rust_string(), "x", "{expr}"),
                _ => panic!("{expr}: expected String 'x', got {v:?}"),
            }
            continue;
        }
        match (&v, expected) {
            (JsValue::Number(n), JsValue::Number(e)) => assert_eq!(n, e, "{expr}"),
            (JsValue::Boolean(b), JsValue::Boolean(e)) => assert_eq!(b, e, "{expr}"),
            _ => panic!("{expr}: expected {expected:?}, got {v:?}"),
        }
    }
}

#[test]
fn end_to_end_param_read_via_bytecode() {
    // Function that just returns its parameter
    let source = "var __r = (function(x){ return x; })(42);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must run");
    assert!(matches!(v, JsValue::Number(n) if n == 42.0), "got {v:?}");
}

#[test]
fn end_to_end_param_arithmetic_via_bytecode() {
    let source = "var __r = (function(x, y){ return x + y * 2; })(3, 5);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must run");
    assert!(matches!(v, JsValue::Number(n) if n == 13.0), "got {v:?}");
}

#[test]
fn end_to_end_param_compare_returns_boolean() {
    let source = "var __r = (function(n){ return n > 10; })(5);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must run");
    assert!(matches!(v, JsValue::Boolean(false)), "got {v:?}");
}

#[test]
fn end_to_end_undeclared_identifier_throws_reference_error() {
    // Should throw ReferenceError, not just falsely succeed via the bytecode path
    let source = "var __r = false; try { (function(){ return undeclaredX; })(); } catch (e) { __r = e instanceof ReferenceError; }";
    let (v, _count) = eval_with_mode(source, true);
    assert!(matches!(v, JsValue::Boolean(true)), "got {v:?}");
}

#[test]
fn end_to_end_param_mutation_via_bytecode() {
    let source = "var __r = (function(x){ x = x + 1; return x; })(5);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must run");
    assert!(matches!(v, JsValue::Number(n) if n == 6.0), "got {v:?}");
}

#[test]
fn end_to_end_multiple_statements_via_bytecode() {
    let source = "var __r = (function(x){ x = x + 1; x = x * 2; return x; })(3);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must run");
    assert!(matches!(v, JsValue::Number(n) if n == 8.0), "got {v:?}");
}

#[test]
fn end_to_end_assignment_returns_assigned_value() {
    // `(x = expr)` evaluates to the assigned value
    let source = "var __r = (function(x){ return (x = 99); })(0);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must run");
    assert!(matches!(v, JsValue::Number(n) if n == 99.0), "got {v:?}");
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
        names: vec![],
        max_stack: 2,
    };
    match run(chunk) {
        Completion::Return(JsValue::String(s)) => assert_eq!(s.to_string(), "x1"),
        other => panic!("expected Return(String(\"x1\")), got {other:?}"),
    }
}

// ----- if / else statement lowering -----

/// Asserts the bytecode path produces the same `__r` value as the
/// tree-walker AND that the bytecode path actually executed a chunk.
fn assert_parity_number(source: &str, expected: f64) {
    let (ast_v, ast_count) = eval_with_mode(source, false);
    let (bc_v, bc_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0, "{source}: AST mode must not run chunks");
    assert!(bc_count >= 1, "{source}: bytecode path must run a chunk");
    match (&ast_v, &bc_v) {
        (JsValue::Number(a), JsValue::Number(b)) => {
            assert_eq!(*a, expected, "{source}: AST value");
            assert_eq!(*b, expected, "{source}: bytecode value");
        }
        _ => panic!("{source}: expected Number({expected}), got ast={ast_v:?} bc={bc_v:?}"),
    }
}

// NOTE: `var`/`let` declarations are not yet compilable, so a body that
// contains one bails the WHOLE body to the tree-walker. To exercise the
// bytecode path these tests use a parameter for input plus `return` in the
// branches — both of which the compiler supports.

#[test]
fn if_true_takes_consequent_branch() {
    // (a) if(true) taken branch
    let source = "var __r = (function(n){ if (true) return 10; return 0; })(0);";
    assert_parity_number(source, 10.0);
}

#[test]
fn if_false_takes_else_branch() {
    // (b) if(false) with else
    let source = "var __r = (function(n){ if (false) return 10; else return 20; })(0);";
    assert_parity_number(source, 20.0);
}

#[test]
fn if_false_no_else_skips() {
    // (c) if with no else (falsy → skip body, fall through to the tail return)
    let source = "var __r = (function(n){ if (false) return 99; return n; })(5);";
    assert_parity_number(source, 5.0);
}

#[test]
fn nested_if_else() {
    // (d) nested if/else (branches are blocks containing nested if/else)
    let nested = "(function(n){ \
        if (n > 10) { if (n > 20) { return 1; } else { return 2; } } \
        else { return 3; } })";
    assert_parity_number(&format!("var __r = {nested}(15);"), 2.0);
    assert_parity_number(&format!("var __r = {nested}(25);"), 1.0);
    assert_parity_number(&format!("var __r = {nested}(5);"), 3.0);
}

#[test]
fn if_branch_contains_return() {
    // (e) if whose branch contains a return.
    // `return` inside a block IS supported (Return + Block arms), so this
    // must take the bytecode path and match the tree-walker.
    let source = "var __r = (function(n){ if (n > 0) { return 1; } return -1; })(5);";
    assert_parity_number(source, 1.0);

    let source_neg = "var __r = (function(n){ if (n > 0) { return 1; } return -1; })(-5);";
    assert_parity_number(source_neg, -1.0);

    // return in the else branch as well
    let source_else = "var __r = (function(n){ if (n > 0) { return 1; } else { return 2; } })(-5);";
    assert_parity_number(source_else, 2.0);
}

#[test]
fn if_truthiness_coercion_matches_tree_walker() {
    // (f) truthiness coercion via JumpIfFalse's to_boolean: 0, "", NaN → falsy.
    // if(0) → falsy → else
    assert_parity_number(
        "var __r = (function(){ if (0) return 1; else return 2; })();",
        2.0,
    );
    // if("") → falsy → else
    assert_parity_number(
        "var __r = (function(){ if ('') return 1; else return 2; })();",
        2.0,
    );
    // if(NaN) → falsy → else (NaN produced via 0/0 — a compilable expression)
    assert_parity_number(
        "var __r = (function(){ if (0/0) return 1; else return 2; })();",
        2.0,
    );
    // if({}) → truthy → consequent. An object literal is NOT a compilable
    // expression, so the body bails to the tree-walker. Assert the value is
    // still correct in both modes (the parity helper requires bytecode to
    // run, which it won't here).
    let src = "var __r = (function(){ if ({}) return 1; else return 2; })();";
    let (ast_v, _) = eval_with_mode(src, false);
    let (bc_v, bc_count) = eval_with_mode(src, true);
    assert_eq!(bc_count, 0, "object-literal test must bail to AST");
    assert!(
        matches!(ast_v, JsValue::Number(n) if n == 1.0),
        "ast {ast_v:?}"
    );
    assert!(
        matches!(bc_v, JsValue::Number(n) if n == 1.0),
        "bc {bc_v:?}"
    );
}

#[test]
fn compile_body_if_lowers_via_vm_directly() {
    use crate::ast::{BinaryOp, IfStatement};
    // if (1 < 2) return 10; else return 20;
    let body = vec![Statement::If(IfStatement {
        test: Expression::Binary(
            BinaryOp::Lt,
            Box::new(Expression::Literal(Literal::Number(1.0))),
            Box::new(Expression::Literal(Literal::Number(2.0))),
        ),
        consequent: Box::new(Statement::Return(Some(Expression::Literal(
            Literal::Number(10.0),
        )))),
        alternate: Some(Box::new(Statement::Return(Some(Expression::Literal(
            Literal::Number(20.0),
        ))))),
    })];
    let chunk = compile_body(&body).expect("compile if/else");
    match run(chunk) {
        Completion::Return(JsValue::Number(n)) => assert_eq!(n, 10.0),
        other => panic!("expected Return(Number(10.0)), got {other:?}"),
    }
}

#[test]
fn if_with_unsupported_branch_bails_to_unsupported() {
    use super::compiler::CompileError;
    use crate::ast::IfStatement;
    // The consequent is a `Throw`, which the compiler does not support, so
    // compile_body must return Err(Unsupported) rather than mis-compiling.
    let body = vec![Statement::If(IfStatement {
        test: Expression::Literal(Literal::Boolean(true)),
        consequent: Box::new(Statement::Throw(Expression::Literal(Literal::Number(1.0)))),
        alternate: None,
    })];
    match compile_body(&body) {
        Err(CompileError::Unsupported(_)) => {}
        other => panic!("expected Err(Unsupported), got {other:?}"),
    }
}

#[test]
fn if_with_unsupported_alternate_bails_to_unsupported() {
    use super::compiler::CompileError;
    use crate::ast::IfStatement;
    let body = vec![Statement::If(IfStatement {
        test: Expression::Literal(Literal::Boolean(false)),
        consequent: Box::new(Statement::Return(Some(Expression::Literal(
            Literal::Number(1.0),
        )))),
        alternate: Some(Box::new(Statement::Throw(Expression::Literal(
            Literal::Number(2.0),
        )))),
    })];
    match compile_body(&body) {
        Err(CompileError::Unsupported(_)) => {}
        other => panic!("expected Err(Unsupported), got {other:?}"),
    }
}

#[test]
fn load_undefined_then_return_completes_with_undefined() {
    let chunk = Chunk {
        code: vec![Op::LoadUndefined as u8, Op::Return as u8],
        constants: vec![],
        names: vec![],
        max_stack: 1,
    };
    match run(chunk) {
        Completion::Return(JsValue::Undefined) => {}
        other => panic!("expected Return(Undefined), got {other:?}"),
    }
}
