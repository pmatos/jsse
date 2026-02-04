use crate::ast::*;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct GeneratorAnalysis {
    pub yield_points: Vec<YieldPoint>,
    pub local_vars: Vec<LocalVariable>,
    pub try_contexts: Vec<TryContext>,
    pub loop_contexts: Vec<LoopContext>,
    pub has_yield_star: bool,
}

#[derive(Debug, Clone)]
pub struct YieldPoint {
    pub id: usize,
    pub is_delegate: bool,
    pub inside_try: Option<usize>,
    pub inside_loop: Option<usize>,
    pub in_expression_context: bool,
}

#[derive(Debug, Clone)]
pub struct LocalVariable {
    pub name: String,
    pub kind: VarKind,
    pub scope_depth: usize,
}

#[derive(Debug, Clone)]
pub struct TryContext {
    pub id: usize,
    pub has_catch: bool,
    pub has_finally: bool,
    pub contains_yields: Vec<usize>,
    pub parent_try: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct LoopContext {
    pub id: usize,
    pub loop_type: LoopType,
    pub label: Option<String>,
    pub contains_yields: Vec<usize>,
    pub parent_loop: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopType {
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

pub fn analyze_generator_body(body: &[Statement], params: &[Pattern]) -> GeneratorAnalysis {
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
                ForInOfLeft::Pattern(pat) => {
                    collect_pattern_vars(
                        pat,
                        VarKind::Var,
                        ctx.scope_depth,
                        &mut analysis.local_vars,
                        ctx,
                    );
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
                ForInOfLeft::Pattern(pat) => {
                    collect_pattern_vars(
                        pat,
                        VarKind::Var,
                        ctx.scope_depth,
                        &mut analysis.local_vars,
                        ctx,
                    );
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

        Statement::FunctionDeclaration(_) | Statement::ClassDeclaration(_) => {
            // Function/class declarations create their own scope
            // We don't descend into them for generator analysis
        }
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

            if let Some(try_id) = ctx.current_try {
                if let Some(try_ctx) = analysis.try_contexts.iter_mut().find(|t| t.id == try_id) {
                    try_ctx.contains_yields.push(yield_id);
                }
            }

            if let Some(loop_id) = ctx.current_loop {
                if let Some(loop_ctx) = analysis.loop_contexts.iter_mut().find(|l| l.id == loop_id)
                {
                    loop_ctx.contains_yields.push(yield_id);
                }
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

        Expression::Array(elements) => {
            for elem in elements.iter().flatten() {
                analyze_expression(elem, analysis, ctx, true);
            }
        }

        Expression::Object(props) => {
            for prop in props {
                if let PropertyKey::Computed(key_expr) = &prop.key {
                    analyze_expression(key_expr, analysis, ctx, true);
                }
                analyze_expression(&prop.value, analysis, ctx, true);
            }
        }

        Expression::Function(_) | Expression::ArrowFunction(_) | Expression::Class(_) => {
            // Don't descend into nested functions/classes
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

        Expression::Call(callee, args) => {
            analyze_expression(callee, analysis, ctx, true);
            for arg in args {
                analyze_expression(arg, analysis, ctx, true);
            }
        }

        Expression::New(callee, args) => {
            analyze_expression(callee, analysis, ctx, true);
            for arg in args {
                analyze_expression(arg, analysis, ctx, true);
            }
        }

        Expression::Member(object, prop) => {
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

        Expression::Import(source) => {
            analyze_expression(source, analysis, ctx, true);
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
    }
}

pub fn contains_yield(stmt: &Statement) -> bool {
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
        Statement::FunctionDeclaration(_) | Statement::ClassDeclaration(_) => false,
    }
}

pub fn expr_contains_yield(expr: &Expression) -> bool {
    match expr {
        Expression::Yield(_, _) => true,
        Expression::Literal(_)
        | Expression::Identifier(_)
        | Expression::This
        | Expression::Super
        | Expression::NewTarget
        | Expression::ImportMeta
        | Expression::PrivateIdentifier(_) => false,
        Expression::Array(elems) => elems.iter().flatten().any(expr_contains_yield),
        Expression::Object(props) => props.iter().any(|p| {
            matches!(&p.key, PropertyKey::Computed(e) if expr_contains_yield(e))
                || expr_contains_yield(&p.value)
        }),
        Expression::Function(_) | Expression::ArrowFunction(_) | Expression::Class(_) => false,
        Expression::Unary(_, e)
        | Expression::Typeof(e)
        | Expression::Void(e)
        | Expression::Delete(e)
        | Expression::Spread(e)
        | Expression::Await(e)
        | Expression::Import(e)
        | Expression::Update(_, _, e) => expr_contains_yield(e),
        Expression::Binary(_, l, r)
        | Expression::Logical(_, l, r)
        | Expression::Assign(_, l, r) => expr_contains_yield(l) || expr_contains_yield(r),
        Expression::Conditional(t, c, a) => {
            expr_contains_yield(t) || expr_contains_yield(c) || expr_contains_yield(a)
        }
        Expression::Call(callee, args) | Expression::New(callee, args) => {
            expr_contains_yield(callee) || args.iter().any(expr_contains_yield)
        }
        Expression::Member(obj, prop) => {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_yield(delegate: bool) -> Expression {
        Expression::Yield(None, delegate)
    }

    fn make_yield_expr(expr: Expression, delegate: bool) -> Expression {
        Expression::Yield(Some(Box::new(expr)), delegate)
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
