use super::chunk::{Chunk, Constant};
use super::compiler::{compile_body, compile_script_body};
use super::op::Op;
use super::vm::run_chunk;
use crate::ast::{ExprBox, Expression, Literal, Statement};
use crate::interpreter::Interpreter;
use crate::interpreter::types::Completion;
use crate::types::{JsString, JsValue};

fn run(chunk: Chunk) -> Completion {
    let mut interp = Interpreter::new();
    let env = interp.realm().global_env.clone();
    run_chunk(&mut interp, &chunk, &env)
}

#[test]
fn bytecode_enabled_defaults_to_true() {
    let interp = Interpreter::new();
    assert!(interp.bytecode_enabled);
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
fn end_to_end_new_expression_compiles_inside_wrapping_function() {
    // Unlike `end_to_end_constructor_with_empty_body_returns_this` above (whose
    // top-level `new f()` falls back to the tree-walker regardless of `new`
    // support, since `function f(){}` is an uncompiled top-level
    // FunctionDeclaration), this pins the `new` expression itself compiling:
    // `g`'s own body contains the `new` expression, and `f`'s body carries a
    // lexical declaration so it can never independently compile — any chunk
    // execution observed here must come from `g`.
    let source = "\
        function f(){ let unused = 0; } \
        function g(){ return new f(); } \
        var __r = g();";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "the `new` expression itself must compile to bytecode"
    );
    assert!(
        v.is_object(),
        "expected new f() to return the instance, got {v:?}"
    );
}

#[test]
fn end_to_end_new_expression_constructs_instance_with_arguments() {
    // `Point`'s body carries a lexical declaration so it can never
    // independently compile (it already supports `this.x = x`-shaped Dot
    // assignment on its own); any chunk execution here must come from the
    // wrapping IIFE, pinning the `new Point(3, 4)` expression itself.
    let source = "\
        function Point(x, y) { let unused = 0; this.x = x; this.y = y; } \
        var __r = (function() { \
            var p = new Point(3, 4); \
            return p.x + p.y; \
        })();";
    let (v, count) = eval_with_mode(source, true);
    assert!(count >= 1, "new with arguments should compile to bytecode");
    assert_eq!(v.as_number(), Some(7.0));
}

#[test]
fn end_to_end_new_expression_derived_class_default_constructor_compiles() {
    // `Base`'s constructor carries a lexical declaration so it can never
    // independently compile; `Derived`'s constructor is synthetic (no source
    // body to compile at all). Any chunk execution here must come from the
    // wrapping IIFE's own `new Derived(9)` expression.
    let source = "\
        class Base { constructor(v) { let unused = 0; this.v = v; } } \
        class Derived extends Base {} \
        var __r = (function() { \
            var d = new Derived(9); \
            return d.v; \
        })();";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "new on a derived default constructor should compile to bytecode"
    );
    assert_eq!(v.as_number(), Some(9.0));
}

#[test]
fn end_to_end_new_expression_derived_class_explicit_super_call_compiles() {
    let source = "\
        class Base { constructor(v) { let unused = 0; this.v = v; } } \
        class Derived extends Base { constructor(v) { let unused2 = 0; super(v); } } \
        var __r = (function() { \
            var d = new Derived(9); \
            return d.v; \
        })();";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "new on a derived explicit-super constructor should compile to bytecode"
    );
    assert_eq!(v.as_number(), Some(9.0));
}

#[test]
fn end_to_end_new_expression_non_constructor_arrow_callee_message_matches_tree_walker() {
    // The callee expression compiles regardless of what value it resolves to
    // at runtime (an Identifier compiles the same way whether it holds a
    // constructor or not) — this pins the IsConstructor check's error text.
    assert_message_parity(
        "var notCtor = () => {}; \
         var __r; \
         try { (function(){ return new notCtor(); })(); } catch (e) { __r = e.message; }",
    );
}

