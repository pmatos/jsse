use super::chunk::{Chunk, Constant};
use super::compiler::{compile_body, compile_script_body};
use super::op::Op;
use super::vm::run_chunk;
use crate::ast::{Expression, Literal, Statement};
use crate::interpreter::Interpreter;
use crate::interpreter::types::Completion;
use crate::types::{JsString, JsValue};

fn run(chunk: Chunk) -> Completion {
    let mut interp = Interpreter::new();
    let env = interp.realm().global_env.clone();
    run_chunk(&mut interp, &chunk, &env, JsValue::UNDEFINED)
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
        .unwrap_or(JsValue::UNDEFINED);
    (v, interp.bytecode_chunks_executed)
}

fn eval_script_completion_with_mode(source: &str, bytecode: bool) -> (Completion, usize) {
    use crate::parser::Parser;
    let mut p = Parser::new(source).expect("parser init");
    let program = p.parse_program().expect("parse");
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = bytecode;
    let completion = interp.run(&program);
    (completion, interp.bytecode_chunks_executed)
}

fn assert_script_completion_number(source: &str, expected: f64) {
    let (tree, tree_count) = eval_script_completion_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_script_completion_with_mode(source, true);
    assert_eq!(tree_count, 0, "tree-walker mode ran bytecode for {source}");
    assert_eq!(
        bytecode_count, 1,
        "eligible Script Body did not run exactly one chunk for {source}"
    );
    for (mode, completion) in [("tree-walker", tree), ("bytecode", bytecode)] {
        match completion {
            Completion::Normal(value) => assert_eq!(
                value.as_number(),
                Some(expected),
                "{mode} completion for {source}: {value:?}"
            ),
            other => panic!("{mode} completion for {source}: {other:?}"),
        }
    }
}

fn assert_script_completion_undefined(source: &str) {
    let (tree, tree_count) = eval_script_completion_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_script_completion_with_mode(source, true);
    assert_eq!(tree_count, 0, "tree-walker mode ran bytecode for {source}");
    assert_eq!(
        bytecode_count, 1,
        "eligible Script Body did not run exactly one chunk for {source}"
    );
    for (mode, completion) in [("tree-walker", tree), ("bytecode", bytecode)] {
        match completion {
            Completion::Normal(value) if value.is_undefined() => {}
            other => panic!("{mode} completion for {source}: {other:?}"),
        }
    }
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
    assert_eq!(ast_v.as_number(), Some(42.0));
    assert_eq!(bc_v.as_number(), Some(42.0));
}

#[test]
fn end_to_end_addition_return_takes_bytecode_path() {
    let source = "var __r = (function(){ return 1 + 2; })();";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode mode must execute at least one chunk");
    assert_eq!(v.as_number(), Some(3.0));
}

#[test]
fn end_to_end_var_declaration_takes_bytecode_path() {
    let source = "var __r = (function(){ var x = 7; return x; })();";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode mode must execute the var body");
    assert_eq!(v.as_number(), Some(7.0));
}

#[test]
fn end_to_end_bytecode_off_unchanged() {
    let source = "var __r = (function(){ return 42; })();";
    let (v, count) = eval_with_mode(source, false);
    assert_eq!(count, 0);
    assert_eq!(v.as_number(), Some(42.0));
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
        v.is_object(),
        "expected new f() to return the instance, got {v:?}"
    );
}

#[test]
fn end_to_end_member_assignment_in_loop_takes_bytecode_path() {
    // Motivating case for issue #388 (mandreel's heap-copy loop shape):
    // a numeric `for` loop whose body is nothing but member/array-element
    // read + write. Before member-access opcodes landed, `Expression::Member`
    // had no `compile_expr` case, so this bailed the whole body to the
    // tree-walker (bc_count == 0) despite being otherwise loop-eligible.
    let source = "var __r = (function(a, b, n) { \
        for (var i = 0; i < n; i++) { a[i] = b[i]; } \
        return a[n - 1]; \
    })([0, 0, 0], [7, 8, 9], 3);";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "member access inside the loop should now compile to bytecode"
    );
    assert_eq!(v.as_number(), Some(9.0));
}

#[test]
fn property_access_loop_releases_consumed_operand_roots() {
    use crate::parser::Parser;

    let source = "
        function collect() { $262.gc(); }
        function make() { return { value: 1 }; }
        function makeKey() { return ['value']; }
        function hot(limit) {
            for (var i = 0; i < limit; i++) {
                collect();
                make().value;
                make()[makeKey()];
                make().value = 1;
                make()[makeKey()] = 1;
            }
            collect();
        }
    ";
    let mut parser = Parser::new(source).expect("parser init");
    let program = parser.parse_program().expect("parse");
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = true;
    assert!(matches!(
        interp.run(&program),
        Completion::Normal(_) | Completion::Empty
    ));
    interp.gc.request();
    interp.gc_safepoint();
    let live_before = interp.objects.live_count();

    let hot = interp.get_global_var_ref("hot").expect("hot binding");
    let chunks_before = interp.bytecode_chunks_executed;
    assert!(matches!(
        interp.call_function(&hot, &JsValue::UNDEFINED, &[JsValue::number(64.0)]),
        Completion::Normal(_)
    ));
    assert!(
        interp.bytecode_chunks_executed > chunks_before,
        "the property-access loop must execute through bytecode"
    );
    assert_eq!(
        interp.objects.live_count(),
        live_before,
        "consumed property bases and keys must be collectable before the bytecode chunk exits"
    );
}

#[test]
fn end_to_end_dot_read_takes_bytecode_path() {
    let source = "var __r = (function(o){ return o.x; })({x: 5});";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "dot read should compile to bytecode");
    assert_eq!(v.as_number(), Some(5.0));
}

#[test]
fn end_to_end_computed_read_on_plain_array_takes_bytecode_path() {
    let source = "var __r = (function(a,i){ return a[i]; })([10,20,30], 1);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "computed array read should compile to bytecode");
    assert_eq!(v.as_number(), Some(20.0));
}

#[test]
fn end_to_end_computed_read_on_typed_array_takes_bytecode_path() {
    let source = "var __r = (function(ta,i){ return ta[i]; })(new Int32Array([1,2,3]), 2);";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "computed typed-array read should compile to bytecode"
    );
    assert_eq!(v.as_number(), Some(3.0));
}

#[test]
fn end_to_end_dot_write_takes_bytecode_path() {
    let source = "var __r = (function(o){ o.x = 9; return o.x; })({});";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "dot write should compile to bytecode");
    assert_eq!(v.as_number(), Some(9.0));
}

