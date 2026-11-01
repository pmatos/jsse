//! Lowering of an async function body onto the generator state machine: each
//! `await` becomes a `yield` and the `Yield` terminators come back as `Await`s.

use super::*;

pub(crate) fn transform_async_function(
    body: &[Statement],
    params: &[Pattern],
) -> GeneratorStateMachine {
    let rewritten = rewrite_stmts_await_to_yield(body);
    let mut machine = transform_generator_inner_opts(&rewritten, params, true, true);
    await_pos::rewrite_terminators_yield_to_await(&mut machine);
    machine
}

fn rewrite_stmts_await_to_yield(stmts: &[Statement]) -> Vec<Statement> {
    stmts.iter().map(rewrite_stmt_await_to_yield).collect()
}

fn rewrite_stmt_await_to_yield(stmt: &Statement) -> Statement {
    match stmt {
        Statement::Empty | Statement::Break(..) | Statement::Continue(_) | Statement::Debugger => {
            stmt.clone()
        }
        Statement::FunctionDeclaration(_) | Statement::ClassDeclaration(_) => stmt.clone(),
        Statement::Expression(e) => Statement::Expression(rewrite_expr(e)),
        Statement::Block(stmts) => Statement::Block(rewrite_stmts_await_to_yield(stmts)),
        Statement::Variable(decl) => Statement::Variable(VariableDeclaration {
            kind: decl.kind,
            declarations: decl
                .declarations
                .iter()
                .map(|d| VariableDeclarator {
                    init_at: d.init_at,
                    pattern: d.pattern.clone(),
                    init: d.init.as_ref().map(rewrite_expr),
                })
                .collect(),
        }),
        Statement::If(if_stmt) => Statement::If(IfStatement {
            test: rewrite_expr(&if_stmt.test),
            consequent: Box::new(rewrite_stmt_await_to_yield(&if_stmt.consequent)),
            alternate: if_stmt
                .alternate
                .as_ref()
                .map(|s| Box::new(rewrite_stmt_await_to_yield(s))),
        }),
        Statement::While(w) => Statement::While(WhileStatement {
            test: rewrite_expr(&w.test),
            body: Box::new(rewrite_stmt_await_to_yield(&w.body)),
        }),
        Statement::DoWhile(d) => Statement::DoWhile(DoWhileStatement {
            test: rewrite_expr(&d.test),
            body: Box::new(rewrite_stmt_await_to_yield(&d.body)),
        }),
        Statement::For(f) => Statement::For(ForStatement {
            init: f.init.as_ref().map(|i| match i {
                ForInit::Variable(v) => ForInit::Variable(VariableDeclaration {
                    kind: v.kind,
                    declarations: v
                        .declarations
                        .iter()
                        .map(|d| VariableDeclarator {
                            init_at: d.init_at,
                            pattern: d.pattern.clone(),
                            init: d.init.as_ref().map(rewrite_expr),
                        })
                        .collect(),
                }),
                ForInit::Expression(e) => ForInit::Expression(rewrite_expr(e)),
            }),
            test: f.test.as_ref().map(rewrite_expr),
            update: f.update.as_deref().map(|e| ExprBox::new(rewrite_expr(e))),
            body: Box::new(rewrite_stmt_await_to_yield(&f.body)),
            scope_capture: Default::default(),
        }),
        Statement::ForIn(f) => Statement::ForIn(ForInStatement {
            left: rewrite_for_left(&f.left),
            right: rewrite_expr(&f.right),
            body: Box::new(rewrite_stmt_await_to_yield(&f.body)),
            scope_capture: Default::default(),
        }),
        Statement::ForOf(f) => Statement::ForOf(ForOfStatement {
            left: rewrite_for_left(&f.left),
            right: rewrite_expr(&f.right),
            body: Box::new(rewrite_stmt_await_to_yield(&f.body)),
            is_await: f.is_await,
            at: f.at,
            scope_capture: Default::default(),
        }),
        Statement::Return(e) => Statement::Return(e.as_ref().map(rewrite_expr)),
        Statement::Throw(e, p) => Statement::Throw(rewrite_expr(e), *p),
        Statement::Try(t) => Statement::Try(TryStatement {
            block: rewrite_stmts_await_to_yield(&t.block),
            handler: t.handler.as_ref().map(|h| CatchClause {
                param: h.param.clone(),
                body: rewrite_stmts_await_to_yield(&h.body),
            }),
            finalizer: t
                .finalizer
                .as_ref()
                .map(|f| rewrite_stmts_await_to_yield(f)),
        }),
        Statement::Switch(s) => Statement::Switch(SwitchStatement {
            discriminant: rewrite_expr(&s.discriminant),
            cases: s
                .cases
                .iter()
                .map(|c| SwitchCase {
                    test: c.test.as_ref().map(rewrite_expr),
                    consequent: rewrite_stmts_await_to_yield(&c.consequent),
                })
                .collect(),
        }),
        Statement::Labeled(label, inner) => {
            Statement::Labeled(label.clone(), Box::new(rewrite_stmt_await_to_yield(inner)))
        }
        Statement::With(e, inner) => Statement::With(
            rewrite_expr(e),
            Box::new(rewrite_stmt_await_to_yield(inner)),
        ),
    }
}