#[test]
fn end_to_end_new_expression_non_constructor_primitive_callee_message_matches_tree_walker() {
    assert_message_parity(
        "var notCtor = 42; \
         var __r; \
         try { (function(){ return new notCtor(); })(); } catch (e) { __r = e.message; }",
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

fn assert_computed_read_undefined_parity(source: &str) {
    let (ast_v, ast_count) = eval_with_mode(source, false);
    let (bc_v, bc_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0, "{source}: AST mode must not run chunks");
    assert!(bc_count >= 1, "{source}: bytecode path must run a chunk");
    assert!(ast_v.is_undefined(), "{source}: AST value was {ast_v:?}");
    assert!(bc_v.is_undefined(), "{source}: bytecode value was {bc_v:?}");
}

fn assert_computed_read_string_parity(source: &str, expected: &str) {
    let (ast_v, ast_count) = eval_with_mode(source, false);
    let (bc_v, bc_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0, "{source}: AST mode must not run chunks");
    assert!(bc_count >= 1, "{source}: bytecode path must run a chunk");
    for (mode, value) in [("AST", ast_v), ("bytecode", bc_v)] {
        assert_eq!(
            value.as_string().map(|s| s.to_string()).as_deref(),
            Some(expected),
            "{source}: {mode} value was {value:?}"
        );
    }
}

#[test]
fn end_to_end_computed_read_typed_array_out_of_bounds_matches_tree_walker() {
    assert_computed_read_undefined_parity(
        "var __r = (function(ta, i) { return ta[i]; })(new Int32Array([1]), 1);",
    );
}

#[test]
fn end_to_end_computed_read_typed_array_negative_index_matches_tree_walker() {
    assert_computed_read_undefined_parity(
        "var __r = (function(ta, i) { return ta[i]; })(new Int32Array([1]), -1);",
    );
}

#[test]
fn end_to_end_computed_read_typed_array_fractional_index_matches_tree_walker() {
    assert_computed_read_undefined_parity(
        "var __r = (function(ta, i) { return ta[i]; })(new Int32Array([1]), 0.5);",
    );
}

#[test]
fn end_to_end_computed_read_typed_array_minus_zero_index_matches_tree_walker() {
    assert_parity_number(
        "var __r = (function(ta, i) { return ta[i]; })(new Int32Array([11]), -0);",
        11.0,
    );
}

#[test]
fn end_to_end_computed_read_typed_array_detached_buffer_matches_tree_walker() {
    assert_computed_read_undefined_parity(
        "var ta = new Int32Array([1]); ta.buffer.transfer(); \
         var __r = (function(ta, i) { return ta[i]; })(ta, 0);",
    );
}

#[test]
fn end_to_end_computed_read_typed_array_through_proxy_matches_tree_walker() {
    assert_computed_read_string_parity(
        "var proxy = new Proxy(new Int32Array([1]), { \
             get: function() { return 'trapped'; } \
         }); \
         var __r = (function(ta, i) { return ta[i]; })(proxy, 0);",
        "trapped",
    );
}

#[test]
fn end_to_end_computed_read_array_out_of_bounds_matches_tree_walker() {
    assert_computed_read_undefined_parity("var __r = (function(a, i) { return a[i]; })([1], 1);");
}

#[test]
fn end_to_end_computed_read_array_negative_index_matches_tree_walker() {
    assert_computed_read_undefined_parity("var __r = (function(a, i) { return a[i]; })([1], -1);");
}

#[test]
fn end_to_end_computed_read_array_fractional_index_matches_tree_walker() {
    assert_computed_read_undefined_parity("var __r = (function(a, i) { return a[i]; })([1], 0.5);");
}

#[test]
fn end_to_end_computed_read_array_minus_zero_index_matches_tree_walker() {
    assert_parity_number(
        "var __r = (function(a, i) { return a[i]; })([11], -0);",
        11.0,
    );
}

#[test]
fn end_to_end_computed_read_array_shadowed_index_matches_tree_walker() {
    assert_parity_number(
        "var a = [10, 20]; \
         Object.defineProperty(a, '1', { get: function() { return 99; } }); \
         var __r = (function(a, i) { return a[i]; })(a, 1);",
        99.0,
    );
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
fn end_to_end_compound_dot_assignment_takes_bytecode_path() {
    assert_parity_number(
        "var __r = (function(o){ o.x += 1; return o.x; })({x: 5});",
        6.0,
    );
}

#[test]
fn compound_dot_assignment_base_survives_gc_during_rhs_evaluation() {
    // Same two-hop shape as `end_to_end_member_chain_base_survives_gc_during_rhs_evaluation`
    // but for `+=` on a Dot target: `a.base` is duplicated (DupN) so GetProp can read the
    // current value while a copy of the base stays pending for the trailing SetProp. `a.rhs`'s
    // own getter forces a GC collection before that pending base is consumed.
    let source = "
        var observed = 0;
        var prototype = { get value() { return 10; }, set value(v) { observed = v; } };
        function makeBase() { return Object.create(prototype); }
        var __r = (function(a) {
            a.base.value += a.rhs;
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
        Some(52.0),
        "the pending `a.base` must survive the nested GC triggered while evaluating `a.rhs`, got {v:?}"
    );
}

#[test]
fn end_to_end_compound_computed_assignment_on_plain_array_takes_bytecode_path() {
    assert_parity_number(
        "var __r = (function(a,i){ a[i] += 1; return a[i]; })([5], 0);",
        6.0,
    );
}

#[test]
fn end_to_end_compound_computed_assignment_on_typed_array_takes_bytecode_path() {
    assert_parity_number(
        "var __r = (function(ta,i){ ta[i] += 1; return ta[i]; })(new Float64Array([5.5]), 0);",
        6.5,
    );
}

#[test]
fn end_to_end_compound_computed_write_null_base_message_matches_tree_walker() {
    assert_message_parity(
        "var __r; try { (function(k){ null[k] += 1; })(0); } catch (e) { __r = e.message; }",
    );
}

#[test]
fn end_to_end_compound_computed_write_undefined_base_message_matches_tree_walker() {
    assert_message_parity(
        "var __r; try { (function(k){ undefined[k] += 1; })(0); } catch (e) { __r = e.message; }",
    );
}

#[test]
fn end_to_end_call_non_callable_object_message_matches_tree_walker() {
    assert_message_parity(
        "var __r; try { (function(o){ var f = o.x; return f(); })({x: {}}); } catch (e) { __r = e.message; }",
    );
}

#[test]
fn compound_computed_assignment_coerces_key_exactly_once() {
    // Pins both halves of sec-getvalue's ordering: the base-nullish check runs
    // before ToPropertyKey's side effect (the null/undefined-base tests above),
    // and the key's `toString` runs exactly once (not once for the read and
    // once for the write). The measured IIFE's own body makes no calls of its
    // own, so any executed chunk must come from it, not from `toString`
    // (which calls `calls.push`, a member call the compiler doesn't support,
    // so it can never mask a bail by compiling on its own).
    let source = "
        var calls = [];
        var key = { toString: function() { calls.push('k'); return 'i'; } };
        var __r = (function(o, k) {
            o[k] += 1;
            return calls.length;
        })({ i: 1 }, key);
    ";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "compound computed assignment should compile to bytecode"
    );
    assert_eq!(
        v.as_number(),
        Some(1.0),
        "key must be coerced exactly once, got {v:?} calls"
    );
}

#[test]
fn compound_computed_assignment_captures_base_before_rhs_evaluation() {
    // `someMutator`'s own body contains a lexical declaration, which the
    // compiler doesn't support, so it can never itself compile — any chunk
    // execution observed here must come from the measured IIFE, not from a
    // helper that happens to compile regardless of whether the fix landed.
    let source = "
        var target = { i: 5 };
        var replaced = { i: 100 };
        var x = target;
        function someMutator() { let unused = 0; x = replaced; return unused + 1; }
        var __r = (function(k) {
            x[k] += someMutator();
            return target.i;
        })('i');
    ";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "compound computed assignment should compile to bytecode"
    );
    assert_eq!(
        v.as_number(),
        Some(6.0),
        "the base used for the final write must be the one captured before the RHS ran, got {v:?}"
    );
}

#[test]
fn compound_computed_assignment_base_and_key_survive_gc_during_rhs_evaluation() {
    let source = "
        var __r = (function(a) {
            a.arr[0] += a.rhs;
            return a.arr[0];
        })({
            arr: [10],
            get rhs() { $262.gc(); return 5; },
        });
    ";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "the containing function should compile to bytecode"
    );
    assert_eq!(
        v.as_number(),
        Some(15.0),
        "the pending base/key must survive the nested GC triggered while evaluating the RHS, got {v:?}"
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
        global_hints: vec![],
        prop_hints: vec![],
        var_names: vec![],
        max_stack: 1,
        max_refs: 0,
        ..Chunk::default()
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
        global_hints: vec![],
        prop_hints: vec![],
        var_names: vec![],
        max_stack: 0,
        max_refs: 0,
        ..Chunk::default()
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
        global_hints: vec![],
        prop_hints: vec![],
        var_names: vec![],
        max_stack: 2,
        max_refs: 0,
        ..Chunk::default()
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
        ExprBox::new(Expression::Literal(Literal::Number(2.0))),
        ExprBox::new(Expression::Literal(Literal::Number(3.0))),
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
        global_hints: vec![],
        prop_hints: vec![],
        var_names: vec![],
        max_stack: 2,
        max_refs: 0,
        ..Chunk::default()
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
    // if({}) → truthy → consequent. An object literal has no native lowering,
    // so it is evaluated by the tree-walker inside the compiled body.
    assert_parity_number(
        "var __r = (function(){ if ({}) return 1; else return 2; })();",
        1.0,
    );
}

#[test]
fn compile_body_if_lowers_via_vm_directly() {
    use crate::ast::{BinaryOp, IfStatement};
    // if (1 < 2) return 10; else return 20;
    let body = vec![Statement::If(IfStatement {
        test: Expression::Binary(
            BinaryOp::Lt,
            ExprBox::new(Expression::Literal(Literal::Number(1.0))),
            ExprBox::new(Expression::Literal(Literal::Number(2.0))),
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
fn if_with_throw_branch_compiles_and_throws() {
    use crate::ast::IfStatement;
    let body = vec![Statement::If(IfStatement {
        test: Expression::Literal(Literal::Boolean(true)),
        consequent: Box::new(Statement::Throw(Expression::Literal(Literal::Number(1.0)))),
        alternate: None,
    })];
    let chunk = compile_body(&body).expect("compile if with throw");
    match run(chunk) {
        Completion::Throw(value) => assert_eq!(value.as_number(), Some(1.0)),
        other => panic!("expected Throw(Number(1.0)), got {other:?}"),
    }
}

#[test]
fn if_with_throw_alternate_compiles_and_throws() {
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
    let chunk = compile_body(&body).expect("compile if/else with throw");
    match run(chunk) {
        Completion::Throw(value) => assert_eq!(value.as_number(), Some(2.0)),
        other => panic!("expected Throw(Number(2.0)), got {other:?}"),
    }
}

#[test]
fn compound_assign_on_member_compiles_to_bytecode() {
    let source = "var __r = (function(a){ a[0] += 1; return a[0]; })([1]);";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "compound assignment on a member target must now compile to bytecode"
    );
    assert_eq!(v.as_number(), Some(2.0));
}

#[test]
fn update_on_member_compiles_to_bytecode() {
    let source = "var __r = (function(a){ a[0]++; return a[0]; })([1]);";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "update expression on a member target must compile to bytecode"
    );
    assert_eq!(v.as_number(), Some(2.0));
}

#[test]
fn optional_member_access_bails_to_unsupported() {
    let source = "var __r = (function(a){ return a?.x; })({x: 3});";
    let (v, count) = eval_with_mode(source, true);
    assert_eq!(
        count, 1,
        "optional member access must bail to the tree-walker (only the script runs on the VM)"
    );
    assert_eq!(v.as_number(), Some(3.0));
}

#[test]
fn private_member_access_compiles_to_private_ops() {
    use crate::ast::{MemberProperty, PropSiteId};
    // `this.#x` lowers to `GetPrivate` over the chunk's private-name pool, so
    // a body of private accesses no longer hands off to the tree-walker.
    let body = vec![Statement::Return(Some(Expression::Member(
        ExprBox::new(Expression::This),
        MemberProperty::Private("x".to_string()),
        PropSiteId::UNASSIGNED,
    )))];
    let chunk = compile_body(&body).expect("private member access compiles");
    assert!(chunk.ast.is_empty(), "{:?}", chunk.fallback_reasons);
    assert_eq!(count_ops(&chunk, Op::GetPrivate), 1);
    assert_eq!(chunk.private_names, vec!["x".to_string()]);
}

#[test]
fn trivial_private_getter_shape_is_recognised() {
    use crate::ast::{BinaryOp, MemberProperty, PropSiteId};
    let read = || {
        Expression::Member(
            ExprBox::new(Expression::This),
            MemberProperty::Private("x".to_string()),
            PropSiteId::UNASSIGNED,
        )
    };
    let chunk = compile_body(&[Statement::Return(Some(read()))]).expect("compiles");
    assert_eq!(chunk.detect_trivial_private_getter(), Some(0));

    let sum = Expression::Binary(
        BinaryOp::Add,
        ExprBox::new(read()),
        ExprBox::new(Expression::Literal(Literal::Number(1.0))),
    );
    let chunk = compile_body(&[Statement::Return(Some(sum))]).expect("compiles");
    assert_eq!(chunk.detect_trivial_private_getter(), None);

    let chunk = compile_body(&[Statement::Expression(read())]).expect("compiles");
    assert_eq!(chunk.detect_trivial_private_getter(), None);
}

#[test]
fn private_member_forms_run_identically_on_both_engines() {
    for (source, expected) in [
        (
            "var __r = (function(){ class A { #a = 1; #b = 2; \
               run(n){ var s = 0; for (var i = 0; i < n; i++) { this.#a = this.#a + 1; s += this.#a + this.#b; this.#b += 1; this.#b++; --this.#a; } return s + ',' + this.#a + ',' + this.#b; } } \
               return new A().run(4); })();",
            "28,1,10",
        ),
        (
            "var __r = (function(){ class A { #m(x){ return this.#v + x; } #v = 5; get #g(){ return this.#v; } set #g(x){ this.#v = x; } \
               static has(o){ return #m in o; } \
               run(){ this.#g += 10; this.#g++; return this.#m(1) + ',' + A.has(this) + ',' + A.has({}); } } \
               return new A().run(); })();",
            "17,true,false",
        ),
        (
            "var __r = (function(){ class A { #x; static t(o){ try { return #x in o; } catch (e) { return e.constructor.name; } } } \
               return [A.t(1), A.t(new A()), A.t({})].join(); })();",
            "TypeError,true,false",
        ),
        (
            "var __r = (function(){ function make(){ return class { #p = 1; static get(o){ return o.#p; } }; } \
               var C = make(), D = make(); var r = C.get(new C()); try { D.get(new C()); } catch (e) { r += e.constructor.name; } return r; })();",
            "1TypeError",
        ),
    ] {
        assert_eq!(run_both_modes(source), expected, "{source}");
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
        global_hints: vec![],
        prop_hints: vec![],
        var_names: vec![],
        max_stack: 1,
        max_refs: 0,
        ..Chunk::default()
    };
    match run(chunk) {
        Completion::Return(value) if value.is_undefined() => {}
        other => panic!("expected Return(Undefined), got {other:?}"),
    }
}

// ----- this expression lowering -----

#[test]
fn this_expression_compiles_and_matches_tree_walker() {
    let source = "function f(){ 'use strict'; return this === undefined ? 1 : 2; } \
                  var __r = f();";
    assert_parity_number(source, 1.0);
}

#[test]
fn sloppy_bare_call_this_matches_tree_walker() {
    let source = "function f(){ return this === undefined ? 1 : 2; } var __r = f();";
    assert_parity_number(source, 2.0);
}

#[test]
fn derived_constructor_this_before_super_still_throws_reference_error() {
    let source = "class Base {} \
                  class Derived extends Base { constructor() { return this; } } \
                  var __r = 0; \
                  try { new Derived(); } \
                  catch (e) { __r = e instanceof ReferenceError ? 1 : 2; }";
    assert_parity_number(source, 1.0);
}

#[test]
fn this_expression_skips_with_object_shadow() {
    // The `with` statement itself bails the enclosing body to the tree-walker,
    // but the nested arrow's own body is still bytecode-eligible and closes
    // over the with-environment. `this` must resolve via ResolveThisBinding
    // (skip object Environment Records), never via ordinary identifier
    // resolution against the with-object's `this` property.
    let source = "function outer() { var f; \
                  with ({ this: 999 }) { f = () => (this === 999 ? 1 : 0); } \
                  return f(); } \
                  var __r = outer();";
    assert_parity_number(source, 0.0);
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
        let chunks_before = interp.bytecode_chunks_executed;
        let completion = interp.run(&redeclaration_program);

        let Completion::Normal(value) = completion else {
            panic!("expected normal Script completion, got {completion:?}");
        };
        assert_eq!(value.as_number(), Some(17.0));
        assert_eq!(
            interp.bytecode_chunks_executed - chunks_before,
            usize::from(bytecode)
        );
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
fn script_completion_composes_with_break_and_continue() {
    // WhileLoopEvaluation's UpdateEmpty(stmtResult, V) step: the break
    // completion's Value is non-empty (1, from the body's own last
    // completion), so UpdateEmpty leaves it as-is.
    assert_script_completion_number("while (true) { 1; break; }", 1.0);
    // Entering the while resets V to undefined (reset_script_completion);
    // the bare `break;`'s completion Value stays empty through the body, so
    // UpdateEmpty sets it to that reset V, overwriting the `1;` before the
    // loop.
    assert_script_completion_undefined("1; while (true) { break; }");
    // ForBodyEvaluation: continue still updates V from the body's last
    // completion (`i`) each iteration, then runs the update, before the
    // test ends the loop at i === 3 with V's last-set value (2).
    assert_script_completion_number("for (var i = 0; i < 3; i++) { i; continue; }", 2.0);
    // The inner loop's own reset sets its V to undefined before `break x`
    // fires; LabelledEvaluation only converts a break to Normal(Value) when
    // Target matches the label, which happens at the outer (labeled) loop,
    // carrying the inner loop's reset-to-undefined V out as the final value.
    assert_script_completion_undefined("x: while (true) { 5; while (true) { break x; } }");
    // DoWhileLoopEvaluation evaluates the body before its test and carries
    // the body's last value through a matching break or a false test.
    assert_script_completion_number("do { 4; break; } while (false);", 4.0);
    assert_script_completion_number("do { 3; } while (false);", 3.0);
    assert_script_completion_undefined("1; do { break; } while (false);");
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

fn assert_script_completion_parity(source: &str, chunks: usize) {
    let (tree, tree_count) = eval_script_completion_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_script_completion_with_mode(source, true);
    assert_eq!(tree_count, 0, "tree-walker mode ran bytecode for {source}");
    assert_eq!(
        bytecode_count, chunks,
        "Script Body ran an unexpected number of chunks for {source}"
    );
    assert_eq!(
        format!("{tree:?}"),
        format!("{bytecode:?}"),
        "completion differs for {source}"
    );
}

#[test]
fn script_handoff_statements_keep_completion_values() {
    for source in [
        "1; try { 2; } finally { 3; }",
        "1; try { 2; } catch (e) { 3; }",
        "1; try { throw 0; } catch (e) { 4; }",
        "1; switch (1) { case 1: 5; break; }",
        "1; switch (1) {}",
        "1; switch (2) { case 1: 5; }",
        "1; class A {}",
        "1; let x = 2;",
        "1; const y = 2; y;",
        "let z = 1; z + 1;",
        "1; function f() {} 2;",
        "1; function f() {}",
        "foo: { 7; break foo; }",
        "1; foo: { break foo; }",
        "1; foo: 2;",
        "while (true) { 1; foo: { 8; break foo; } break; }",
        "do { 4; try { 6; break; } finally {} } while (false);",
        "for (var i = 0; i < 2; i++) { 3; try { continue; } finally {} }",
        "for (var i = 0; i < 3; i++) { i; switch (i) { case 1: continue; } }",
        "x: for (var i = 0; i < 3; i++) { 5; for (var j of [1]) { 9; continue x; } }",
        "x: for (var i = 0; i < 3; i++) { 5; for (var j of [1]) { 9; break x; } }",
        "var o = { a: 1 }; for (var k in o) { k; }",
        "var n = 0; for (let i = 0; i < 3; i++) { n += i; i; }",
        "var n = 0; for (const i of [1, 2]) { n += i; }",
        "{ let a = 1; a; }",
        "1; { let a = 1; }",
        "1; { function g() {} }",
        "var r = 0; label: { r = 1; { 2; break label; } }",
        "1; if (true) { let a = 3; try { a; } finally {} }",
        "var a = 1; a; ;",
        "1; throw_free: try {} finally {}",
        "3; do { try { 5; continue; } finally {} } while (false);",
    ] {
        // A script that is nothing but hand-offs stays on the tree-walker;
        // with one native statement in front it compiles and must agree.
        let (_, whole) = eval_script_completion_with_mode(source, true);
        assert!(whole <= 1, "{source}");
        assert_script_completion_parity(source, whole);
        assert_script_completion_parity(&format!("0; {source}"), 1);
    }
}

#[test]
fn script_with_unsupported_nodes_compiles_instead_of_bailing() {
    use crate::parser::Parser;
    for source in [
        "var a = 1; class A {} let x = 1; const y = 2;",
        "var a = 1; function f() {} try { f(); } catch (e) {}",
        "var a = 1; switch (1) { case 1: break; }",
        "var a = 1; foo: { break foo; }",
        "var a = 1; for (var k in {}) {} for (var v of []) {}",
        "var a = [1, 2]; a?.length;",
    ] {
        let mut parser = Parser::new(source).expect("parser init");
        let program = parser.parse_program().expect("parse");
        compile_script_body(program.body.as_slice())
            .unwrap_or_else(|e| panic!("{source} must compile: {e:?}"));
    }
}

#[test]
fn script_that_is_only_handoffs_stays_on_the_tree_walker() {
    use crate::parser::Parser;
    let mut parser = Parser::new("class A {} let x = 1; foo: { break foo; }").expect("parser init");
    let program = parser.parse_program().expect("parse");
    match compile_script_body(program.body.as_slice()) {
        Err(e) => assert_eq!(e.reason(), "fallback-only body"),
        Ok(_) => panic!("a script with no native work must not compile"),
    }
}

#[test]
fn script_handoff_completion_value_is_rooted_across_nested_gc() {
    use crate::parser::Parser;

    let source = "var collect = $262.gc; try { Object(); } finally {} var ignored = collect();";
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
    assert!(interp.get_object(object_id).is_some());
    assert!(interp.gc_bytecode_roots.is_empty());
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
fn lexical_for_loop_runs_on_tree_walker_inside_compiled_body() {
    assert_parity_number(
        "var __r = (function(){ var sum = 0; for (let i = 0; i < 3; i++) sum += i; return sum; })();",
        3.0,
    );
}

#[test]
fn loop_with_break_takes_bytecode_path() {
    let source =
        "var __r = (function(){ var i = 0; while (true) { i++; if (i > 2) break; } return i; })();";
    let (value, count) = eval_with_mode(source, true);
    assert!(count >= 1, "break lowering must take the bytecode path");
    assert_eq!(value.as_number(), Some(3.0));
}

#[test]
fn labeled_literal_true_while_omits_repeated_test_dispatch() {
    use crate::ast::WhileStatement;

    let body = vec![Statement::Labeled(
        "loop".to_string(),
        Box::new(Statement::While(WhileStatement {
            test: Expression::Literal(Literal::Boolean(true)),
            body: Box::new(Statement::Break(Some("loop".to_string()))),
        })),
    )];
    let chunk = compile_body(&body).expect("compile labeled while(true)");
    assert_eq!(chunk.code[0], Op::Safepoint as u8);
    assert_eq!(
        chunk.code[1],
        Op::Jump as u8,
        "the loop body should start without LoadTrue and JumpIfFalse"
    );
    match run(chunk) {
        Completion::Return(value) if value.is_undefined() => {}
        other => panic!("expected Return(Undefined), got {other:?}"),
    }
}

#[test]
fn labeled_literal_true_for_omits_repeated_test_dispatch() {
    use crate::ast::ForStatement;

    let body = vec![Statement::Labeled(
        "loop".to_string(),
        Box::new(Statement::For(ForStatement {
            init: None,
            test: Some(Expression::Literal(Literal::Boolean(true))),
            update: None,
            body: Box::new(Statement::Break(Some("loop".to_string()))),
            scope_capture: Default::default(),
        })),
    )];
    let chunk = compile_body(&body).expect("compile labeled for(;true;)");
    assert_eq!(chunk.code[0], Op::Safepoint as u8);
    assert_eq!(
        chunk.code[1],
        Op::Jump as u8,
        "the loop body should start without LoadTrue and JumpIfFalse"
    );
    match run(chunk) {
        Completion::Return(value) if value.is_undefined() => {}
        other => panic!("expected Return(Undefined), got {other:?}"),
    }
}

#[test]
fn unlabeled_continue_in_while_retests_condition() {
    let source = "var __r = (function(){ var i = 0, n = 0; while (i < 5) { i++; if (i % 2 === 0) continue; n += i; } return n; })();";
    assert_parity_number(source, 9.0);
}

#[test]
fn unlabeled_break_and_continue_in_for_loop() {
    // continue skips n += i at i===2; break stops before n += i at i===5.
    let source = "var __r = (function(){ var n = 0; for (var i = 0; i < 10; i++) { if (i === 2) continue; if (i === 5) break; n += i; } return n; })();";
    assert_parity_number(source, 8.0);
}

#[test]
fn labeled_while_with_matching_break_and_continue() {
    let source = "var __r = (function(){ var n = 0; outer: while (true) { n++; if (n === 2) continue outer; if (n === 4) break outer; } return n; })();";
    assert_parity_number(source, 4.0);
}

#[test]
fn labeled_do_while_continue_evaluates_test_before_next_body() {
    let source = "var __r = (function(){ var i = 0, tests = 0; loop: do { i++; if (i < 3) continue loop; } while (++tests < 2); return i * 10 + tests; })();";
    assert_parity_number(source, 22.0);
}

#[test]
fn labeled_continue_reaches_past_inner_loop() {
    let source = "var __r = (function(){ var n = 0; outer: for (var i = 0; i < 3; i++) { for (var j = 0; j < 3; j++) { if (j === 1) continue outer; n++; } } return n; })();";
    assert_parity_number(source, 3.0);
}

#[test]
fn stacked_labels_on_one_loop_either_label_breaks_it() {
    let source_a = "var __r = (function(){ var n = 0; a: b: while (n < 5) { n++; if (n === 3) break a; } return n; })();";
    assert_parity_number(source_a, 3.0);

    let source_b = "var __r = (function(){ var n = 0; a: b: while (n < 5) { n++; if (n === 3) break b; } return n; })();";
    assert_parity_number(source_b, 3.0);
}

#[test]
fn labeled_non_loop_statement_runs_on_tree_walker_inside_compiled_body() {
    assert_parity_number(
        "var __r = (function(){ outer: { break outer; } return 1; })();",
        1.0,
    );
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
        bytecode_count, 3,
        "script, caller and callee all compile; the object literal is a tree-walker node"
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
    assert_eq!(
        bytecode_count, 2,
        "the script's immediately invoked function and its body compile; the native callee has no Body"
    );
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
         100-iteration hot loop; dispatch_body must thread current_ic \
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
fn vm_operand_stacks_are_pooled_and_reentrant() {
    use crate::parser::Parser;
    let source = "\
        function add(a, b) { return a + b; } \
        function viaNative(x) { return [x, x + 1, x + 2].map(function (v) { return add(v, 1); }); } \
        var total = 0; \
        for (var i = 0; i < 20; i++) { total += viaNative(i).length + add(i, i); } \
        var __r = total;";
    let mut p = Parser::new(source).expect("parser init");
    let program = p.parse_program().expect("parse");
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = true;
    let _ = interp.run(&program);
    let total = interp.get_global_var_ref("__r").and_then(|v| v.as_number());
    assert_eq!(total, Some((0..20).map(|i| 3.0 + 2.0 * i as f64).sum()));
    assert!(
        !interp.vm_stack_pool.is_empty(),
        "stacks must be returned to the pool"
    );
    assert!(interp.vm_stack_pool.len() <= 64);
    assert!(
        interp.vm_stack_pool.iter().all(|stack| stack.is_empty()),
        "pooled stacks must not keep operands alive"
    );
    assert!(interp.vm_ref_pool.iter().all(|refs| refs.is_empty()));
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
fn pending_construct_argument_survives_gc_during_later_argument() {
    // `new`-callee analog of `pending_argument_survives_gc_during_later_call_argument`:
    // `new Marker()`'s freshly-allocated, otherwise-unreferenced instance sits
    // pending as the first Construct argument while the second argument's own
    // call forces a GC before the outer Construct consumes it.
    let source = "\
        var collect = $262.gc; \
        var observed = 0; \
        function Marker() { this.marker = 42; } \
        function Combine(value, ignored) { observed = value.marker; } \
        function run() { new Combine(new Marker(), collect()); } \
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
fn pending_construct_callee_survives_gc_during_argument_evaluation() {
    // `new`-callee analog of `pending_with_getter_callee_survives_gc_during_argument`:
    // the getter returns a freshly-allocated, otherwise-unreferenced function
    // value that sits pending as the Construct callee while the argument's
    // own call forces a GC before the outer Construct consumes it.
    let source = "\
        var collect = $262.gc; \
        var observed = 0; \
        var factory = { \
            get Ctor() { \
                return function(ignored) { observed = 17; }; \
            } \
        }; \
        function run() { new (factory.Ctor)(collect()); } \
        run(); \
        var __r = observed;";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bytecode, bytecode_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert!(bytecode_count >= 1, "run must execute through bytecode");
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
fn direct_eval_ineligible_spread_ok() {
    use super::compiler::CompileError;
    use crate::ast::CallSiteId;

    let direct_eval = vec![Statement::Expression(Expression::Call(
        ExprBox::new(Expression::Identifier("eval".to_string())),
        vec![Expression::Literal(Literal::String(
            "var x = 1".encode_utf16().collect(),
        ))],
        CallSiteId::UNASSIGNED,
    ))];
    assert!(matches!(
        compile_body(&direct_eval),
        Err(CompileError::Ineligible(_))
    ));

    let spread = vec![Statement::Expression(Expression::Call(
        ExprBox::new(Expression::Identifier("f".to_string())),
        vec![Expression::Spread(ExprBox::new(Expression::Identifier(
            "args".to_string(),
        )))],
        CallSiteId::UNASSIGNED,
    ))];
    assert!(work_ops(&compile_body(&spread).unwrap()).contains(&Op::CallSpreadSole));
}

#[test]
fn unsupported_member_callees_remain_ineligible() {
    use super::compiler::CompileError;
    use crate::ast::{CallSiteId, MemberProperty, PropSiteId};

    let callee = Expression::Member(
        ExprBox::new(Expression::Super),
        MemberProperty::Dot("method".to_string()),
        PropSiteId::UNASSIGNED,
    );
    let call = vec![Statement::Expression(Expression::Call(
        ExprBox::new(callee),
        vec![],
        CallSiteId::UNASSIGNED,
    ))];
    assert!(matches!(
        compile_body(&call),
        Err(CompileError::Ineligible(_))
    ));
}

#[test]
fn nested_tail_positions_lower_to_return_call_leaves() {
    let count = |body: &str| {
        let ops = decode_ops(&compile_returning_body(body));
        (
            ops.iter().filter(|(op, _)| *op == Op::ReturnCall).count(),
            ops.iter().any(|(op, _)| *op == Op::ExecAst),
        )
    };
    assert_eq!(count("return a && f(b);"), (1, false));
    assert_eq!(count("return a || f(b);"), (1, false));
    assert_eq!(count("return a ?? f(b);"), (1, false));
    assert_eq!(count("return a ? f(b) : g(b);"), (2, false));
    assert_eq!(count("return a, f(b);"), (1, false));
    assert_eq!(count("return a ? (b && f(a)) : g(b);"), (2, false));
    assert_eq!(count("return a ? 1 : f(b);"), (1, false));
}

#[test]
fn nested_tail_call_leaf_compile_failures_keep_the_tree_walker() {
    for body in [
        "var x = 1; return a && f(...b);",
        "var x = 1; return a && g`t`;",
    ] {
        let ops = decode_ops(&compile_returning_body(body));
        assert!(
            ops.iter().any(|(op, _)| *op == Op::ExecAst),
            "{body}: {ops:?}"
        );
        assert!(
            !ops.iter().any(|(op, _)| *op == Op::ReturnCall),
            "{body}: {ops:?}"
        );
    }
}

#[test]
fn nested_tail_calls_stay_ordinary_calls_inside_cleanup_regions() {
    for body in [
        "try { return a && f(b); } catch (e) { return 0; }",
        "try { return a ? f(b) : g(b); } finally { b; }",
        "for (var x of a) { return b ? f(x) : x; }",
    ] {
        let ops = decode_ops(&compile_returning_body(body));
        assert!(
            !ops.iter().any(|(op, _)| *op == Op::ReturnCall),
            "{body}: {ops:?}"
        );
        assert!(
            !ops.iter().any(|(op, _)| *op == Op::ExecAst),
            "{body}: {ops:?}"
        );
    }
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
    // The Script's own loop issues five calls to outer(); outer()'s five
    // strict tail calls to leaf() come on top.
    assert_eq!(
        interp.perf.calls_from_vm, 10,
        "the five strict tail calls from outer()'s compiled body must be \
         counted beside the script's five calls, but calls_from_vm was {}",
        interp.perf.calls_from_vm
    );
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "released out of LIFO order")]
fn unrooting_a_non_top_bytecode_root_asserts() {
    let mut interp = Interpreter::new();
    let (first, second) = (interp.create_object_id(), interp.create_object_id());
    interp.gc_bytecode_roots.push(first);
    interp.gc_bytecode_roots.push(second);
    interp.gc_bytecode_roots.pop_expected(first);
}

fn run_bytecode_script(source: &str) -> (Completion, Interpreter) {
    use crate::parser::Parser;
    let mut parser = Parser::new(source).expect("parser init");
    let program = parser.parse_program().expect("parse");
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = true;
    let completion = interp.run(&program);
    (completion, interp)
}

#[test]
fn multi_statement_script_completion_stays_balanced_under_bytecode() {
    let source =
        "Object(); if (true) { Object(); } for (var i = 0; i < 2; i++) { Object(); } Object();";
    let (completion, interp) = run_bytecode_script(source);
    assert!(
        interp.bytecode_chunks_executed >= 1,
        "this source must run through the bytecode VM for the test to be meaningful"
    );
    assert!(
        matches!(completion, Completion::Normal(_)),
        "unexpected completion: {completion:?}"
    );
    assert!(
        interp.gc_bytecode_roots.is_empty(),
        "bytecode operand roots must be released at every statement boundary"
    );
}

#[test]
fn throw_with_live_outer_operand_does_not_trip_chunk_exit_assert() {
    // `compile_call` only accepts a bare `Identifier` callee, so an IIFE
    // callee (a `Function` expression) bails the whole script out of the
    // bytecode VM before this scenario can be exercised. `Object()` and
    // `decodeURIComponent` are both called by plain identifier, so this
    // compiles and runs on the VM: `Object()`'s result is left live on the
    // operand stack while `decodeURIComponent('%')` throws, reproducing
    // issue #331's "outer in-progress expression operand" case for real.
    let source = "var __r = Object() + decodeURIComponent('%');";
    let (completion, interp) = run_bytecode_script(source);
    assert!(
        interp.bytecode_chunks_executed >= 1,
        "this source must run through the bytecode VM for the test to be meaningful"
    );
    assert!(
        matches!(completion, Completion::Throw(_)),
        "unexpected completion: {completion:?}"
    );
    assert!(
        interp.gc_bytecode_roots.is_empty(),
        "the outer chunk's unconditional truncate must release the live \
         `Object()` operand left on the stack by `decodeURIComponent`'s abrupt throw"
    );
}

// --- Statement-boundary safepoints (issue #808) ---
//
// All of these assert *deltas* between two runs, not absolute counts:
// `Interpreter::new()` plus the entry path used can itself cross a safepoint
// before the chunk under test ever executes, so an absolute count is fragile
// to unrelated call-path changes. A delta between "N statements" and "N+1
// statements" (or "N iterations" and "N+1 iterations") isolates exactly the
// thing being tested.

fn literal_expr_stmt(n: f64) -> Statement {
    Statement::Expression(Expression::Literal(Literal::Number(n)))
}

fn literal_return_stmt(n: f64) -> Statement {
    Statement::Return(Some(Expression::Literal(Literal::Number(n))))
}

/// Compiles `body` as a function chunk and runs it on a fresh interpreter,
/// returning how many safepoints fired. A fresh `Interpreter::new()` plus
/// `run_chunk` (unlike `interp.run(&program)`) performs no declaration
/// instantiation of its own, so this count is exactly the safepoints the
/// chunk itself triggers.
fn run_body_count_safepoints(body: &[Statement]) -> u64 {
    let chunk = compile_body(body).expect("compile");
    let mut interp = Interpreter::new();
    let env = interp.realm().global_env.clone();
    let _ = run_chunk(&mut interp, &chunk, &env);
    interp.gc.safepoint_calls()
}

/// Shared by the top-level and nested-block variants below: each wraps the
/// same three-vs-four-statement bodies differently, but both must show that
/// one extra statement adds exactly one safepoint.
fn assert_one_statement_adds_one_safepoint(wrap: impl Fn(Vec<Statement>) -> Vec<Statement>) {
    let three = wrap(vec![
        literal_expr_stmt(1.0),
        literal_expr_stmt(2.0),
        literal_return_stmt(3.0),
    ]);
    let four = wrap(vec![
        literal_expr_stmt(1.0),
        literal_expr_stmt(2.0),
        literal_expr_stmt(3.0),
        literal_return_stmt(4.0),
    ]);
    let three_count = run_body_count_safepoints(&three);
    let four_count = run_body_count_safepoints(&four);
    assert_eq!(
        four_count - three_count,
        1,
        "one extra statement must add exactly one safepoint \
         (three={three_count}, four={four_count})"
    );
}

#[test]
fn statement_list_safepoint_count_scales_with_statement_count() {
    assert_one_statement_adds_one_safepoint(|body| body);
}

/// A statement that compiles to no bytecode (e.g. `;`) must not get its own
/// safepoint either — nothing could have been allocated since the prior one,
/// so emitting one would be a pure wasted VM dispatch.
#[test]
fn zero_byte_statement_adds_no_safepoint() {
    let without_empty = vec![literal_return_stmt(1.0)];
    let with_empty = vec![Statement::Empty, literal_return_stmt(1.0)];
    let without_count = run_body_count_safepoints(&without_empty);
    let with_count = run_body_count_safepoints(&with_empty);
    assert_eq!(
        with_count, without_count,
        "an empty statement must not add a safepoint of its own \
         (without={without_count}, with={with_count})"
    );
}

#[test]
fn nested_block_statement_list_safepoint_count_scales_with_statement_count() {
    // Wrapping in a block (rather than leaving the statements at top level)
    // proves Statement::Block has its own emission site in
    // `compile_statement_list`, not just the function/script top level.
    assert_one_statement_adds_one_safepoint(|body| vec![Statement::Block(body)]);
}

/// Runs `source` as a full script and returns its safepoint count, after
/// asserting the script body took the bytecode path (not a silent
/// tree-walker fallback, which would make the safepoint-count assertion
/// pass vacuously). Unlike `run_body_count_safepoints`, this goes through
/// the full `interp.run(&program)` pipeline.
fn run_script_safepoints(source: &str) -> u64 {
    let mut parser = crate::parser::Parser::new(source).expect("parser init");
    let program = parser.parse_program().expect("parse");
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = true;
    let completion = interp.run(&program);
    assert!(
        matches!(completion, Completion::Normal(_) | Completion::Empty),
        "{completion:?}"
    );
    assert_eq!(
        interp.bytecode_chunks_executed, 1,
        "script body must take the bytecode path"
    );
    interp.gc.safepoint_calls()
}

#[test]
fn script_body_statement_list_safepoint_count_scales_with_statement_count() {
    let three_count = run_script_safepoints("1; 2; 3;");
    let four_count = run_script_safepoints("1; 2; 3; 4;");
    assert_eq!(
        four_count - three_count,
        1,
        "one extra top-level script statement must add exactly one safepoint \
         (three={three_count}, four={four_count})"
    );
}

/// Guards the `tests/gc_stress.rs` `--bytecode` straight-line scenario
/// against silently falling back to the tree-walker: a top-level function
/// *declaration* makes the whole script body bail (`compile_statement`'s
/// catch-all), but the call sites and `make`'s own body are compiled and
/// executed separately, so this must still take the bytecode path.
#[test]
fn loop_free_allocating_calls_take_bytecode_path() {
    let source = "\
        function make(n) { var o = new Object(); o.n = n; return o; } \
        var results = []; \
        results.push(make(1).n); \
        results.push(make(2).n); \
        results.push(make(3).n); \
        results.push(make(4).n); \
        var __r = results.join(',');";
    let (v, count) = eval_with_mode(source, true);
    assert!(
        count >= 1,
        "straight-line allocating calls should compile make()'s body to bytecode"
    );
    assert_eq!(
        v.as_string().map(|s| s.to_string()).as_deref(),
        Some("1,2,3,4")
    );
}

/// A braced loop body is a `Statement::Block`, which gets its own
/// statement-boundary safepoint on top of the loop's back-edge one — two
/// safepoints per iteration, not one. Contrast with the unbraced body in
/// `single_statement_loop_body_adds_no_extra_safepoint_beyond_backedge` below.
#[test]
fn braced_single_statement_loop_body_adds_block_safepoint_per_iteration() {
    let three_count =
        run_script_safepoints("var sink = 0; for (var i = 0; i < 3; i++) { sink = i; }");
    let four_count =
        run_script_safepoints("var sink = 0; for (var i = 0; i < 4; i++) { sink = i; }");
    assert_eq!(
        four_count - three_count,
        2,
        "one extra iteration of a braced loop body must add two safepoints: \
         the block's own statement-boundary one plus the back-edge one \
         (three={three_count}, four={four_count})"
    );
}

#[test]
fn single_statement_loop_body_adds_no_extra_safepoint_beyond_backedge() {
    let three_count = run_script_safepoints("var sink = 0; for (var i = 0; i < 3; i++) sink = i;");
    let four_count = run_script_safepoints("var sink = 0; for (var i = 0; i < 4; i++) sink = i;");
    assert_eq!(
        four_count - three_count,
        1,
        "one extra loop iteration must add exactly one back-edge safepoint, \
         not an extra statement-boundary safepoint for the single-statement \
         loop body (three={three_count}, four={four_count})"
    );
}

#[test]
fn member_call_on_builtin_namespace_takes_bytecode_path() {
    assert_parity_number(
        "var __r = (function(o){ var s = 0; for (var i = 0; i < 4; i++) { s += Math.floor(o[i] / 2); } return s; })([1, 2, 3, 5]);",
        4.0,
    );
}

#[test]
fn member_call_binds_receiver_as_this() {
    assert_parity_number(
        "var o = { v: 7, m: function(a){ return this.v + a; } }; \
         var __r = (function(){ return o.m(1) + o['m'](2); })();",
        17.0,
    );
}

#[test]
fn member_call_on_primitive_base_uses_prototype_method() {
    assert_parity_number(
        "var __r = (function(){ return 'abc'.charCodeAt(1) + (5).toFixed(1).length; })();",
        101.0,
    );
}

#[test]
fn member_call_evaluates_base_key_get_then_arguments_in_order() {
    let source = "\
        var order = []; \
        var o = { get m(){ order.push('get'); return function(){ order.push('call'); }; } }; \
        var __r = (function(){ o[(order.push('key'), 'm')]((order.push('arg'), 1)); return order.join(); })();";
    let (ast, ast_count) = eval_with_mode(source, false);
    let (bc, bc_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0);
    assert!(bc_count >= 1, "member call must run on the VM");
    for (mode, v) in [("AST", ast), ("bytecode", bc)] {
        assert_eq!(
            v.as_string().map(|s| s.to_string()).as_deref(),
            Some("key,get,arg,call"),
            "{mode} evaluation order"
        );
    }
}

#[test]
fn member_call_on_nullish_base_throws_type_error() {
    let source = "var n = null; (function(){ n.x(); })();";
    let (completion, interp) = run_bytecode_script(source);
    assert!(interp.bytecode_chunks_executed >= 1, "must run on the VM");
    assert!(
        matches!(completion, Completion::Throw(_)),
        "unexpected completion: {completion:?}"
    );
    assert!(interp.gc_bytecode_roots.is_empty());
}

#[test]
fn member_call_releases_operand_roots() {
    let source = "var o = { m: function(a){ return a; } }; \
        (function(){ var s = 0; for (var i = 0; i < 8; i++) { s += o.m(i) + o['m'](1); } return s; })();";
    let (completion, interp) = run_bytecode_script(source);
    assert!(
        matches!(completion, Completion::Normal(_)),
        "unexpected completion: {completion:?}"
    );
    assert!(interp.bytecode_chunks_executed >= 1);
    assert!(interp.gc_bytecode_roots.is_empty());
}

#[test]
fn strict_member_call_in_tail_position_returns_through_trampoline() {
    assert_parity_number(
        "var o = { v: 3, m: function(a){ 'use strict'; return this.v + a; } }; \
         var __r = (function(){ 'use strict'; return o.m(4); })();",
        7.0,
    );
}

/// Byte widths of an op's operands, in order (1 = u8, 2 = u16/i16, 4 = u32).
fn operand_layout(op: Op) -> &'static [usize] {
    match op {
        Op::LoadConst
        | Op::LoadName
        | Op::ResolveName
        | Op::LoadResolvedName
        | Op::StoreResolvedName
        | Op::StoreResolvedNamePop
        | Op::GetProp
        | Op::SetProp
        | Op::LoadCalleeName
        | Op::LoadMethod
        | Op::Jump
        | Op::JumpIfFalse
        | Op::JumpIfTrue
        | Op::CaseStrictEq
        | Op::JumpIfTruthyKeep
        | Op::JumpIfFalsyKeep
        | Op::JumpIfNotNullishKeep
        | Op::ArgSpreadFirst => &[2],
        Op::UpdateName => &[2, 1],
        Op::LoadLocal | Op::StoreLocal | Op::StoreLocalPop => &[2, 2],
        Op::UpdateLocal => &[2, 2, 1],
        Op::Call | Op::ReturnCall | Op::Construct => &[2, 4],
        Op::DupN => &[1],
        Op::BinConst | Op::CmpJumpIfFalse => &[1, 2],
        Op::LocalBinConst => &[2, 2, 1, 2],
        Op::LocalLocalBin => &[2, 2, 2, 2, 1],
        Op::MoveLocal => &[2, 2, 2, 2],
        Op::LocalLocalCmpJump => &[2, 2, 2, 2, 1, 2],
        Op::LocalConstCmpJump => &[2, 2, 1, 2, 2],
        Op::PushEnv | Op::EvalAst => &[2],
        Op::PopEnv => &[1],
        Op::ExecAst => &[2, 2, 1],
        Op::InitLexical => &[2, 1, 2],
        Op::IterStep | Op::ForInStep | Op::EndFinally | Op::BindHead => &[2],
        Op::IterClose => &[1],
        Op::DropBelow => &[1, 1],
        Op::NewArrayConst
        | Op::NewArray
        | Op::ArrayAppend
        | Op::NewObject
        | Op::ObjectAppend
        | Op::NewRegExp => &[2],
        Op::MakeClosure => &[2, 1],
        Op::Yield => &[2, 1, 2],
        _ => &[],
    }
}

/// Decodes a whole chunk into `(op, operands)` through the operand-width
/// table; operand values are widened to u16 (a u32 keeps its low half).
pub(super) fn decode_ops(chunk: &Chunk) -> Vec<(Op, Vec<u16>)> {
    let mut out = Vec::new();
    let mut pc = 0;
    while pc < chunk.code.len() {
        let op = Op::from_u8(chunk.code[pc]).expect("valid opcode");
        pc += 1;
        let mut operands = Vec::new();
        for &width in operand_layout(op) {
            operands.push(match width {
                1 => u16::from(chunk.code[pc]),
                _ => u16::from(chunk.code[pc]) | (u16::from(chunk.code[pc + 1]) << 8),
            });
            pc += width;
        }
        out.push((op, operands));
    }
    out
}

fn compile_function_source(source: &str, params: &[&str]) -> Chunk {
    try_compile_function_source(source, params).expect("compile")
}

/// Compiles the body of `function f(a, b) { <body> }`, where `return` is legal.
fn compile_returning_body(body: &str) -> Chunk {
    use super::compiler::{FrameShape, compile_function_body};
    use crate::parser::Parser;

    let source = format!("function f(a, b) {{ {body}\n}}");
    let mut parser = Parser::new(&source).expect("parser init");
    let program = parser.parse_program().expect("parse");
    let Some(Statement::FunctionDeclaration(decl)) = program.body.as_slice().first() else {
        panic!("function declaration");
    };
    let params = vec!["a".to_string(), "b".to_string()];
    compile_function_body(
        decl.body.statements.as_slice(),
        &FrameShape {
            arguments_slot: false,
            simple_params: &params,
            separate_body_env: false,
        },
    )
    .expect("compile")
}

fn try_compile_function_source(
    source: &str,
    params: &[&str],
) -> Result<Chunk, super::compiler::CompileError> {
    use super::compiler::{FrameShape, compile_function_body};
    use crate::parser::Parser;

    let mut parser = Parser::new(source).expect("parser init");
    let program = parser.parse_program().expect("parse");
    let params: Vec<String> = params.iter().map(|p| p.to_string()).collect();
    compile_function_body(
        program.body.as_slice(),
        &FrameShape {
            arguments_slot: false,
            simple_params: &params,
            separate_body_env: false,
        },
    )
}

/// The chunk's ops without statement-boundary safepoints.
pub(super) fn work_ops(chunk: &Chunk) -> Vec<Op> {
    decode_ops(chunk)
        .into_iter()
        .map(|(op, _)| op)
        .filter(|op| *op != Op::Safepoint)
        .collect()
}

#[test]
fn function_body_compiles_frame_names_to_local_slots_and_free_names_by_name() {
    let chunk = compile_function_source("var c = a + g; g = c; c++; a = 1;", &["a"]);
    let name = |idx: u16| chunk.names[usize::from(idx)].as_str().to_string();
    let ops: Vec<(Op, Vec<String>)> = decode_ops(&chunk)
        .into_iter()
        .filter(|(op, _)| !matches!(op, Op::Safepoint | Op::Pop))
        .map(|(op, operands)| {
            let shown = match op {
                Op::LoadLocal | Op::StoreLocal | Op::StoreLocalPop => {
                    vec![name(operands[0]), operands[1].to_string()]
                }
                Op::UpdateLocal => vec![name(operands[0]), operands[1].to_string()],
                Op::LoadName
                | Op::ResolveName
                | Op::LoadResolvedName
                | Op::StoreResolvedName
                | Op::StoreResolvedNamePop => vec![name(operands[0])],
                _ => Vec::new(),
            };
            (op, shown)
        })
        .collect();
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        ops,
        vec![
            (Op::LoadLocal, s(&["a", "0"])),
            (Op::LoadName, s(&["g"])),
            (Op::Add, vec![]),
            (Op::StoreLocalPop, s(&["c", "1"])),
            (Op::ResolveName, s(&["g"])),
            (Op::LoadLocal, s(&["c", "1"])),
            (Op::StoreResolvedNamePop, s(&["g"])),
            (Op::UpdateLocal, s(&["c", "1"])),
            (Op::LoadConst, vec![]),
            (Op::StoreLocalPop, s(&["a", "0"])),
            (Op::ReturnUndefined, vec![]),
        ]
    );
}

#[test]
fn function_body_without_frame_names_keeps_by_name_ops() {
    let chunk = compile_function_source("g = h + 1;", &[]);
    let ops = work_ops(&chunk);
    assert!(ops.contains(&Op::ResolveName));
    assert!(ops.contains(&Op::StoreResolvedNamePop));
    assert!(!ops.contains(&Op::StoreLocal));
    assert!(!ops.contains(&Op::StoreLocalPop));
}

#[test]
fn statement_assignment_to_local_fuses_store_and_pop() {
    let chunk = compile_function_source("var c = a + 1; c = c + 2;", &["a"]);
    let ops = work_ops(&chunk);
    assert_eq!(ops.iter().filter(|op| **op == Op::StoreLocalPop).count(), 2);
    assert!(!ops.contains(&Op::Pop));
    assert!(!ops.contains(&Op::StoreLocal));
}

// ----- native throw / typeof / member update / instanceof / in / BigInt -----

/// Like `assert_message_parity` for any result: the compiled and
/// tree-walker runs print the same value and the compiled run executed a
/// chunk.
fn assert_value_parity(source: &str) -> String {
    assert_message_parity(source);
    format!("{}", eval_with_mode(source, true).0)
}

#[test]
fn throw_in_compiled_function_reaches_caller_catch() {
    let shown = assert_value_parity(
        "var __r; try { (function(n){ if (n < 0) throw new RangeError('neg'); return n; })(-1); } \
         catch (e) { __r = e.name + ':' + e.message; }",
    );
    assert_eq!(shown, "RangeError:neg");
}

#[test]
fn throw_of_primitive_value_is_preserved() {
    let shown = assert_value_parity(
        "var __r; try { (function(){ var a = 1; throw a + 41; })(); } catch (e) { __r = e; }",
    );
    assert_eq!(shown, "42");
}

#[test]
fn typeof_matches_tree_walker_for_each_result() {
    let shown = assert_value_parity(
        "var __r = (function(o){ var u; return [typeof u, typeof 1, typeof 's', typeof true, \
         typeof o, typeof null, typeof function(){}, typeof Symbol(), typeof 1n, \
         typeof undeclared_name, typeof o.missing].join(); })({});",
    );
    assert_eq!(
        shown,
        "undefined,number,string,boolean,object,object,function,symbol,bigint,undefined,undefined"
    );
}

#[test]
fn typeof_of_tdz_binding_throws_reference_error() {
    let shown = assert_value_parity(
        "var __r; (function(){ try { return (function(){ typeof x; })(); } catch (e) { __r = e.name; } })();",
    );
    assert_eq!(shown, "undefined");
}

#[test]
fn member_update_prefix_postfix_and_decrement() {
    let shown = assert_value_parity(
        "var __r = (function(o, k){ var r = []; r.push(o.x++); r.push(++o.x); r.push(o.x--); \
         r.push(--o.x); r.push(o[k]++); r.push(o[k]); r.push(--o[k]); return r.join() + '|' + o.x + o.y; })({x: 1, y: 5}, 'y');",
    );
    assert_eq!(shown, "1,3,3,1,5,6,5|15");
}

#[test]
fn member_update_evaluates_base_and_key_once() {
    let shown = assert_value_parity(
        "var __r = (function(){ var n = 0, m = 0, o = {a: 1}; \
         function base() { n++; return o; } function key() { m++; return 'a'; } \
         base().a++; base()[key()]--; ++base()[key()]; return [n, m, o.a].join(); })();",
    );
    assert_eq!(shown, "3,2,2");
}

#[test]
fn member_update_coerces_non_number_operands() {
    let shown = assert_value_parity(
        "var __r = (function(){ var o = {s: '5', n: null, v: {valueOf() { return 7; }}, b: 1n}; \
         var a = o.s++; var b = o.n++; var c = o.v++; var d = o.b++; \
         return [typeof a, a, b, c, d, o.s, o.n, o.v, o.b].join(); })();",
    );
    assert_eq!(shown, "number,5,0,7,1,6,1,8,2");
}

#[test]
fn member_update_runs_getter_and_setter_once_each() {
    let shown = assert_value_parity(
        "var __r = (function(){ var g = 0, s = 0, v = 10; \
         var o = {get x() { g++; return v; }, set x(n) { s++; v = n; }}; \
         var old = o.x++; return [old, v, g, s].join(); })();",
    );
    assert_eq!(shown, "10,11,1,1");
}

#[test]
fn member_update_on_primitive_base_matches_tree_walker() {
    let shown = assert_value_parity(
        "var __r = (function(){ var calls = 0; function f() { calls++; return 'abc'; } \
         var r = f().length++; return r + ',' + calls; })();",
    );
    assert_eq!(shown, "3,1");
}

#[test]
fn member_update_on_nullish_base_throws_same_message() {
    assert_message_parity(
        "var __r; try { (function(o){ o.x++; })(null); } catch (e) { __r = e.message; }",
    );
    assert_message_parity(
        "var __r; try { (function(o, k){ o[k]--; })(undefined, 0); } catch (e) { __r = e.message; }",
    );
}

#[test]
fn member_update_on_frozen_object_follows_strictness() {
    assert_value_parity(
        "var __r = (function(){ var o = Object.freeze({x: 1}); o.x++; return o.x; })();",
    );
    let shown = assert_value_parity(
        "var __r; try { (function(){ 'use strict'; var o = Object.freeze({x: 1}); o.x++; })(); } \
         catch (e) { __r = e.name; }",
    );
    assert_eq!(shown, "TypeError");
}

#[test]
fn instanceof_and_in_follow_spec_hooks() {
    let shown = assert_value_parity(
        "var __r = (function(){ function C() {} var c = new C(); \
         var hook = {[Symbol.hasInstance](v) { return v === 1; }}; \
         var bound = C.bind(null); \
         var seen = []; var p = new Proxy({}, {has(t, k) { seen.push(String(k)); return k === 'q'; }}); \
         return [c instanceof C, c instanceof Object, 1 instanceof hook, 2 instanceof hook, \
         c instanceof bound, 'q' in p, 'z' in p, 0 in [5], 1 in [5], seen.join('')].join(); })();",
    );
    assert_eq!(shown, "true,true,true,false,true,true,false,true,false,qz");
}

#[test]
fn instanceof_and_in_type_errors_match_tree_walker() {
    assert_message_parity(
        "var __r; try { (function(){ return ({}) instanceof 1; })(); } catch (e) { __r = e.message; }",
    );
    assert_message_parity(
        "var __r; try { (function(){ return ({}) instanceof ({}); })(); } catch (e) { __r = e.message; }",
    );
    assert_message_parity(
        "var __r; try { (function(){ return 'x' in 1; })(); } catch (e) { __r = e.message; }",
    );
}

#[test]
fn bigint_literals_compile_to_constants() {
    let shown = assert_value_parity(
        "var __r = (function(){ var a = 10n ** 5n; return [a, 0x1Fn, 0o7n, 0b101n, 1n + 2n, \
         typeof 0n].join(); })();",
    );
    assert_eq!(shown, "100000,31,7,5,3,bigint");
    assert_message_parity(
        "var __r; try { (function(){ return 1n + 1; })(); } catch (e) { __r = e.name; }",
    );
}

#[test]
fn regexp_literal_still_allocates_fresh_object_per_evaluation() {
    let shown = assert_value_parity(
        "var __r = (function(){ function mk() { return /a/g; } return mk() === mk(); })();",
    );
    assert_eq!(shown, "false");
}

// ----- native switch and function-top-level let/const -----

#[test]
fn switch_selector_order_fallthrough_and_default_placement() {
    let shown = assert_value_parity(
        "var __r = (function(n){ var log = '', r = ''; switch (n) { \
         case (log += 's1', 1): r += 'one'; \
         case (log += 's2', 2): r += 'two'; break; \
         default: r += 'dflt'; \
         case (log += 's3', 3): r += 'three'; break; \
         case (log += 's4', 4): r += 'four'; } return r + '|' + log; })(9);",
    );
    assert_eq!(shown, "dfltthree|s1s2s3s4");
}

#[test]
fn switch_compiles_without_a_tree_walker_hand_off() {
    use super::compiler::compile_body;
    use crate::parser::Parser;
    let mut parser = Parser::new("var r = 0; switch (r) { case 1: r = 2; break; default: r = 3; }")
        .expect("parser init");
    let program = parser.parse_program().expect("parse");
    let chunk = compile_body(program.body.as_slice()).expect("compile");
    assert!(
        chunk.ast.is_empty(),
        "switch must compile natively, got fallbacks {:?}",
        chunk.fallback_reasons
    );
}

#[test]
fn switch_case_tests_fuse_into_case_strict_eq() {
    let chunk = compile_function_source(
        "switch (a) { case 1: b = 1; break; case c: b = 2; break; default: b = 3; }",
        &["a", "b", "c"],
    );
    let ops = work_ops(&chunk);
    assert_eq!(ops.iter().filter(|op| **op == Op::CaseStrictEq).count(), 2);
    assert!(!ops.contains(&Op::DupN));
    assert!(!ops.contains(&Op::StrictEq));
    assert!(!ops.contains(&Op::JumpIfTrue));
}

#[test]
fn switch_break_and_continue_targets() {
    let shown = assert_value_parity(
        "var __r = (function(){ var out = ''; \
         outer: for (var i = 0; i < 5; i++) { \
           switch (i) { case 0: continue; case 1: out += 'a'; break; \
             case 2: out += 'b'; continue outer; \
             case 3: try { out += 't'; continue; } finally { out += 'f'; } \
             default: break outer; } \
           out += 'x' + i; } return out; })();",
    );
    assert_eq!(shown, "ax1btf");
}

#[test]
fn switch_with_lexical_case_body_falls_back_whole() {
    let shown = assert_value_parity(
        "var __r = (function(x){ var seen = typeof x; switch (x) { case 1: let y = 'one'; return y; \
         case 2: return typeof q; } return seen; })(1);",
    );
    assert_eq!(shown, "one");
}

#[test]
fn top_level_let_const_initialise_hoisted_tdz_bindings() {
    let shown = assert_value_parity(
        "var __r = (function(){ var seen = []; \
         try { seen.push(x); } catch (e) { seen.push(e.name); } \
         let x = 1; const y = x + 1; let z; \
         seen.push(x, y, typeof z); \
         try { y = 5; } catch (e) { seen.push(e.name); } \
         return seen.join(); })();",
    );
    assert_eq!(shown, "ReferenceError,1,2,undefined,TypeError");
}

#[test]
fn block_scoped_lexical_stays_in_its_block() {
    let shown = assert_value_parity(
        "var __r = (function(){ var x = 'outer'; { let x = 'inner'; } \
         for (let i = 0; i < 2; i++) { let x = i; } return x + typeof i; })();",
    );
    assert_eq!(shown, "outerundefined");
}

fn count_ops(chunk: &Chunk, wanted: Op) -> usize {
    work_ops(chunk).iter().filter(|op| **op == wanted).count()
}

#[test]
fn block_with_lexicals_runs_in_its_own_environment_without_a_hand_off() {
    let chunk = compile_function_source("{ let a = n + 1; const b = a * 2; s += b; }", &["n", "s"]);
    assert_eq!(count_ops(&chunk, Op::ExecAst), 0, "{:?}", work_ops(&chunk));
    assert_eq!(count_ops(&chunk, Op::PushEnv), 1);
    assert_eq!(count_ops(&chunk, Op::PopEnv), 1);
    assert_eq!(count_ops(&chunk, Op::InitLexical), 2);
    assert!(chunk.fallback_reasons.is_empty());
}

#[test]
fn non_capturing_for_let_keeps_one_environment_for_every_iteration() {
    let chunk = compile_function_source(
        "for (let i = 0; i < n; i++) { const v = i * 2; s += v; }",
        &["n", "s"],
    );
    assert_eq!(count_ops(&chunk, Op::ExecAst), 0, "{:?}", work_ops(&chunk));
    assert_eq!(count_ops(&chunk, Op::PushEnv), 2, "head and body");
    assert_eq!(count_ops(&chunk, Op::CopyIterEnv), 0);
}

#[test]
fn capturing_for_let_copies_its_environment_before_the_test_and_the_update() {
    let chunk = compile_function_source(
        "for (let i = 0; i < n; i++) { fs.push(function () { return i; }); }",
        &["n", "fs"],
    );
    assert_eq!(count_ops(&chunk, Op::ExecAst), 0, "{:?}", work_ops(&chunk));
    assert_eq!(count_ops(&chunk, Op::CopyIterEnv), 2);
    assert_eq!(count_ops(&chunk, Op::PushEnv), 1);
}

#[test]
fn local_ops_address_block_bindings_and_still_hit_the_frame_for_outer_names() {
    use super::chunk::BLOCK_SLOT_FLAG;
    let chunk = compile_function_source(
        "for (let i = 0; i < n; i++) { const v = i + 1; s = s + v; }",
        &["n", "s"],
    );
    let slots: Vec<(String, u16)> = decode_ops(&chunk)
        .into_iter()
        .filter(|(op, _)| {
            matches!(
                op,
                Op::LoadLocal
                    | Op::StoreLocal
                    | Op::StoreLocalPop
                    | Op::LocalLocalBin
                    | Op::MoveLocal
            )
        })
        .flat_map(|(op, operands)| {
            let refs = if matches!(op, Op::LocalLocalBin | Op::MoveLocal) {
                2
            } else {
                1
            };
            operands
                .chunks(2)
                .take(refs)
                .map(|pair| (chunk.names[usize::from(pair[0])].to_string(), pair[1]))
                .collect::<Vec<_>>()
        })
        .collect();
    let slot_of = |name: &str| {
        slots
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no local op names {name}: {slots:?}"))
            .1
    };
    assert_eq!(slot_of("s") & BLOCK_SLOT_FLAG, 0, "frame local stays frame");
    assert_eq!(slot_of("v") & BLOCK_SLOT_FLAG, BLOCK_SLOT_FLAG);
    assert_eq!(
        slot_of("v") >> 8,
        0x81,
        "the body block is the second scope"
    );
}

#[test]
fn break_out_of_nested_blocks_leaves_their_environments_first() {
    let chunk = compile_function_source(
        "for (let i = 0; i < n; i++) { let a = i; { let b = a; if (b > 2) break; } }",
        &["n"],
    );
    let pops: Vec<u16> = decode_ops(&chunk)
        .into_iter()
        .filter(|(op, _)| *op == Op::PopEnv)
        .map(|(_, operands)| operands[0])
        .collect();
    assert!(
        pops.contains(&2),
        "the break leaves the two blocks it is nested in: {pops:?}"
    );
}

#[test]
fn block_function_declaration_hoists_at_block_entry() {
    let chunk = compile_function_source("{ function g() { return 1; } r = g(); }", &["r"]);
    assert_eq!(chunk.block_scopes.len(), 1);
    assert_eq!(chunk.block_scopes[0].functions.len(), 1);
}

#[test]
fn closures_capture_a_fresh_for_let_binding_per_iteration() {
    let shown = assert_value_parity(
        "var __r = (function(){ var fs = []; \
         for (let i = 0; i < 3; i++) { fs.push(function () { return i; }); } \
         return fs.map(function (f) { return f(); }).join(); })();",
    );
    assert_eq!(shown, "0,1,2");
}

#[test]
fn closure_created_in_for_let_update_sees_the_next_iteration_copy() {
    let shown = assert_value_parity(
        "var __r = (function(){ var fs = []; \
         for (let i = 0; i < 3; fs.push(function () { return i; }), i++) {} \
         return fs.map(function (f) { return f(); }).join(); })();",
    );
    assert_eq!(shown, "1,2,3");
}

#[test]
fn closure_created_in_for_let_initializer_keeps_the_head_binding() {
    let shown = assert_value_parity(
        "var __r = (function(){ var seen = []; \
         for (let i = 0, g = function () { return i; }; i < 2; i++) { seen.push(g()); } \
         return seen.join(); })();",
    );
    assert_eq!(shown, "0,0");
}

#[test]
fn block_lexical_is_fresh_per_execution_and_closures_keep_their_own() {
    let shown = assert_value_parity(
        "var __r = (function(){ var fs = []; \
         for (var i = 0; i < 3; i++) { let c = i * 10; fs.push(function () { return ++c; }); } \
         return fs.map(function (f) { return f() + f(); }).join(); })();",
    );
    assert_eq!(shown, "3,23,43");
}

#[test]
fn block_tdz_read_and_write_throw_reference_error() {
    let shown = assert_value_parity(
        "var __r = (function(){ var seen = []; { \
         try { seen.push(x); } catch (e) { seen.push(e.name); } \
         try { x = 1; } catch (e) { seen.push(e.name); } \
         try { seen.push(typeof x); } catch (e) { seen.push(e.name); } \
         let x = 2; seen.push(x); } return seen.join(); })();",
    );
    assert_eq!(shown, "ReferenceError,ReferenceError,ReferenceError,2");
}

#[test]
fn for_let_head_initializer_reading_its_own_binding_throws() {
    let shown = assert_value_parity(
        "var __r = (function(){ try { for (let i = i; ;) { break; } } \
         catch (e) { return e.name; } return 'none'; })();",
    );
    assert_eq!(shown, "ReferenceError");
}

#[test]
fn block_lexical_shadows_a_parameter_only_inside_the_block() {
    let shown = assert_value_parity(
        "var __r = (function(a){ var r = []; { let a = 10; a++; r.push(a); } \
         for (let a = 0; a < 2; a++) { r.push(a); } r.push(a); return r.join(); })(7);",
    );
    assert_eq!(shown, "11,0,1,7");
}

#[test]
fn labeled_break_and_continue_leave_nested_let_blocks() {
    let shown = assert_value_parity(
        "var __r = (function(){ var r = []; \
         outer: for (let i = 0; i < 4; i++) { \
           for (let j = 0; j < 4; j++) { let k = i * 10 + j; \
             if (j == 1) continue outer; if (i == 3) break outer; r.push(k); } } \
         return r.join(); })();",
    );
    assert_eq!(shown, "0,10,20");
}

#[test]
fn hand_off_break_and_continue_unwind_block_environments() {
    let shown = assert_value_parity(
        "var __r = (function(){ var r = []; \
         for (let i = 0; i < 5; i++) { let x = i; \
           try { if (i == 1) continue; if (i == 3) break; r.push(x); } finally { r.push('f'); } } \
         var late = 0; for (let k = 0; k < 2; k++) { late += k; } \
         return r.join() + ':' + late; })();",
    );
    assert_eq!(shown, "0,f,f,2,f,f:1");
}

#[test]
fn return_and_throw_from_inside_block_environments_leave_the_frame_clean() {
    let shown = assert_value_parity(
        "var __r = (function(){ \
         function f(n) { for (let i = 0; i < 5; i++) { let q = i + n; \
           if (q > 3) { const z = q * 2; return z; } if (q < 0) throw new Error('neg' + q); } return -1; } \
         var out = [f(0), f(1), f(10)]; \
         try { f(-5); } catch (e) { out.push(e.message); } \
         out.push(f(0)); return out.join(); })();",
    );
    assert_eq!(shown, "8,8,20,neg-5,8");
}

#[test]
fn switch_case_block_scope_has_tdz_and_runs_selectors_inside_it() {
    let shown = assert_value_parity(
        "var __r = (function(){ var r = []; \
         function pick(v) { switch (v) { \
           case 1: let a = 'one'; r.push(a); \
           case 2: try { r.push(a); } catch (e) { r.push(e.name); } break; \
           case 3: { const b = 'three'; r.push(b); } } } \
         pick(1); pick(2); pick(3); return r.join(); })();",
    );
    assert_eq!(shown, "one,one,ReferenceError,three");
}

#[test]
fn block_class_and_destructuring_declarations_bind_in_the_block() {
    let shown = assert_value_parity(
        "var __r = (function(o){ var r = []; { class A { m() { return 'm'; } } \
         const { p, q } = o; let [x, y] = [p + 1, q + 1]; r.push(new A().m(), x + y); } \
         r.push(typeof A, typeof p); return r.join(); })({ p: 1, q: 2 });",
    );
    assert_eq!(shown, "m,5,undefined,undefined");
}

#[test]
fn block_function_declarations_are_hoisted_and_annex_b_copied() {
    let shown = assert_value_parity(
        "var __r = (function(){ var r = []; \
         { r.push(typeof g); function g() { return 1; } r.push(g()); } \
         r.push(typeof g); return r.join(); })();",
    );
    assert_eq!(shown, "function,1,function");
}

#[test]
fn block_scoped_const_assignment_throws_type_error() {
    let shown = assert_value_parity(
        "var __r = (function(){ for (const i = 0; i < 1;) { try { i++; } \
         catch (e) { return e.name; } } return 'none'; })();",
    );
    assert_eq!(shown, "TypeError");
}

#[test]
fn deeply_nested_blocks_beyond_the_slot_encoding_still_resolve() {
    let mut open = String::new();
    let mut close = String::new();
    for i in 0..140 {
        open.push_str(&format!("{{ let v{i} = {i}; "));
        close.push_str(" }");
    }
    let source = format!(
        "var __r = (function(){{ var r = 0; {open} r = v0 + v139; {close} return r; }})();"
    );
    let shown = assert_value_parity(&source);
    assert_eq!(shown, "139");
}

#[test]
fn many_bindings_in_one_block_beyond_the_slot_encoding_still_resolve() {
    let mut decls = String::new();
    for i in 0..300 {
        decls.push_str(&format!("let v{i} = {i}; "));
    }
    let source =
        format!("var __r = (function(){{ {{ {decls} return v0 + v255 + v256 + v299; }} }})();");
    let shown = assert_value_parity(&source);
    assert_eq!(shown, "810");
}

#[test]
fn jump_offset_overflow_in_one_statement_falls_back_for_that_statement_only() {
    // A switch whose body exceeds the 16-bit jump range must not make the
    // whole function ineligible: that statement alone runs on the tree-walker.
    let mut body = String::new();
    for i in 0..4000 {
        body.push_str(&format!(
            "case {i}: r += {i}; r += 1; r += 2; r += 3; break;"
        ));
    }
    let source =
        format!("var __r = (function(n){{ var r = 0; switch (n) {{ {body} }} return r; }})(3999);");
    let (v, count) = eval_with_mode(&source, true);
    let (ast_v, _) = eval_with_mode(&source, false);
    assert!(count >= 1, "function must still compile to bytecode");
    assert_eq!(v.as_number(), ast_v.as_number());
    assert_eq!(v.as_number(), Some(3999.0 + 6.0));
}

#[test]
fn statement_assignment_to_free_name_resolves_before_rhs() {
    let chunk = compile_function_source("g = f();", &[]);
    assert_eq!(
        work_ops(&chunk),
        vec![
            Op::ResolveName,
            Op::LoadCalleeName,
            Op::Call,
            Op::StoreResolvedNamePop,
            Op::ReturnUndefined,
        ]
    );
}

#[test]
fn value_context_assignment_keeps_unfused_store() {
    let chunk = compile_function_source("var c = 0; var y = (c = 1);", &["a"]);
    let ops = work_ops(&chunk);
    assert!(
        ops.contains(&Op::StoreLocal),
        "inner c = 1 pushes its value"
    );
    assert!(ops.contains(&Op::StoreLocalPop), "the declarator discards");
}

#[test]
fn script_goal_expression_statement_keeps_completion_value() {
    use crate::parser::Parser;
    let mut parser = Parser::new("x = 1;").expect("parser init");
    let program = parser.parse_program().expect("parse");
    let chunk = compile_script_body(program.body.as_slice()).expect("compile");
    let ops = work_ops(&chunk);
    assert!(ops.contains(&Op::SetCompletion));
    assert!(ops.contains(&Op::StoreResolvedName));
    assert!(!ops.contains(&Op::StoreResolvedNamePop));
}

#[test]
fn comma_statement_discards_each_element() {
    let chunk = compile_function_source("a = 1, b = 2, a + b;", &["a", "b"]);
    let ops = work_ops(&chunk);
    assert_eq!(ops.iter().filter(|op| **op == Op::StoreLocalPop).count(), 2);
    assert_eq!(ops.iter().filter(|op| **op == Op::Pop).count(), 1);
}

#[test]
fn statement_update_of_local_discards_its_result() {
    let chunk = compile_function_source("i++; --i; var j = i++;", &["i"]);
    let modes: Vec<u16> = decode_ops(&chunk)
        .into_iter()
        .filter(|(op, _)| *op == Op::UpdateLocal)
        .map(|(_, operands)| operands[2])
        .collect();
    // i++ (post inc, discard), --i (pre dec, discard), i++ in value context.
    assert_eq!(modes, vec![0b100, 0b111, 0]);
    assert!(!work_ops(&chunk).contains(&Op::Pop));
}

#[test]
fn for_loop_update_has_no_pop() {
    let chunk = compile_function_source("for (var i = 0; i < n; i++) {}", &["n"]);
    let ops = work_ops(&chunk);
    assert!(!ops.contains(&Op::Pop));
    assert!(ops.contains(&Op::LocalLocalCmpJump));
}

#[test]
fn binary_with_number_literal_rhs_folds_the_constant() {
    let chunk = compile_function_source("var r = x | 0; var s = y & 65535;", &["y"]);
    let ops = work_ops(&chunk);
    assert!(ops.contains(&Op::BinConst), "free name lhs");
    assert!(ops.contains(&Op::LocalBinConst), "frame local lhs");
    assert!(!ops.contains(&Op::LoadConst));
    assert!(!ops.contains(&Op::BitOr) && !ops.contains(&Op::BitAnd));
}

#[test]
fn compound_assignment_with_number_literal_folds_the_constant() {
    let chunk = compile_function_source("c += 1; g *= 2; c -= d;", &["c", "d"]);
    assert_eq!(
        work_ops(&chunk),
        vec![
            Op::LocalBinConst,
            Op::StoreLocalPop,
            Op::ResolveName,
            Op::LoadResolvedName,
            Op::BinConst,
            Op::StoreResolvedNamePop,
            Op::LocalLocalBin,
            Op::StoreLocalPop,
            Op::ReturnUndefined,
        ]
    );
}

#[test]
fn binary_with_non_literal_rhs_stays_generic() {
    let chunk = compile_function_source("var r = x + 's'; var t = x + y; var u = x - -1;", &[]);
    let ops = work_ops(&chunk);
    assert!(!ops.contains(&Op::BinConst));
    assert!(!ops.contains(&Op::LocalBinConst));
    assert!(ops.contains(&Op::Add));
    assert!(ops.contains(&Op::Sub));
}

#[test]
fn comparison_in_branch_compiles_to_cmp_jump() {
    for source in [
        "if (a < b) { c = 1; }",
        "while (a < b) { c = 1; }",
        "for (;a <= b;) { c = 1; }",
        "do { c = 1; } while (a > b);",
        "c = a >= b ? 1 : 2;",
        "if (a === b) { c = 1; }",
    ] {
        let chunk = compile_function_source(source, &[]);
        let ops = work_ops(&chunk);
        assert!(ops.contains(&Op::CmpJumpIfFalse), "{source}");
        assert!(!ops.contains(&Op::JumpIfFalse), "{source}");
    }
}

#[test]
fn comparison_in_value_context_stays_unfused() {
    let chunk = compile_function_source("var t = a < b;", &[]);
    let ops = work_ops(&chunk);
    assert!(ops.contains(&Op::Lt));
    assert!(!ops.contains(&Op::CmpJumpIfFalse));
}

#[test]
fn local_compared_with_literal_branches_through_local_const_cmp_jump() {
    let chunk = compile_function_source("for (var i = 0; i < 10; i++) {}", &[]);
    let ops = work_ops(&chunk);
    assert!(ops.contains(&Op::LocalConstCmpJump));
    assert!(!ops.contains(&Op::JumpIfFalse));
    assert!(!ops.contains(&Op::CmpJumpIfFalse));
}

#[test]
fn local_pairs_fuse_into_one_op() {
    let chunk = compile_function_source(
        "var t = a + b; c = a; if (a < b) { c = 1; }",
        &["a", "b", "c"],
    );
    let ops = work_ops(&chunk);
    assert!(ops.contains(&Op::LocalLocalBin));
    assert!(ops.contains(&Op::MoveLocal));
    assert!(ops.contains(&Op::LocalLocalCmpJump));
    assert!(!ops.contains(&Op::Add) && !ops.contains(&Op::Lt));
    let chunk = compile_function_source("var u = a instanceof b; var w = a in b;", &["a", "b"]);
    let ops = work_ops(&chunk);
    assert!(!ops.contains(&Op::LocalLocalBin));
}

/// The `arith` loop of `benchmarks/scripts/bench_opmix.js` must stay within
/// 19 dispatched ops per iteration (issue #903: 43 before fusion; #1030: 25
/// before the local-pair ops).
#[test]
fn opmix_arith_loop_body_is_at_most_19_ops() {
    let chunk = compile_function_source(
        "var r0 = 0, r1 = 1, r2 = 2;
        for (var i = 0; i < n; i++) {
            r0 = (r0 + 4) | 0;
            r1 = (r0 + r1) | 0;
            r1 = r1 & 65535;
            r2 = r1 >> 2;
            r0 = (r2 + r0) | 0;
            r0 = r0 & 65535;
        }",
        &["n"],
    );
    let decoded = decode_ops(&chunk);
    let loop_start = decoded
        .iter()
        .position(|(op, _)| *op == Op::LocalLocalCmpJump)
        .expect("loop test");
    let back_jump = decoded
        .iter()
        .rposition(|(op, _)| *op == Op::Jump)
        .expect("back edge");
    let per_iteration = decoded[loop_start..=back_jump]
        .iter()
        .filter(|(op, _)| *op != Op::Safepoint)
        .count();
    assert!(
        per_iteration <= 19,
        "arith loop dispatches {per_iteration} ops per iteration"
    );
}

#[test]
fn effect_context_forms_match_tree_walker() {
    assert_parity_number(
        "var __r = (function(a, b){ a += b; a -= 1; a <<= 2; b = a, a = b + 1, a++; return a + b; })(5, 3);",
        {
            let (mut a, mut b) = (5i32, 3i32);
            a += b;
            a -= 1;
            a <<= 2;
            b = a;
            a = b + 1;
            a += 1;
            f64::from(a + b)
        },
    );
    assert_parity_number(
        "var __r = (function(n){ var s = 0; for (var i = 0, j = 10; i < n; i = i + 2, j--) { s = s + i * j; } return s; })(9);",
        (0..9)
            .step_by(2)
            .enumerate()
            .map(|(k, i)| (i * (10 - k)) as f64)
            .sum::<f64>(),
    );
}

#[test]
fn update_discard_coerces_like_value_context() {
    // Objects and BigInts are not compilable literals, so the function reads
    // them as free names.
    assert_parity_number(
        "var o = { n: 0, valueOf() { this.n++; return 3; } }; var big = 1n; \
         var __r = (function(){ var s = '5'; s++; o++; big++; big--; return s + o + o * 0; })() + o * 0 + 0;",
        // s -> 6, o -> 4 (valueOf ran once), big stays 1n: 6 + 4 + 0.
        10.0,
    );
}

#[test]
fn number_binary_ops_match_tree_walker_on_edge_values() {
    let ops = [
        "+", "-", "*", "/", "%", "**", "<<", ">>", ">>>", "&", "|", "^", "<", ">", "<=", ">=",
        "==", "!=", "===", "!==",
    ];
    let values = [
        "0",
        "-0",
        "1",
        "-1",
        "2147483648",
        "4294967296",
        "1e21",
        "NaN",
        "Infinity",
        "0.5",
    ];
    for op in ops {
        let mut body = String::from("var acc = ''; ");
        for l in values {
            for r in values {
                body.push_str(&format!(
                    "var v = ({l}) {op} ({r}); \
                     var w = (function(a, b){{ return a {op} b; }})({l}, {r}); \
                     acc += (Object.is(v, w) ? '' : '!') + (Object.is(v, -0) ? '-0' : String(v)) + ','; "
                ));
            }
        }
        let source = format!("var __r = (function(){{ {body} return acc; }})();");
        let (ast_v, _) = eval_with_mode(&source, false);
        let (bc_v, bc_count) = eval_with_mode(&source, true);
        assert!(bc_count >= 1, "{op}: must run on the VM");
        let text = |v: &JsValue| v.as_string().map(|s| s.to_string());
        let (ast, bc) = (text(&ast_v), text(&bc_v));
        assert!(ast.is_some(), "{op}: tree-walker produced no string");
        assert_eq!(ast, bc, "{op}");
        assert!(
            !bc.unwrap().contains('!'),
            "{op}: literal-folded and call forms disagreed"
        );
    }
}

#[test]
fn compare_branches_match_tree_walker_on_mixed_operands() {
    let source = "var __r = (function(){
        var log = '';
        var o = { valueOf() { log += 'o'; return 2; } };
        var p = { valueOf() { log += 'p'; return 1; } };
        var n = 0;
        if (o > p) n += 1;
        if (o < p) n += 10;
        if (o <= p) n += 100;
        if (o >= p) n += 1000;
        if (NaN < 1) n += 10000;
        if (!(NaN >= 1)) n += 100000;
        if ('10' < 9) n += 1000000;
        if (1n < 2) n += 10000000;
        if (null == undefined) n += 100000000;
        return n + log.length;
    })();";
    let (ast_v, _) = eval_with_mode(source, false);
    let (bc_v, bc_count) = eval_with_mode(source, true);
    assert!(bc_count >= 1);
    assert_eq!(ast_v.as_number(), bc_v.as_number());
    assert_eq!(bc_v.as_number(), Some(110101001.0 + 8.0));
}

/// An operand byte can equal a return opcode (a slot or name index in 512..1024
/// has a high byte of 2 or 3); the trailing-return check must look at the last
/// *opcode*, not the last byte (issue #903 review).
#[test]
fn wide_operand_final_statement_still_ends_in_return() {
    for target in [512usize, 700, 800, 1023] {
        let mut source = String::new();
        for i in 0..=target {
            source.push_str(&format!("var v{i}; "));
        }
        source.push_str(&format!("v{target} = 1;"));
        let chunk = compile_function_source(&source, &[]);
        assert_eq!(
            work_ops(&chunk).last(),
            Some(&Op::ReturnUndefined),
            "slot {target}"
        );
        let free = format!("g{target} = 1;");
        let mut names = String::new();
        for i in 0..target {
            names.push_str(&format!("g{i} = 0; "));
        }
        let chunk = compile_function_source(&format!("{names}{free}"), &[]);
        assert_eq!(
            work_ops(&chunk).last(),
            Some(&Op::ReturnUndefined),
            "name {target}"
        );
    }
}

/// Evaluates `source` in both modes and requires the same string result, with
/// the bytecode run executing at least one chunk.
fn assert_parity_string(source: &str, expected: &str) {
    let (ast_v, ast_count) = eval_with_mode(source, false);
    let (bc_v, bc_count) = eval_with_mode(source, true);
    assert_eq!(ast_count, 0, "{source}: AST mode must not run chunks");
    assert!(bc_count >= 1, "{source}: bytecode path must run a chunk");
    let text = |v: &JsValue| v.as_string().map(|s| s.to_string());
    assert_eq!(text(&ast_v).as_deref(), Some(expected), "{source}: AST");
    assert_eq!(text(&bc_v).as_deref(), Some(expected), "{source}: bytecode");
}

#[test]
fn literals_and_closures_compile_without_tree_walker_hand_offs() {
    let chunk = compile_function_source(
        "var a = [1, 2, 3]; var b = [x, y + 1]; var o = { p: x, q: 2, r: function () {} };
         var f = function () { return x; }; var g = () => y; var re = /ab/g; var h = [1, , 3];
         [a, b, o, f, g, re, h];",
        &["x", "y"],
    );
    assert!(chunk.ast.is_empty(), "no node left on the tree-walker");
    let ops = work_ops(&chunk);
    for op in [
        Op::NewArrayConst,
        Op::NewArray,
        Op::NewObject,
        Op::MakeClosure,
        Op::NewRegExp,
    ] {
        assert!(ops.contains(&op), "missing {op:?} in {ops:?}");
    }
    assert!(!ops.contains(&Op::EvalAst) && !ops.contains(&Op::ExecAst));
}

#[test]
fn await_inside_a_nested_async_arrow_does_not_keep_the_body_whole() {
    let chunk = compile_function_source(
        "var total = 0;
         for (let datum of data) total += datum;
         var later = async () => { let s = await data; for await (let d of s) total += d; };
         total;",
        &["data"],
    );
    assert!(chunk.ast.is_empty(), "no node left on the tree-walker");
    assert!(work_ops(&chunk).contains(&Op::MakeClosure));
}

#[test]
fn inherited_constructs_in_a_nested_async_arrow_still_keep_the_body_whole() {
    use super::compiler::CompileError;

    for source in [
        "var later = async () => { await 1; return eval('1'); };",
        "var later = async () => { await 1; return import('x'); };",
    ] {
        assert!(
            matches!(
                try_compile_function_source(source, &[]),
                Err(CompileError::Ineligible(_))
            ),
            "{source}: expected Ineligible"
        );
    }
}

#[test]
fn unsupported_literal_forms_hand_off_the_whole_literal() {
    for (source, reason) in [
        ("[...x];", "array literal: spread"),
        ("[x, , y];", "array literal: elision"),
        ("({ ...x });", "object literal: spread"),
        ("({ [x]: 1 });", "object literal: computed key"),
        ("(class {});", "expression:Class"),
    ] {
        let chunk = compile_function_source(&format!("var pad = 0; {source}"), &["x", "y"]);
        assert_eq!(chunk.fallback_reasons, vec![reason], "{source}");
    }
}

#[test]
fn array_literal_forms_match_tree_walker() {
    assert_parity_string(
        "var __r = (function (v) {
            var u = [undefined, v]; var h = [1, , 3]; var n = [-1, -0, 1.5, 'a', true, null, 7n];
            var m = [v, [v], { a: v }];
            return [0 in [undefined], 0 in [,], 1 in h, h.length, Object.keys(u).join(), 1 / n[1],
                    typeof n[6], m[1][0], m[2].a, [].length, [,].length].join();
        })(9);",
        "true,false,false,3,0,1,-Infinity,bigint,9,9,0,1",
    );
}

#[test]
fn array_literal_is_fresh_per_evaluation() {
    assert_parity_string(
        "var __r = (function () {
            var seen = [];
            for (var i = 0; i < 3; i++) { var c = [1, 2]; var d = [i, i + 1]; var o = { k: i };
                seen.push(c, d, o, function () {}); }
            var distinct = 0;
            for (var a = 0; a < seen.length; a++)
                for (var b = a + 1; b < seen.length; b++) if (seen[a] === seen[b]) distinct++;
            return String(distinct) + seen[0].length + (seen[0] !== seen[4]);
        })();",
        "02true",
    );
}

#[test]
fn large_array_literal_chunks_match_tree_walker() {
    let mut items = String::new();
    for i in 0..700 {
        items.push_str(&format!("v + {i}, "));
    }
    let source = format!(
        "var __r = (function (v) {{ var a = [{items} undefined, v]; \
         return a.length + ',' + a[0] + ',' + a[699] + ',' + a[700] + ',' + (700 in a) + ',' + a[701]; }})(1);"
    );
    assert_parity_string(&source, "702,1,700,undefined,true,1");
}

#[test]
fn large_object_literal_chunks_match_tree_walker() {
    let mut props = String::new();
    for i in 0..600 {
        props.push_str(&format!("k{i}: v + {i}, "));
    }
    let source = format!(
        "var __r = (function (v) {{ var o = {{ {props} m() {{ return super.x; }}, get g() {{ return 7; }}, \
         __proto__: {{ x: 'px' }}, dup: 1, dup: 2 }};
         return Object.keys(o).length + ',' + o.k0 + ',' + o.k599 + ',' + o.m() + ',' + o.g + ',' + o.dup; }})(1);"
    );
    assert_parity_string(&source, "603,1,600,px,7,2");
}