#[test]
fn end_to_end_computed_write_on_plain_array_takes_bytecode_path() {
    let source = "var __r = (function(a,i,v){ a[i] = v; return a[i]; })([1,2,3], 1, 42);";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "computed array write should compile to bytecode"
    );
    assert_eq!(v.as_number(), Some(42.0));
}

#[test]
fn end_to_end_computed_write_on_typed_array_takes_bytecode_path() {
    let source =
        "var __r = (function(ta,i,v){ ta[i] = v; return ta[i]; })(new Int32Array(3), 1, 42);";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "computed typed-array write should compile to bytecode"
    );
    assert_eq!(v.as_number(), Some(42.0));
}

fn assert_message_parity(source: &str) {
    let (ast_v, _) = eval_with_mode(source, false);
    let (bc_v, bc_count) = eval_with_mode(source, true);
    assert!(
        bc_count >= 1,
        "expected the member-access opcodes to compile for {source}"
    );
    assert_eq!(
        format!("{ast_v}"),
        format!("{bc_v}"),
        "tree-walker and bytecode paths must throw the exact same message for {source}"
    );
}

#[test]
fn end_to_end_dot_read_null_base_message_matches_tree_walker() {
    assert_message_parity(
        "var __r; try { (function(o){ return o.x; })(null); } catch (e) { __r = e.message; }",
    );
}

#[test]
fn end_to_end_computed_read_null_base_message_matches_tree_walker() {
    // The computed-key null/undefined-base check does NOT interpolate the
    // key into the message (unlike the Dot case) — this pins that exactly,
    // per the corrected design in the plan after adversarial review.
    assert_message_parity(
        "var __r; try { (function(k){ return null[k]; })(0); } catch (e) { __r = e.message; }",
    );
}

#[test]
fn end_to_end_dot_write_undefined_base_message_matches_tree_walker() {
    assert_message_parity(
        "var __r; try { (function(o){ o.x = 1; })(undefined); } catch (e) { __r = e.message; }",
    );
}

#[test]
fn end_to_end_computed_write_null_base_message_matches_tree_walker() {
    assert_message_parity(
        "var __r; try { (function(k){ null[k] = 1; })(0); } catch (e) { __r = e.message; }",
    );
}

#[test]
fn end_to_end_member_chain_base_survives_gc_during_rhs_evaluation() {
    // Two-hop GC-rooting regression (mirrors test262-extra/GC-member-assignment-base-rooting.js,
    // but exercises the *bytecode* GetProp/SetProp opcodes specifically). `a.base` is read
    // first, pushing a freshly-allocated, otherwise-unreferenced object onto the VM operand
    // stack. `a.rhs` is evaluated next as the assignment's RHS — a *separate*, later-dispatched
    // GetProp opcode whose own getter forces a GC collection ($262.gc()) before the pending
    // `a.base` result is ever consumed by the final SetProp("value"). A single-hop test (e.g.
    // a bare `x.value = rhs()` where `x` is a local variable) would pass even if the VM's
    // whole-stack rooting fix were missing, since `x` stays separately reachable via the
    // environment/call-frame roots regardless — this shape specifically requires the fix.
    let source = "
        var observed = 0;
        var prototype = { set value(v) { observed = v; } };
        function makeBase() { return Object.create(prototype); }
        var __r = (function(a) {
            a.base.value = a.rhs;
            return observed;
        })({
            get base() { return makeBase(); },
            get rhs() { $262.gc(); return 42; },
        });
    ";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "the containing function should compile to bytecode"
    );
    assert_eq!(
        v.as_number(),
        Some(42.0),
        "the `a.base` result must survive the nested GC triggered while evaluating `a.rhs`, got {v:?}"
    );
}

#[test]
fn end_to_end_getprop_call_argument_survives_gc_during_sibling_arg_evaluation() {
    // Regression for merging PR #399 (calls) with PR #397 (member access): `GetProp`
    // pushed its result with a raw `stack.push`, bypassing `push_value`'s
    // `gc_bytecode_roots` rooting. Harmless while GetProp and Call were compiled by
    // separate PRs, but the merged VM now compiles `identity(a.child, forceGc())`
    // end-to-end: `a.child`'s freshly-allocated, otherwise-unreferenced result sits
    // on the *same chunk's* operand stack as a pending Call argument while the
    // *second* argument's own nested call runs and forces a GC before the outer
    // Call consumes it.
    let source = "
        function identity(x, _y) { return x; }
        function forceGc() { $262.gc(); return 0; }
        var __r = (function(a) {
            return identity(a.child, forceGc()).tag;
        })({
            get child() {
                var o = Object.create(null);
                o.tag = 'ok';
                return o;
            },
        });
    ";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "the containing function should compile to bytecode"
    );
    assert!(
        v.as_string().is_some_and(|s| s.to_string() == "ok"),
        "GetProp's call-argument result must survive the GC forced while evaluating a sibling call argument, got {v:?}"
    );
}

#[test]
fn load_const_and_return_yields_number_completion() {
    let chunk = Chunk {
        code: vec![Op::LoadConst as u8, 0, 0, Op::Return as u8],
        constants: vec![Constant::Number(42.0)],
        names: vec![],
        var_names: vec![],
        max_stack: 1,
        max_refs: 0,
    };
    match run(chunk) {
        Completion::Return(value) => assert_eq!(value.as_number(), Some(42.0)),
        other => panic!("expected Return(Number(42.0)), got {other:?}"),
    }
}

#[test]
fn return_undefined_completes_with_undefined() {
    let chunk = Chunk {
        code: vec![Op::ReturnUndefined as u8],
        constants: vec![],
        names: vec![],
        var_names: vec![],
        max_stack: 0,
        max_refs: 0,
    };
    match run(chunk) {
        Completion::Return(value) if value.is_undefined() => {}
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
        var_names: vec![],
        max_stack: 2,
        max_refs: 0,
    };
    match run(chunk) {
        Completion::Return(value) => assert_eq!(value.as_number(), Some(5.0)),
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
        Completion::Return(value) => assert_eq!(value.as_number(), Some(42.0)),
        other => panic!("expected Return(Number(42.0)), got {other:?}"),
    }
}

