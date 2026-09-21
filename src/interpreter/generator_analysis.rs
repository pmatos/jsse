use crate::ast::*;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub(crate) struct GeneratorAnalysis {
    pub yield_points: Vec<YieldPoint>,
    pub local_vars: Vec<LocalVariable>,
    pub try_contexts: Vec<TryContext>,
    pub loop_contexts: Vec<LoopContext>,
    pub has_yield_star: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct YieldPoint {
    pub id: usize,
    pub is_delegate: bool,
    pub inside_try: Option<usize>,
    pub inside_loop: Option<usize>,
    pub in_expression_context: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct LocalVariable {
    pub name: String,
    pub kind: VarKind,
    pub scope_depth: usize,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct TryContext {
    pub id: usize,
    pub has_catch: bool,
    pub has_finally: bool,
    pub contains_yields: Vec<usize>,
    pub parent_try: Option<usize>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct LoopContext {
    pub id: usize,
    pub loop_type: LoopType,
    pub label: Option<String>,
    pub contains_yields: Vec<usize>,
    pub parent_loop: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopType {
    While,
    DoWhile,
    For,
    ForIn,
    ForOf,
}

struct AnalysisContext {
    yield_counter: usize,
    try_counter: usize,
    loop_counter: usize,
    scope_depth: usize,
    current_try: Option<usize>,
    current_loop: Option<usize>,
    current_label: Option<String>,
    seen_vars: HashSet<String>,
}

impl AnalysisContext {
    fn new() -> Self {
        Self {
            yield_counter: 0,
            try_counter: 0,
            loop_counter: 0,
            scope_depth: 0,
            current_try: None,
            current_loop: None,
            current_label: None,
            seen_vars: HashSet::new(),
        }
    }
}

pub(crate) fn analyze_generator_body(body: &[Statement], params: &[Pattern]) -> GeneratorAnalysis {
    let mut analysis = GeneratorAnalysis {
        yield_points: Vec::new(),
        local_vars: Vec::new(),
        try_contexts: Vec::new(),
        loop_contexts: Vec::new(),
        has_yield_star: false,
    };
    let mut ctx = AnalysisContext::new();

    for param in params {
        collect_pattern_vars(param, VarKind::Var, 0, &mut analysis.local_vars, &mut ctx);
    }

    analyze_statements(body, &mut analysis, &mut ctx);

    analysis
}

fn analyze_statements(
    stmts: &[Statement],
    analysis: &mut GeneratorAnalysis,
    ctx: &mut AnalysisContext,
) {
    for stmt in stmts {
        analyze_statement(stmt, analysis, ctx);
    }
}

fn analyze_statement(
    stmt: &Statement,
    analysis: &mut GeneratorAnalysis,
    ctx: &mut AnalysisContext,
) {
    match stmt {
        Statement::Empty | Statement::Debugger => {}

        Statement::Expression(expr) => {
            analyze_expression(expr, analysis, ctx, false);
        }

        Statement::Block(stmts) => {
            ctx.scope_depth += 1;
            analyze_statements(stmts, analysis, ctx);
            ctx.scope_depth -= 1;
        }

        Statement::Variable(decl) => {
            for declarator in &decl.declarations {
                collect_pattern_vars(
                    &declarator.pattern,
                    decl.kind,
                    ctx.scope_depth,
                    &mut analysis.local_vars,
                    ctx,
                );
                if let Some(init) = &declarator.init {
                    analyze_expression(init, analysis, ctx, true);
                }
            }
        }

        Statement::If(if_stmt) => {
            analyze_expression(&if_stmt.test, analysis, ctx, true);
            analyze_statement(&if_stmt.consequent, analysis, ctx);
            if let Some(alt) = &if_stmt.alternate {
                analyze_statement(alt, analysis, ctx);
            }
        }

        Statement::While(while_stmt) => {
            let loop_id = ctx.loop_counter;
            ctx.loop_counter += 1;
            let parent_loop = ctx.current_loop;

            analysis.loop_contexts.push(LoopContext {
                id: loop_id,
                loop_type: LoopType::While,
                label: ctx.current_label.take(),
                contains_yields: Vec::new(),
                parent_loop,
            });

            ctx.current_loop = Some(loop_id);
            analyze_expression(&while_stmt.test, analysis, ctx, true);
            analyze_statement(&while_stmt.body, analysis, ctx);
            ctx.current_loop = parent_loop;
        }

        Statement::DoWhile(do_while_stmt) => {
            let loop_id = ctx.loop_counter;
            ctx.loop_counter += 1;
            let parent_loop = ctx.current_loop;

            analysis.loop_contexts.push(LoopContext {
                id: loop_id,
                loop_type: LoopType::DoWhile,
                label: ctx.current_label.take(),
                contains_yields: Vec::new(),
                parent_loop,
            });

            ctx.current_loop = Some(loop_id);
            analyze_statement(&do_while_stmt.body, analysis, ctx);
            analyze_expression(&do_while_stmt.test, analysis, ctx, true);
            ctx.current_loop = parent_loop;
        }

        Statement::For(for_stmt) => {
            let loop_id = ctx.loop_counter;
            ctx.loop_counter += 1;
            let parent_loop = ctx.current_loop;

            analysis.loop_contexts.push(LoopContext {
                id: loop_id,
                loop_type: LoopType::For,
                label: ctx.current_label.take(),
                contains_yields: Vec::new(),
                parent_loop,
            });

            ctx.current_loop = Some(loop_id);
            ctx.scope_depth += 1;

            if let Some(init) = &for_stmt.init {
                match init {
                    ForInit::Variable(decl) => {
                        for declarator in &decl.declarations {
                            collect_pattern_vars(
                                &declarator.pattern,
                                decl.kind,
                                ctx.scope_depth,
                                &mut analysis.local_vars,
                                ctx,
                            );
                            if let Some(expr) = &declarator.init {
                                analyze_expression(expr, analysis, ctx, true);
                            }
                        }
                    }
                    ForInit::Expression(expr) => {
                        analyze_expression(expr, analysis, ctx, true);
                    }
                }
            }
            if let Some(test) = &for_stmt.test {
                analyze_expression(test, analysis, ctx, true);
            }
            if let Some(update) = &for_stmt.update {
                analyze_expression(update, analysis, ctx, true);
            }
            analyze_statement(&for_stmt.body, analysis, ctx);

            ctx.scope_depth -= 1;
            ctx.current_loop = parent_loop;
        }

        Statement::ForIn(for_in_stmt) => {
            let loop_id = ctx.loop_counter;
            ctx.loop_counter += 1;
            let parent_loop = ctx.current_loop;

            analysis.loop_contexts.push(LoopContext {
                id: loop_id,
                loop_type: LoopType::ForIn,
                label: ctx.current_label.take(),
                contains_yields: Vec::new(),
                parent_loop,
            });

            ctx.current_loop = Some(loop_id);
            ctx.scope_depth += 1;

            match &for_in_stmt.left {
                ForInOfLeft::Variable(decl) => {
                    for declarator in &decl.declarations {
                        collect_pattern_vars(
                            &declarator.pattern,
                            decl.kind,
                            ctx.scope_depth,
                            &mut analysis.local_vars,
                            ctx,
                        );
                    }
                }
                ForInOfLeft::Pattern(_) => {
                    // Pattern LHS is an assignment target, not a declaration
                }
                ForInOfLeft::Expression(expr) => {
                    analyze_expression(expr, analysis, ctx, true);
                }
            }
            analyze_expression(&for_in_stmt.right, analysis, ctx, true);
            analyze_statement(&for_in_stmt.body, analysis, ctx);

            ctx.scope_depth -= 1;
            ctx.current_loop = parent_loop;
        }

        Statement::ForOf(for_of_stmt) => {
            let loop_id = ctx.loop_counter;
            ctx.loop_counter += 1;
            let parent_loop = ctx.current_loop;

            analysis.loop_contexts.push(LoopContext {
                id: loop_id,
                loop_type: LoopType::ForOf,
                label: ctx.current_label.take(),
                contains_yields: Vec::new(),
                parent_loop,
            });

            ctx.current_loop = Some(loop_id);
            ctx.scope_depth += 1;

            match &for_of_stmt.left {
                ForInOfLeft::Variable(decl) => {
                    for declarator in &decl.declarations {
                        collect_pattern_vars(
                            &declarator.pattern,
                            decl.kind,
                            ctx.scope_depth,
                            &mut analysis.local_vars,
                            ctx,
                        );
                    }
                }
                ForInOfLeft::Pattern(_) => {
                    // Pattern LHS is an assignment target, not a declaration
                }
                ForInOfLeft::Expression(expr) => {
                    analyze_expression(expr, analysis, ctx, true);
                }
            }
            analyze_expression(&for_of_stmt.right, analysis, ctx, true);
            analyze_statement(&for_of_stmt.body, analysis, ctx);

            ctx.scope_depth -= 1;
            ctx.current_loop = parent_loop;
        }

        Statement::Return(expr) => {
            if let Some(e) = expr {
                analyze_expression(e, analysis, ctx, true);
            }
        }

        Statement::Break(_) | Statement::Continue(_) => {}

        Statement::Throw(expr) => {
            analyze_expression(expr, analysis, ctx, true);
        }

        Statement::Try(try_stmt) => {
            let try_id = ctx.try_counter;
            ctx.try_counter += 1;
            let parent_try = ctx.current_try;

            analysis.try_contexts.push(TryContext {
                id: try_id,
                has_catch: try_stmt.handler.is_some(),
                has_finally: try_stmt.finalizer.is_some(),
                contains_yields: Vec::new(),
                parent_try,
            });

            ctx.current_try = Some(try_id);
            analyze_statements(&try_stmt.block, analysis, ctx);

            if let Some(handler) = &try_stmt.handler {
                ctx.scope_depth += 1;
                if let Some(param) = &handler.param {
                    collect_pattern_vars(
                        param,
                        VarKind::Let,
                        ctx.scope_depth,
                        &mut analysis.local_vars,
                        ctx,
                    );
                }
                analyze_statements(&handler.body, analysis, ctx);
                ctx.scope_depth -= 1;
            }

            ctx.current_try = parent_try;

            if let Some(finalizer) = &try_stmt.finalizer {
                analyze_statements(finalizer, analysis, ctx);
            }
        }

        Statement::Switch(switch_stmt) => {
            analyze_expression(&switch_stmt.discriminant, analysis, ctx, true);
            ctx.scope_depth += 1;
            for case in &switch_stmt.cases {
                if let Some(test) = &case.test {
                    analyze_expression(test, analysis, ctx, true);
                }
                analyze_statements(&case.consequent, analysis, ctx);
            }
            ctx.scope_depth -= 1;
        }

        Statement::Labeled(label, inner_stmt) => {
            ctx.current_label = Some(label.clone());
            analyze_statement(inner_stmt, analysis, ctx);
            ctx.current_label = None;
        }

        Statement::With(expr, inner_stmt) => {
            analyze_expression(expr, analysis, ctx, true);
            analyze_statement(inner_stmt, analysis, ctx);
        }

        Statement::FunctionDeclaration(_) => {
            // Function declarations create their own scope
            // We don't descend into them for generator analysis
        }

        Statement::ClassDeclaration(class_decl) => {
            // Method/field values and static blocks are their own function
            // scopes, but the heritage and computed keys are evaluated by the
            // class definition itself in the generator's execution context
            // (ClassDefinitionEvaluation, §15.7.14).
            analyze_class(
                class_decl.super_class.as_deref(),
                &class_decl.body,
                analysis,
                ctx,
            );
        }
    }
}

fn analyze_class(
    super_class: Option<&Expression>,
    elements: &[ClassElement],
    analysis: &mut GeneratorAnalysis,
    ctx: &mut AnalysisContext,
) {
    for expr in class_scope_exprs(super_class, elements) {
        analyze_expression(expr, analysis, ctx, true);
    }
}

fn analyze_expression(
    expr: &Expression,
    analysis: &mut GeneratorAnalysis,
    ctx: &mut AnalysisContext,
    in_expression_context: bool,
) {
    match expr {
        Expression::Yield(inner_expr, is_delegate) => {
            let yield_id = ctx.yield_counter;
            ctx.yield_counter += 1;

            if *is_delegate {
                analysis.has_yield_star = true;
            }

            let yield_point = YieldPoint {
                id: yield_id,
                is_delegate: *is_delegate,
                inside_try: ctx.current_try,
                inside_loop: ctx.current_loop,
                in_expression_context,
            };

            analysis.yield_points.push(yield_point);

            if let Some(try_id) = ctx.current_try
                && let Some(try_ctx) = analysis.try_contexts.iter_mut().find(|t| t.id == try_id)
            {
                try_ctx.contains_yields.push(yield_id);
            }

            if let Some(loop_id) = ctx.current_loop
                && let Some(loop_ctx) = analysis.loop_contexts.iter_mut().find(|l| l.id == loop_id)
            {
                loop_ctx.contains_yields.push(yield_id);
            }

            if let Some(inner) = inner_expr {
                analyze_expression(inner, analysis, ctx, true);
            }
        }

        Expression::Literal(_)
        | Expression::Identifier(_)
        | Expression::This
        | Expression::Super
        | Expression::NewTarget
        | Expression::ImportMeta
        | Expression::PrivateIdentifier(_) => {}

        Expression::Array(elements, _) => {
            for elem in elements.iter().flatten() {
                analyze_expression(elem, analysis, ctx, true);
            }
        }

        Expression::Object(props, _) => {
            for prop in props {
                if let PropertyKey::Computed(key_expr) = &prop.key {
                    analyze_expression(key_expr, analysis, ctx, true);
                }
                analyze_expression(&prop.value, analysis, ctx, true);
            }
        }

        Expression::Function(_) | Expression::ArrowFunction(_) => {
            // Don't descend into nested functions
        }

        Expression::Class(class_expr) => {
            analyze_class(
                class_expr.super_class.as_deref(),
                &class_expr.body,
                analysis,
                ctx,
            );
        }

        Expression::Unary(_, inner) => {
            analyze_expression(inner, analysis, ctx, true);
        }

        Expression::Binary(_, left, right) => {
            analyze_expression(left, analysis, ctx, true);
            analyze_expression(right, analysis, ctx, true);
        }

        Expression::Logical(_, left, right) => {
            analyze_expression(left, analysis, ctx, true);
            analyze_expression(right, analysis, ctx, true);
        }

        Expression::Update(_, _, inner) => {
            analyze_expression(inner, analysis, ctx, true);
        }

        Expression::Assign(_, left, right) => {
            analyze_expression(left, analysis, ctx, true);
            analyze_expression(right, analysis, ctx, true);
        }

        Expression::Conditional(test, consequent, alternate) => {
            analyze_expression(test, analysis, ctx, true);
            analyze_expression(consequent, analysis, ctx, true);
            analyze_expression(alternate, analysis, ctx, true);
        }

        Expression::Call(callee, args, _) => {
            analyze_expression(callee, analysis, ctx, true);
            for arg in args {
                analyze_expression(arg, analysis, ctx, true);
            }
        }

        Expression::New(callee, args, _) => {
            analyze_expression(callee, analysis, ctx, true);
            for arg in args {
                analyze_expression(arg, analysis, ctx, true);
            }
        }

        Expression::Member(object, prop, _) => {
            analyze_expression(object, analysis, ctx, true);
            if let MemberProperty::Computed(key) = prop {
                analyze_expression(key, analysis, ctx, true);
            }
        }

        Expression::OptionalChain(base, chain) => {
            analyze_expression(base, analysis, ctx, true);
            analyze_expression(chain, analysis, ctx, true);
        }

        Expression::Comma(exprs) | Expression::Sequence(exprs) => {
            for e in exprs {
                analyze_expression(e, analysis, ctx, true);
            }
        }

        Expression::Spread(inner) => {
            analyze_expression(inner, analysis, ctx, true);
        }

        Expression::Await(inner) => {
            analyze_expression(inner, analysis, ctx, true);
        }

        Expression::TaggedTemplate(tag, template) => {
            analyze_expression(tag, analysis, ctx, true);
            for expr in &template.expressions {
                analyze_expression(expr, analysis, ctx, true);
            }
        }

        Expression::Template(template) => {
            for expr in &template.expressions {
                analyze_expression(expr, analysis, ctx, true);
            }
        }

        Expression::Typeof(inner) | Expression::Void(inner) | Expression::Delete(inner) => {
            analyze_expression(inner, analysis, ctx, true);
        }

        Expression::Import(source, opts)
        | Expression::ImportDefer(source, opts)
        | Expression::ImportSource(source, opts) => {
            analyze_expression(source, analysis, ctx, true);
            if let Some(opts_expr) = opts {
                analyze_expression(opts_expr, analysis, ctx, true);
            }
        }
    }
}

fn collect_pattern_vars(
    pattern: &Pattern,
    kind: VarKind,
    scope_depth: usize,
    local_vars: &mut Vec<LocalVariable>,
    ctx: &mut AnalysisContext,
) {
    match pattern {
        Pattern::Identifier(name) => {
            if ctx.seen_vars.insert(name.clone()) {
                local_vars.push(LocalVariable {
                    name: name.clone(),
                    kind,
                    scope_depth,
                });
            }
        }
        Pattern::Array(elements) => {
            for elem in elements.iter().flatten() {
                match elem {
                    ArrayPatternElement::Pattern(p) => {
                        collect_pattern_vars(p, kind, scope_depth, local_vars, ctx);
                    }
                    ArrayPatternElement::Rest(p) => {
                        collect_pattern_vars(p, kind, scope_depth, local_vars, ctx);
                    }
                }
            }
        }
        Pattern::Object(props) => {
            for prop in props {
                match prop {
                    ObjectPatternProperty::KeyValue(_, p) => {
                        collect_pattern_vars(p, kind, scope_depth, local_vars, ctx);
                    }
                    ObjectPatternProperty::Shorthand(name) => {
                        if ctx.seen_vars.insert(name.clone()) {
                            local_vars.push(LocalVariable {
                                name: name.clone(),
                                kind,
                                scope_depth,
                            });
                        }
                    }
                    ObjectPatternProperty::Rest(p) => {
                        collect_pattern_vars(p, kind, scope_depth, local_vars, ctx);
                    }
                }
            }
        }
        Pattern::Assign(inner, _) => {
            collect_pattern_vars(inner, kind, scope_depth, local_vars, ctx);
        }
        Pattern::Rest(inner) => {
            collect_pattern_vars(inner, kind, scope_depth, local_vars, ctx);
        }
        Pattern::MemberExpression(_) => {}
    }
}

pub(crate) fn contains_yield(stmt: &Statement) -> bool {
    match stmt {
        Statement::Empty | Statement::Debugger | Statement::Break(_) | Statement::Continue(_) => {
            false
        }
        Statement::Expression(expr) => expr_contains_yield(expr),
        Statement::Block(stmts) => stmts.iter().any(contains_yield),
        Statement::Variable(decl) => decl
            .declarations
            .iter()
            .any(|d| d.init.as_ref().is_some_and(expr_contains_yield)),
        Statement::If(if_stmt) => {
            expr_contains_yield(&if_stmt.test)
                || contains_yield(&if_stmt.consequent)
                || if_stmt
                    .alternate
                    .as_ref()
                    .is_some_and(|s| contains_yield(s))
        }
        Statement::While(w) => expr_contains_yield(&w.test) || contains_yield(&w.body),
        Statement::DoWhile(d) => contains_yield(&d.body) || expr_contains_yield(&d.test),
        Statement::For(f) => {
            f.init.as_ref().is_some_and(|i| match i {
                ForInit::Variable(v) => v
                    .declarations
                    .iter()
                    .any(|d| d.init.as_ref().is_some_and(expr_contains_yield)),
                ForInit::Expression(e) => expr_contains_yield(e),
            }) || f.test.as_ref().is_some_and(expr_contains_yield)
                || f.update.as_ref().is_some_and(expr_contains_yield)
                || contains_yield(&f.body)
        }
        Statement::ForIn(f) => expr_contains_yield(&f.right) || contains_yield(&f.body),
        Statement::ForOf(f) => expr_contains_yield(&f.right) || contains_yield(&f.body),
        Statement::Return(e) => e.as_ref().is_some_and(expr_contains_yield),
        Statement::Throw(e) => expr_contains_yield(e),
        Statement::Try(t) => {
            t.block.iter().any(contains_yield)
                || t.handler
                    .as_ref()
                    .is_some_and(|h| h.body.iter().any(contains_yield))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|f| f.iter().any(contains_yield))
        }
        Statement::Switch(s) => {
            expr_contains_yield(&s.discriminant)
                || s.cases.iter().any(|c| {
                    c.test.as_ref().is_some_and(expr_contains_yield)
                        || c.consequent.iter().any(contains_yield)
                })
        }
        Statement::Labeled(_, inner) => contains_yield(inner),
        Statement::With(e, s) => expr_contains_yield(e) || contains_yield(s),
        Statement::FunctionDeclaration(_) => false,
        Statement::ClassDeclaration(c) => class_contains_yield(c.super_class.as_deref(), &c.body),
    }
}

fn class_contains_yield(super_class: Option<&Expression>, elements: &[ClassElement]) -> bool {
    class_scope_exprs(super_class, elements).any(expr_contains_yield)
}

pub(crate) fn expr_contains_yield(expr: &Expression) -> bool {
    match expr {
        Expression::Yield(_, _) => true,
        Expression::Literal(_)
        | Expression::Identifier(_)
        | Expression::This
        | Expression::Super
        | Expression::NewTarget
        | Expression::ImportMeta
        | Expression::PrivateIdentifier(_) => false,
        Expression::Array(elems, _) => elems.iter().flatten().any(expr_contains_yield),
        Expression::Object(props, _) => props.iter().any(|p| {
            matches!(&p.key, PropertyKey::Computed(e) if expr_contains_yield(e))
                || expr_contains_yield(&p.value)
        }),
        Expression::Function(_) | Expression::ArrowFunction(_) => false,
        Expression::Class(c) => class_contains_yield(c.super_class.as_deref(), &c.body),
        Expression::Unary(_, e)
        | Expression::Typeof(e)
        | Expression::Void(e)
        | Expression::Delete(e)
        | Expression::Spread(e)
        | Expression::Await(e)
        | Expression::Update(_, _, e) => expr_contains_yield(e),
        Expression::Import(e, opts)
        | Expression::ImportDefer(e, opts)
        | Expression::ImportSource(e, opts) => {
            expr_contains_yield(e) || opts.as_ref().is_some_and(|o| expr_contains_yield(o))
        }
        Expression::Binary(_, l, r)
        | Expression::Logical(_, l, r)
        | Expression::Assign(_, l, r) => expr_contains_yield(l) || expr_contains_yield(r),
        Expression::Conditional(t, c, a) => {
            expr_contains_yield(t) || expr_contains_yield(c) || expr_contains_yield(a)
        }
        Expression::Call(callee, args, _) | Expression::New(callee, args, _) => {
            expr_contains_yield(callee) || args.iter().any(expr_contains_yield)
        }
        Expression::Member(obj, prop, _) => {
            expr_contains_yield(obj)
                || matches!(prop, MemberProperty::Computed(e) if expr_contains_yield(e))
        }
        Expression::OptionalChain(base, chain) => {
            expr_contains_yield(base) || expr_contains_yield(chain)
        }
        Expression::Comma(exprs) | Expression::Sequence(exprs) => {
            exprs.iter().any(expr_contains_yield)
        }
        Expression::TaggedTemplate(tag, tpl) => {
            expr_contains_yield(tag) || tpl.expressions.iter().any(expr_contains_yield)
        }
        Expression::Template(tpl) => tpl.expressions.iter().any(expr_contains_yield),
    }
}

pub(crate) fn expr_contains_suspension(expr: &Expression) -> bool {
    match expr {
        Expression::Yield(_, _) | Expression::Await(_) => true,
        Expression::Literal(_)
        | Expression::Identifier(_)
        | Expression::This
        | Expression::Super
        | Expression::NewTarget
        | Expression::ImportMeta
        | Expression::PrivateIdentifier(_) => false,
        Expression::Array(elems, _) => elems.iter().flatten().any(expr_contains_suspension),
        Expression::Object(props, _) => props.iter().any(|p| {
            matches!(&p.key, PropertyKey::Computed(e) if expr_contains_suspension(e))
                || expr_contains_suspension(&p.value)
        }),
        Expression::Function(_) | Expression::ArrowFunction(_) => false,
        Expression::Class(c) => class_contains_suspension(c.super_class.as_deref(), &c.body),
        Expression::Unary(_, e)
        | Expression::Typeof(e)
        | Expression::Void(e)
        | Expression::Delete(e)
        | Expression::Spread(e)
        | Expression::Update(_, _, e) => expr_contains_suspension(e),
        Expression::Import(e, opts)
        | Expression::ImportDefer(e, opts)
        | Expression::ImportSource(e, opts) => {
            expr_contains_suspension(e)
                || opts.as_ref().is_some_and(|o| expr_contains_suspension(o))
        }
        Expression::Binary(_, l, r)
        | Expression::Logical(_, l, r)
        | Expression::Assign(_, l, r) => expr_contains_suspension(l) || expr_contains_suspension(r),
        Expression::Conditional(t, c, a) => {
            expr_contains_suspension(t)
                || expr_contains_suspension(c)
                || expr_contains_suspension(a)
        }
        Expression::Call(callee, args, _) | Expression::New(callee, args, _) => {
            expr_contains_suspension(callee) || args.iter().any(expr_contains_suspension)
        }
        Expression::Member(obj, prop, _) => {
            expr_contains_suspension(obj)
                || matches!(prop, MemberProperty::Computed(e) if expr_contains_suspension(e))
        }
        Expression::OptionalChain(base, chain) => {
            expr_contains_suspension(base) || expr_contains_suspension(chain)
        }
        Expression::Comma(exprs) | Expression::Sequence(exprs) => {
            exprs.iter().any(expr_contains_suspension)
        }
        Expression::TaggedTemplate(tag, tpl) => {
            expr_contains_suspension(tag) || tpl.expressions.iter().any(expr_contains_suspension)
        }
        Expression::Template(tpl) => tpl.expressions.iter().any(expr_contains_suspension),
    }
}

/// Checks if a statement is, or is reached through `if`/labeled statements from,
/// a Block that directly declares `await using`. It does not look through
/// loops, `try` or `switch`; `has_suspendable_await_using_block` extends the
/// reach to those containers.
pub(crate) fn has_block_with_await_using(stmt: &Statement) -> bool {
    match stmt {
        Statement::Block(stmts) => block_has_await_using(stmts),
        Statement::If(i) => {
            has_block_with_await_using(&i.consequent)
                || i.alternate
                    .as_ref()
                    .is_some_and(|s| has_block_with_await_using(s))
        }
        Statement::Labeled(_, inner) => has_block_with_await_using(inner),
        _ => false,
    }
}

pub(crate) fn block_has_await_using(stmts: &[Statement]) -> bool {
    stmts
        .iter()
        .any(|s| matches!(s, Statement::Variable(decl) if decl.kind == VarKind::AwaitUsing))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AwaitUsingScan {
    /// No `await using` block is reachable.
    None,
    /// Every reachable `await using` block can be emitted intact as the last
    /// statement of its own state, and lowering the containers on the way to it
    /// leaves their lexical scoping unobservable.
    Isolatable,
    /// Lowering a container would flatten a lexical scope that user code can
    /// observe, so the whole statement keeps running in the tree-walker.
    Blocked,
}

impl AwaitUsingScan {
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Blocked, _) | (_, Self::Blocked) => Self::Blocked,
            (Self::Isolatable, _) | (_, Self::Isolatable) => Self::Isolatable,
            _ => Self::None,
        }
    }

    fn blocked_unless_none(self) -> Self {
        match self {
            Self::None => Self::None,
            _ => Self::Blocked,
        }
    }
}

fn declares_lexical_binding(stmt: &Statement) -> bool {
    match stmt {
        Statement::Variable(decl) => decl.kind != VarKind::Var,
        Statement::ClassDeclaration(_) | Statement::FunctionDeclaration(_) => true,
        Statement::Labeled(_, inner) => declares_lexical_binding(inner),
        _ => false,
    }
}

/// A statement list the transform flattens into the enclosing state graph. Its
/// declarations lose their block scope once flattened, so a list that holds an
/// isolatable block next to a lexical declaration cannot be lowered.
fn scan_flattened_list<'a>(stmts: impl Iterator<Item = &'a Statement> + Clone) -> AwaitUsingScan {
    let combined = stmts.clone().fold(AwaitUsingScan::None, |acc, s| {
        acc.combine(scan_await_using(s))
    });
    if combined == AwaitUsingScan::Isolatable && stmts.into_iter().any(declares_lexical_binding) {
        AwaitUsingScan::Blocked
    } else {
        combined
    }
}

/// Scans a `try`/`catch`/`finally` clause's own statement list: if it
/// directly declares `await using` (no extra `{ }`), the clause body itself
/// is isolatable — its own scope is opened/closed around it, exactly like a
/// nested block that directly declares `await using` — otherwise fall back to
/// scanning it as a flattened list for a further nested isolatable block.
fn scan_clause_body(stmts: &[Statement]) -> AwaitUsingScan {
    if block_has_await_using(stmts) {
        AwaitUsingScan::Isolatable
    } else {
        scan_flattened_list(stmts.iter())
    }
}

fn scan_await_using(stmt: &Statement) -> AwaitUsingScan {
    match stmt {
        Statement::Block(stmts) if block_has_await_using(stmts) => AwaitUsingScan::Isolatable,
        Statement::Block(stmts) => scan_flattened_list(stmts.iter()),
        Statement::If(i) => scan_await_using(&i.consequent).combine(
            i.alternate
                .as_ref()
                .map_or(AwaitUsingScan::None, |a| scan_await_using(a)),
        ),
        Statement::Labeled(_, inner) => scan_await_using(inner),
        Statement::While(w) => scan_await_using(&w.body),
        Statement::DoWhile(d) => scan_await_using(&d.body),
        Statement::For(f) => {
            let body = scan_await_using(&f.body);
            match &f.init {
                Some(ForInit::Variable(decl)) if decl.kind != VarKind::Var => {
                    body.blocked_unless_none()
                }
                _ => body,
            }
        }
        Statement::ForIn(f) => scan_await_using(&f.body).blocked_unless_none(),
        Statement::ForOf(f) => {
            let body = scan_await_using(&f.body);
            match &f.left {
                ForInOfLeft::Variable(decl)
                    if matches!(decl.kind, VarKind::Using | VarKind::AwaitUsing) =>
                {
                    body.blocked_unless_none()
                }
                _ => body,
            }
        }
        Statement::Try(t) => {
            let mut result = scan_clause_body(&t.block);
            if let Some(handler) = &t.handler {
                result = result.combine(scan_clause_body(&handler.body));
            }
            if let Some(finalizer) = &t.finalizer {
                result = result.combine(scan_clause_body(finalizer));
            }
            result
        }
        Statement::Switch(s) => {
            scan_flattened_list(s.cases.iter().flat_map(|c| c.consequent.iter()))
        }
        Statement::With(_, body) => scan_await_using(body).blocked_unless_none(),
        _ => AwaitUsingScan::None,
    }
}

/// Checks if a statement reaches an `await using` block that an async function
/// can isolate into its own state, through the containers the state-machine
/// transform can lower: `if`, labeled statements, plain blocks, loop bodies,
/// `try`/`catch`/`finally` bodies and `switch` cases. The block's disposal then
/// suspends the function at its Awaits instead of draining the queue inline.
///
/// Containers whose lowering would flatten an observable lexical scope
/// (`for (let ..)`, `for-in`, `with`, a list declaring a binding beside the
/// block) are excluded and keep running in the tree-walker.
pub(crate) fn has_suspendable_await_using_block(stmt: &Statement) -> bool {
    scan_await_using(stmt) == AwaitUsingScan::Isolatable
}

pub(crate) fn contains_suspension(stmt: &Statement) -> bool {
    match stmt {
        Statement::Empty | Statement::Debugger | Statement::Break(_) | Statement::Continue(_) => {
            false
        }
        Statement::Expression(expr) => expr_contains_suspension(expr),
        Statement::Block(stmts) => stmts.iter().any(contains_suspension),
        Statement::Variable(decl) => decl
            .declarations
            .iter()
            .any(|d| d.init.as_ref().is_some_and(expr_contains_suspension)),
        Statement::If(if_stmt) => {
            expr_contains_suspension(&if_stmt.test)
                || contains_suspension(&if_stmt.consequent)
                || if_stmt
                    .alternate
                    .as_ref()
                    .is_some_and(|s| contains_suspension(s))
        }
        Statement::While(w) => expr_contains_suspension(&w.test) || contains_suspension(&w.body),
        Statement::DoWhile(d) => contains_suspension(&d.body) || expr_contains_suspension(&d.test),
        Statement::For(f) => {
            f.init.as_ref().is_some_and(|i| match i {
                ForInit::Variable(v) => v
                    .declarations
                    .iter()
                    .any(|d| d.init.as_ref().is_some_and(expr_contains_suspension)),
                ForInit::Expression(e) => expr_contains_suspension(e),
            }) || f.test.as_ref().is_some_and(expr_contains_suspension)
                || f.update.as_ref().is_some_and(expr_contains_suspension)
                || contains_suspension(&f.body)
        }
        Statement::ForIn(f) => expr_contains_suspension(&f.right) || contains_suspension(&f.body),
        Statement::ForOf(f) => expr_contains_suspension(&f.right) || contains_suspension(&f.body),
        Statement::Return(e) => e.as_ref().is_some_and(expr_contains_suspension),
        Statement::Throw(e) => expr_contains_suspension(e),
        Statement::Try(t) => {
            t.block.iter().any(contains_suspension)
                || t.handler
                    .as_ref()
                    .is_some_and(|h| h.body.iter().any(contains_suspension))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|f| f.iter().any(contains_suspension))
        }
        Statement::Switch(s) => {
            expr_contains_suspension(&s.discriminant)
                || s.cases.iter().any(|c| {
                    c.test.as_ref().is_some_and(expr_contains_suspension)
                        || c.consequent.iter().any(contains_suspension)
                })
        }
        Statement::Labeled(_, inner) => contains_suspension(inner),
        Statement::With(e, s) => expr_contains_suspension(e) || contains_suspension(s),
        Statement::FunctionDeclaration(_) => false,
        Statement::ClassDeclaration(c) => {
            class_contains_suspension(c.super_class.as_deref(), &c.body)
        }
    }
}