fn rewrite_for_left(left: &ForInOfLeft) -> ForInOfLeft {
    match left {
        ForInOfLeft::Variable(_) | ForInOfLeft::Pattern(_) => left.clone(),
        ForInOfLeft::Expression(e) => ForInOfLeft::Expression(rewrite_expr(e)),
    }
}

fn rewrite_expr(expr: &Expression) -> Expression {
    match expr {
        Expression::Await(inner, at) => {
            let operand = await_pos::tag(rewrite_expr(inner), *at);
            Expression::Yield(Some(ExprBox::new(operand)), YieldKind::PLAIN)
        }
        Expression::Function(_)
        | Expression::ArrowFunction(_)
        | Expression::Class(_)
        | Expression::Literal(_)
        | Expression::Identifier(_)
        | Expression::This
        | Expression::Super
        | Expression::NewTarget
        | Expression::ImportMeta
        | Expression::PrivateIdentifier(_) => expr.clone(),
        Expression::Array(elems, trailing) => Expression::Array(
            elems.iter().map(|e| e.as_ref().map(rewrite_expr)).collect(),
            *trailing,
        ),
        Expression::Object(props, trailing) => Expression::Object(
            props
                .iter()
                .map(|p| Property {
                    key: match &p.key {
                        PropertyKey::Computed(e) => {
                            PropertyKey::Computed(ExprBox::new(rewrite_expr(e)))
                        }
                        other => other.clone(),
                    },
                    value: rewrite_expr(&p.value),
                    kind: p.kind,
                    computed: p.computed,
                    shorthand: p.shorthand,
                    method: p.method,
                })
                .collect(),
            *trailing,
        ),
        Expression::Unary(op, e) => Expression::Unary(*op, ExprBox::new(rewrite_expr(e))),
        Expression::Binary(op, l, r, pos) => Expression::Binary(
            *op,
            ExprBox::new(rewrite_expr(l)),
            ExprBox::new(rewrite_expr(r)),
            *pos,
        ),
        Expression::Logical(op, l, r) => Expression::Logical(
            *op,
            ExprBox::new(rewrite_expr(l)),
            ExprBox::new(rewrite_expr(r)),
        ),
        Expression::Update(op, prefix, e, at) => {
            Expression::Update(*op, *prefix, ExprBox::new(rewrite_expr(e)), *at)
        }
        Expression::Assign(op, l, r, at) => {
            // A destructuring-assignment LHS is not a suspension boundary the
            // tree-walker can split: leave any `await` in it to pattern lowering.
            let new_l = if *op == AssignOp::Assign
                && matches!(l.as_ref(), Expression::Array(..) | Expression::Object(..))
            {
                l.clone()
            } else {
                ExprBox::new(rewrite_expr(l))
            };
            Expression::Assign(*op, new_l, ExprBox::new(rewrite_expr(r)), *at)
        }
        Expression::Conditional(t, c, a) => Expression::Conditional(
            ExprBox::new(rewrite_expr(t)),
            ExprBox::new(rewrite_expr(c)),
            ExprBox::new(rewrite_expr(a)),
        ),
        Expression::Call(callee, args, site) => Expression::Call(
            ExprBox::new(rewrite_expr(callee)),
            args.iter().map(rewrite_expr).collect(),
            CallSiteId(u32::MAX, site.1),
        ),
        Expression::New(callee, args, site) => Expression::New(
            ExprBox::new(rewrite_expr(callee)),
            args.iter().map(rewrite_expr).collect(),
            CallSiteId(u32::MAX, site.1),
        ),
        Expression::Member(obj, prop, _) => Expression::Member(
            ExprBox::new(rewrite_expr(obj)),
            match prop {
                MemberProperty::Computed(e) => {
                    MemberProperty::Computed(ExprBox::new(rewrite_expr(e)))
                }
                other => other.clone(),
            },
            PropSiteId::UNASSIGNED,
        ),
        Expression::OptionalChain(base, chain) => Expression::OptionalChain(
            ExprBox::new(rewrite_expr(base)),
            ExprBox::new(rewrite_expr(chain)),
        ),
        Expression::Comma(exprs) => Expression::Comma(exprs.iter().map(rewrite_expr).collect()),
        Expression::Spread(e) => Expression::Spread(ExprBox::new(rewrite_expr(e))),
        Expression::PreIterated(e) => Expression::PreIterated(ExprBox::new(rewrite_expr(e))),
        Expression::ClassProtoParent(e, at) => {
            Expression::ClassProtoParent(ExprBox::new(rewrite_expr(e)), *at)
        }
        Expression::BoundCallee(func, this) => Expression::BoundCallee(
            ExprBox::new(rewrite_expr(func)),
            ExprBox::new(rewrite_expr(this)),
        ),
        Expression::ResolvedHeritage(sup, proto) => Expression::ResolvedHeritage(
            ExprBox::new(rewrite_expr(sup)),
            ExprBox::new(rewrite_expr(proto)),
        ),
        Expression::Yield(inner, delegate) => Expression::Yield(
            inner.as_ref().map(|e| ExprBox::new(rewrite_expr(e))),
            *delegate,
        ),
        Expression::TaggedTemplate(tag, tl) => Expression::TaggedTemplate(
            ExprBox::new(rewrite_expr(tag)),
            TemplateLiteral {
                id: tl.id,
                quasis: tl.quasis.clone(),
                raw_quasis: tl.raw_quasis.clone(),
                raw_stand_ins: tl.raw_stand_ins,
                expressions: tl.expressions.iter().map(rewrite_expr).collect(),
            },
        ),
        Expression::Template(tl) => Expression::Template(TemplateLiteral {
            id: tl.id,
            quasis: tl.quasis.clone(),
            raw_quasis: tl.raw_quasis.clone(),
            raw_stand_ins: tl.raw_stand_ins,
            expressions: tl.expressions.iter().map(rewrite_expr).collect(),
        }),
        Expression::Typeof(e) => Expression::Typeof(ExprBox::new(rewrite_expr(e))),
        Expression::Void(e) => Expression::Void(ExprBox::new(rewrite_expr(e))),
        Expression::Delete(e) => Expression::Delete(ExprBox::new(rewrite_expr(e))),
        Expression::Sequence(exprs) => {
            Expression::Sequence(exprs.iter().map(rewrite_expr).collect())
        }
        Expression::Import(spec, opts, at) => Expression::Import(
            ExprBox::new(rewrite_expr(spec)),
            opts.as_ref().map(|o| ExprBox::new(rewrite_expr(o))),
            *at,
        ),
        Expression::ImportDefer(spec, opts) => Expression::ImportDefer(
            ExprBox::new(rewrite_expr(spec)),
            opts.as_ref().map(|o| ExprBox::new(rewrite_expr(o))),
        ),
        Expression::ImportSource(spec, opts) => Expression::ImportSource(
            ExprBox::new(rewrite_expr(spec)),
            opts.as_ref().map(|o| ExprBox::new(rewrite_expr(o))),
        ),
    }
}