#[test]
fn sequential_chunks_reuse_operand_stack_storage() {
    let body = vec![Statement::Return(Some(Expression::Literal(
        Literal::Number(42.0),
    )))];
    let chunk = compile_body(&body).expect("compile");
    let mut interp = Interpreter::new();
    let env = interp.realm().global_env.clone();

    for invocation in 1..=2 {
        match run_chunk(&mut interp, &chunk, &env, JsValue::UNDEFINED) {
            Completion::Return(value) => assert_eq!(value.as_number(), Some(42.0)),
            other => panic!("invocation {invocation} returned {other:?}"),
        }
        assert_eq!(interp.vm_operand_stack_pool.len(), 1);
    }
}

#[test]
fn nested_chunks_release_independent_reference_stacks() {
    use crate::parser::Parser;

    let source = "\
        function inner(value) { return value + 1; } \
        function outer(value) { return inner(value); } \
        var __r = outer(41);";
    let mut parser = Parser::new(source).expect("parser init");
    let program = parser.parse_program().expect("parse");
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = true;

    assert!(matches!(
        interp.run(&program),
        Completion::Normal(_) | Completion::Empty
    ));
    assert_eq!(
        interp.get_global_var_ref("__r").and_then(|v| v.as_number()),
        Some(42.0)
    );
    assert!(interp.bytecode_chunks_executed >= 2);
    assert_eq!(interp.vm_ref_stack_pool.len(), 2);
}

#[test]
fn releasing_reference_stack_drops_environment_handles_before_pooling() {
    use crate::interpreter::eval::IdentifierRef;
    use crate::interpreter::types::Environment;
    use std::rc::Rc;

    let mut interp = Interpreter::new();
    let env = Environment::new_function_scope(None);
    let mut refs = interp.acquire_vm_ref_stack(1);
    refs.push(IdentifierRef::SpecificEnv(env.clone()));
    assert_eq!(Rc::strong_count(&env), 2);

    interp.release_vm_ref_stack(refs);

    assert_eq!(Rc::strong_count(&env), 1);
    assert_eq!(interp.vm_ref_stack_pool.len(), 1);
    assert!(interp.vm_ref_stack_pool[0].is_empty());
    interp.recycle_function_environment(env);
    assert_eq!(interp.function_env_pool.len(), 1);
}

#[test]
fn compile_body_empty_returns_undefined() {
    let chunk = compile_body(&[]).expect("compile");
    match run(chunk) {
        Completion::Return(value) if value.is_undefined() => {}
        other => panic!("expected Return(Undefined), got {other:?}"),
    }
}

#[test]
fn compile_script_body_returns_statement_completion() {
    let body = vec![Statement::Expression(Expression::Literal(Literal::Number(
        42.0,
    )))];
    let chunk = compile_script_body(&body).expect("compile script");
    match run(chunk) {
        Completion::Normal(value) => assert_eq!(value.as_number(), Some(42.0)),
        other => panic!("expected Normal(Number(42.0)), got {other:?}"),
    }

    let empty_chunk = compile_script_body(&[]).expect("compile empty script");
    match run(empty_chunk) {
        Completion::Empty => {}
        other => panic!("expected Empty, got {other:?}"),
    }
}

#[test]
fn compile_body_bare_return_yields_undefined() {
    let body = vec![Statement::Return(None)];
    let chunk = compile_body(&body).expect("compile");
    match run(chunk) {
        Completion::Return(value) if value.is_undefined() => {}
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
        Completion::Return(value) => assert_eq!(value.as_number(), Some(5.0)),
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
        assert_eq!(v.as_number(), Some(expected), "{expr}");
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
        assert_eq!(v.as_boolean(), Some(expected), "{expr}");
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
        let number = v
            .as_number()
            .unwrap_or_else(|| panic!("{expr}: expected Number, got {v:?}"));
        assert_eq!(number as i64, expected as i64, "{expr}");
    }
}

#[test]
fn end_to_end_unary_ops_via_bytecode() {
    let cases: &[(&str, JsValue)] = &[
        ("(function(){ return -5; })()", JsValue::number(-5.0)),
        ("(function(){ return +'3'; })()", JsValue::number(3.0)),
        ("(function(){ return !true; })()", JsValue::FALSE),
        ("(function(){ return !0; })()", JsValue::TRUE),
        ("(function(){ return ~5; })()", JsValue::number(-6.0)),
        ("(function(){ return void 0; })()", JsValue::UNDEFINED),
        (
            "(function(){ return void 'anything'; })()",
            JsValue::UNDEFINED,
        ),
    ];
    for (expr, expected) in cases {
        let source = format!("var __r = {expr};");
        let (v, count) = eval_with_mode(&source, true);
        assert!(count >= 1, "{expr}: bytecode path must run");
        if let Some(expected_number) = expected.as_number() {
            assert_eq!(v.as_number(), Some(expected_number), "{expr}");
        } else if let Some(expected_boolean) = expected.as_boolean() {
            assert_eq!(v.as_boolean(), Some(expected_boolean), "{expr}");
        } else if expected.is_undefined() {
            assert!(v.is_undefined(), "{expr}: expected undefined, got {v:?}");
        } else {
            panic!("{expr}: unsupported expected value {expected:?}");
        }
    }
}

#[test]
fn end_to_end_ternary_conditional_via_bytecode() {
    let cases: &[(&str, JsValue)] = &[
        (
            "(function(){ return true ? 1 : 2; })()",
            JsValue::number(1.0),
        ),
        (
            "(function(){ return false ? 1 : 2; })()",
            JsValue::number(2.0),
        ),
        ("(function(){ return 0 ? 1 : 2; })()", JsValue::number(2.0)),
        (
            "(function(){ return 1 ? 'yes' : 'no'; })()",
            JsValue::number(0.0), /* placeholder */
        ),
        (
            "(function(){ return null ? 1 : 2; })()",
            JsValue::number(2.0),
        ),
    ];
    for (i, (expr, expected)) in cases.iter().enumerate() {
        let source = format!("var __r = {expr};");
        let (v, count) = eval_with_mode(&source, true);
        assert!(count >= 1, "{expr}: bytecode path must run");
        if i == 3 {
            // String case — check separately
            let string = v
                .as_string()
                .unwrap_or_else(|| panic!("{expr}: expected String 'yes', got {v:?}"));
            assert_eq!(string.to_rust_string(), "yes", "{expr}");
            continue;
        }
        assert_eq!(v.as_number(), expected.as_number(), "{expr}");
    }
}