#[test]
fn object_literal_forms_match_tree_walker() {
    assert_parity_string(
        "var __r = (function (v) {
            var base = { greet() { return 'base'; } };
            var o = { a: 1, 'b c': 2, 3: 'three', v, f: function () {}, g: () => {}, h: function named() {},
                      m() { return super.greet() + v; }, get x() { return 'gx'; }, set x(y) { this._x = y; },
                      get dup() { return 1; }, dup: 5, __proto__: base };
            var p = { __proto__: null, '__proto__x': 1 }; var q = { __proto__() { return 1; } };
            var s = { get a() { return 1; }, set a(v) {} };
            var d = Object.getOwnPropertyDescriptor(s, 'a');
            return [Object.keys(o).join('|'), o.f.name, o.g.name, o.h.name, o.m.name, o.m(),
                    Object.getOwnPropertyDescriptor(o, 'x').get.name, Object.getOwnPropertyDescriptor(o, 'x').set.name,
                    Object.getPrototypeOf(o) === base, Object.getPrototypeOf(p), typeof q.__proto__,
                    typeof d.get + typeof d.set, o.dup, new (function () {}).constructor === Object].join();
        })('V');",
        "3|a|b c|v|f|g|h|m|x|dup,f,g,named,m,baseV,get x,set x,true,,function,functionfunction,5,false",
    );
}

