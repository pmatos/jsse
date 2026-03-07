use crate::ast::*;
use crate::interpreter::generator_analysis::*;
use crate::types::JsValue;
use std::collections::HashMap;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GeneratorStateMachine {
    pub states: Vec<GeneratorState>,
    pub local_vars: Vec<LocalVariable>,
    pub params: Vec<Pattern>,
    pub num_yields: usize,
    pub temp_vars: Vec<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GeneratorState {
    pub id: usize,
    pub statements: Vec<Statement>,
    pub terminator: StateTerminator,
}

#[derive(Debug, Clone)]
pub enum StateTerminator {
    Yield {
        value: Option<Expression>,
        is_delegate: bool,
        resume_state: usize,
        sent_value_binding: Option<SentValueBinding>,
    },
    Return(Option<Expression>),
    Throw(Expression),
    Goto(usize),
    ConditionalGoto {
        condition: Expression,
        true_state: usize,
        false_state: usize,
    },
    TryEnter {
        try_state: usize,
        catch_state: Option<CatchInfo>,
        finally_state: Option<usize>,
        after_state: usize,
    },
    TryExit {
        after_state: usize,
    },
    EnterCatch {
        body_state: usize,
        param: Option<Pattern>,
    },
    EnterFinally {
        body_state: usize,
    },
    SwitchDispatch {
        discriminant: Expression,
        cases: Vec<SwitchCaseTarget>,
        default_state: Option<usize>,
        after_state: usize,
    },
    ForOfInit {
        iterable: Expression,
        iter_var: String,
        #[allow(dead_code)]
        next_var: String,
        #[allow(dead_code)]
        left: ForInOfLeft,
        head_state: usize,
        #[allow(dead_code)]
        after_state: usize,
        is_await: bool,
    },
    ForOfHead {
        iter_var: String,
        #[allow(dead_code)]
        next_var: String,
        left: ForInOfLeft,
        body_state: usize,
        after_state: usize,
        is_await: bool,
    },
    Await {
        value: Expression,
        resume_state: usize,
        sent_value_binding: Option<SentValueBinding>,
    },
    Completed,
}

#[derive(Debug, Clone)]
pub struct CatchInfo {
    pub state: usize,
    pub param: Option<Pattern>,
}

#[derive(Debug, Clone)]
pub struct SwitchCaseTarget {
    pub test: Expression,
    pub state: usize,
}

#[derive(Debug, Clone)]
pub struct SentValueBinding {
    pub kind: SentValueBindingKind,
}

#[derive(Debug, Clone)]
pub enum SentValueBindingKind {
    Variable(String),
    Pattern(Pattern),
    #[allow(dead_code)]
    Discard,
    InlineYield {
        yield_target: usize,
        prev_sent: Vec<JsValue>,
    },
}

struct TransformContext {
    states: Vec<GeneratorState>,
    current_state_id: usize,
    current_statements: Vec<Statement>,
    #[allow(dead_code)]
    analysis: GeneratorAnalysis,
    yield_counter: usize,
    break_targets: HashMap<Option<String>, usize>,
    continue_targets: HashMap<Option<String>, usize>,
    try_stack: Vec<TryInfo>,
    temp_vars: Vec<String>,
    is_async: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TryInfo {
    catch_state: Option<CatchInfo>,
    finally_state: Option<usize>,
    after_state: usize,
}

impl TransformContext {
    fn new(analysis: GeneratorAnalysis, is_async: bool) -> Self {
        Self {
            states: Vec::new(),
            current_state_id: 0,
            current_statements: Vec::new(),
            analysis,
            yield_counter: 0,
            break_targets: HashMap::new(),
            continue_targets: HashMap::new(),
            try_stack: Vec::new(),
            temp_vars: Vec::new(),
            is_async,
        }
    }

    fn new_temp_var(&mut self, prefix: &str) -> String {
        let name = format!("${}_{}", prefix, self.yield_counter);
        self.temp_vars.push(name.clone());
        name
    }

    fn new_state(&mut self) -> usize {
        let id = self.states.len();
        self.states.push(GeneratorState {
            id,
            statements: Vec::new(),
            terminator: StateTerminator::Completed,
        });
        id
    }

    fn finalize_current_state(&mut self, terminator: StateTerminator) {
        if self.current_state_id < self.states.len() {
            self.states[self.current_state_id].statements =
                std::mem::take(&mut self.current_statements);
            self.states[self.current_state_id].terminator = terminator;
        }
    }

    fn emit_statement(&mut self, stmt: Statement) {
        self.current_statements.push(stmt);
    }
}

pub fn transform_generator(body: &[Statement], params: &[Pattern]) -> GeneratorStateMachine {
    transform_generator_inner(body, params, false)
}

pub fn transform_async_generator(body: &[Statement], params: &[Pattern]) -> GeneratorStateMachine {
    transform_generator_inner(body, params, true)
}

fn transform_generator_inner(
    body: &[Statement],
    params: &[Pattern],
    is_async: bool,
) -> GeneratorStateMachine {
    let analysis = analyze_generator_body(body, params);

    if analysis.yield_points.is_empty() && !is_async {
        return create_simple_machine(body, params, &analysis);
    }

    // For async generators, also check for await expressions
    if is_async && analysis.yield_points.is_empty() && !body.iter().any(contains_suspension) {
        return create_simple_machine(body, params, &analysis);
    }

    let mut ctx = TransformContext::new(analysis.clone(), is_async);

    let start_state = ctx.new_state();
    ctx.current_state_id = start_state;

    let end_state = ctx.new_state();

    transform_statements(body, &mut ctx, end_state);

    if !matches!(
        ctx.states[ctx.current_state_id].terminator,
        StateTerminator::Return(_) | StateTerminator::Throw(_)
    ) {
        ctx.finalize_current_state(StateTerminator::Goto(end_state));
    }

    ctx.states[end_state].terminator = StateTerminator::Completed;

    GeneratorStateMachine {
        states: ctx.states,
        local_vars: analysis.local_vars,
        params: params.to_vec(),
        num_yields: analysis.yield_points.len(),
        temp_vars: ctx.temp_vars,
    }
}

fn create_simple_machine(
    body: &[Statement],
    params: &[Pattern],
    analysis: &GeneratorAnalysis,
) -> GeneratorStateMachine {
    GeneratorStateMachine {
        states: vec![GeneratorState {
            id: 0,
            statements: body.to_vec(),
            terminator: StateTerminator::Completed,
        }],
        local_vars: analysis.local_vars.clone(),
        params: params.to_vec(),
        num_yields: 0,
        temp_vars: vec![],
    }
}

fn stmt_has_suspension(stmt: &Statement, is_async: bool) -> bool {
    if is_async {
        contains_suspension(stmt)
    } else {
        contains_yield(stmt)
    }
}

fn expr_has_suspension(expr: &Expression, is_async: bool) -> bool {
    if is_async {
        expr_contains_suspension(expr)
    } else {
        expr_contains_yield(expr)
    }
}

fn transform_statements(stmts: &[Statement], ctx: &mut TransformContext, after_state: usize) {
    for (i, stmt) in stmts.iter().enumerate() {
        let is_last = i == stmts.len() - 1;
        let next_after = if is_last { after_state } else { usize::MAX };

        if stmt_has_suspension(stmt, ctx.is_async) {
            transform_yielding_statement(stmt, ctx, next_after);
        } else {
            ctx.emit_statement(stmt.clone());
        }
    }
}

fn transform_yielding_statement(stmt: &Statement, ctx: &mut TransformContext, after_state: usize) {
    match stmt {
        Statement::Expression(expr) => {
            transform_yielding_expression(expr, ctx, after_state, None);
        }

        Statement::Block(stmts) => {
            transform_statements(stmts, ctx, after_state);
        }

        Statement::Variable(decl) => {
            transform_variable_declaration(decl, ctx, after_state);
        }

        Statement::If(if_stmt) => {
            transform_if_statement(if_stmt, ctx, after_state);
        }

        Statement::While(while_stmt) => {
            transform_while_statement(while_stmt, ctx, after_state);
        }

        Statement::DoWhile(do_while_stmt) => {
            transform_do_while_statement(do_while_stmt, ctx, after_state);
        }

        Statement::For(for_stmt) => {
            transform_for_statement(for_stmt, ctx, after_state);
        }

        Statement::ForIn(for_in_stmt) => {
            transform_for_in_statement(for_in_stmt, ctx, after_state);
        }

        Statement::ForOf(for_of_stmt) => {
            transform_for_of_statement(for_of_stmt, ctx, after_state);
        }

        Statement::Return(expr) => {
            if let Some(e) = expr {
                if expr_has_suspension(e, ctx.is_async) {
                    let temp_var = ctx.new_temp_var("return");
                    let binding = SentValueBindingKind::Variable(temp_var.clone());
                    transform_yielding_expression(e, ctx, usize::MAX, Some(binding));
                    ctx.finalize_current_state(StateTerminator::Return(Some(
                        Expression::Identifier(temp_var),
                    )));
                } else {
                    ctx.finalize_current_state(StateTerminator::Return(Some(e.clone())));
                }
            } else {
                ctx.finalize_current_state(StateTerminator::Return(None));
            }
            // Advance to a fresh unreachable state so subsequent terminators
            // (e.g. TryExit after a finally body) don't overwrite this Return.
            ctx.current_state_id = ctx.new_state();
        }

        Statement::Throw(expr) => {
            if expr_has_suspension(expr, ctx.is_async) {
                let temp_var = ctx.new_temp_var("throw");
                let binding = SentValueBindingKind::Variable(temp_var.clone());
                transform_yielding_expression(expr, ctx, usize::MAX, Some(binding));
                ctx.finalize_current_state(StateTerminator::Throw(Expression::Identifier(
                    temp_var,
                )));
            } else {
                ctx.finalize_current_state(StateTerminator::Throw(expr.clone()));
            }
            ctx.current_state_id = ctx.new_state();
        }

        Statement::Try(try_stmt) => {
            transform_try_statement(try_stmt, ctx, after_state);
        }

        Statement::Switch(switch_stmt) => {
            transform_switch_statement(switch_stmt, ctx, after_state);
        }

        Statement::Labeled(label, inner) => {
            transform_labeled_statement(label, inner, ctx, after_state);
        }

        Statement::With(expr, inner) => {
            if expr_has_suspension(expr, ctx.is_async) {
                let temp_var = ctx.new_temp_var("with");
                let binding = SentValueBindingKind::Variable(temp_var.clone());
                transform_yielding_expression(expr, ctx, usize::MAX, Some(binding));
                let new_with =
                    Statement::With(Expression::Identifier(temp_var), Box::new(*inner.clone()));
                if stmt_has_suspension(inner, ctx.is_async) {
                    transform_yielding_statement(&new_with, ctx, after_state);
                } else {
                    ctx.emit_statement(new_with);
                }
            } else {
                ctx.emit_statement(Statement::With(expr.clone(), Box::new(Statement::Empty)));
                transform_yielding_statement(inner, ctx, after_state);
            }
        }

        _ => {
            ctx.emit_statement(stmt.clone());
        }
    }
}

fn transform_yielding_expression(
    expr: &Expression,
    ctx: &mut TransformContext,
    _after_state: usize,
    binding: Option<SentValueBindingKind>,
) {
    match expr {
        Expression::Yield(inner_expr, is_delegate) => {
            let yield_value = if let Some(inner) = inner_expr {
                if expr_has_suspension(inner, ctx.is_async) {
                    let temp_var = ctx.new_temp_var("yield_val");
                    let inner_binding = SentValueBindingKind::Variable(temp_var.clone());
                    transform_yielding_expression(inner, ctx, usize::MAX, Some(inner_binding));
                    Some(Expression::Identifier(temp_var))
                } else {
                    Some(*inner.clone())
                }
            } else {
                None
            };

            let resume_state = ctx.new_state();

            let sent_value_binding = binding.map(|b| SentValueBinding { kind: b });

            ctx.finalize_current_state(StateTerminator::Yield {
                value: yield_value,
                is_delegate: *is_delegate,
                resume_state,
                sent_value_binding,
            });

            ctx.current_state_id = resume_state;
            ctx.yield_counter += 1;
        }

        Expression::Await(inner_expr) if ctx.is_async => {
            let await_value = if expr_has_suspension(inner_expr, ctx.is_async) {
                let temp_var = ctx.new_temp_var("await_val");
                let inner_binding = SentValueBindingKind::Variable(temp_var.clone());
                transform_yielding_expression(inner_expr, ctx, usize::MAX, Some(inner_binding));
                Expression::Identifier(temp_var)
            } else {
                *inner_expr.clone()
            };

            let resume_state = ctx.new_state();
            let sent_value_binding = binding.map(|b| SentValueBinding { kind: b });

            ctx.finalize_current_state(StateTerminator::Await {
                value: await_value,
                resume_state,
                sent_value_binding,
            });

            ctx.current_state_id = resume_state;
            ctx.yield_counter += 1;
        }

        Expression::Conditional(test, consequent, alternate) => {
            if expr_has_suspension(test, ctx.is_async) {
                let temp_var = ctx.new_temp_var("cond_test");
                let test_binding = SentValueBindingKind::Variable(temp_var.clone());
                transform_yielding_expression(test, ctx, usize::MAX, Some(test_binding));

                let after_cond = ctx.new_state();
                let true_state = ctx.new_state();
                let false_state = ctx.new_state();

                ctx.finalize_current_state(StateTerminator::ConditionalGoto {
                    condition: Expression::Identifier(temp_var),
                    true_state,
                    false_state,
                });

                ctx.current_state_id = true_state;
                transform_yielding_expression(consequent, ctx, after_cond, binding.clone());
                ctx.finalize_current_state(StateTerminator::Goto(after_cond));

                ctx.current_state_id = false_state;
                transform_yielding_expression(alternate, ctx, after_cond, binding);
                ctx.finalize_current_state(StateTerminator::Goto(after_cond));

                ctx.current_state_id = after_cond;
            } else if expr_has_suspension(consequent, ctx.is_async)
                || expr_has_suspension(alternate, ctx.is_async)
            {
                let after_cond = ctx.new_state();
                let true_state = ctx.new_state();
                let false_state = ctx.new_state();

                ctx.finalize_current_state(StateTerminator::ConditionalGoto {
                    condition: *test.clone(),
                    true_state,
                    false_state,
                });

                ctx.current_state_id = true_state;
                if expr_has_suspension(consequent, ctx.is_async) {
                    transform_yielding_expression(consequent, ctx, after_cond, binding.clone());
                } else {
                    emit_expression_with_binding(consequent, &binding, ctx);
                }
                ctx.finalize_current_state(StateTerminator::Goto(after_cond));

                ctx.current_state_id = false_state;
                if expr_has_suspension(alternate, ctx.is_async) {
                    transform_yielding_expression(alternate, ctx, after_cond, binding);
                } else {
                    emit_expression_with_binding(alternate, &binding, ctx);
                }
                ctx.finalize_current_state(StateTerminator::Goto(after_cond));

                ctx.current_state_id = after_cond;
            }
        }

        Expression::Logical(op, left, right) => {
            if expr_has_suspension(left, ctx.is_async) {
                let temp_var = ctx.new_temp_var("logical");
                let left_binding = SentValueBindingKind::Variable(temp_var.clone());
                transform_yielding_expression(left, ctx, usize::MAX, Some(left_binding));

                if expr_has_suspension(right, ctx.is_async) {
                    let after_logical = ctx.new_state();
                    let eval_right_state = ctx.new_state();

                    let condition = match op {
                        LogicalOp::And => Expression::Identifier(temp_var.clone()),
                        LogicalOp::Or => Expression::Unary(
                            UnaryOp::Not,
                            Box::new(Expression::Identifier(temp_var.clone())),
                        ),
                        LogicalOp::NullishCoalescing => Expression::Binary(
                            BinaryOp::StrictNotEq,
                            Box::new(Expression::Identifier(temp_var.clone())),
                            Box::new(Expression::Literal(Literal::Null)),
                        ),
                    };

                    ctx.finalize_current_state(StateTerminator::ConditionalGoto {
                        condition,
                        true_state: eval_right_state,
                        false_state: after_logical,
                    });

                    ctx.current_state_id = eval_right_state;
                    transform_yielding_expression(right, ctx, after_logical, binding);
                    ctx.finalize_current_state(StateTerminator::Goto(after_logical));

                    ctx.current_state_id = after_logical;
                } else {
                    let combined = Expression::Logical(
                        *op,
                        Box::new(Expression::Identifier(temp_var)),
                        right.clone(),
                    );
                    emit_expression_with_binding(&combined, &binding, ctx);
                }
            } else if expr_has_suspension(right, ctx.is_async) {
                let after_logical = ctx.new_state();
                let eval_right_state = ctx.new_state();

                let condition = match op {
                    LogicalOp::And => *left.clone(),
                    LogicalOp::Or => Expression::Unary(UnaryOp::Not, left.clone()),
                    LogicalOp::NullishCoalescing => Expression::Binary(
                        BinaryOp::StrictNotEq,
                        left.clone(),
                        Box::new(Expression::Literal(Literal::Null)),
                    ),
                };

                ctx.finalize_current_state(StateTerminator::ConditionalGoto {
                    condition,
                    true_state: eval_right_state,
                    false_state: after_logical,
                });

                emit_expression_with_binding(left, &binding, ctx);

                ctx.current_state_id = eval_right_state;
                transform_yielding_expression(right, ctx, after_logical, binding);
                ctx.finalize_current_state(StateTerminator::Goto(after_logical));

                ctx.current_state_id = after_logical;
            }
        }

        Expression::Binary(op, left, right) => {
            if expr_has_suspension(left, ctx.is_async) {
                let temp_var = ctx.new_temp_var("binary_left");
                let left_binding = SentValueBindingKind::Variable(temp_var.clone());
                transform_yielding_expression(left, ctx, usize::MAX, Some(left_binding));

                if expr_has_suspension(right, ctx.is_async) {
                    let temp_var2 = ctx.new_temp_var("binary_right");
                    let right_binding = SentValueBindingKind::Variable(temp_var2.clone());
                    transform_yielding_expression(right, ctx, usize::MAX, Some(right_binding));

                    let combined = Expression::Binary(
                        *op,
                        Box::new(Expression::Identifier(temp_var)),
                        Box::new(Expression::Identifier(temp_var2)),
                    );
                    emit_expression_with_binding(&combined, &binding, ctx);
                } else {
                    let combined = Expression::Binary(
                        *op,
                        Box::new(Expression::Identifier(temp_var)),
                        right.clone(),
                    );
                    emit_expression_with_binding(&combined, &binding, ctx);
                }
            } else if expr_has_suspension(right, ctx.is_async) {
                let temp_var = ctx.new_temp_var("binary_right");
                let right_binding = SentValueBindingKind::Variable(temp_var.clone());
                transform_yielding_expression(right, ctx, usize::MAX, Some(right_binding));

                let combined = Expression::Binary(
                    *op,
                    left.clone(),
                    Box::new(Expression::Identifier(temp_var)),
                );
                emit_expression_with_binding(&combined, &binding, ctx);
            }
        }

        Expression::Call(callee, args) => {
            transform_call_expression(callee, args, binding, ctx);
        }

        Expression::New(callee, args) => {
            let mut temp_callee = *callee.clone();
            if expr_has_suspension(callee, ctx.is_async) {
                let temp_var = ctx.new_temp_var("new_callee");
                let callee_binding = SentValueBindingKind::Variable(temp_var.clone());
                transform_yielding_expression(callee, ctx, usize::MAX, Some(callee_binding));
                temp_callee = Expression::Identifier(temp_var);
            }

            let mut temp_args = Vec::new();
            for (i, arg) in args.iter().enumerate() {
                if let Expression::Spread(inner) = arg {
                    if expr_has_suspension(inner, ctx.is_async) {
                        let temp_var = ctx.new_temp_var(&format!("new_arg_{}", i));
                        let arg_binding = SentValueBindingKind::Variable(temp_var.clone());
                        transform_yielding_expression(inner, ctx, usize::MAX, Some(arg_binding));
                        temp_args.push(Expression::Spread(Box::new(Expression::Identifier(
                            temp_var,
                        ))));
                    } else {
                        temp_args.push(arg.clone());
                    }
                } else if expr_has_suspension(arg, ctx.is_async) {
                    let temp_var = ctx.new_temp_var(&format!("new_arg_{}", i));
                    let arg_binding = SentValueBindingKind::Variable(temp_var.clone());
                    transform_yielding_expression(arg, ctx, usize::MAX, Some(arg_binding));
                    temp_args.push(Expression::Identifier(temp_var));
                } else {
                    temp_args.push(arg.clone());
                }
            }

            let combined = Expression::New(Box::new(temp_callee), temp_args);
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Assign(op, left, right) => {
            if expr_has_suspension(right, ctx.is_async) {
                let temp_var = ctx.new_temp_var("assign");
                let right_binding = SentValueBindingKind::Variable(temp_var.clone());
                transform_yielding_expression(right, ctx, usize::MAX, Some(right_binding));

                let combined = Expression::Assign(
                    *op,
                    left.clone(),
                    Box::new(Expression::Identifier(temp_var)),
                );
                emit_expression_with_binding(&combined, &binding, ctx);
            } else if expr_has_suspension(left, ctx.is_async) {
                // Yield in LHS (e.g. destructuring defaults like [x = yield] = vals)
                // Evaluate RHS to temp, emit assignment as statement (yield propagates at runtime)
                let temp_var = ctx.new_temp_var("assign_rhs");
                ctx.emit_statement(Statement::Variable(crate::ast::VariableDeclaration {
                    kind: crate::ast::VarKind::Var,
                    declarations: vec![crate::ast::VariableDeclarator {
                        pattern: Pattern::Identifier(temp_var.clone()),
                        init: Some(*right.clone()),
                    }],
                }));
                let combined = Expression::Assign(
                    *op,
                    left.clone(),
                    Box::new(Expression::Identifier(temp_var)),
                );
                emit_expression_with_binding(&combined, &binding, ctx);
            }
        }

        Expression::Sequence(exprs) | Expression::Comma(exprs) => {
            for (i, e) in exprs.iter().enumerate() {
                let is_last = i == exprs.len() - 1;
                if expr_has_suspension(e, ctx.is_async) {
                    let b = if is_last { binding.clone() } else { None };
                    transform_yielding_expression(e, ctx, usize::MAX, b);
                } else if is_last {
                    emit_expression_with_binding(e, &binding, ctx);
                } else {
                    ctx.emit_statement(Statement::Expression(e.clone()));
                }
            }
        }

        Expression::Array(elements, trailing_flag) => {
            let mut new_elements = Vec::new();
            for (i, elem) in elements.iter().enumerate() {
                match elem {
                    Some(Expression::Spread(inner)) if expr_has_suspension(inner, ctx.is_async) => {
                        let temp_var = ctx.new_temp_var(&format!("arr_elem_{}", i));
                        let elem_binding = SentValueBindingKind::Variable(temp_var.clone());
                        transform_yielding_expression(inner, ctx, usize::MAX, Some(elem_binding));
                        new_elements.push(Some(Expression::Spread(Box::new(
                            Expression::Identifier(temp_var),
                        ))));
                    }
                    Some(e) if expr_has_suspension(e, ctx.is_async) => {
                        let temp_var = ctx.new_temp_var(&format!("arr_elem_{}", i));
                        let elem_binding = SentValueBindingKind::Variable(temp_var.clone());
                        transform_yielding_expression(e, ctx, usize::MAX, Some(elem_binding));
                        new_elements.push(Some(Expression::Identifier(temp_var)));
                    }
                    other => {
                        new_elements.push(other.clone());
                    }
                }
            }
            let combined = Expression::Array(new_elements, *trailing_flag);
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Object(props) => {
            use crate::ast::{Property, PropertyKey};
            let mut new_props = Vec::new();
            for (i, prop) in props.iter().enumerate() {
                let key_has_suspension = match &prop.key {
                    PropertyKey::Computed(e) => expr_has_suspension(e, ctx.is_async),
                    _ => false,
                };
                let new_key = if key_has_suspension {
                    if let PropertyKey::Computed(e) = &prop.key {
                        let temp_var = ctx.new_temp_var(&format!("obj_key_{}", i));
                        let key_binding = SentValueBindingKind::Variable(temp_var.clone());
                        transform_yielding_expression(e, ctx, usize::MAX, Some(key_binding));
                        PropertyKey::Computed(Box::new(Expression::Identifier(temp_var)))
                    } else {
                        prop.key.clone()
                    }
                } else {
                    prop.key.clone()
                };

                let new_value = if let Expression::Spread(inner) = &prop.value {
                    if expr_has_suspension(inner, ctx.is_async) {
                        let temp_var = ctx.new_temp_var(&format!("obj_val_{}", i));
                        let val_binding = SentValueBindingKind::Variable(temp_var.clone());
                        transform_yielding_expression(inner, ctx, usize::MAX, Some(val_binding));
                        Expression::Spread(Box::new(Expression::Identifier(temp_var)))
                    } else {
                        prop.value.clone()
                    }
                } else if expr_has_suspension(&prop.value, ctx.is_async) {
                    let temp_var = ctx.new_temp_var(&format!("obj_val_{}", i));
                    let val_binding = SentValueBindingKind::Variable(temp_var.clone());
                    transform_yielding_expression(&prop.value, ctx, usize::MAX, Some(val_binding));
                    Expression::Identifier(temp_var)
                } else {
                    prop.value.clone()
                };

                new_props.push(Property {
                    key: new_key,
                    value: new_value,
                    kind: prop.kind,
                    computed: prop.computed,
                    shorthand: false, // Can't be shorthand anymore if we transformed
                    method: prop.method,
                });
            }
            let combined = Expression::Object(new_props);
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Member(obj, prop) => {
            let mut temp_obj = *obj.clone();
            if expr_has_suspension(obj, ctx.is_async) {
                let tv = ctx.new_temp_var("mem_obj");
                let b = SentValueBindingKind::Variable(tv.clone());
                transform_yielding_expression(obj, ctx, usize::MAX, Some(b));
                temp_obj = Expression::Identifier(tv);
            }
            match prop {
                MemberProperty::Computed(e) if expr_has_suspension(e, ctx.is_async) => {
                    let tv = ctx.new_temp_var("mem_prop");
                    let b = SentValueBindingKind::Variable(tv.clone());
                    transform_yielding_expression(e, ctx, usize::MAX, Some(b));
                    let combined = Expression::Member(
                        Box::new(temp_obj),
                        MemberProperty::Computed(Box::new(Expression::Identifier(tv))),
                    );
                    emit_expression_with_binding(&combined, &binding, ctx);
                }
                _ => {
                    let combined = Expression::Member(Box::new(temp_obj), prop.clone());
                    emit_expression_with_binding(&combined, &binding, ctx);
                }
            }
        }

        Expression::OptionalChain(base, chain) => {
            let mut temp_base = *base.clone();
            if expr_has_suspension(base, ctx.is_async) {
                let tv = ctx.new_temp_var("oc_base");
                let b = SentValueBindingKind::Variable(tv.clone());
                transform_yielding_expression(base, ctx, usize::MAX, Some(b));
                temp_base = Expression::Identifier(tv);
            }
            if expr_has_suspension(chain, ctx.is_async) {
                let base_var = ctx.new_temp_var("oc_bv");
                ctx.emit_statement(Statement::Variable(VariableDeclaration {
                    kind: VarKind::Var,
                    declarations: vec![VariableDeclarator {
                        pattern: Pattern::Identifier(base_var.clone()),
                        init: Some(temp_base),
                    }],
                }));
                let after_oc = ctx.new_state();
                let eval_chain_state = ctx.new_state();
                let skip_state = ctx.new_state();
                // temp != null catches both null and undefined via loose equality
                let condition = Expression::Binary(
                    BinaryOp::NotEq,
                    Box::new(Expression::Identifier(base_var.clone())),
                    Box::new(Expression::Literal(Literal::Null)),
                );
                ctx.finalize_current_state(StateTerminator::ConditionalGoto {
                    condition,
                    true_state: eval_chain_state,
                    false_state: skip_state,
                });
                // Skip: result is undefined
                ctx.current_state_id = skip_state;
                emit_expression_with_binding(
                    &Expression::Identifier("undefined".to_string()),
                    &binding,
                    ctx,
                );
                ctx.finalize_current_state(StateTerminator::Goto(after_oc));
                // Eval chain: substitute base and transform as regular expression
                ctx.current_state_id = eval_chain_state;
                let regular = oc_chain_to_regular_expr(chain, &base_var);
                if expr_has_suspension(&regular, ctx.is_async) {
                    transform_yielding_expression(&regular, ctx, usize::MAX, binding);
                } else {
                    emit_expression_with_binding(&regular, &binding, ctx);
                }
                ctx.finalize_current_state(StateTerminator::Goto(after_oc));
                ctx.current_state_id = after_oc;
            } else {
                let combined = Expression::OptionalChain(Box::new(temp_base), chain.clone());
                emit_expression_with_binding(&combined, &binding, ctx);
            }
        }

        Expression::Unary(op, inner) => {
            let tv = ctx.new_temp_var("unary");
            let b = SentValueBindingKind::Variable(tv.clone());
            transform_yielding_expression(inner, ctx, usize::MAX, Some(b));
            let combined = Expression::Unary(*op, Box::new(Expression::Identifier(tv)));
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Typeof(inner) => {
            let tv = ctx.new_temp_var("typeof");
            let b = SentValueBindingKind::Variable(tv.clone());
            transform_yielding_expression(inner, ctx, usize::MAX, Some(b));
            let combined = Expression::Typeof(Box::new(Expression::Identifier(tv)));
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Void(inner) => {
            let tv = ctx.new_temp_var("void");
            let b = SentValueBindingKind::Variable(tv.clone());
            transform_yielding_expression(inner, ctx, usize::MAX, Some(b));
            let combined = Expression::Void(Box::new(Expression::Identifier(tv)));
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Delete(inner) => {
            let tv = ctx.new_temp_var("del");
            let b = SentValueBindingKind::Variable(tv.clone());
            transform_yielding_expression(inner, ctx, usize::MAX, Some(b));
            let combined = Expression::Delete(Box::new(Expression::Identifier(tv)));
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Update(op, prefix, inner) => {
            let tv = ctx.new_temp_var("upd");
            let b = SentValueBindingKind::Variable(tv.clone());
            transform_yielding_expression(inner, ctx, usize::MAX, Some(b));
            let combined = Expression::Update(*op, *prefix, Box::new(Expression::Identifier(tv)));
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Template(tpl) => {
            let mut new_exprs = Vec::new();
            for (i, e) in tpl.expressions.iter().enumerate() {
                if expr_has_suspension(e, ctx.is_async) {
                    let tv = ctx.new_temp_var(&format!("tpl_{}", i));
                    let b = SentValueBindingKind::Variable(tv.clone());
                    transform_yielding_expression(e, ctx, usize::MAX, Some(b));
                    new_exprs.push(Expression::Identifier(tv));
                } else {
                    new_exprs.push(e.clone());
                }
            }
            let combined = Expression::Template(TemplateLiteral {
                id: tpl.id,
                quasis: tpl.quasis.clone(),
                raw_quasis: tpl.raw_quasis.clone(),
                expressions: new_exprs,
            });
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::TaggedTemplate(tag, tpl) => {
            let mut temp_tag = *tag.clone();
            if expr_has_suspension(tag, ctx.is_async) {
                let tv = ctx.new_temp_var("tag_fn");
                let b = SentValueBindingKind::Variable(tv.clone());
                transform_yielding_expression(tag, ctx, usize::MAX, Some(b));
                temp_tag = Expression::Identifier(tv);
            }
            let mut new_exprs = Vec::new();
            for (i, e) in tpl.expressions.iter().enumerate() {
                if expr_has_suspension(e, ctx.is_async) {
                    let tv = ctx.new_temp_var(&format!("ttpl_{}", i));
                    let b = SentValueBindingKind::Variable(tv.clone());
                    transform_yielding_expression(e, ctx, usize::MAX, Some(b));
                    new_exprs.push(Expression::Identifier(tv));
                } else {
                    new_exprs.push(e.clone());
                }
            }
            let combined = Expression::TaggedTemplate(
                Box::new(temp_tag),
                TemplateLiteral {
                    id: tpl.id,
                    quasis: tpl.quasis.clone(),
                    raw_quasis: tpl.raw_quasis.clone(),
                    expressions: new_exprs,
                },
            );
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Spread(inner) => {
            let tv = ctx.new_temp_var("spread");
            let b = SentValueBindingKind::Variable(tv.clone());
            transform_yielding_expression(inner, ctx, usize::MAX, Some(b));
            let combined = Expression::Spread(Box::new(Expression::Identifier(tv)));
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Import(spec, opts)
        | Expression::ImportDefer(spec, opts)
        | Expression::ImportSource(spec, opts) => {
            let mut temp_spec = *spec.clone();
            if expr_has_suspension(spec, ctx.is_async) {
                let tv = ctx.new_temp_var("imp_spec");
                let b = SentValueBindingKind::Variable(tv.clone());
                transform_yielding_expression(spec, ctx, usize::MAX, Some(b));
                temp_spec = Expression::Identifier(tv);
            }
            let mut temp_opts = opts.clone().map(|o| *o);
            if let Some(o) = opts
                && expr_has_suspension(o, ctx.is_async)
            {
                let tv = ctx.new_temp_var("imp_opts");
                let b = SentValueBindingKind::Variable(tv.clone());
                transform_yielding_expression(o, ctx, usize::MAX, Some(b));
                temp_opts = Some(Expression::Identifier(tv));
            }
            let boxed_opts = temp_opts.map(Box::new);
            let combined = match expr {
                Expression::Import(_, _) => Expression::Import(Box::new(temp_spec), boxed_opts),
                Expression::ImportDefer(_, _) => {
                    Expression::ImportDefer(Box::new(temp_spec), boxed_opts)
                }
                Expression::ImportSource(_, _) => {
                    Expression::ImportSource(Box::new(temp_spec), boxed_opts)
                }
                _ => unreachable!(),
            };
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        _ => {
            emit_expression_with_binding(expr, &binding, ctx);
        }
    }
}

/// Convert an OptionalChain's chain expression into a regular expression
/// by substituting the base placeholder with a variable reference.
fn oc_chain_to_regular_expr(chain: &Expression, base_var: &str) -> Expression {
    match chain {
        Expression::Identifier(name) if name.is_empty() => {
            Expression::Identifier(base_var.to_string())
        }
        Expression::Identifier(name) => Expression::Member(
            Box::new(Expression::Identifier(base_var.to_string())),
            MemberProperty::Dot(name.clone()),
        ),
        Expression::Member(inner, prop) => {
            let inner_expr = oc_chain_to_regular_expr(inner, base_var);
            Expression::Member(Box::new(inner_expr), prop.clone())
        }
        Expression::Call(callee, args) => {
            let callee_expr = oc_chain_to_regular_expr(callee, base_var);
            Expression::Call(Box::new(callee_expr), args.clone())
        }
        other => other.clone(),
    }
}

fn transform_call_expression(
    callee: &Expression,
    args: &[Expression],
    binding: Option<SentValueBindingKind>,
    ctx: &mut TransformContext,
) {
    let mut temp_callee = callee.clone();
    if expr_has_suspension(callee, ctx.is_async) {
        let temp_var = ctx.new_temp_var("call_callee");
        let callee_binding = SentValueBindingKind::Variable(temp_var.clone());
        transform_yielding_expression(callee, ctx, usize::MAX, Some(callee_binding));
        temp_callee = Expression::Identifier(temp_var);
    }

    let mut temp_args = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if let Expression::Spread(inner) = arg {
            if expr_has_suspension(inner, ctx.is_async) {
                let temp_var = ctx.new_temp_var(&format!("call_arg_{}", i));
                let arg_binding = SentValueBindingKind::Variable(temp_var.clone());
                transform_yielding_expression(inner, ctx, usize::MAX, Some(arg_binding));
                temp_args.push(Expression::Spread(Box::new(Expression::Identifier(
                    temp_var,
                ))));
            } else {
                temp_args.push(arg.clone());
            }
        } else if expr_has_suspension(arg, ctx.is_async) {
            let temp_var = ctx.new_temp_var(&format!("call_arg_{}", i));
            let arg_binding = SentValueBindingKind::Variable(temp_var.clone());
            transform_yielding_expression(arg, ctx, usize::MAX, Some(arg_binding));
            temp_args.push(Expression::Identifier(temp_var));
        } else {
            temp_args.push(arg.clone());
        }
    }

    let combined = Expression::Call(Box::new(temp_callee), temp_args);
    emit_expression_with_binding(&combined, &binding, ctx);
}

fn emit_expression_with_binding(
    expr: &Expression,
    binding: &Option<SentValueBindingKind>,
    ctx: &mut TransformContext,
) {
    match binding {
        Some(SentValueBindingKind::Variable(name)) => {
            let assign = Expression::Assign(
                AssignOp::Assign,
                Box::new(Expression::Identifier(name.clone())),
                Box::new(expr.clone()),
            );
            ctx.emit_statement(Statement::Expression(assign));
        }
        Some(SentValueBindingKind::Pattern(pattern)) => {
            let decl = Statement::Variable(VariableDeclaration {
                kind: VarKind::Let,
                declarations: vec![VariableDeclarator {
                    pattern: pattern.clone(),
                    init: Some(expr.clone()),
                }],
            });
            ctx.emit_statement(decl);
        }
        Some(SentValueBindingKind::Discard)
        | Some(SentValueBindingKind::InlineYield { .. })
        | None => {
            ctx.emit_statement(Statement::Expression(expr.clone()));
        }
    }
}

fn transform_variable_declaration(
    decl: &VariableDeclaration,
    ctx: &mut TransformContext,
    _after_state: usize,
) {
    for declarator in &decl.declarations {
        if let Some(init) = &declarator.init {
            if expr_has_suspension(init, ctx.is_async) {
                let binding = match &declarator.pattern {
                    Pattern::Identifier(name) => SentValueBindingKind::Variable(name.clone()),
                    pat => SentValueBindingKind::Pattern(pat.clone()),
                };
                transform_yielding_expression(init, ctx, usize::MAX, Some(binding));
            } else {
                let stmt = Statement::Variable(VariableDeclaration {
                    kind: decl.kind,
                    declarations: vec![declarator.clone()],
                });
                ctx.emit_statement(stmt);
            }
        } else {
            let stmt = Statement::Variable(VariableDeclaration {
                kind: decl.kind,
                declarations: vec![declarator.clone()],
            });
            ctx.emit_statement(stmt);
        }
    }
}

fn transform_if_statement(if_stmt: &IfStatement, ctx: &mut TransformContext, after_state: usize) {
    let after_if = if after_state == usize::MAX {
        ctx.new_state()
    } else {
        after_state
    };

    if expr_has_suspension(&if_stmt.test, ctx.is_async) {
        let temp_var = ctx.new_temp_var("if_test");
        let test_binding = SentValueBindingKind::Variable(temp_var.clone());
        transform_yielding_expression(&if_stmt.test, ctx, usize::MAX, Some(test_binding));

        let true_state = ctx.new_state();
        let false_state = if if_stmt.alternate.is_some() {
            ctx.new_state()
        } else {
            after_if
        };

        ctx.finalize_current_state(StateTerminator::ConditionalGoto {
            condition: Expression::Identifier(temp_var),
            true_state,
            false_state,
        });

        ctx.current_state_id = true_state;
        if stmt_has_suspension(&if_stmt.consequent, ctx.is_async) {
            transform_yielding_statement(&if_stmt.consequent, ctx, after_if);
        } else {
            ctx.emit_statement(*if_stmt.consequent.clone());
        }
        ctx.finalize_current_state(StateTerminator::Goto(after_if));

        if let Some(alt) = &if_stmt.alternate {
            ctx.current_state_id = false_state;
            if stmt_has_suspension(alt, ctx.is_async) {
                transform_yielding_statement(alt, ctx, after_if);
            } else {
                ctx.emit_statement(*alt.clone());
            }
            ctx.finalize_current_state(StateTerminator::Goto(after_if));
        }

        ctx.current_state_id = after_if;
    } else {
        let true_state = ctx.new_state();
        let false_state = if if_stmt.alternate.is_some() {
            ctx.new_state()
        } else {
            after_if
        };

        ctx.finalize_current_state(StateTerminator::ConditionalGoto {
            condition: if_stmt.test.clone(),
            true_state,
            false_state,
        });

        ctx.current_state_id = true_state;
        if stmt_has_suspension(&if_stmt.consequent, ctx.is_async) {
            transform_yielding_statement(&if_stmt.consequent, ctx, after_if);
        } else {
            ctx.emit_statement(*if_stmt.consequent.clone());
        }
        ctx.finalize_current_state(StateTerminator::Goto(after_if));

        if let Some(alt) = &if_stmt.alternate {
            ctx.current_state_id = false_state;
            if stmt_has_suspension(alt, ctx.is_async) {
                transform_yielding_statement(alt, ctx, after_if);
            } else {
                ctx.emit_statement(*alt.clone());
            }
            ctx.finalize_current_state(StateTerminator::Goto(after_if));
        }

        ctx.current_state_id = after_if;
    }
}

fn transform_while_statement(
    while_stmt: &WhileStatement,
    ctx: &mut TransformContext,
    after_state: usize,
) {
    let after_loop = if after_state == usize::MAX {
        ctx.new_state()
    } else {
        after_state
    };

    let test_state = ctx.new_state();
    let body_state = ctx.new_state();

    ctx.finalize_current_state(StateTerminator::Goto(test_state));

    ctx.break_targets.insert(None, after_loop);
    ctx.continue_targets.insert(None, test_state);

    ctx.current_state_id = test_state;
    if expr_has_suspension(&while_stmt.test, ctx.is_async) {
        let temp_var = ctx.new_temp_var("while_test");
        let test_binding = SentValueBindingKind::Variable(temp_var.clone());
        transform_yielding_expression(&while_stmt.test, ctx, usize::MAX, Some(test_binding));

        ctx.finalize_current_state(StateTerminator::ConditionalGoto {
            condition: Expression::Identifier(temp_var),
            true_state: body_state,
            false_state: after_loop,
        });
    } else {
        ctx.finalize_current_state(StateTerminator::ConditionalGoto {
            condition: while_stmt.test.clone(),
            true_state: body_state,
            false_state: after_loop,
        });
    }

    ctx.current_state_id = body_state;
    if stmt_has_suspension(&while_stmt.body, ctx.is_async) {
        transform_yielding_statement(&while_stmt.body, ctx, test_state);
    } else {
        ctx.emit_statement(*while_stmt.body.clone());
    }
    ctx.finalize_current_state(StateTerminator::Goto(test_state));

    ctx.break_targets.remove(&None);
    ctx.continue_targets.remove(&None);

    ctx.current_state_id = after_loop;
}

fn transform_do_while_statement(
    do_while_stmt: &DoWhileStatement,
    ctx: &mut TransformContext,
    after_state: usize,
) {
    let after_loop = if after_state == usize::MAX {
        ctx.new_state()
    } else {
        after_state
    };

    let body_state = ctx.new_state();
    let test_state = ctx.new_state();

    ctx.finalize_current_state(StateTerminator::Goto(body_state));

    ctx.break_targets.insert(None, after_loop);
    ctx.continue_targets.insert(None, test_state);

    ctx.current_state_id = body_state;
    if stmt_has_suspension(&do_while_stmt.body, ctx.is_async) {
        transform_yielding_statement(&do_while_stmt.body, ctx, test_state);
    } else {
        ctx.emit_statement(*do_while_stmt.body.clone());
    }
    ctx.finalize_current_state(StateTerminator::Goto(test_state));

    ctx.current_state_id = test_state;
    if expr_has_suspension(&do_while_stmt.test, ctx.is_async) {
        let temp_var = ctx.new_temp_var("dowhile_test");
        let test_binding = SentValueBindingKind::Variable(temp_var.clone());
        transform_yielding_expression(&do_while_stmt.test, ctx, usize::MAX, Some(test_binding));

        ctx.finalize_current_state(StateTerminator::ConditionalGoto {
            condition: Expression::Identifier(temp_var),
            true_state: body_state,
            false_state: after_loop,
        });
    } else {
        ctx.finalize_current_state(StateTerminator::ConditionalGoto {
            condition: do_while_stmt.test.clone(),
            true_state: body_state,
            false_state: after_loop,
        });
    }

    ctx.break_targets.remove(&None);
    ctx.continue_targets.remove(&None);

    ctx.current_state_id = after_loop;
}

fn transform_for_statement(
    for_stmt: &ForStatement,
    ctx: &mut TransformContext,
    after_state: usize,
) {
    let after_loop = if after_state == usize::MAX {
        ctx.new_state()
    } else {
        after_state
    };

    if let Some(init) = &for_stmt.init {
        match init {
            ForInit::Variable(decl) => {
                if decl.declarations.iter().any(|d| {
                    d.init
                        .as_ref()
                        .is_some_and(|e| expr_has_suspension(e, ctx.is_async))
                }) {
                    transform_variable_declaration(decl, ctx, usize::MAX);
                } else {
                    ctx.emit_statement(Statement::Variable(decl.clone()));
                }
            }
            ForInit::Expression(expr) => {
                if expr_has_suspension(expr, ctx.is_async) {
                    transform_yielding_expression(expr, ctx, usize::MAX, None);
                } else {
                    ctx.emit_statement(Statement::Expression(expr.clone()));
                }
            }
        }
    }

    let test_state = ctx.new_state();
    let body_state = ctx.new_state();
    let update_state = ctx.new_state();

    ctx.finalize_current_state(StateTerminator::Goto(test_state));

    ctx.break_targets.insert(None, after_loop);
    ctx.continue_targets.insert(None, update_state);

    ctx.current_state_id = test_state;
    if let Some(test) = &for_stmt.test {
        if expr_has_suspension(test, ctx.is_async) {
            let temp_var = ctx.new_temp_var("for_test");
            let test_binding = SentValueBindingKind::Variable(temp_var.clone());
            transform_yielding_expression(test, ctx, usize::MAX, Some(test_binding));

            ctx.finalize_current_state(StateTerminator::ConditionalGoto {
                condition: Expression::Identifier(temp_var),
                true_state: body_state,
                false_state: after_loop,
            });
        } else {
            ctx.finalize_current_state(StateTerminator::ConditionalGoto {
                condition: test.clone(),
                true_state: body_state,
                false_state: after_loop,
            });
        }
    } else {
        ctx.finalize_current_state(StateTerminator::Goto(body_state));
    }

    ctx.current_state_id = body_state;
    if stmt_has_suspension(&for_stmt.body, ctx.is_async) {
        transform_yielding_statement(&for_stmt.body, ctx, update_state);
    } else {
        ctx.emit_statement(*for_stmt.body.clone());
    }
    ctx.finalize_current_state(StateTerminator::Goto(update_state));

    ctx.current_state_id = update_state;
    if let Some(update) = &for_stmt.update {
        if expr_has_suspension(update, ctx.is_async) {
            transform_yielding_expression(update, ctx, test_state, None);
        } else {
            ctx.emit_statement(Statement::Expression(update.clone()));
        }
    }
    ctx.finalize_current_state(StateTerminator::Goto(test_state));

    ctx.break_targets.remove(&None);
    ctx.continue_targets.remove(&None);

    ctx.current_state_id = after_loop;
}

fn transform_for_in_statement(
    _for_in_stmt: &ForInStatement,
    ctx: &mut TransformContext,
    _after_state: usize,
) {
    // For-in with yields is complex - for now emit as-is and let runtime handle
    // A full implementation would need to capture the iterator state
    ctx.emit_statement(Statement::Empty);
}

fn transform_for_of_statement(
    for_of_stmt: &ForOfStatement,
    ctx: &mut TransformContext,
    after_state: usize,
) {
    let after_loop = if after_state == usize::MAX {
        ctx.new_state()
    } else {
        after_state
    };

    let head_state = ctx.new_state();
    let body_state = ctx.new_state();

    let iter_var = ctx.new_temp_var("forofiter");
    let next_var = ctx.new_temp_var("forofnext");

    ctx.break_targets.insert(None, after_loop);
    ctx.continue_targets.insert(None, head_state);

    ctx.finalize_current_state(StateTerminator::ForOfInit {
        iterable: for_of_stmt.right.clone(),
        iter_var: iter_var.clone(),
        next_var: next_var.clone(),
        left: for_of_stmt.left.clone(),
        head_state,
        after_state: after_loop,
        is_await: for_of_stmt.is_await,
    });

    ctx.current_state_id = head_state;
    ctx.finalize_current_state(StateTerminator::ForOfHead {
        iter_var: iter_var.clone(),
        next_var: next_var.clone(),
        left: for_of_stmt.left.clone(),
        body_state,
        after_state: after_loop,
        is_await: for_of_stmt.is_await,
    });

    ctx.current_state_id = body_state;
    if stmt_has_suspension(&for_of_stmt.body, ctx.is_async) {
        transform_yielding_statement(&for_of_stmt.body, ctx, head_state);
    } else {
        ctx.emit_statement(*for_of_stmt.body.clone());
    }
    ctx.finalize_current_state(StateTerminator::Goto(head_state));

    ctx.break_targets.remove(&None);
    ctx.continue_targets.remove(&None);

    ctx.current_state_id = after_loop;
}

fn transform_try_statement(
    try_stmt: &TryStatement,
    ctx: &mut TransformContext,
    after_state: usize,
) {
    let after_try = if after_state == usize::MAX {
        ctx.new_state()
    } else {
        after_state
    };

    let try_body_state = ctx.new_state();

    let catch_info = try_stmt.handler.as_ref().map(|h| {
        let catch_entry_state = ctx.new_state();
        CatchInfo {
            state: catch_entry_state,
            param: h.param.clone(),
        }
    });

    let finally_entry_state = if try_stmt.finalizer.is_some() {
        Some(ctx.new_state())
    } else {
        None
    };

    ctx.finalize_current_state(StateTerminator::TryEnter {
        try_state: try_body_state,
        catch_state: catch_info.clone(),
        finally_state: finally_entry_state,
        after_state: after_try,
    });

    ctx.try_stack.push(TryInfo {
        catch_state: catch_info.clone(),
        finally_state: finally_entry_state,
        after_state: after_try,
    });

    ctx.current_state_id = try_body_state;
    transform_statements(&try_stmt.block, ctx, after_try);
    if let Some(fin_state) = finally_entry_state {
        ctx.finalize_current_state(StateTerminator::Goto(fin_state));
    } else {
        ctx.finalize_current_state(StateTerminator::Goto(after_try));
    }

    if let Some(ref info) = catch_info {
        let catch_body_state = ctx.new_state();
        ctx.current_state_id = info.state;
        ctx.finalize_current_state(StateTerminator::EnterCatch {
            body_state: catch_body_state,
            param: info.param.clone(),
        });

        ctx.current_state_id = catch_body_state;
        if let Some(handler) = &try_stmt.handler {
            transform_statements(&handler.body, ctx, after_try);
        }
        if let Some(fin_state) = finally_entry_state {
            ctx.finalize_current_state(StateTerminator::Goto(fin_state));
        } else {
            ctx.finalize_current_state(StateTerminator::Goto(after_try));
        }
    }

    if let Some(fin_entry_state) = finally_entry_state {
        let finally_body_state = ctx.new_state();
        ctx.current_state_id = fin_entry_state;
        ctx.finalize_current_state(StateTerminator::EnterFinally {
            body_state: finally_body_state,
        });

        ctx.current_state_id = finally_body_state;
        if let Some(finalizer) = &try_stmt.finalizer {
            transform_statements(finalizer, ctx, after_try);
        }
        ctx.finalize_current_state(StateTerminator::TryExit {
            after_state: after_try,
        });
    }

    ctx.try_stack.pop();
    ctx.current_state_id = after_try;
}

fn transform_switch_statement(
    switch_stmt: &SwitchStatement,
    ctx: &mut TransformContext,
    after_state: usize,
) {
    let after_switch = if after_state == usize::MAX {
        ctx.new_state()
    } else {
        after_state
    };

    ctx.break_targets.insert(None, after_switch);

    let mut temp_discriminant = switch_stmt.discriminant.clone();
    if expr_has_suspension(&switch_stmt.discriminant, ctx.is_async) {
        let temp_var = ctx.new_temp_var("switch_disc");
        let disc_binding = SentValueBindingKind::Variable(temp_var.clone());
        transform_yielding_expression(
            &switch_stmt.discriminant,
            ctx,
            usize::MAX,
            Some(disc_binding),
        );
        temp_discriminant = Expression::Identifier(temp_var);
    }

    let mut case_states = Vec::new();
    let mut case_targets = Vec::new();
    let mut default_state = None;

    for case in &switch_stmt.cases {
        let case_state = ctx.new_state();
        case_states.push(case_state);

        if let Some(test) = &case.test {
            case_targets.push(SwitchCaseTarget {
                test: test.clone(),
                state: case_state,
            });
        } else {
            default_state = Some(case_state);
        }
    }

    ctx.finalize_current_state(StateTerminator::SwitchDispatch {
        discriminant: temp_discriminant,
        cases: case_targets,
        default_state,
        after_state: after_switch,
    });

    for (i, case) in switch_stmt.cases.iter().enumerate() {
        ctx.current_state_id = case_states[i];
        let next_state = if i + 1 < case_states.len() {
            case_states[i + 1]
        } else {
            after_switch
        };

        if case
            .consequent
            .iter()
            .any(|s| stmt_has_suspension(s, ctx.is_async))
        {
            transform_statements(&case.consequent, ctx, next_state);
        } else {
            for stmt in &case.consequent {
                ctx.emit_statement(stmt.clone());
            }
        }
        ctx.finalize_current_state(StateTerminator::Goto(next_state));
    }

    ctx.break_targets.remove(&None);
    ctx.current_state_id = after_switch;
}

fn transform_labeled_statement(
    label: &str,
    stmt: &Statement,
    ctx: &mut TransformContext,
    after_state: usize,
) {
    let after_labeled = if after_state == usize::MAX {
        ctx.new_state()
    } else {
        after_state
    };

    ctx.break_targets
        .insert(Some(label.to_string()), after_labeled);

    if stmt_has_suspension(stmt, ctx.is_async) {
        transform_yielding_statement(stmt, ctx, after_labeled);
    } else {
        ctx.emit_statement(stmt.clone());
    }

    ctx.break_targets.remove(&Some(label.to_string()));
    ctx.finalize_current_state(StateTerminator::Goto(after_labeled));
    ctx.current_state_id = after_labeled;
}

pub fn transform_async_function(body: &[Statement], params: &[Pattern]) -> GeneratorStateMachine {
    let rewritten = rewrite_stmts_await_to_yield(body);
    let mut machine = transform_generator(&rewritten, params);
    rewrite_terminators_yield_to_await(&mut machine);
    machine
}

fn rewrite_terminators_yield_to_await(machine: &mut GeneratorStateMachine) {
    for state in &mut machine.states {
        let replacement = match &state.terminator {
            StateTerminator::Yield {
                value,
                is_delegate: false,
                resume_state,
                sent_value_binding,
            } => Some(StateTerminator::Await {
                value: value
                    .clone()
                    .unwrap_or(Expression::Identifier("undefined".to_string())),
                resume_state: *resume_state,
                sent_value_binding: sent_value_binding.clone(),
            }),
            _ => None,
        };
        if let Some(new_term) = replacement {
            state.terminator = new_term;
        }
    }
}

fn rewrite_stmts_await_to_yield(stmts: &[Statement]) -> Vec<Statement> {
    stmts.iter().map(rewrite_stmt_await_to_yield).collect()
}

fn rewrite_stmt_await_to_yield(stmt: &Statement) -> Statement {
    match stmt {
        Statement::Empty | Statement::Break(_) | Statement::Continue(_) | Statement::Debugger => {
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
                            pattern: d.pattern.clone(),
                            init: d.init.as_ref().map(rewrite_expr),
                        })
                        .collect(),
                }),
                ForInit::Expression(e) => ForInit::Expression(rewrite_expr(e)),
            }),
            test: f.test.as_ref().map(rewrite_expr),
            update: f.update.as_ref().map(rewrite_expr),
            body: Box::new(rewrite_stmt_await_to_yield(&f.body)),
        }),
        Statement::ForIn(f) => Statement::ForIn(ForInStatement {
            left: rewrite_for_left(&f.left),
            right: rewrite_expr(&f.right),
            body: Box::new(rewrite_stmt_await_to_yield(&f.body)),
        }),
        Statement::ForOf(f) => Statement::ForOf(ForOfStatement {
            left: rewrite_for_left(&f.left),
            right: rewrite_expr(&f.right),
            body: Box::new(rewrite_stmt_await_to_yield(&f.body)),
            is_await: f.is_await,
        }),
        Statement::Return(e) => Statement::Return(e.as_ref().map(rewrite_expr)),
        Statement::Throw(e) => Statement::Throw(rewrite_expr(e)),
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
        Expression::Await(inner) => Expression::Yield(Some(Box::new(rewrite_expr(inner))), false),
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
        Expression::Object(props) => Expression::Object(
            props
                .iter()
                .map(|p| Property {
                    key: match &p.key {
                        PropertyKey::Computed(e) => {
                            PropertyKey::Computed(Box::new(rewrite_expr(e)))
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
        ),
        Expression::Unary(op, e) => Expression::Unary(*op, Box::new(rewrite_expr(e))),
        Expression::Binary(op, l, r) => {
            Expression::Binary(*op, Box::new(rewrite_expr(l)), Box::new(rewrite_expr(r)))
        }
        Expression::Logical(op, l, r) => {
            Expression::Logical(*op, Box::new(rewrite_expr(l)), Box::new(rewrite_expr(r)))
        }
        Expression::Update(op, prefix, e) => {
            Expression::Update(*op, *prefix, Box::new(rewrite_expr(e)))
        }
        Expression::Assign(op, l, r) => {
            Expression::Assign(*op, Box::new(rewrite_expr(l)), Box::new(rewrite_expr(r)))
        }
        Expression::Conditional(t, c, a) => Expression::Conditional(
            Box::new(rewrite_expr(t)),
            Box::new(rewrite_expr(c)),
            Box::new(rewrite_expr(a)),
        ),
        Expression::Call(callee, args) => Expression::Call(
            Box::new(rewrite_expr(callee)),
            args.iter().map(rewrite_expr).collect(),
        ),
        Expression::New(callee, args) => Expression::New(
            Box::new(rewrite_expr(callee)),
            args.iter().map(rewrite_expr).collect(),
        ),
        Expression::Member(obj, prop) => Expression::Member(
            Box::new(rewrite_expr(obj)),
            match prop {
                MemberProperty::Computed(e) => MemberProperty::Computed(Box::new(rewrite_expr(e))),
                other => other.clone(),
            },
        ),
        Expression::OptionalChain(base, chain) => {
            Expression::OptionalChain(Box::new(rewrite_expr(base)), Box::new(rewrite_expr(chain)))
        }
        Expression::Comma(exprs) => Expression::Comma(exprs.iter().map(rewrite_expr).collect()),
        Expression::Spread(e) => Expression::Spread(Box::new(rewrite_expr(e))),
        Expression::Yield(inner, delegate) => {
            Expression::Yield(inner.as_ref().map(|e| Box::new(rewrite_expr(e))), *delegate)
        }
        Expression::TaggedTemplate(tag, tl) => Expression::TaggedTemplate(
            Box::new(rewrite_expr(tag)),
            TemplateLiteral {
                id: tl.id,
                quasis: tl.quasis.clone(),
                raw_quasis: tl.raw_quasis.clone(),
                expressions: tl.expressions.iter().map(rewrite_expr).collect(),
            },
        ),
        Expression::Template(tl) => Expression::Template(TemplateLiteral {
            id: tl.id,
            quasis: tl.quasis.clone(),
            raw_quasis: tl.raw_quasis.clone(),
            expressions: tl.expressions.iter().map(rewrite_expr).collect(),
        }),
        Expression::Typeof(e) => Expression::Typeof(Box::new(rewrite_expr(e))),
        Expression::Void(e) => Expression::Void(Box::new(rewrite_expr(e))),
        Expression::Delete(e) => Expression::Delete(Box::new(rewrite_expr(e))),
        Expression::Sequence(exprs) => {
            Expression::Sequence(exprs.iter().map(rewrite_expr).collect())
        }
        Expression::Import(spec, opts) => Expression::Import(
            Box::new(rewrite_expr(spec)),
            opts.as_ref().map(|o| Box::new(rewrite_expr(o))),
        ),
        Expression::ImportDefer(spec, opts) => Expression::ImportDefer(
            Box::new(rewrite_expr(spec)),
            opts.as_ref().map(|o| Box::new(rewrite_expr(o))),
        ),
        Expression::ImportSource(spec, opts) => Expression::ImportSource(
            Box::new(rewrite_expr(spec)),
            opts.as_ref().map(|o| Box::new(rewrite_expr(o))),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_yield() -> Expression {
        Expression::Yield(None, false)
    }

    fn make_yield_expr(val: f64) -> Expression {
        Expression::Yield(
            Some(Box::new(Expression::Literal(Literal::Number(val)))),
            false,
        )
    }

    #[test]
    fn test_simple_transform() {
        let body = vec![
            Statement::Expression(make_yield_expr(1.0)),
            Statement::Expression(make_yield_expr(2.0)),
        ];
        let sm = transform_generator(&body, &[]);

        assert_eq!(sm.num_yields, 2);
        assert!(sm.states.len() >= 3);
    }

    #[test]
    fn test_no_yields() {
        let body = vec![Statement::Expression(Expression::Literal(Literal::Number(
            42.0,
        )))];
        let sm = transform_generator(&body, &[]);

        assert_eq!(sm.num_yields, 0);
        assert_eq!(sm.states.len(), 1);
    }

    #[test]
    fn test_yield_in_variable() {
        let body = vec![Statement::Variable(VariableDeclaration {
            kind: VarKind::Let,
            declarations: vec![VariableDeclarator {
                pattern: Pattern::Identifier("x".to_string()),
                init: Some(make_yield()),
            }],
        })];
        let sm = transform_generator(&body, &[]);

        assert_eq!(sm.num_yields, 1);
        assert!(sm.states.len() >= 2);
    }

    #[test]
    fn test_while_with_yield() {
        let body = vec![Statement::While(WhileStatement {
            test: Expression::Literal(Literal::Boolean(true)),
            body: Box::new(Statement::Expression(make_yield())),
        })];
        let sm = transform_generator(&body, &[]);

        assert_eq!(sm.num_yields, 1);
        assert!(sm.states.len() >= 3);
    }

    #[test]
    fn test_try_with_yield() {
        let body = vec![Statement::Try(TryStatement {
            block: vec![Statement::Expression(make_yield())],
            handler: None,
            finalizer: Some(vec![Statement::Expression(Expression::Literal(
                Literal::Number(1.0),
            ))]),
        })];
        let sm = transform_generator(&body, &[]);

        assert_eq!(sm.num_yields, 1);
        let has_try_enter = sm.states.iter().any(|s| {
            matches!(
                s.terminator,
                StateTerminator::TryEnter {
                    finally_state: Some(_),
                    ..
                }
            )
        });
        assert!(has_try_enter);
    }
}