#[test]
fn end_to_end_logical_short_circuit_via_bytecode() {
    // && returns lhs if falsy, else rhs
    // || returns lhs if truthy, else rhs
    // ?? returns lhs if non-nullish, else rhs
    let cases: &[(&str, JsValue)] = &[
        ("(function(){ return true && 5; })()", JsValue::number(5.0)),
        ("(function(){ return false && 5; })()", JsValue::FALSE),
        ("(function(){ return 0 && 5; })()", JsValue::number(0.0)),
        ("(function(){ return 1 && 2; })()", JsValue::number(2.0)),
        ("(function(){ return false || 5; })()", JsValue::number(5.0)),
        ("(function(){ return 7 || 5; })()", JsValue::number(7.0)),
        ("(function(){ return 0 || 5; })()", JsValue::number(5.0)),
        ("(function(){ return null ?? 5; })()", JsValue::number(5.0)),
        ("(function(){ return 0 ?? 5; })()", JsValue::number(0.0)),
        (
            "(function(){ return 'x' ?? 5; })()",
            JsValue::number(0.0), /* placeholder */
        ),
    ];
    for (i, (expr, expected)) in cases.iter().enumerate() {
        let source = format!("var __r = {expr};");
        let (v, count) = eval_with_mode(&source, true);
        assert!(count >= 1, "{expr}: bytecode path must run");
        if i == 9 {
            // 'x' ?? 5 → 'x'
            let string = v
                .as_string()
                .unwrap_or_else(|| panic!("{expr}: expected String 'x', got {v:?}"));
            assert_eq!(string.to_rust_string(), "x", "{expr}");
            continue;
        }
        if let Some(expected_number) = expected.as_number() {
            assert_eq!(v.as_number(), Some(expected_number), "{expr}");
        } else if let Some(expected_boolean) = expected.as_boolean() {
            assert_eq!(v.as_boolean(), Some(expected_boolean), "{expr}");
        } else {
            panic!("{expr}: unsupported expected value {expected:?}");
        }
    }
}

#[test]
fn end_to_end_param_read_via_bytecode() {
    // Function that just returns its parameter
    let source = "var __r = (function(x){ return x; })(42);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must run");
    assert_eq!(v.as_number(), Some(42.0), "got {v:?}");
}

#[test]
fn end_to_end_param_arithmetic_via_bytecode() {
    let source = "var __r = (function(x, y){ return x + y * 2; })(3, 5);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must run");
    assert_eq!(v.as_number(), Some(13.0), "got {v:?}");
}

#[test]
fn end_to_end_param_compare_returns_boolean() {
    let source = "var __r = (function(n){ return n > 10; })(5);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must run");
    assert_eq!(v.as_boolean(), Some(false), "got {v:?}");
}

#[test]
fn end_to_end_undeclared_identifier_throws_reference_error() {
    // Should throw ReferenceError, not just falsely succeed via the bytecode path
    let source = "var __r = false; try { (function(){ return undeclaredX; })(); } catch (e) { __r = e instanceof ReferenceError; }";
    let (v, _count) = eval_with_mode(source, true);
    assert_eq!(v.as_boolean(), Some(true), "got {v:?}");
}

#[test]
fn end_to_end_param_mutation_via_bytecode() {
    let source = "var __r = (function(x){ x = x + 1; return x; })(5);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must run");
    assert_eq!(v.as_number(), Some(6.0), "got {v:?}");
}

#[test]
fn end_to_end_multiple_statements_via_bytecode() {
    let source = "var __r = (function(x){ x = x + 1; x = x * 2; return x; })(3);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must run");
    assert_eq!(v.as_number(), Some(8.0), "got {v:?}");
}

#[test]
fn end_to_end_assignment_returns_assigned_value() {
    // `(x = expr)` evaluates to the assigned value
    let source = "var __r = (function(x){ return (x = 99); })(0);";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "bytecode path must run");
    assert_eq!(v.as_number(), Some(99.0), "got {v:?}");
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
        constants: vec![
            Constant::String(JsString::from_str("x")),
            Constant::Number(1.0),
        ],
        names: vec![],
        var_names: vec![],
        max_stack: 2,
        max_refs: 0,
    };
    match run(chunk) {
        Completion::Return(value) => assert_eq!(
            value
                .as_string()
                .expect("expected Return(String(\"x1\"))")
                .to_string(),
            "x1"
        ),
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
    assert_eq!(ast_v.as_number(), Some(expected), "{source}: AST value");
    assert_eq!(bc_v.as_number(), Some(expected), "{source}: bytecode value");
}

// NOTE: lexical declarations are not yet compilable, so a body containing one
// still bails the WHOLE body to the tree-walker.

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
    assert_eq!(ast_v.as_number(), Some(1.0), "ast {ast_v:?}");
    assert_eq!(bc_v.as_number(), Some(1.0), "bc {bc_v:?}");
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
        Completion::Return(value) => assert_eq!(value.as_number(), Some(10.0)),
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
fn compound_assign_on_member_bails_to_unsupported() {
    let source = "var __r = (function(a){ a[0] += 1; return a[0]; })([1]);";
    let (v, count) = eval_with_mode(source, true);
    assert_eq!(
        count, 0,
        "compound assignment on a member target must bail to the tree-walker"
    );
    assert_eq!(v.as_number(), Some(2.0));
}

#[test]
fn update_on_member_bails_to_unsupported() {
    let source = "var __r = (function(a){ a[0]++; return a[0]; })([1]);";
    let (v, count) = eval_with_mode(source, true);
    assert_eq!(
        count, 0,
        "update expression on a member target must bail to the tree-walker"
    );
    assert_eq!(v.as_number(), Some(2.0));
}

#[test]
fn optional_member_access_bails_to_unsupported() {
    let source = "var __r = (function(a){ return a?.x; })({x: 3});";
    let (v, count) = eval_with_mode(source, true);
    assert_eq!(
        count, 0,
        "optional member access must bail to the tree-walker"
    );
    assert_eq!(v.as_number(), Some(3.0));
}

#[test]
fn private_field_member_bails_to_unsupported() {
    use super::compiler::CompileError;
    use crate::ast::{MemberProperty, PropSiteId};
    // `this.#x` — private-field access always bails, regardless of what the
    // base expression is; the `MemberProperty::Private` arm short-circuits
    // before ever recursing into the base.
    let body = vec![Statement::Return(Some(Expression::Member(
        Box::new(Expression::This),
        MemberProperty::Private("x".to_string()),
        PropSiteId::UNASSIGNED,
    )))];
    match compile_body(&body) {
        Err(CompileError::Unsupported(_)) => {}
        other => panic!("expected Err(Unsupported), got {other:?}"),
    }
}