#[test]
fn named_evaluation_matches_tree_walker() {
    assert_parity_string(
        "var __r = (function () {
            var a = function () {}; let b = () => {}; const c = async function () {};
            var d; d = function () {}; var e; e = (0, function () {});
            var o = { m: function () {}, n: () => {} }; o.p = function () {};
            var cls = class {}; var f = function inner() {}; var g = function () {}; g += 1;
            d ||= 1;
            return [a.name, b.name, c.name, d.name, e.name === '', o.m.name, o.n.name, o.p.name === '',
                    cls.name, f.name].join();
        })();",
        "a,b,c,d,true,m,n,true,cls,inner",
    );
}

#[test]
fn closures_capture_the_current_block_environment() {
    assert_parity_string(
        "var __r = (function () {
            var fs = [];
            for (let i = 0; i < 3; i++) { let j = i * 10; fs.push(function () { return i + j; }); fs.push(() => i - j); }
            { const k = 'k'; fs.push(() => k); }
            return fs.map(function (f) { return f(); }).join();
        })();",
        "0,0,11,-9,22,-18,k",
    );
}

#[test]
fn closure_flavours_match_tree_walker() {
    assert_parity_string(
        "var __r = (function (arg) {
            var gen = function* () { yield 1; yield arg; };
            var asyncFn = async function () {}; var arrow = async (x) => x;
            var strictFn = function () { 'use strict'; return this; };
            var sloppy = function () { return typeof this; };
            var usesArgs = function () { return arguments.length; };
            var viaNewTarget = function () { return new.target === undefined; };
            var outer = function () { return () => new.target === undefined; };
            var it = gen();
            return [it.next().value, it.next().value, Object.getPrototypeOf(asyncFn) === Object.getPrototypeOf(async function () {}),
                    typeof arrow, typeof strictFn(), sloppy(), usesArgs(1, 2, 3), viaNewTarget(), outer()(),
                    typeof new (function () { this.a = 1; })().a, gen.prototype !== undefined,
                    arrow.hasOwnProperty('prototype'), (function f(n) { return n ? f(n - 1) + 1 : 0; })(4)].join();
        })(5);",
        "1,5,true,function,undefined,object,3,true,true,number,true,false,4",
    );
}

