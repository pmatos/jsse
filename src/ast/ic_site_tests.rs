use super::{
    BinaryOp, Body, CallSiteId, ExportDeclaration, ExprBox, Expression, FunctionExpr, Literal,
    MemberProperty, ModuleItem, Program, PropSiteId, SourceType, Statement,
};
use super::{
    assign_expr_sites, assign_ic_sites, assign_ic_sites_for_body, assign_ic_sites_for_module,
    clear_expr_ic_sites, func_uses_arguments,
};

fn ident(name: &str) -> Expression {
    Expression::Identifier(name.to_string(), 0)
}

fn call(callee: Expression, args: Vec<Expression>) -> Expression {
    Expression::Call(ExprBox::new(callee), args, CallSiteId::UNASSIGNED)
}

fn prop(obj: Expression, name: &str) -> Expression {
    Expression::Member(
        ExprBox::new(obj),
        MemberProperty::Dot(name.to_string()),
        PropSiteId::UNASSIGNED,
    )
}

fn expr_stmt(e: Expression) -> Statement {
    Statement::Expression(e, 0)
}

fn body_with(stmts: Vec<Statement>) -> Body {
    Body::new(stmts)
}

#[test]
fn single_call_site_gets_id_zero() {
    let mut body = body_with(vec![expr_stmt(call(ident("f"), vec![]))]);
    assign_ic_sites(&mut body);
    assert!(body.ic.assigned);
    assert_eq!(body.ic.call_site_count, 1);
    assert_eq!(body.ic.prop_site_count, 0);
    match &body.statements[0] {
        Statement::Expression(Expression::Call(_, _, id), _) => assert_eq!(id.0, 0),
        _ => panic!("expected Call"),
    }
}

#[test]
fn mixed_call_and_prop_sites_numbered_separately() {
    let mut body = body_with(vec![expr_stmt(call(
        prop(ident("o"), "m"),
        vec![prop(ident("o"), "x")],
    ))]);
    assign_ic_sites(&mut body);
    assert_eq!(body.ic.call_site_count, 1);
    assert_eq!(body.ic.prop_site_count, 2);
    match &body.statements[0] {
        Statement::Expression(Expression::Call(callee, _, call_id), _) => {
            assert_eq!(call_id.0, 0);
            match callee.as_ref() {
                Expression::Member(_, _, id) => assert_eq!(id.0, 0),
                _ => panic!("expected Member callee"),
            }
        }
        _ => panic!("expected Call"),
    }
}

#[test]
fn nested_function_body_resets_counters() {
    let inner_body = body_with(vec![expr_stmt(call(ident("g"), vec![]))]);
    let func = FunctionExpr {
        name: None,
        params: vec![],
        body: inner_body,
        is_async: false,
        is_generator: false,
        source_text: None,
        body_is_strict: false,
        info: Default::default(),
    };
    let mut outer = body_with(vec![expr_stmt(call(
        Expression::Function(func),
        vec![prop(ident("o"), "x")],
    ))]);
    assign_ic_sites(&mut outer);

    assert_eq!(outer.ic.call_site_count, 1);
    assert_eq!(outer.ic.prop_site_count, 1);

    match &outer.statements[0] {
        Statement::Expression(Expression::Call(_, _, id), _) => assert_eq!(id.0, 0),
        _ => panic!("expected outer Call"),
    }

    let inner_func = match &outer.statements[0] {
        Statement::Expression(Expression::Call(callee, _, _), _) => match callee.as_ref() {
            Expression::Function(f) => f,
            _ => panic!("expected Function"),
        },
        _ => panic!("expected Call"),
    };
    assert_eq!(inner_func.body.ic.call_site_count, 1);
    assert_eq!(inner_func.body.ic.prop_site_count, 0);
    match &inner_func.body.statements[0] {
        Statement::Expression(Expression::Call(_, _, id), _) => assert_eq!(id.0, 0),
        _ => panic!("expected inner Call"),
    }
}

#[test]
fn assign_ic_sites_for_body_counts_only_inner_body() {
    let mut body = body_with(vec![expr_stmt(call(ident("f"), vec![]))]);
    assign_ic_sites_for_body(&mut body);
    assert_eq!(body.ic.call_site_count, 1);
    assert_eq!(body.ic.prop_site_count, 0);
}

#[test]
fn assign_ic_sites_for_module_numbers_top_level_items() {
    let mut program = Program {
        source_type: SourceType::Module,
        body: Body::new(vec![]),
        module_items: vec![
            ModuleItem::ExportDeclaration(ExportDeclaration::Default(Box::new(call(
                ident("f"),
                vec![prop(ident("o"), "x")],
            )))),
            ModuleItem::Statement(expr_stmt(call(ident("g"), vec![]))),
        ],
        body_is_strict: true,
    };
    assign_ic_sites_for_module(&mut program);
    assert!(program.body.ic.assigned);
    assert_eq!(program.body.ic.call_site_count, 2);
    assert_eq!(program.body.ic.prop_site_count, 1);
}

fn num(v: f64) -> Expression {
    Expression::Literal(Literal::Number(v))
}

fn add(l: Expression, r: Expression) -> Expression {
    Expression::Binary(BinaryOp::Add, ExprBox::new(l), ExprBox::new(r), 0)
}

/// `a.b.b.b…`, `depth` members deep. Built with a loop, so the *builder*
/// is not what is under test.
fn deep_member_chain(depth: usize) -> Expression {
    let mut e = ident("a");
    for _ in 0..depth {
        e = prop(e, "b");
    }
    e
}