#[test]
fn compile_body_one_armed_if_returning_consequent_false_path_is_safe() {
    use crate::ast::IfStatement;
    // Regression (PR #159): a one-armed `if` whose consequent ends in `return`,
    // as the LAST statement of the body. The false arm's `JumpIfFalse` targets
    // the end of the chunk, so `finish()` must append a trailing
    // `ReturnUndefined` — otherwise the VM runs `pc` off the end of `code` and
    // panics. With a constant-false test the consequent never runs, so the
    // chunk must complete as `Return(Undefined)`.
    let body = vec![Statement::If(IfStatement {
        test: Expression::Literal(Literal::Boolean(false)),
        consequent: Box::new(Statement::Return(Some(Expression::Literal(
            Literal::Number(1.0),
        )))),
        alternate: None,
    })];
    let chunk = compile_body(&body).expect("compile one-armed if");
    match run(chunk) {
        Completion::Return(value) if value.is_undefined() => {}
        other => panic!("expected Return(Undefined) on false path, got {other:?}"),
    }
}

#[test]
fn end_to_end_one_armed_if_last_statement_both_paths() {
    // Same regression via the real bytecode path. The true path returns the
    // consequent's value; the false path falls through to the implicit
    // `ReturnUndefined`. Both must match the tree-walker and not panic.
    let true_src = "var __r = (function(x){ if (x) return 1; })(true);";
    let (av, ac) = eval_with_mode(true_src, false);
    let (bv, bc) = eval_with_mode(true_src, true);
    assert_eq!(ac, 0, "AST mode must not run chunks");
    assert!(bc >= 1, "bytecode path must run (true)");
    assert_eq!(av.as_number(), Some(1.0), "ast true {av:?}");
    assert_eq!(bv.as_number(), Some(1.0), "bc true {bv:?}");

    let false_src = "var __r = (function(x){ if (x) return 1; })(false);";
    let (av2, ac2) = eval_with_mode(false_src, false);
    let (bv2, bc2) = eval_with_mode(false_src, true);
    assert_eq!(ac2, 0, "AST mode must not run chunks");
    assert!(bc2 >= 1, "bytecode path must run (false)");
    assert!(av2.is_undefined(), "ast false {av2:?}");
    assert!(bv2.is_undefined(), "bc false {bv2:?}");
}

#[test]
fn load_undefined_then_return_completes_with_undefined() {
    let chunk = Chunk {
        code: vec![Op::LoadUndefined as u8, Op::Return as u8],
        constants: vec![],
        names: vec![],
        var_names: vec![],
        max_stack: 1,
        max_refs: 0,
    };
    match run(chunk) {
        Completion::Return(value) if value.is_undefined() => {}
        other => panic!("expected Return(Undefined), got {other:?}"),
    }
}

// ----- var declarations, updates, compound assignment, and loops -----

#[test]
fn var_bindings_are_hoisted_before_initializers() {
    let source = "var __r = (function(){ var before = x; var x = 7; return before; })();";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert!(bytecode_count >= 1, "bytecode path must run");
    assert!(ast.is_undefined());
    assert!(bytecode.is_undefined());
}

#[test]
fn multiple_var_initializers_run_in_source_order() {
    assert_parity_number(
        "var __r = (function(){ var a = 1, b = a + 2; return b; })();",
        3.0,
    );
}

#[test]
fn prefix_and_postfix_updates_preserve_result_value() {
    assert_parity_number(
        "var __r = (function(){ var x = 1; var old = x++; var now = ++x; return old * 100 + now * 10 + x; })();",
        133.0,
    );
    assert_parity_number(
        "var __r = (function(){ var x = 3; return x-- * 10 + --x; })();",
        31.0,
    );
}

#[test]
fn identifier_update_preserves_bigint_semantics() {
    let source = "var __r = (function(x){ x++; return x; })(1n);";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert!(bytecode_count >= 1, "bytecode path must run");
    let ast_bigint = ast
        .as_bigint()
        .unwrap_or_else(|| panic!("expected BigInt AST result, got {ast:?}"));
    let bytecode_bigint = bytecode
        .as_bigint()
        .unwrap_or_else(|| panic!("expected BigInt bytecode result, got {bytecode:?}"));
    assert_eq!(ast_bigint.value.to_string(), "2");
    assert_eq!(bytecode_bigint.value.to_string(), "2");
}

#[test]
fn compound_assignment_preserves_captured_identifier_reference() {
    let source = "\
        var obj = { get x(){ delete this.x; return 2; } }; \
        var f; \
        with (obj) { f = function(){ x += 3; return 0; }; } \
        f(); \
        var __r = Object.prototype.hasOwnProperty.call(obj, 'x') ? obj.x : -1;";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert!(bytecode_count >= 1, "nested function must use bytecode");
    assert_eq!(ast.as_number(), Some(5.0));
    assert_eq!(bytecode.as_number(), Some(5.0));
}

#[test]
fn numeric_for_loop_takes_bytecode_path() {
    assert_parity_number(
        "var __r = (function(n){ var sum = 0; for (var i = 0; i < n; i++) { sum += i; } return sum; })(10);",
        45.0,
    );
}