fn class_contains_suspension(super_class: Option<&Expression>, elements: &[ClassElement]) -> bool {
    class_scope_exprs(super_class, elements).any(expr_contains_suspension)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_yield(delegate: bool) -> Expression {
        Expression::Yield(None, delegate)
    }

    fn make_await() -> Expression {
        Expression::Await(ExprBox::new(Expression::Literal(Literal::Number(1.0))))
    }

    fn make_function_expr() -> FunctionExpr {
        FunctionExpr {
            name: None,
            params: vec![],
            body: Body::new(vec![]),
            is_async: false,
            is_generator: false,
            source_text: None,
            body_is_strict: false,
        }
    }

    fn let_class_expr(super_class: Option<Expression>, body: Vec<ClassElement>) -> Statement {
        Statement::Variable(VariableDeclaration {
            kind: VarKind::Let,
            declarations: vec![VariableDeclarator {
                pattern: Pattern::Identifier("C".to_string()),
                init: Some(Expression::Class(ClassExpr {
                    name: None,
                    super_class: super_class.map(Box::new),
                    body,
                    source_text: None,
                })),
            }],
        })
    }

    fn class_decl(super_class: Option<Expression>, body: Vec<ClassElement>) -> Statement {
        Statement::ClassDeclaration(ClassDecl {
            name: "C".to_string(),
            super_class: super_class.map(Box::new),
            body,
            source_text: None,
        })
    }

    fn computed_method(key: Expression) -> ClassElement {
        ClassElement::Method(ClassMethod {
            key: PropertyKey::Computed(ExprBox::new(key)),
            kind: ClassMethodKind::Method,
            value: make_function_expr(),
            is_static: false,
            computed: true,
        })
    }

    fn scan_first_statement(src: &str) -> bool {
        let program = crate::parser::Parser::new(&format!("async function f() {{ {src} }}"))
            .expect("parser init")
            .parse_program()
            .expect("parse program");
        let Some(Statement::FunctionDeclaration(f)) = program.body.as_slice().first() else {
            panic!("expected a function declaration");
        };
        has_suspendable_await_using_block(&f.body.as_slice()[0])
    }

    #[test]
    fn suspendable_await_using_block_through_containers() {
        let isolatable = [
            "{ await using a = null; }",
            "if (c) { await using a = null; } else { x(); }",
            "if (c) x(); else { await using a = null; }",
            "l: { await using a = null; }",
            "{ { await using a = null; } }",
            "try { { await using a = null; } } catch (e) {}",
            "try {} catch (e) { { await using a = null; } }",
            "try {} finally { { await using a = null; } }",
            "while (c) { await using a = null; }",
            "do { await using a = null; } while (c);",
            "for (;;) { await using a = null; }",
            "for (var i = 0; i < 2; i++) { await using a = null; }",
            "for (x of y) { await using a = null; }",
            "for (var x of y) { await using a = null; }",
            "for (let x of y) { await using a = null; }",
            "for (const x of y) { await using a = null; }",
            "for await (const x of y) { await using a = null; }",
            "for await (x of y) { { await using a = null; } }",
            "outer: while (c) { { await using a = null; } }",
            "switch (x) { case 1: { await using a = null; } break; }",
            "switch (x) { case 1: y(); { await using a = null; } default: z(); }",
        ];
        for src in isolatable {
            assert!(scan_first_statement(src), "expected isolatable: {src}");
        }
    }

    #[test]
    fn no_await_using_block_is_not_suspendable() {
        let none = [
            "await 0;",
            "await using a = null;",
            "{ let a = null; }",
            "try { x(); } catch (e) {}",
            "while (c) { x(); }",
            "for await (const x of y) { z(); }",
            "for (let i = 0; i < 2; i++) { x(); }",
            "switch (x) { case 1: y(); }",
            "async function g() { { await using a = null; } }",
        ];
        for src in none {
            assert!(!scan_first_statement(src), "expected no scan hit: {src}");
        }
    }

    #[test]
    fn lowering_that_would_flatten_a_lexical_scope_is_blocked() {
        let blocked = [
            "for (let i = 0; i < 3; i++) { { await using a = null; } }",
            "for (const i = 0; ;) { { await using a = null; } }",
            "while (c) { let j = i; { await using a = null; } }",
            "for (k in o) { { await using a = null; } }",
            "try { let x = 2; { await using a = null; } } finally {}",
            "try {} catch (e) { const x = 1; { await using a = null; } }",
            "try {} finally { class C {} { await using a = null; } }",
            "{ let x = 1; { await using a = null; } }",
            "with (o) { { await using a = null; } }",
            "switch (x) { case 1: let y = 1; case 2: { await using a = null; } }",
            "for (await using r of y) { { await using a = null; } }",
            "if (c) { await using a = null; } else { let x = 1; { await using b = null; } }",
        ];
        for src in blocked {
            assert!(!scan_first_statement(src), "expected blocked: {src}");
        }
    }

    #[test]
    fn test_simple_yields() {
        let body = vec![
            Statement::Expression(make_yield(false)),
            Statement::Expression(make_yield(false)),
        ];
        let analysis = analyze_generator_body(&body, &[]);

        assert_eq!(analysis.yield_points.len(), 2);
        assert_eq!(analysis.yield_points[0].id, 0);
        assert_eq!(analysis.yield_points[1].id, 1);
        assert!(!analysis.has_yield_star);
    }

    #[test]
    fn test_yield_star() {
        let body = vec![Statement::Expression(make_yield(true))];
        let analysis = analyze_generator_body(&body, &[]);

        assert_eq!(analysis.yield_points.len(), 1);
        assert!(analysis.yield_points[0].is_delegate);
        assert!(analysis.has_yield_star);
    }

    #[test]
    fn test_yield_in_try() {
        let body = vec![Statement::Try(TryStatement {
            block: vec![Statement::Expression(make_yield(false))],
            handler: None,
            finalizer: Some(vec![]),
        })];
        let analysis = analyze_generator_body(&body, &[]);

        assert_eq!(analysis.yield_points.len(), 1);
        assert_eq!(analysis.yield_points[0].inside_try, Some(0));
        assert_eq!(analysis.try_contexts.len(), 1);
        assert!(analysis.try_contexts[0].has_finally);
        assert!(!analysis.try_contexts[0].has_catch);
        assert_eq!(analysis.try_contexts[0].contains_yields, vec![0]);
    }

    #[test]
    fn test_yield_in_loop() {
        let body = vec![Statement::While(WhileStatement {
            test: Expression::Literal(Literal::Boolean(true)),
            body: Box::new(Statement::Expression(make_yield(false))),
        })];
        let analysis = analyze_generator_body(&body, &[]);

        assert_eq!(analysis.yield_points.len(), 1);
        assert_eq!(analysis.yield_points[0].inside_loop, Some(0));
        assert_eq!(analysis.loop_contexts.len(), 1);
        assert_eq!(analysis.loop_contexts[0].loop_type, LoopType::While);
        assert_eq!(analysis.loop_contexts[0].contains_yields, vec![0]);
    }

    #[test]
    fn test_local_variables() {
        let body = vec![Statement::Variable(VariableDeclaration {
            kind: VarKind::Let,
            declarations: vec![
                VariableDeclarator {
                    pattern: Pattern::Identifier("x".to_string()),
                    init: Some(Expression::Literal(Literal::Number(1.0))),
                },
                VariableDeclarator {
                    pattern: Pattern::Identifier("y".to_string()),
                    init: None,
                },
            ],
        })];
        let analysis = analyze_generator_body(&body, &[]);

        assert_eq!(analysis.local_vars.len(), 2);
        assert_eq!(analysis.local_vars[0].name, "x");
        assert_eq!(analysis.local_vars[0].kind, VarKind::Let);
        assert_eq!(analysis.local_vars[1].name, "y");
    }

    #[test]
    fn test_params_as_locals() {
        let params = vec![
            Pattern::Identifier("a".to_string()),
            Pattern::Identifier("b".to_string()),
        ];
        let body = vec![];
        let analysis = analyze_generator_body(&body, &params);

        assert_eq!(analysis.local_vars.len(), 2);
        assert_eq!(analysis.local_vars[0].name, "a");
        assert_eq!(analysis.local_vars[1].name, "b");
    }

    #[test]
    fn test_nested_try_loop() {
        let body = vec![Statement::Try(TryStatement {
            block: vec![Statement::While(WhileStatement {
                test: Expression::Literal(Literal::Boolean(true)),
                body: Box::new(Statement::Expression(make_yield(false))),
            })],
            handler: Some(CatchClause {
                param: Some(Pattern::Identifier("e".to_string())),
                body: vec![],
            }),
            finalizer: None,
        })];
        let analysis = analyze_generator_body(&body, &[]);

        assert_eq!(analysis.yield_points.len(), 1);
        assert_eq!(analysis.yield_points[0].inside_try, Some(0));
        assert_eq!(analysis.yield_points[0].inside_loop, Some(0));

        assert_eq!(analysis.try_contexts.len(), 1);
        assert!(analysis.try_contexts[0].has_catch);
        assert!(!analysis.try_contexts[0].has_finally);

        assert_eq!(analysis.loop_contexts.len(), 1);
        assert_eq!(analysis.loop_contexts[0].loop_type, LoopType::While);

        assert_eq!(analysis.local_vars.len(), 1);
        assert_eq!(analysis.local_vars[0].name, "e");
    }

    #[test]
    fn test_contains_yield() {
        let stmt_with_yield = Statement::Expression(make_yield(false));
        let stmt_without_yield = Statement::Expression(Expression::Literal(Literal::Number(1.0)));

        assert!(contains_yield(&stmt_with_yield));
        assert!(!contains_yield(&stmt_without_yield));
    }

    #[test]
    fn test_yield_in_class_computed_method_key() {
        let body = vec![class_decl(None, vec![computed_method(make_yield(false))])];
        let analysis = analyze_generator_body(&body, &[]);

        assert_eq!(analysis.yield_points.len(), 1);
        assert!(contains_yield(&body[0]));
    }

    #[test]
    fn test_yield_in_class_heritage() {
        let body = vec![class_decl(Some(make_yield(false)), vec![])];
        let analysis = analyze_generator_body(&body, &[]);

        assert_eq!(analysis.yield_points.len(), 1);
        assert!(contains_yield(&body[0]));
    }

    #[test]
    fn test_await_in_class_computed_method_key() {
        let stmt = class_decl(None, vec![computed_method(make_await())]);

        assert!(contains_suspension(&stmt));
    }

    #[test]
    fn test_await_in_class_heritage() {
        let stmt = class_decl(Some(make_await()), vec![]);

        assert!(contains_suspension(&stmt));
    }

    #[test]
    fn test_yield_in_class_expression_heritage() {
        let body = vec![let_class_expr(Some(make_yield(false)), vec![])];
        let analysis = analyze_generator_body(&body, &[]);

        assert_eq!(analysis.yield_points.len(), 1);
        assert!(contains_yield(&body[0]));
    }

    #[test]
    fn test_yield_in_class_expression_computed_method_key() {
        let body = vec![let_class_expr(
            None,
            vec![computed_method(make_yield(false))],
        )];
        let analysis = analyze_generator_body(&body, &[]);

        assert_eq!(analysis.yield_points.len(), 1);
        assert!(contains_yield(&body[0]));
    }

    #[test]
    fn test_yield_in_nested_class_expression_heritage() {
        let inner = Expression::Class(ClassExpr {
            name: None,
            super_class: Some(Box::new(make_yield(false))),
            body: vec![],
            source_text: None,
        });
        let body = vec![class_decl(Some(inner), vec![])];
        let analysis = analyze_generator_body(&body, &[]);

        assert_eq!(analysis.yield_points.len(), 1);
        assert!(contains_yield(&body[0]));
    }

    #[test]
    fn test_await_in_class_expression_computed_method_key() {
        let stmt = let_class_expr(None, vec![computed_method(make_await())]);

        assert!(contains_suspension(&stmt));
    }

    #[test]
    fn test_class_method_body_yield_is_not_a_class_scope_yield() {
        let mut method = make_function_expr();
        method.body = Body::new(vec![Statement::Expression(make_yield(false))]);
        let body = vec![let_class_expr(
            None,
            vec![ClassElement::Method(ClassMethod {
                key: PropertyKey::Identifier("m".to_string()),
                kind: ClassMethodKind::Method,
                value: method,
                is_static: false,
                computed: false,
            })],
        )];
        let analysis = analyze_generator_body(&body, &[]);

        assert!(analysis.yield_points.is_empty());
        assert!(!contains_yield(&body[0]));
    }

    #[test]
    fn test_yield_in_expression_context() {
        let body = vec![Statement::Variable(VariableDeclaration {
            kind: VarKind::Let,
            declarations: vec![VariableDeclarator {
                pattern: Pattern::Identifier("x".to_string()),
                init: Some(make_yield(false)),
            }],
        })];
        let analysis = analyze_generator_body(&body, &[]);

        assert_eq!(analysis.yield_points.len(), 1);
        assert!(analysis.yield_points[0].in_expression_context);
    }
}