/// `seed + 1 + 1 + …`, `depth` additions deep — the flat operand chain
/// from jsse#612.
fn deep_add_chain(seed: Expression, depth: usize) -> Expression {
    let mut e = seed;
    for _ in 0..depth {
        e = add(e, num(1.0));
    }
    e
}

/// Nesting depth used by the stack-safety tests below. Chosen so that the
/// *recursive* form of these passes could not possibly fit in
/// `SMALL_STACK`: at the measured frame costs that is ~100 MB on debug and
/// ~3 MB on release, against a 256 KiB stack. An iterative pass keeps its
/// state on the heap and needs O(1) native stack, so it passes in both
/// profiles — which is the point, since the abort ceiling itself is very
/// profile-dependent (~122k debug vs ~3-4M release) and a fixed source
/// size could never regression-test both.
const DEEP: usize = 100_000;
const SMALL_STACK: usize = 256 * 1024;

/// Runs `f` on a thread far too small to hold the old recursive walks.
///
/// A native stack overflow aborts the process rather than unwinding, so a
/// regression here fails the whole test binary loudly instead of reporting
/// a tidy assertion failure. That is intended: the bug being guarded
/// against is precisely an uncatchable SIGABRT.
fn on_small_stack<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(f)
        .expect("spawn probe thread")
        .join()
        .expect("probe thread panicked")
}

#[test]
fn deep_member_chain_is_numbered_without_native_recursion() {
    on_small_stack(|| {
        let mut body = body_with(vec![expr_stmt(deep_member_chain(DEEP))]);
        assign_ic_sites(&mut body);
        assert_eq!(body.ic.prop_site_count, DEEP as u32);
        assert_eq!(body.ic.call_site_count, 0);

        // Post-order numbering: the innermost member is 0 and the
        // outermost DEEP-1. Walking out-to-in must see them descending
        // with no gaps — that density is what `IcStore` slot indexing
        // relies on.
        let mut cur = match &body.statements[0] {
            Statement::Expression(e, _) => e,
            other => panic!("expected an expression statement, got {other:?}"),
        };
        for expected in (0..DEEP as u32).rev() {
            match cur {
                Expression::Member(obj, _, id) => {
                    assert_eq!(id.0, expected, "member at depth {expected}");
                    cur = obj;
                }
                other => panic!("expected Member at depth {expected}, got {other:?}"),
            }
        }
    });
}

#[test]
fn deep_call_chain_is_numbered_without_native_recursion() {
    on_small_stack(|| {
        // `a()()()…` — the other production that parses in a continuation
        // loop and so is not bounded by `MAX_PARSE_DEPTH`.
        let mut e = ident("a");
        for _ in 0..DEEP {
            e = call(e, vec![]);
        }
        let mut body = body_with(vec![expr_stmt(e)]);
        assign_ic_sites(&mut body);
        assert_eq!(body.ic.call_site_count, DEEP as u32);
        assert_eq!(body.ic.prop_site_count, 0);
    });
}

#[test]
fn deep_operand_chain_arguments_scan_has_no_native_recursion() {
    let (without, with_at_far_end) = on_small_stack(|| {
        // No `arguments` anywhere: the scan cannot short-circuit and must
        // walk every operand — the worst case for stack use.
        let clean = body_with(vec![Statement::Return(
            Some(deep_add_chain(num(1.0), DEEP)),
            0,
        )]);
        let without = func_uses_arguments(&[], &clean);

        // Same shape, with `arguments` buried at the far end of the chain.
        let dirty = body_with(vec![Statement::Return(
            Some(deep_add_chain(ident("arguments"), DEEP)),
            0,
        )]);
        let with_at_far_end = func_uses_arguments(&[], &dirty);

        (without, with_at_far_end)
    });
    assert!(!without, "a chain of literals does not reference arguments");
    assert!(
        with_at_far_end,
        "`arguments` at the deep end of the chain must still be found"
    );
}

#[test]
fn deep_member_chain_is_cleared_without_native_recursion() {
    on_small_stack(|| {
        let mut e = deep_member_chain(DEEP);
        let (mut call_id, mut prop_id) = (0u32, 0u32);
        assign_expr_sites(&mut e, &mut call_id, &mut prop_id);
        assert_eq!(prop_id, DEEP as u32);

        clear_expr_ic_sites(&mut e);
        let mut cur = &e;
        for depth in (0..DEEP).rev() {
            match cur {
                Expression::Member(obj, _, id) => {
                    assert_eq!(*id, PropSiteId::UNASSIGNED, "member at depth {depth}");
                    cur = obj;
                }
                other => panic!("expected Member at depth {depth}, got {other:?}"),
            }
        }
    });
}

#[test]
fn deep_add_chain_drops_without_native_recursion() {
    on_small_stack(|| {
        let e = deep_add_chain(num(1.0), DEEP);
        drop(e);
    });
}

#[test]
fn deep_member_chain_drops_without_native_recursion() {
    on_small_stack(|| {
        let e = deep_member_chain(DEEP);
        drop(e);
    });
}

#[test]
fn deep_call_chain_drops_without_native_recursion() {
    on_small_stack(|| {
        let mut e = ident("a");
        for _ in 0..DEEP {
            e = call(e, vec![]);
        }
        drop(e);
    });
}

#[test]
fn expr_box_debug_is_transparent() {
    for e in [
        add(num(1.0), num(2.0)),
        prop(ident("a"), "b"),
        call(ident("f"), vec![num(1.0)]),
    ] {
        let boxed = ExprBox::new(e.clone());
        assert_eq!(format!("{boxed:?}"), format!("{e:?}"));
    }
}