#[test]
fn literal_values_survive_gc_during_later_element_evaluation() {
    let source = "
        function make() { return { tag: 'fresh' }; }
        var __r = (function () {
            var a = [make(), [make()], function () { return 1; }, ($262.gc(), make()), { k: make() }];
            var o = { first: make(), second: [make()], third: ($262.gc(), make()), fn: function () {} };
            var big = [make(), make(), make(), make()];
            return [a[0].tag, a[1][0].tag, a[2](), a[3].tag, a[4].k.tag, o.first.tag, o.second[0].tag,
                    o.third.tag, typeof o.fn, big.length].join();
        })();
    ";
    assert_parity_string(
        source,
        "fresh,fresh,1,fresh,fresh,fresh,fresh,fresh,function,4",
    );
}

#[test]
fn literal_operands_are_released_after_the_literal() {
    let source = "(function () { var s = 0; for (var i = 0; i < 6; i++) {
            var a = [i, { v: i }, function () {}, [i, i]]; var o = { a: a, b: { c: [i] } }; s += o.b.c[0] + a.length; } return s; })();";
    let (completion, interp) = run_bytecode_script(source);
    assert!(
        matches!(completion, Completion::Normal(_)),
        "unexpected completion: {completion:?}"
    );
    assert!(interp.bytecode_chunks_executed >= 1);
    assert!(interp.gc_bytecode_roots.is_empty());
}