#[test]
fn top_level_numeric_for_loop_takes_bytecode_path() {
    let source = "var __r = 0; for (var i = 0; i < 10; i++) { __r += i; }";
    let (value, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(
        bytecode_count, 1,
        "the eligible Script Body must execute exactly one bytecode chunk"
    );
    assert_eq!(value.as_number(), Some(45.0));
}

#[test]
fn top_level_bytecode_var_is_a_global_property() {
    use crate::parser::Parser;

    let mut parser = Parser::new("var topLevelBytecodeGlobal = 42;").expect("parser init");
    let program = parser.parse_program().expect("parse");
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = true;
    let completion = interp.run(&program);

    assert!(matches!(completion, Completion::Empty));
    assert_eq!(interp.bytecode_chunks_executed, 1);
    let global_id = interp
        .realm()
        .global_env
        .borrow()
        .global_object_id
        .expect("global object");
    let descriptor = interp
        .get_object(global_id)
        .expect("global object cell")
        .borrow()
        .get_own_property("topLevelBytecodeGlobal")
        .expect("global var property");
    assert_eq!(
        descriptor.value.and_then(|value| value.as_number()),
        Some(42.0)
    );
    assert_eq!(descriptor.configurable, Some(false));
}

#[test]
fn top_level_bytecode_var_redeclaration_preserves_existing_global_property() {
    use crate::parser::Parser;

    for bytecode in [false, true] {
        let mut interp = Interpreter::new();
        interp.bytecode_enabled = bytecode;

        let mut setup = Parser::new(
            "Object.defineProperty(globalThis, 'existingBytecodeGlobal', { \
                value: 17, writable: true, configurable: true \
            });",
        )
        .expect("setup parser init");
        let setup_program = setup.parse_program().expect("parse setup");
        assert!(matches!(interp.run(&setup_program), Completion::Normal(_)));

        let mut redeclaration = Parser::new("var existingBytecodeGlobal; existingBytecodeGlobal;")
            .expect("redeclaration parser init");
        let redeclaration_program = redeclaration.parse_program().expect("parse redeclaration");
        let completion = interp.run(&redeclaration_program);

        let Completion::Normal(value) = completion else {
            panic!("expected normal Script completion, got {completion:?}");
        };
        assert_eq!(value.as_number(), Some(17.0));
        assert_eq!(interp.bytecode_chunks_executed, usize::from(bytecode));
    }
}

#[test]
fn top_level_bytecode_checks_global_declarations_before_execution() {
    use crate::parser::Parser;

    for bytecode in [false, true] {
        let mut parser = Parser::new("var blockedBytecodeGlobal = 1; 2;").expect("parser init");
        let program = parser.parse_program().expect("parse");
        let mut interp = Interpreter::new();
        interp.bytecode_enabled = bytecode;
        let global_id = interp
            .realm()
            .global_env
            .borrow()
            .global_object_id
            .expect("global object");
        interp
            .get_object(global_id)
            .expect("global object cell")
            .borrow_mut()
            .extensible = false;

        assert!(matches!(interp.run(&program), Completion::Throw(_)));
        assert!(interp.get_global_var_ref("blockedBytecodeGlobal").is_none());
        assert_eq!(
            interp.bytecode_chunks_executed, 0,
            "declaration failure must prevent chunk execution"
        );
    }
}

#[test]
fn script_statement_list_completion_matches_tree_walker() {
    assert_script_completion_number("1;", 1.0);
    assert_script_completion_number("1; ; var ignored;", 1.0);
    assert_script_completion_number("1; if (true) { 2; }", 2.0);
    assert_script_completion_undefined("1; if (false) { 2; }");
    assert_script_completion_undefined("1; if (true) { var ignored; }");
    assert_script_completion_undefined("1; for (var i = 0; i < 0; i++) { 2; }");
    assert_script_completion_number("for (var i = 0; i < 3; i++) { i; }", 2.0);
    assert_script_completion_undefined("for (var i = 0; i < 3; i++) {}");
    assert_script_completion_number("while (false) {}; 3;", 3.0);
}

#[test]
fn script_completion_value_is_rooted_across_nested_gc() {
    use crate::parser::Parser;

    let source = "var collect = $262.gc; Object(); var ignored = collect();";
    let mut parser = Parser::new(source).expect("parser init");
    let program = parser.parse_program().expect("parse");
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = true;

    let completion = interp.run(&program);
    assert_eq!(interp.bytecode_chunks_executed, 1);
    let object_id = match completion {
        Completion::Normal(value) => value
            .as_object_id()
            .unwrap_or_else(|| panic!("expected object completion, got {value:?}")),
        other => panic!("expected normal object completion, got {other:?}"),
    };
    assert!(
        interp.get_object(object_id).is_some(),
        "the completion object must survive collection in a later initializer"
    );
    assert!(
        interp.gc_bytecode_roots.is_empty(),
        "script chunk roots must be released at exit"
    );
}

#[test]
fn script_direct_eval_stays_on_tree_walker() {
    let source = "var __r = eval('1 + 2');";
    let (value, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(
        bytecode_count, 0,
        "direct eval must keep the Script ineligible"
    );
    assert_eq!(value.as_number(), Some(3.0));
}

#[test]
fn while_loop_takes_bytecode_path() {
    assert_parity_number(
        "var __r = (function(n){ var sum = 0; while (n > 0) { sum += n; n--; } return sum; })(10);",
        55.0,
    );
}

#[test]
fn for_loop_optional_clauses_preserve_order() {
    assert_parity_number(
        "var __r = (function(i){ for (; i < 3; i++) {} return i; })(0);",
        3.0,
    );
    assert_parity_number(
        "var __r = (function(){ for (var i = 0; i < 3;) { i++; } return i; })();",
        3.0,
    );
}

#[test]
fn nested_for_and_while_loops_take_bytecode_path() {
    assert_parity_number(
        "var __r = (function(){ var n = 0; for (var i = 0; i < 3; i++) { var j = 0; while (j < 2) { n += i; j++; } } return n; })();",
        6.0,
    );
}

#[test]
fn while_loop_htmldda_truthiness_matches_tree_walker() {
    // $262.IsHTMLDDA carries the [[IsHTMLDDA]] slot (Annex B.3.6) and must
    // coerce to false despite being an object, matching document.all. `h` is
    // declared outside the compiled function (member access on `$262` isn't
    // itself compilable) and read as a captured identifier in the loop test.
    assert_parity_number(
        "var h = $262.IsHTMLDDA; var __r = (function(){ var i = 0; while (h) { i++; if (i > 2) return 99; } return 1; })();",
        1.0,
    );
}

#[test]
fn for_loop_htmldda_truthiness_matches_tree_walker() {
    assert_parity_number(
        "var h = $262.IsHTMLDDA; var __r = (function(){ var i = 0; for (; h; ) { i++; if (i > 2) return 99; } return 1; })();",
        1.0,
    );
}

#[test]
fn lexical_for_loop_falls_back_to_tree_walker() {
    let source = "var __r = (function(){ var sum = 0; for (let i = 0; i < 3; i++) sum += i; return sum; })();";
    let (value, count) = eval_with_mode(source, true);
    assert_eq!(count, 0, "lexical loop must remain ineligible");
    assert_eq!(value.as_number(), Some(3.0));
}

#[test]
fn loop_with_break_falls_back_to_tree_walker() {
    let source = "var __r = (function(){ var i = 0; while (true) { i++; break; } return i; })();";
    let (value, count) = eval_with_mode(source, true);
    assert_eq!(count, 0, "break lowering is not part of this slice");
    assert_eq!(value.as_number(), Some(1.0));
}

// ----- direct identifier calls -----

#[test]
fn direct_call_compiles_caller_and_compilable_callee() {
    let source = "\
        function addOne(value) { return value + 1; } \
        var __r = (function(value) { return addOne(value); })(41);";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert!(
        bytecode_count >= 2,
        "caller and callee must both execute bytecode"
    );
    assert_eq!(ast.as_number(), Some(42.0));
    assert_eq!(bytecode.as_number(), Some(42.0));
}

#[test]
fn direct_call_bridges_to_ineligible_callee() {
    let source = "\
        function readObject(value) { var box = { value: value }; return box.value; } \
        var __r = (function(value) { return readObject(value); })(37);";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert_eq!(
        bytecode_count, 1,
        "only the caller should compile; the object/member callee must fall back"
    );
    assert_eq!(ast.as_number(), Some(37.0));
    assert_eq!(bytecode.as_number(), Some(37.0));
}

#[test]
fn direct_native_call_takes_bytecode_path() {
    let source = "var __r = (function(value) { return parseInt(value); })('42');";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert_eq!(bytecode_count, 1, "native callee has no bytecode Body");
    assert_eq!(ast.as_number(), Some(42.0));
    assert_eq!(bytecode.as_number(), Some(42.0));
}

#[test]
fn direct_native_call_preserves_persistent_callback_root() {
    use crate::parser::Parser;

    let source = "\
        function makeCallback() { return function() {}; } \
        function schedule(callback) { return setTimeout(callback, 0); }";
    let mut parser = Parser::new(source).expect("parser init");
    let program = parser.parse_program().expect("parse");
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = true;
    assert!(matches!(
        interp.run(&program),
        Completion::Normal(_) | Completion::Empty
    ));

    let make_callback = interp
        .get_global_var_ref("makeCallback")
        .expect("makeCallback binding");
    let callback = match interp.call_function(&make_callback, &JsValue::UNDEFINED, &[]) {
        Completion::Normal(value) => value,
        other => panic!("makeCallback failed: {other:?}"),
    };
    let callback_id = callback
        .as_object_id()
        .expect("makeCallback must return a function object");
    let schedule = interp
        .get_global_var_ref("schedule")
        .expect("schedule binding");
    assert!(matches!(
        interp.call_function(&schedule, &JsValue::UNDEFINED, &[callback]),
        Completion::Normal(_)
    ));
    assert!(
        interp.bytecode_chunks_executed >= 1,
        "schedule must execute through bytecode"
    );
    assert!(
        interp.gc_bytecode_roots.is_empty(),
        "bytecode operand roots must be released with the frame"
    );

    interp.gc.request();
    interp.gc_safepoint();
    assert!(
        interp.get_object_cell(callback_id).is_some(),
        "setTimeout's persistent callback root must survive the bytecode frame"
    );
}

#[test]
fn loop_with_direct_calls_takes_bytecode_path() {
    assert_parity_number(
        "\
            function addOne(value) { return value + 1; } \
            var __r = (function(limit) { \
                var sum = 0; \
                for (var i = 0; i < limit; i++) { sum += addOne(i); } \
                return sum; \
            })(5);",
        15.0,
    );
}

#[test]
fn direct_call_preserves_with_base_as_this_value() {
    let source = "\
        var scope = { \
            value: 40, \
            invoke: function(value) { return this.value + value; } \
        }; \
        var runner; \
        with (scope) { runner = function() { return invoke(2); }; } \
        var __r = runner();";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert!(bytecode_count >= 1, "runner must execute through bytecode");
    assert_eq!(ast.as_number(), Some(42.0));
    assert_eq!(bytecode.as_number(), Some(42.0));
}

#[test]
fn direct_call_clears_stale_with_base_before_global_resolution() {
    let source = "
        var observed = 0;
        function recordThis() {
            'use strict';
            observed = this === undefined ? 1 : -1;
        }
        var runner;
        with ({ value: 42 }) {
            runner = function() {
                var ignored = value;
                recordThis();
            };
        }
        runner();
        var __r = observed;
    ";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert!(bytecode_count >= 1, "runner must execute through bytecode");
    assert_eq!(ast.as_number(), Some(1.0));
    assert_eq!(
        bytecode.as_number(),
        Some(1.0),
        "a global call must not inherit a with base from an earlier identifier read"
    );
}

#[test]
fn direct_call_checks_global_proxy_binding_once() {
    let source = "
        var hasCount = 0;
        var callCount = 0;
        var outcome = 0;
        var oldPrototype = Object.getPrototypeOf($262.global);
        var target = { fn: function() { callCount = callCount + 1; } };
        var proxy = new Proxy(target, {
            has: function(target, key) {
                if (key === 'fn') {
                    hasCount = hasCount + 1;
                    return hasCount === 1;
                }
                return Reflect.has(target, key);
            },
        });
        Object.setPrototypeOf($262.global, proxy);
        function run() { fn(); }
        try {
            run();
            outcome = 100;
        } catch (error) {
            outcome = -100;
        }
        Object.setPrototypeOf($262.global, oldPrototype);
        var __r = outcome + hasCount * 10 + callCount;
    ";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert!(bytecode_count >= 1, "run must execute through bytecode");
    assert_eq!(ast.as_number(), Some(111.0));
    assert_eq!(
        bytecode.as_number(),
        Some(111.0),
        "bytecode must invoke the stateful global proxy has trap exactly once, got {bytecode:?}"
    );
}

#[test]
fn direct_call_hits_call_site_ic_in_bytecode() {
    // Issue #432: the bytecode Call opcode must drive the same call-site IC
    // as the tree-walker's eval_call, not run cold on every iteration. A
    // 100-iteration hot loop on the same plain user function must register
    // IC hits and route through the fast-dispatch entry that skips the
    // proxy/wrapped/class-ctor checks (mirrors the tree-walker's
    // `call_ic_records_after_repeated_call` / `call_ic_fast_dispatch_actually_skips_entry_checks`
    // in `interpreter::tests`).
    use crate::parser::Parser;
    let source = "
        function f() { return 7; }
        var __r = (function() {
            var sum = 0;
            for (var i = 0; i < 100; i++) { sum = sum + f(); }
            return sum;
        })();
    ";
    let mut p = Parser::new(source).expect("parser init");
    let program = p.parse_program().expect("parse");
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = true;
    let _ = interp.run(&program);
    let v = interp
        .get_global_var_ref("__r")
        .unwrap_or(JsValue::UNDEFINED);
    assert_eq!(v.as_number(), Some(700.0));
    assert!(
        interp.bytecode_chunks_executed >= 1,
        "loop must execute through bytecode"
    );
    assert!(
        interp.call_ic_hit_count() > 0,
        "expected call-IC hits from the bytecode Call opcode after a \
         100-iteration hot loop; dispatch_body must thread current_ic_handle \
         into the bytecode branch for this to fire"
    );
    assert!(
        interp.call_ic_fast_dispatch_count() > 0,
        "expected the bytecode Call opcode to route IC hits through \
         call_function_ic_validated, not the slow call_function path"
    );
}

#[test]
fn pending_argument_survives_gc_during_later_call_argument() {
    let source = "\
        var collect = $262.gc; \
        var observed = 0; \
        function makeValue() { return { marker: 42 }; } \
        function consume(value, ignored) { observed = value.marker; } \
        function run() { consume(makeValue(), collect()); } \
        run(); \
        var __r = observed;";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert!(bytecode_count >= 1, "run must execute through bytecode");
    assert_eq!(ast.as_number(), Some(42.0));
    assert_eq!(bytecode.as_number(), Some(42.0));
}

#[test]
fn pending_with_getter_callee_survives_gc_during_argument() {
    let source = "\
        var collect = $262.gc; \
        var observed = 0; \
        var scope; \
        scope = { \
            get invoke() { \
                return function(ignored) { observed = this === scope ? 17 : -1; }; \
            } \
        }; \
        var runner; \
        with (scope) { runner = function() { invoke(collect()); }; } \
        runner(); \
        var __r = observed;";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert!(bytecode_count >= 1, "runner must execute through bytecode");
    assert_eq!(ast.as_number(), Some(17.0));
    assert_eq!(bytecode.as_number(), Some(17.0));
}

#[test]
fn strict_direct_return_call_preserves_tail_calls() {
    let source = "\
        function recur(count) { \
            'use strict'; \
            if (count === 0) return 42; \
            return recur(count - 1); \
        } \
        var __r = recur(2000);";
    let (value, bytecode_count) = eval_with_mode(source, true);
    assert!(
        bytecode_count >= 2001,
        "every recursive dispatch must execute bytecode"
    );
    assert_eq!(value.as_number(), Some(42.0));
}

#[test]
fn non_callable_direct_call_throws_via_bytecode() {
    let source = "\
        var value = 1; \
        var __r = false; \
        function run() { return value(); } \
        try { run(); } catch (error) { __r = error instanceof TypeError; }";
    let (value, bytecode_count) = eval_with_mode(source, true);
    assert!(bytecode_count >= 1, "run must execute through bytecode");
    assert_eq!(value.as_boolean(), Some(true));
}

#[test]
fn direct_eval_and_spread_calls_remain_ineligible() {
    use super::compiler::CompileError;
    use crate::ast::CallSiteId;

    let direct_eval = vec![Statement::Expression(Expression::Call(
        Box::new(Expression::Identifier("eval".to_string())),
        vec![Expression::Literal(Literal::String(
            "var x = 1".encode_utf16().collect(),
        ))],
        CallSiteId::UNASSIGNED,
    ))];
    assert!(matches!(
        compile_body(&direct_eval),
        Err(CompileError::Unsupported(_))
    ));

    let spread = vec![Statement::Expression(Expression::Call(
        Box::new(Expression::Identifier("f".to_string())),
        vec![Expression::Spread(Box::new(Expression::Identifier(
            "args".to_string(),
        )))],
        CallSiteId::UNASSIGNED,
    ))];
    assert!(matches!(
        compile_body(&spread),
        Err(CompileError::Unsupported(_))
    ));
}

#[test]
fn member_calls_and_nested_tail_positions_remain_ineligible() {
    use super::compiler::CompileError;
    use crate::ast::{CallSiteId, LogicalOp, MemberProperty, PropSiteId};

    let member_call = vec![Statement::Expression(Expression::Call(
        Box::new(Expression::Member(
            Box::new(Expression::Identifier("object".to_string())),
            MemberProperty::Dot("method".to_string()),
            PropSiteId::UNASSIGNED,
        )),
        vec![],
        CallSiteId::UNASSIGNED,
    ))];
    assert!(matches!(
        compile_body(&member_call),
        Err(CompileError::Unsupported(_))
    ));

    let nested_tail_call = Expression::Logical(
        LogicalOp::And,
        Box::new(Expression::Identifier("condition".to_string())),
        Box::new(Expression::Call(
            Box::new(Expression::Identifier("f".to_string())),
            vec![],
            CallSiteId::UNASSIGNED,
        )),
    );
    assert!(matches!(
        compile_body(&[Statement::Return(Some(nested_tail_call))]),
        Err(CompileError::Unsupported(_))
    ));
}

#[test]
fn top_level_bytecode_string_literals_preserve_utf16_code_units() {
    let source = r#"var __r = "\uD800" === "\uFFFD";"#;
    let (value, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(
        bytecode_count, 1,
        "the eligible Script Body must execute through bytecode"
    );
    assert_eq!(value.as_boolean(), Some(false));
}

/// A strict-mode tail call leaves `run_chunk_inner` through the trampoline
/// return, not the dispatch site, so `calls_from_vm` must be incremented
/// before that early return or every strict tail call goes uncounted.
///
/// The five counted calls are exactly `outer`'s five `return leaf(n)` tail
/// calls. `outer(total)` itself is *not* counted: the top-level script body
/// contains function declarations, which the compiler rejects, so the loop runs
/// on the tree-walker. Before the fix this assertion saw 0, not 5.
#[cfg(feature = "perf-counters")]
#[test]
fn strict_tail_calls_are_counted_as_vm_issued_calls() {
    let src = "'use strict';\
               function leaf(n) { return n + 1; }\
               function outer(n) { return leaf(n); }\
               var total = 0;\
               for (var i = 0; i < 5; i++) { total = outer(total); }\
               total;";
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = true;
    let mut parser = crate::parser::Parser::new(src).expect("parser init");
    let program = parser.parse_program().expect("parse");
    let completion = interp.run(&program);
    assert!(
        matches!(completion, Completion::Normal(_) | Completion::Empty),
        "{completion:?}"
    );
    assert!(
        interp.perf.body_compiled > 0,
        "outer() must compile for this test to exercise Op::ReturnCall"
    );
    assert_eq!(
        interp.perf.calls_from_vm, 5,
        "the five strict tail calls from outer()'s compiled body must be \
         counted, but calls_from_vm was {}",
        interp.perf.calls_from_vm
    );
}