#[test]
fn regexp_literal_creates_a_fresh_object_each_evaluation() {
    assert_parity_string(
        "var __r = (function () {
            var make = function () { return /a(b)/gi; };
            var x = make(), y = make();
            x.lastIndex = 3;
            return [x === y, y.lastIndex, x.source, x.flags, make().test('xAB'), /(?:)/.source, new RegExp('').source].join();
        })();",
        "false,0,a(b),gi,true,(?:),(?:)",
    );
}

#[test]
fn super_property_in_object_literal_method_uses_the_literal_as_home_object() {
    assert_parity_string(
        "var __r = (function () {
            var proto = { who() { return 'proto'; } };
            var o = { __proto__: proto, who() { return 'o>' + super.who(); } };
            var p = { __proto__: proto, get w() { return super.who(); } };
            return o.who() + ',' + p.w;
        })();",
        "o>proto,proto",
    );
}

#[test]
fn immediately_invoked_closures_compile_natively_and_call_with_undefined_this() {
    let chunk = compile_function_source(
        "(function (a) { return a; })(1); (() => 2)(); (0, o.m)(); f()();",
        &["o"],
    );
    assert!(
        chunk.ast.is_empty(),
        "no call callee left on the tree-walker"
    );
    assert_parity_string(
        "var __r = (function () {
            var o = { m: function () { return this === o; } };
            return [(function (a) { return a + 1; })(2), (() => 7)(), (0, o.m)(), (o.m)(), (o?.m)(),
                    (function () { return function () { return 9; }; })()(),
                    (function () { 'use strict'; return typeof this; })()].join();
        })();",
        "3,7,false,true,true,9,undefined",
    );
}

#[test]
fn try_catch_finally_and_iteration_compile_without_hand_offs() {
    let chunk = compile_function_source(
        "try { s += 1; } catch (e) { s += e; } finally { s += 2; }
         for (var k in o) { s += k; }
         for (const v of a) { s += v; }
         for (let [p, q] of a) { s += p; }
         outer: for (x of a) { for (y in o) { if (y) continue outer; break; } }",
        &["s", "o", "a", "x", "y"],
    );
    assert!(chunk.ast.is_empty(), "{:?}", chunk.fallback_reasons);
    let ops = work_ops(&chunk);
    assert_eq!(count_ops(&chunk, Op::ExecAst), 0, "{ops:?}");
    assert_eq!(count_ops(&chunk, Op::GetIterator), 3);
    assert_eq!(count_ops(&chunk, Op::ForInHead), 2);
    assert_eq!(count_ops(&chunk, Op::EndFinally), 1);
    // The try's block and catch, plus the three for-of loops' bodies.
    assert_eq!(chunk.handlers.len(), 1 + 1 + 1 + 3 - 1);
}

#[test]
fn hoisted_for_of_head_scope_is_entered_once_per_loop() {
    // `PushEnv` after `IterStep` is a fresh environment per iteration; before
    // it, the head's one scope. The first `PushEnv` is the TDZ scope the
    // iterated expression runs in.
    let envs_around_step = |source: &str, params: &[&str]| {
        let ops = work_ops(&compile_function_source(source, params));
        let step = ops
            .iter()
            .position(|op| *op == Op::IterStep)
            .expect("IterStep");
        let before = ops[..step].iter().filter(|op| **op == Op::PushEnv).count();
        let after = ops[step..].iter().filter(|op| **op == Op::PushEnv).count();
        (before, after)
    };
    assert_eq!(
        envs_around_step("for (const v of a) { s += v; }", &["s", "a"]),
        (2, 0)
    );
    assert_eq!(
        envs_around_step(
            "for (const v of a) { fs.push(function () { return v; }); }",
            &["fs", "a"]
        ),
        (1, 1)
    );
    assert_eq!(
        envs_around_step("for (const [p, q] of a) { s += p; }", &["s", "a"]),
        (1, 1)
    );
}

#[test]
fn escaping_hand_off_inside_a_finalizer_region_keeps_the_try_on_the_tree_walker() {
    // A fallback `return` would skip the finalizer, so the whole `try` is
    // handed over instead of just the labelled block.
    let chunk = compile_function_source(
        "var pad = 0; for (var i = 0; i < 2; i++) { try { a: { continue; } } finally { pad = 1; } }",
        &["x"],
    );
    assert_eq!(
        chunk.fallback_reasons,
        vec!["completion leaves a cleanup region"]
    );
    assert_eq!(count_ops(&chunk, Op::EndFinally), 0);
}

#[test]
fn escaping_hand_off_in_a_for_in_body_is_dropped_by_the_escape_table() {
    // for-in has no iterator close, so the loop stays native and its slot is
    // dropped when a hand-off statement breaks out of it.
    assert_value_parity(
        "var __r = (function(){ var out = []; \
         for (var k in {a:1,b:2}) { l: { if (k === 'a') break l; out.push(k); } } \
         outer: for (var i = 0; i < 2; i++) { for (var k in {a:1}) { l2: { continue outer; } } } \
         return out.join() + i; })();",
    );
}

fn run_both_modes(source: &str) -> String {
    let (tree, _) = eval_with_mode(source, false);
    let (vm, chunks) = eval_with_mode(source, true);
    assert!(chunks >= 1, "no chunk ran for {source}");
    let show = |v: &JsValue| {
        v.as_number()
            .map(|n| n.to_string())
            .or_else(|| v.as_string().map(|s| s.to_rust_string()))
            .unwrap_or_else(|| format!("{v:?}"))
    };
    assert_eq!(show(&tree), show(&vm), "{source}");
    show(&vm)
}

#[test]
fn try_finally_completion_routing_matches_tree_walker() {
    for (source, expected) in [
        (
            "var __r = (function(){ var l = []; for (var i = 0; i < 3; i++) { try { if (i === 1) continue; \
             if (i === 2) break; l.push('b' + i); } finally { l.push('f' + i); } } return l.join(); })();",
            "b0,f0,f1,f2",
        ),
        (
            "var __r = (function(){ try { return 'a'; } finally { return 'b'; } })();",
            "b",
        ),
        (
            "var __r = (function(){ for (var i = 0; i < 2; i++) { try { throw 1; } finally { break; } } return i; })();",
            "0",
        ),
        (
            "var __r = (function(){ var l = []; try { try { throw 'x'; } finally { l.push('in'); } } \
             catch (e) { l.push(e); } return l.join(); })();",
            "in,x",
        ),
    ] {
        assert_eq!(run_both_modes(source), expected, "{source}");
    }
}

#[test]
fn for_of_closes_its_iterator_on_every_early_exit() {
    let source = "var __r = (function(){ var log = []; \
        var it = { [Symbol.iterator]() { return { next() { return { value: 1, done: false }; }, \
        return() { log.push('r'); return {}; } }; } }; \
        for (var a of it) break; \
        (function () { for (var a of it) return; })(); \
        try { for (var a of it) throw 0; } catch (e) {} \
        outer: for (var b of [1]) { for (var a of it) continue outer; } \
        return log.join(''); })();";
    assert_eq!(run_both_modes(source), "rrrr");
}

#[test]
fn handler_keeps_pending_operands_rooted_and_restores_the_stack() {
    let source = "var __r = (function(){ var s = 0; var o = { m: function (a, b) { return a + b; } }; \
        function boom() { $262.gc(); throw 1; } \
        for (var i = 0; i < 4; i++) { try { s += o.m({}, [1, boom()]); } catch (e) { s += e; } } \
        return s; })();";
    let mut parser = crate::parser::Parser::new(source).expect("parser init");
    let program = parser.parse_program().expect("parse");
    let mut interp = Interpreter::new();
    let _ = interp.run(&program);
    assert_eq!(
        interp.get_global_var_ref("__r").and_then(|v| v.as_number()),
        Some(4.0)
    );
    assert!(interp.gc_bytecode_roots.is_empty());
}

#[test]
fn lexical_only_body_precomputes_its_declaration_prologue() {
    let chunk = compile_function_source(
        "var v = 1; let a = 1; const { b, c: [d] } = o; class K {} a + v;",
        &[],
    );
    assert!(chunk.needs_instantiation);
    let lexicals = chunk
        .top_level_lexicals
        .as_ref()
        .expect("a body of var and lexical declarations needs only the lexical prologue");
    let shown: Vec<(&str, bool)> = lexicals
        .iter()
        .map(|&(idx, is_const)| (chunk.names[usize::from(idx)].as_str(), is_const))
        .collect();
    assert_eq!(
        shown,
        [("a", false), ("b", true), ("d", true), ("K", true)],
        "source order, with immutability"
    );
}

#[test]
fn function_declarations_keep_the_generic_instantiation() {
    for source in [
        "let a = 1; function f() {} a;",
        "let a = 1; { function f() {} } a;",
        "let a = 1; if (a) function f() {} a;",
        "let a = 1; l: function f() {} a;",
    ] {
        let chunk = compile_function_source(source, &[]);
        assert!(chunk.needs_instantiation, "{source}");
        assert!(chunk.top_level_lexicals.is_none(), "{source}");
    }
}

#[test]
fn body_without_lexicals_has_no_declaration_prologue() {
    let chunk = compile_function_source("var a = 1; a;", &[]);
    assert!(!chunk.needs_instantiation);
    assert!(chunk.top_level_lexicals.is_none());
}

fn try_compile_generator_source(source: &str) -> Result<Chunk, super::compiler::CompileError> {
    use super::compiler::{FrameShape, compile_generator_body};
    use crate::parser::Parser;

    let wrapped = format!("function* g(a) {{ {source}\n}}");
    let mut parser = Parser::new(&wrapped).expect("parser init");
    let program = parser.parse_program().expect("parse");
    let Some(Statement::FunctionDeclaration(decl)) = program.body.as_slice().first() else {
        panic!("function declaration");
    };
    compile_generator_body(
        decl.body.statements.as_slice(),
        &FrameShape {
            arguments_slot: false,
            simple_params: &["a".to_string()],
            separate_body_env: false,
        },
    )
}

#[test]
fn generator_yield_compiles_to_a_suspension_followed_by_a_return_stub() {
    let chunk = try_compile_generator_source("yield a; return 1;").expect("compile");
    let ops = decode_ops(&chunk);
    let at = ops
        .iter()
        .position(|(op, _)| *op == Op::Yield)
        .expect("yield op");
    // The persistent stack height is 0 and the skip lands after the stub,
    // which is the unwind a `return` statement emits.
    assert_eq!(ops[at].1[0], 0);
    assert_eq!(ops[at + 1].0, Op::Return, "return stub follows the yield");
}

#[test]
fn generator_yield_inside_a_for_of_records_the_iterator_as_persistent() {
    let chunk = try_compile_generator_source("for (const x of a) { yield x; }").expect("compile");
    let (_, operands) = decode_ops(&chunk)
        .into_iter()
        .find(|(op, _)| *op == Op::Yield)
        .expect("yield op");
    assert_eq!(
        operands[0], 1,
        "the for-of iterator slot stays on the stack"
    );
    let ops = work_ops(&chunk);
    assert!(
        ops.contains(&Op::IterClose),
        "a return completion closes the iterator"
    );
}

#[test]
fn generator_chunk_rejects_what_a_suspending_frame_cannot_host() {
    for source in [
        "yield* a;",
        "var [x = yield] = a;",
        "var { x = yield } = a;",
        "return `${yield}`;",
        "return { [yield]: 1 };",
        "class C extends (yield) {}",
        "for (const [x = yield] of a) {}",
        "try {} catch ([x = yield]) {}",
        "using r = a;",
        "for (using r of a) {}",
        "with (a) { yield 1; }",
        "eval('1'); yield 1;",
        "return new.target;",
    ] {
        assert!(
            try_compile_generator_source(source).is_err(),
            "expected {source} to stay on the state machine"
        );
    }
}

#[test]
fn generator_chunk_accepts_statement_and_expression_yields() {
    for source in [
        "yield;",
        "var x = yield a;",
        "x = (yield a) + (yield a);",
        "try { yield 1; } finally { yield 2; }",
        "for (;;) { switch (yield) { case yield: break; } }",
        "while (yield) { if (yield) continue; }",
        "a.b = yield a;",
        "return f(1, yield, 3);",
        "return f(...[yield]);",
        "label: for (const x of a) { try { yield x; } catch (e) { break label; } }",
    ] {
        let chunk =
            try_compile_generator_source(source).unwrap_or_else(|e| panic!("{source}: {e:?}"));
        assert!(
            decode_ops(&chunk).iter().any(|(op, _)| *op == Op::Yield),
            "{source}"
        );
    }
}
