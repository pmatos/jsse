use crate::ast::*;
use crate::interpreter::generator_analysis::*;
use crate::types::JsValue;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct GeneratorStateMachine {
    pub states: Vec<GeneratorState>,
    pub local_vars: Vec<LocalVariable>,
    pub params: Vec<Pattern>,
    pub num_yields: usize,
    pub temp_vars: Vec<String>,
    #[cfg(feature = "perf-counters")]
    pub(crate) perf_key: Option<crate::interpreter::perf_counters::BodyKey>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct GeneratorState {
    pub id: usize,
    pub body: Body,
    pub terminator: StateTerminator,
    /// Jumps that a yield-free statement in `body` can surface as a raw
    /// `break`/`continue` completion, with the terminator that stands in for
    /// the state's own when one does.
    pub inline_jumps: Vec<InlineJump>,
    /// Number of frames the driver's lexical scope stack must hold while this
    /// state executes (`sec-block-runtime-semantics-evaluation`). Stamped from
    /// `TransformContext::scope_depth` at the moment this state is sealed, so
    /// every state, however it's reached, carries its own static depth — the
    /// driver reconciles toward it on every dispatch, which is what makes
    /// break/continue/return/throw unwind the scope stack for free.
    pub scope_depth: usize,
    /// What the driver must do to the top of the scope stack before this
    /// state can run, when `scope_depth` alone doesn't say (a plain entry
    /// just needs a push; a `for`-head per-iteration frame needs a fresh
    /// copy even when the depth hasn't changed).
    pub scope_action: Option<ScopeAction>,
}

/// See [`GeneratorState::scope_action`].
#[derive(Debug, Clone)]
pub(crate) enum ScopeAction {
    /// Push a fresh empty declarative environment (`NewDeclarativeEnvironment`)
    /// when this state's `scope_depth` is deeper than the stack's current
    /// size. Used for plain blocks, `try`/`finally` blocks, and loop bodies —
    /// every one of them is a `Block` per grammar, and a block always starts
    /// empty; its own `let`/`const`/`class` names are hoisted into it the
    /// ordinary way once the state's statements run.
    OpenBlock,
    /// `CreatePerIterationEnvironment` (`sec-createperiterationenvironment`):
    /// push a fresh frame copying the named bindings' current values forward,
    /// replacing the frame already at this depth if one is there (the `for`
    /// loop's per-iteration frame lives at a *constant* depth across
    /// test/body/update, so a `for`-head refresh, unlike `OpenBlock`, cannot
    /// rely on a depth change to know when to fire). `bool` is whether the
    /// binding is `const` (spec still runs this for `const` heads; see
    /// `exec_for`, which this generalizes).
    CopyForward(Vec<(String, bool)>),
}

#[derive(Debug, Clone)]
pub(crate) struct InlineJump {
    pub kind: JumpKind,
    pub label: Option<String>,
    pub terminator: StateTerminator,
}

impl GeneratorState {
    /// The terminator to run instead of `self.terminator` when the state body
    /// completed with a `break`/`continue` that a native statement in it could
    /// not consume. `None` for any other completion.
    pub(crate) fn inline_jump_terminator(
        &self,
        completion: &crate::interpreter::types::Completion,
    ) -> Option<StateTerminator> {
        use crate::interpreter::types::Completion;
        if self.inline_jumps.is_empty() {
            return None;
        }
        let (kind, label) = match completion {
            Completion::Break(label, _) => (JumpKind::Break, label),
            Completion::Continue(label, _) => (JumpKind::Continue, label),
            _ => return None,
        };
        self.inline_jumps
            .iter()
            .find(|jump| jump.kind == kind && jump.label == *label)
            .map(|jump| jump.terminator.clone())
    }
}

#[derive(Debug, Clone)]
pub(crate) enum StateTerminator {
    Yield {
        value: Option<Expression>,
        is_delegate: bool,
        resume_state: usize,
        sent_value_binding: Option<SentValueBinding>,
    },
    Return(Option<Expression>),
    Throw(Expression),
    Goto(usize),
    LoopControl(LoopControlTarget),
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
        label_set: Vec<String>,
        #[allow(dead_code)]
        next_var: String,
        #[allow(dead_code)]
        left: ForInOfLeft,
        head_state: usize,
        #[allow(dead_code)]
        after_state: usize,
        is_await: bool,
        is_for_in: bool,
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
    /// Opens a block scope: creates the block's own `Environment` (a child of
    /// whatever env is active), pushes it onto the driver's `scope_stack`, and
    /// continues at `body_state` executing against it. Emitted only for a
    /// block that directly declares `await using` in a plain async function
    /// (never a generator or async generator) — see
    /// `has_block_with_await_using`.
    EnterScope {
        body_state: usize,
    },
    /// Closes the block scope most recently opened by `EnterScope`: pops it
    /// from `scope_stack` and disposes its resources (suspendably, at each
    /// `Await` of `DisposeResources`) before continuing at `after_state`.
    ExitScope {
        after_state: usize,
    },
    Completed,
}

/// State-machine target for an abrupt `break` or `continue` completion.
///
/// The depths describe the execution context that remains active at
/// `target_state`, allowing the state-machine drivers (sync/async generators
/// via `route_generator_loop_control`, async functions via
/// `route_loop_control!`) to run intervening finalizers and close only the
/// `for-of` iterators crossed by the jump.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoopControlTarget {
    pub target_state: usize,
    pub try_depth: usize,
    pub for_of_depth: usize,
    /// Number of block scopes (`EnterScope`/`ExitScope`) open when this
    /// target's loop/label was registered, so `route_loop_control!` never
    /// disposes a scope that lexically encloses the target itself. Only
    /// async functions emit `EnterScope`/`ExitScope`; generator routing does
    /// not consume this field.
    pub scope_depth: usize,
}

/// Clear IC sites in a sent-value binding pattern (the destructuring target of
/// `x = yield` / `x = await`). The pattern is applied to the resumed value before
/// the next `exec_body` switches to a state-body store, so any computed-key or
/// default-value sites in it run under the caller's handle and must be cleared.
fn clear_sent_value_binding(binding: &mut Option<SentValueBinding>) {
    if let Some(SentValueBinding {
        kind: SentValueBindingKind::Pattern(p),
    }) = binding
    {
        crate::ast::clear_pattern_ic_sites(p);
    }
}

/// Reset IC site ids in a terminator's expressions to UNASSIGNED. See
/// `ast::clear_expr_ic_sites`: terminator expressions run under the caller's IC
/// handle rather than any state body's store, so they must take the slow path.
fn clear_terminator_ic_sites(t: &mut StateTerminator) {
    use crate::ast::{clear_expr_ic_sites, clear_for_in_of_left, clear_pattern_ic_sites};
    match t {
        StateTerminator::Yield {
            value,
            sent_value_binding,
            ..
        } => {
            if let Some(v) = value {
                clear_expr_ic_sites(v);
            }
            clear_sent_value_binding(sent_value_binding);
        }
        StateTerminator::Return(v) => {
            if let Some(v) = v {
                clear_expr_ic_sites(v);
            }
        }
        StateTerminator::Throw(v) => clear_expr_ic_sites(v),
        StateTerminator::ConditionalGoto { condition, .. } => clear_expr_ic_sites(condition),
        StateTerminator::SwitchDispatch {
            discriminant,
            cases,
            ..
        } => {
            clear_expr_ic_sites(discriminant);
            for case in cases.iter_mut() {
                clear_expr_ic_sites(&mut case.test);
            }
        }
        // ForOfInit only evaluates `iterable` here; the `left` binding is applied
        // later in ForOfHead, which is where its sites are cleared.
        StateTerminator::ForOfInit { iterable, .. } => clear_expr_ic_sites(iterable),
        StateTerminator::ForOfHead { left, .. } => clear_for_in_of_left(left),
        StateTerminator::Await {
            value,
            sent_value_binding,
            ..
        } => {
            clear_expr_ic_sites(value);
            clear_sent_value_binding(sent_value_binding);
        }
        StateTerminator::TryEnter { catch_state, .. } => {
            if let Some(ci) = catch_state
                && let Some(p) = ci.param.as_mut()
            {
                clear_pattern_ic_sites(p);
            }
        }
        StateTerminator::EnterCatch { param, .. } => {
            if let Some(p) = param {
                clear_pattern_ic_sites(p);
            }
        }
        StateTerminator::Goto(_)
        | StateTerminator::LoopControl(_)
        | StateTerminator::TryExit { .. }
        | StateTerminator::EnterFinally { .. }
        | StateTerminator::EnterScope { .. }
        | StateTerminator::ExitScope { .. }
        | StateTerminator::Completed => {}
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CatchInfo {
    pub state: usize,
    pub param: Option<Pattern>,
}

#[derive(Debug, Clone)]
pub(crate) struct SwitchCaseTarget {
    pub test: Expression,
    pub state: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct SentValueBinding {
    pub kind: SentValueBindingKind,
}

#[derive(Debug, Clone)]
pub(crate) enum SentValueBindingKind {
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
    pending_inline_jumps: Vec<InlineJump>,
    #[allow(dead_code)]
    analysis: GeneratorAnalysis,
    yield_counter: usize,
    temp_counter: usize,
    break_targets: HashMap<Option<String>, LoopControlTarget>,
    continue_targets: HashMap<Option<String>, LoopControlTarget>,
    try_stack: Vec<TryInfo>,
    for_of_depth: usize,
    /// Number of lexical-scope-stack frames active at the current transform
    /// point. Includes ordinary blocks, catch bindings, `for`-head
    /// per-iteration frames, and suspendable `await using` scopes; stamped
    /// onto states and loop-control targets for reconciliation and disposal.
    scope_depth: usize,
    iteration_labels: Vec<String>,
    temp_vars: Vec<String>,
    generated_temps: HashSet<String>,
    is_async: bool,
    detect_for_await: bool,
    with_scopes: Vec<String>,
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
            pending_inline_jumps: Vec::new(),
            analysis,
            yield_counter: 0,
            temp_counter: 0,
            break_targets: HashMap::new(),
            continue_targets: HashMap::new(),
            try_stack: Vec::new(),
            for_of_depth: 0,
            scope_depth: 0,
            iteration_labels: Vec::new(),
            temp_vars: Vec::new(),
            generated_temps: HashSet::new(),
            is_async,
            detect_for_await: false,
            with_scopes: Vec::new(),
        }
    }

    fn new_temp_var(&mut self, prefix: &str) -> String {
        let id = self.temp_counter;
        self.temp_counter += 1;
        let name = format!("${}_{}", prefix, id);
        self.temp_vars.push(name.clone());
        self.generated_temps.insert(name.clone());
        name
    }

    fn new_state(&mut self) -> usize {
        let id = self.states.len();
        self.states.push(GeneratorState {
            id,
            body: Body::new(Vec::new()),
            terminator: StateTerminator::Completed,
            inline_jumps: Vec::new(),
            scope_depth: 0,
            scope_action: None,
        });
        id
    }

    fn finalize_current_state(&mut self, terminator: StateTerminator) {
        if self.current_state_id < self.states.len() {
            let mut stmts = std::mem::take(&mut self.current_statements);
            if !self.with_scopes.is_empty() && !stmts.is_empty() {
                let block = Statement::Block(stmts);
                let mut wrapped = block;
                for with_var in self.with_scopes.iter().rev() {
                    wrapped = Statement::With(
                        Expression::Identifier(with_var.clone()),
                        Box::new(wrapped),
                    );
                }
                stmts = vec![wrapped];
            }
            let mut body = Body::new(stmts);
            crate::ast::assign_ic_sites_for_body(&mut body);
            // Terminator expressions (yield/await/return/throw values, branch and
            // switch conditions, for-of iterables) are evaluated by the state
            // driver under the caller's IC handle, not this state body's store,
            // so their site ids must stay UNASSIGNED (IC slow path).
            let mut terminator = terminator;
            clear_terminator_ic_sites(&mut terminator);
            self.states[self.current_state_id].body = body;
            self.states[self.current_state_id].terminator = terminator;
            self.states[self.current_state_id].inline_jumps =
                std::mem::take(&mut self.pending_inline_jumps);
            self.states[self.current_state_id].scope_depth = self.scope_depth;
        }
    }

    fn emit_statement(&mut self, stmt: Statement) {
        self.record_inline_jumps(&stmt);
        self.current_statements.push(stmt);
    }

    /// A statement emitted verbatim runs natively, so a `break`/`continue` it
    /// cannot consume surfaces as a raw completion from the state body. Record
    /// where each such jump must go so the driver can honour it.
    fn record_inline_jumps(&mut self, stmt: &Statement) {
        if self.break_targets.is_empty() && self.continue_targets.is_empty() {
            return;
        }
        for (kind, label) in escaping_jumps(stmt) {
            let recorded = self
                .pending_inline_jumps
                .iter()
                .any(|jump| jump.kind == kind && jump.label == label);
            if recorded {
                continue;
            }
            if let Some(target) = self.jump_target(kind, &label) {
                let terminator = StateTerminator::LoopControl(target);
                self.pending_inline_jumps.push(InlineJump {
                    kind,
                    label,
                    terminator,
                });
            }
        }
    }

    fn jump_target(&self, kind: JumpKind, label: &Option<String>) -> Option<LoopControlTarget> {
        let targets = match kind {
            JumpKind::Break => &self.break_targets,
            JumpKind::Continue => &self.continue_targets,
        };
        targets.get(label).copied()
    }

    fn loop_control_target(&self, target_state: usize, for_of_depth: usize) -> LoopControlTarget {
        LoopControlTarget {
            target_state,
            try_depth: self.try_stack.len(),
            for_of_depth,
            scope_depth: self.scope_depth,
        }
    }

    fn install_labeled_continue_targets(
        &mut self,
        labels: &[String],
        target: LoopControlTarget,
    ) -> Vec<(String, Option<LoopControlTarget>)> {
        labels
            .iter()
            .cloned()
            .map(|label| {
                let previous = self.continue_targets.insert(Some(label.clone()), target);
                (label, previous)
            })
            .collect()
    }

    fn restore_labeled_continue_targets(
        &mut self,
        previous: Vec<(String, Option<LoopControlTarget>)>,
    ) {
        for (label, target) in previous {
            if let Some(target) = target {
                self.continue_targets.insert(Some(label), target);
            } else {
                self.continue_targets.remove(&Some(label));
            }
        }
    }
}

pub(crate) fn transform_generator(body: &[Statement], params: &[Pattern]) -> GeneratorStateMachine {
    transform_generator_inner(body, params, false)
}

pub(crate) fn transform_async_generator(
    body: &[Statement],
    params: &[Pattern],
) -> GeneratorStateMachine {
    transform_generator_inner(body, params, true)
}

fn transform_generator_inner(
    body: &[Statement],
    params: &[Pattern],
    is_async: bool,
) -> GeneratorStateMachine {
    transform_generator_inner_opts(body, params, is_async, false)
}

fn transform_generator_inner_opts(
    body: &[Statement],
    params: &[Pattern],
    is_async: bool,
    detect_for_await: bool,
) -> GeneratorStateMachine {
    let analysis = analyze_generator_body(body, params);

    if analysis.yield_points.is_empty() && !is_async {
        return create_simple_machine(body, params, &analysis);
    }

    // For async generators/functions, also check for await expressions, for-await-of,
    // and return statements (which need Return(None) vs Return(Some) distinction for tick counting)
    if is_async
        && analysis.yield_points.is_empty()
        && !body.iter().any(contains_suspension)
        && (!detect_for_await || !body.iter().any(stmt_contains_for_await))
        && !body.iter().any(|s| {
            stmt_contains_for_of_head(s, |f| {
                f.is_await && !for_in_of_left_contains_suspension(&f.left)
            })
        })
        && (detect_for_await || !body.iter().any(stmt_contains_await_using_head))
        && !body.iter().any(stmt_contains_return)
        && !body.iter().any(has_block_with_await_using)
        && !(detect_for_await && body.iter().any(has_suspendable_await_using_block))
    {
        return create_simple_machine(body, params, &analysis);
    }

    let mut ctx = TransformContext::new(analysis.clone(), is_async);
    ctx.detect_for_await = detect_for_await;

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
        #[cfg(feature = "perf-counters")]
        perf_key: None,
    }
}

fn create_simple_machine(
    body: &[Statement],
    params: &[Pattern],
    analysis: &GeneratorAnalysis,
) -> GeneratorStateMachine {
    let mut body = Body::new(body.to_vec());
    crate::ast::assign_ic_sites_for_body(&mut body);
    GeneratorStateMachine {
        states: vec![GeneratorState {
            id: 0,
            body,
            terminator: StateTerminator::Completed,
            inline_jumps: Vec::new(),
            scope_depth: 0,
            scope_action: None,
        }],
        local_vars: analysis.local_vars.clone(),
        params: params.to_vec(),
        num_yields: 0,
        temp_vars: vec![],
        #[cfg(feature = "perf-counters")]
        perf_key: None,
    }
}

fn stmt_contains_for_await(stmt: &Statement) -> bool {
    stmt_contains_for_of_head(stmt, ForOfStatement::awaits_at_head)
}

fn stmt_contains_await_using_head(stmt: &Statement) -> bool {
    stmt_contains_for_of_head(stmt, ForOfStatement::disposes_at_head)
}

fn stmt_contains_for_of_head(
    stmt: &Statement,
    head: impl Fn(&ForOfStatement) -> bool + Copy,
) -> bool {
    let contains = |s: &Statement| stmt_contains_for_of_head(s, head);
    match stmt {
        Statement::ForOf(f) => head(f),
        Statement::Block(stmts) => stmts.iter().any(contains),
        Statement::If(i) => {
            contains(&i.consequent) || i.alternate.as_ref().is_some_and(|s| contains(s))
        }
        Statement::While(w) => contains(&w.body),
        Statement::DoWhile(d) => contains(&d.body),
        Statement::For(f) => contains(&f.body),
        Statement::ForIn(f) => contains(&f.body),
        Statement::Try(t) => {
            t.block.iter().any(contains)
                || t.handler
                    .as_ref()
                    .is_some_and(|h| h.body.iter().any(contains))
                || t.finalizer.as_ref().is_some_and(|f| f.iter().any(contains))
        }
        Statement::Switch(s) => s.cases.iter().any(|c| c.consequent.iter().any(contains)),
        Statement::Labeled(_, inner) => contains(inner),
        Statement::With(_, s) => contains(s),
        _ => false,
    }
}

fn stmt_contains_return(stmt: &Statement) -> bool {
    match stmt {
        Statement::Return(_) => true,
        Statement::Block(stmts) => stmts.iter().any(stmt_contains_return),
        Statement::If(i) => {
            stmt_contains_return(&i.consequent)
                || i.alternate
                    .as_ref()
                    .is_some_and(|s| stmt_contains_return(s))
        }
        Statement::While(w) => stmt_contains_return(&w.body),
        Statement::DoWhile(d) => stmt_contains_return(&d.body),
        Statement::For(f) => stmt_contains_return(&f.body),
        Statement::ForIn(f) => stmt_contains_return(&f.body),
        Statement::ForOf(f) => stmt_contains_return(&f.body),
        Statement::Try(t) => {
            t.block.iter().any(stmt_contains_return)
                || t.handler
                    .as_ref()
                    .is_some_and(|h| h.body.iter().any(stmt_contains_return))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|f| f.iter().any(stmt_contains_return))
        }
        Statement::Switch(s) => s
            .cases
            .iter()
            .any(|c| c.consequent.iter().any(stmt_contains_return)),
        Statement::Labeled(_, inner) => stmt_contains_return(inner),
        Statement::With(_, s) => stmt_contains_return(s),
        // Don't recurse into function/class boundaries
        _ => false,
    }
}

fn stmt_has_suspension(stmt: &Statement, is_async: bool, detect_for_await: bool) -> bool {
    // A `for await` head performs `Await(nextResult)` on every step
    // (ForIn/OfBodyEvaluation) whether or not its body suspends, so it is a
    // suspension point in any async context, generators included. A `yield`
    // or `await` inside the head's own binding target still needs the
    // tree-walker's inline replay, so such a loop is left native.
    if let Statement::ForOf(f) = stmt
        && ((is_async && f.disposes_at_head())
            || (detect_for_await && f.awaits_at_head())
            || (is_async && f.is_await && !for_in_of_left_contains_suspension(&f.left)))
    {
        return true;
    }
    if is_async {
        contains_suspension(stmt) || (detect_for_await && has_suspendable_await_using_block(stmt))
    } else {
        contains_yield(stmt)
    }
}

fn stmt_has_break_or_continue(stmt: &Statement) -> bool {
    match stmt {
        Statement::Break(_) | Statement::Continue(_) => true,
        Statement::Block(stmts) => stmts.iter().any(stmt_has_break_or_continue),
        Statement::If(if_stmt) => {
            stmt_has_break_or_continue(&if_stmt.consequent)
                || if_stmt
                    .alternate
                    .as_ref()
                    .is_some_and(|a| stmt_has_break_or_continue(a))
        }
        Statement::Labeled(_, inner) => stmt_has_break_or_continue(inner),
        // Don't recurse into nested loops/switch — their break/continue targets are separate
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JumpKind {
    Break,
    Continue,
}

/// Which `break`/`continue` completions the native statement currently being
/// walked would consume itself.
#[derive(Default)]
struct JumpScope {
    breakable: bool,
    in_loop: bool,
    break_labels: Vec<String>,
    continue_labels: Vec<String>,
}

/// The `break`/`continue` completions that escape `stmt` when it runs natively,
/// i.e. those not consumed by a loop, switch, or label inside it. Repeats are
/// not collapsed.
fn escaping_jumps(stmt: &Statement) -> Vec<(JumpKind, Option<String>)> {
    let mut out = Vec::new();
    collect_escaping_jumps(stmt, &mut JumpScope::default(), &mut out);
    out
}

fn collect_escaping_jumps(
    stmt: &Statement,
    scope: &mut JumpScope,
    out: &mut Vec<(JumpKind, Option<String>)>,
) {
    match stmt {
        Statement::Break(label) => {
            let consumed = match label {
                None => scope.breakable,
                Some(l) => scope.break_labels.contains(l),
            };
            if !consumed {
                out.push((JumpKind::Break, label.clone()));
            }
        }
        Statement::Continue(label) => {
            let consumed = match label {
                None => scope.in_loop,
                Some(l) => scope.continue_labels.contains(l),
            };
            if !consumed {
                out.push((JumpKind::Continue, label.clone()));
            }
        }
        Statement::Block(stmts) => {
            for s in stmts {
                collect_escaping_jumps(s, scope, out);
            }
        }
        Statement::If(if_stmt) => {
            collect_escaping_jumps(&if_stmt.consequent, scope, out);
            if let Some(alt) = &if_stmt.alternate {
                collect_escaping_jumps(alt, scope, out);
            }
        }
        Statement::Try(try_stmt) => {
            let handler = try_stmt.handler.iter().flat_map(|h| h.body.iter());
            let finalizer = try_stmt.finalizer.iter().flatten();
            for s in try_stmt.block.iter().chain(handler).chain(finalizer) {
                collect_escaping_jumps(s, scope, out);
            }
        }
        Statement::With(_, body) => collect_escaping_jumps(body, scope, out),
        Statement::Labeled(label, inner) => {
            let labels_loop = labels_iteration_statement(inner);
            scope.break_labels.push(label.clone());
            if labels_loop {
                scope.continue_labels.push(label.clone());
            }
            collect_escaping_jumps(inner, scope, out);
            scope.break_labels.pop();
            if labels_loop {
                scope.continue_labels.pop();
            }
        }
        Statement::While(WhileStatement { body, .. })
        | Statement::DoWhile(DoWhileStatement { body, .. })
        | Statement::For(ForStatement { body, .. })
        | Statement::ForIn(ForInStatement { body, .. })
        | Statement::ForOf(ForOfStatement { body, .. }) => {
            let saved = (scope.breakable, scope.in_loop);
            scope.breakable = true;
            scope.in_loop = true;
            collect_escaping_jumps(body, scope, out);
            (scope.breakable, scope.in_loop) = saved;
        }
        Statement::Switch(switch_stmt) => {
            let saved = scope.breakable;
            scope.breakable = true;
            for s in switch_stmt.cases.iter().flat_map(|c| c.consequent.iter()) {
                collect_escaping_jumps(s, scope, out);
            }
            scope.breakable = saved;
        }
        // Function and class bodies have their own jump targets, and
        // expressions cannot contain a `break`/`continue` statement.
        _ => {}
    }
}

fn labels_iteration_statement(stmt: &Statement) -> bool {
    match stmt {
        Statement::While(_)
        | Statement::DoWhile(_)
        | Statement::For(_)
        | Statement::ForIn(_)
        | Statement::ForOf(_) => true,
        Statement::Labeled(_, inner) => labels_iteration_statement(inner),
        _ => false,
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

        if stmt_has_suspension(stmt, ctx.is_async, ctx.detect_for_await) {
            transform_yielding_statement(stmt, ctx, next_after);
        } else if ctx.is_async && matches!(stmt, Statement::Return(_)) {
            // Return statements in async generators need Return terminators
            // for proper Return(None) vs Return(Some) tick distinction
            transform_yielding_statement(stmt, ctx, next_after);
        } else if (ctx.is_async && has_block_with_await_using(stmt))
            || (stmt_has_break_or_continue(stmt) && !ctx.break_targets.is_empty())
        {
            transform_yielding_statement(stmt, ctx, next_after);
        } else {
            ctx.emit_statement(stmt.clone());
        }
    }
}

fn hoist_suspending_expr(
    expr: &Expression,
    prefix: &str,
    ctx: &mut TransformContext,
) -> Option<Expression> {
    if !expr_has_suspension(expr, ctx.is_async) {
        return None;
    }
    let temp_var = ctx.new_temp_var(prefix);
    let binding = SentValueBindingKind::Variable(temp_var.clone());
    transform_yielding_expression(expr, ctx, usize::MAX, Some(binding));
    Some(Expression::Identifier(temp_var))
}

/// Hoist the sub-expressions of a class heritage clause and computed element
/// keys that themselves contain a `yield`/`await` into temp vars (heritage
/// first, then each key in declaration order), so the suspension belongs to
/// the enclosing generator rather than to the class.
///
/// Only suspending sub-expressions move: non-suspending siblings stay in the
/// rebuilt class and run when it is evaluated, after every hoisted suspension,
/// and a hoisted key's ToPropertyKey is likewise deferred to that point. Both
/// depart from the source order of ClassDefinitionEvaluation (spec §15.7.14).
fn hoist_class_suspensions(
    super_class: &mut Option<Box<Expression>>,
    elements: &mut [ClassElement],
    ctx: &mut TransformContext,
) {
    if let Some(sc) = super_class
        && let Some(hoisted) = hoist_suspending_expr(sc, "class_heritage", ctx)
    {
        **sc = hoisted;
    }
    for key in elements.iter_mut().filter_map(ClassElement::key_mut) {
        if let PropertyKey::Computed(e) = key
            && let Some(hoisted) = hoist_suspending_expr(e, "class_key", ctx)
        {
            **e = hoisted;
        }
    }
}

/// Lowers a block scope (a block, or a try/catch/finally clause body, that
/// directly declares `await using`) through `EnterScope`/`ExitScope`: the
/// interior is lowered by the ordinary per-statement pipeline, split across
/// as many states as it needs, executing against the scope's own
/// `Environment` (created at `EnterScope`) rather than the enclosing one.
/// `after_state` is where control goes once the scope's own disposal (at
/// `ExitScope`) finishes on a normal (non-abrupt) exit.
fn transform_scope_block(stmts: &[Statement], ctx: &mut TransformContext, after_state: usize) {
    let body_state = ctx.new_state();
    let exit_state = ctx.new_state();
    ctx.finalize_current_state(StateTerminator::EnterScope { body_state });
    ctx.current_state_id = body_state;
    ctx.scope_depth += 1;
    transform_statements(stmts, ctx, exit_state);
    if ctx.current_state_id != exit_state {
        ctx.finalize_current_state(StateTerminator::Goto(exit_state));
    }
    ctx.current_state_id = exit_state;
    // `ExitScope` must execute while its frame is still the state's required
    // depth; otherwise reconciliation would truncate it before disposal.
    ctx.finalize_current_state(StateTerminator::ExitScope { after_state });
    ctx.scope_depth -= 1;
    ctx.current_state_id = after_state;
}

fn transform_yielding_statement(stmt: &Statement, ctx: &mut TransformContext, after_state: usize) {
    match stmt {
        Statement::Expression(expr) => {
            transform_yielding_expression(expr, ctx, after_state, None);
        }

        Statement::ClassDeclaration(class_decl) => {
            let mut class_decl = class_decl.clone();
            hoist_class_suspensions(&mut class_decl.super_class, &mut class_decl.body, ctx);
            ctx.emit_statement(Statement::ClassDeclaration(class_decl));
        }

        Statement::Block(stmts) => {
            if ctx.is_async && ctx.detect_for_await && block_has_await_using(stmts) {
                // A block with `await using`, in a plain async function: give
                // it a real scope (`EnterScope`/`ExitScope`) so its interior
                // lowers through the ordinary per-statement pipeline instead
                // of being tree-walked intact — see issue #683.
                let resume_state = if after_state == usize::MAX {
                    ctx.new_state()
                } else {
                    after_state
                };
                transform_scope_block(stmts, ctx, resume_state);
            } else {
                // §14.2.2 Block Evaluation: a fresh declarative environment per
                // entry, discarded on the way out. Force a state boundary
                // (`entry_state`) so the block's own content never shares a
                // not-yet-sealed state with statements lexically outside it —
                // those would otherwise get stamped with the block's (bumped)
                // `scope_depth` instead of the outer one.
                let entry_state = ctx.new_state();
                ctx.finalize_current_state(StateTerminator::Goto(entry_state));
                ctx.current_state_id = entry_state;
                ctx.scope_depth += 1;
                ctx.states[entry_state].scope_action = Some(ScopeAction::OpenBlock);

                let inner_after = ctx.new_state();
                transform_statements(stmts, ctx, inner_after);
                if ctx.current_state_id != inner_after {
                    ctx.finalize_current_state(StateTerminator::Goto(inner_after));
                }
                ctx.current_state_id = inner_after;
                ctx.scope_depth -= 1;

                // `inner_after` must only be finalized once `scope_depth` is
                // back to the outer value, whether that's done here (bridging
                // to a real `after_state`) or later by our caller (the
                // `after_state == MAX` case, where `inner_after` itself *is*
                // the fresh join point the caller will seal).
                if after_state != usize::MAX {
                    ctx.finalize_current_state(StateTerminator::Goto(after_state));
                    ctx.current_state_id = after_state;
                }
            }
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
            let with_var = ctx.new_temp_var("with");
            if expr_has_suspension(expr, ctx.is_async) {
                let binding = SentValueBindingKind::Variable(with_var.clone());
                transform_yielding_expression(expr, ctx, usize::MAX, Some(binding));
            } else {
                ctx.emit_statement(Statement::Expression(Expression::Assign(
                    AssignOp::Assign,
                    ExprBox::new(Expression::Identifier(with_var.clone())),
                    ExprBox::new(expr.clone()),
                )));
            }
            if stmt_has_suspension(inner, ctx.is_async, ctx.detect_for_await) {
                let with_body_state = ctx.new_state();
                ctx.finalize_current_state(StateTerminator::Goto(with_body_state));
                ctx.current_state_id = with_body_state;
                ctx.with_scopes.push(with_var.clone());
                transform_yielding_statement(inner, ctx, after_state);
                ctx.with_scopes.pop();
            } else {
                ctx.emit_statement(Statement::With(
                    Expression::Identifier(with_var),
                    Box::new(*inner.clone()),
                ));
            }
        }

        Statement::Break(label) => {
            if let Some(target) = ctx.jump_target(JumpKind::Break, label) {
                let terminator = StateTerminator::LoopControl(target);
                ctx.finalize_current_state(terminator);
                ctx.current_state_id = ctx.new_state();
            } else {
                ctx.emit_statement(stmt.clone());
            }
        }

        Statement::Continue(label) => {
            if let Some(target) = ctx.jump_target(JumpKind::Continue, label) {
                let terminator = StateTerminator::LoopControl(target);
                ctx.finalize_current_state(terminator);
                ctx.current_state_id = ctx.new_state();
            } else {
                ctx.emit_statement(stmt.clone());
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
                } else if !ctx.with_scopes.is_empty() {
                    let temp_var = ctx.new_temp_var("yield_val");
                    ctx.emit_statement(Statement::Expression(Expression::Assign(
                        AssignOp::Assign,
                        ExprBox::new(Expression::Identifier(temp_var.clone())),
                        ExprBox::new(inner.clone().into_expression()),
                    )));
                    Some(Expression::Identifier(temp_var))
                } else {
                    Some(inner.clone().into_expression())
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
                inner_expr.clone().into_expression()
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
                    condition: test.clone().into_expression(),
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
            let left_var = ctx.new_temp_var("logical");
            bind_expression_to_temp(left, &left_var, ctx);

            if expr_has_suspension(right, ctx.is_async) {
                let after_logical = ctx.new_state();
                let eval_right_state = ctx.new_state();

                let condition = short_circuit_continue_test(*op, &left_var);
                ctx.finalize_current_state(StateTerminator::ConditionalGoto {
                    condition,
                    true_state: eval_right_state,
                    false_state: after_logical,
                });

                ctx.current_state_id = eval_right_state;
                transform_yielding_expression(
                    right,
                    ctx,
                    usize::MAX,
                    Some(SentValueBindingKind::Variable(left_var.clone())),
                );
                ctx.finalize_current_state(StateTerminator::Goto(after_logical));

                ctx.current_state_id = after_logical;
                emit_expression_with_binding(&Expression::Identifier(left_var), &binding, ctx);
            } else {
                let combined = Expression::Logical(
                    *op,
                    ExprBox::new(Expression::Identifier(left_var)),
                    right.clone(),
                );
                emit_expression_with_binding(&combined, &binding, ctx);
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
                        ExprBox::new(Expression::Identifier(temp_var)),
                        ExprBox::new(Expression::Identifier(temp_var2)),
                    );
                    emit_expression_with_binding(&combined, &binding, ctx);
                } else {
                    let combined = Expression::Binary(
                        *op,
                        ExprBox::new(Expression::Identifier(temp_var)),
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
                    ExprBox::new(Expression::Identifier(temp_var)),
                );
                emit_expression_with_binding(&combined, &binding, ctx);
            }
        }

        Expression::Call(callee, args, _) => {
            transform_call_expression(callee, args, binding, ctx);
        }

        Expression::New(callee, args, _) => {
            let mut temp_callee = callee.clone().into_expression();
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
                        temp_args.push(Expression::Spread(ExprBox::new(Expression::Identifier(
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

            let combined =
                Expression::New(ExprBox::new(temp_callee), temp_args, CallSiteId::UNASSIGNED);
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Assign(op, left, right) => {
            let right_suspends = expr_has_suspension(right, ctx.is_async);
            let target = lower_reference_operand(left, right_suspends, ctx);
            if !right_suspends {
                let combined = Expression::Assign(*op, ExprBox::new(target), right.clone());
                emit_expression_with_binding(&combined, &binding, ctx);
            } else if let Some(logical_op) = op.logical_op() {
                let store = Expression::Assign(
                    AssignOp::Assign,
                    ExprBox::new(target.clone()),
                    right.clone(),
                );
                let combined =
                    Expression::Logical(logical_op, ExprBox::new(target), ExprBox::new(store));
                transform_yielding_expression(&combined, ctx, usize::MAX, binding);
            } else {
                let old_value = op
                    .binary_op()
                    .filter(|_| {
                        matches!(target, Expression::Identifier(_) | Expression::Member(..))
                    })
                    .map(|binary_op| {
                        let old_var = ctx.new_temp_var("assign_old");
                        let old_binding = Some(SentValueBindingKind::Variable(old_var.clone()));
                        emit_expression_with_binding(&target, &old_binding, ctx);
                        (binary_op, old_var)
                    });
                let value_var = ctx.new_temp_var("assign");
                bind_expression_to_temp(right, &value_var, ctx);
                let mut value = Expression::Identifier(value_var);
                let mut assign_op = *op;
                if let Some((binary_op, old_var)) = old_value {
                    value = Expression::Binary(
                        binary_op,
                        ExprBox::new(Expression::Identifier(old_var)),
                        ExprBox::new(value),
                    );
                    assign_op = AssignOp::Assign;
                }
                let combined =
                    Expression::Assign(assign_op, ExprBox::new(target), ExprBox::new(value));
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
                        new_elements.push(Some(Expression::Spread(ExprBox::new(
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

        Expression::Object(props, trailing_flag) => {
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
                        PropertyKey::Computed(ExprBox::new(Expression::Identifier(temp_var)))
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
                        Expression::Spread(ExprBox::new(Expression::Identifier(temp_var)))
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
            let combined = Expression::Object(new_props, *trailing_flag);
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Class(class_expr) => {
            let mut class_expr = class_expr.clone();
            hoist_class_suspensions(&mut class_expr.super_class, &mut class_expr.body, ctx);
            emit_expression_with_binding(&Expression::Class(class_expr), &binding, ctx);
        }

        Expression::Member(..) => {
            let combined = lower_reference_operand(expr, false, ctx);
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::OptionalChain(base, chain) => {
            if expr_has_suspension(chain, ctx.is_async) {
                lower_optional_chain(
                    base,
                    chain,
                    Expression::Identifier("undefined".to_string()),
                    |regular| regular,
                    binding,
                    ctx,
                );
            } else {
                let temp_base = hoist_suspending_expr(base, "oc_base", ctx)
                    .unwrap_or_else(|| base.clone().into_expression());
                let combined = Expression::OptionalChain(ExprBox::new(temp_base), chain.clone());
                emit_expression_with_binding(&combined, &binding, ctx);
            }
        }

        Expression::Unary(op, inner) => {
            let tv = ctx.new_temp_var("unary");
            let b = SentValueBindingKind::Variable(tv.clone());
            transform_yielding_expression(inner, ctx, usize::MAX, Some(b));
            let combined = Expression::Unary(*op, ExprBox::new(Expression::Identifier(tv)));
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Typeof(inner) => {
            let tv = ctx.new_temp_var("typeof");
            let b = SentValueBindingKind::Variable(tv.clone());
            transform_yielding_expression(inner, ctx, usize::MAX, Some(b));
            let combined = Expression::Typeof(ExprBox::new(Expression::Identifier(tv)));
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Void(inner) => {
            let tv = ctx.new_temp_var("void");
            let b = SentValueBindingKind::Variable(tv.clone());
            transform_yielding_expression(inner, ctx, usize::MAX, Some(b));
            let combined = Expression::Void(ExprBox::new(Expression::Identifier(tv)));
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Delete(inner) => match &**inner {
            Expression::Member(..) => {
                let target = lower_reference_operand(inner, false, ctx);
                let combined = Expression::Delete(ExprBox::new(target));
                emit_expression_with_binding(&combined, &binding, ctx);
            }
            Expression::OptionalChain(base, chain) => lower_optional_chain(
                base,
                chain,
                Expression::Literal(Literal::Boolean(true)),
                |regular| Expression::Delete(ExprBox::new(regular)),
                binding,
                ctx,
            ),
            other => {
                transform_yielding_expression(other, ctx, usize::MAX, None);
                emit_expression_with_binding(
                    &Expression::Literal(Literal::Boolean(true)),
                    &binding,
                    ctx,
                );
            }
        },

        Expression::Update(op, prefix, inner) => {
            let target = lower_reference_operand(inner, false, ctx);
            let combined = Expression::Update(*op, *prefix, ExprBox::new(target));
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
            let mut temp_tag = tag.clone().into_expression();
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
                ExprBox::new(temp_tag),
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
            let combined = Expression::Spread(ExprBox::new(Expression::Identifier(tv)));
            emit_expression_with_binding(&combined, &binding, ctx);
        }

        Expression::Import(spec, opts)
        | Expression::ImportDefer(spec, opts)
        | Expression::ImportSource(spec, opts) => {
            let mut temp_spec = spec.clone().into_expression();
            if expr_has_suspension(spec, ctx.is_async) {
                let tv = ctx.new_temp_var("imp_spec");
                let b = SentValueBindingKind::Variable(tv.clone());
                transform_yielding_expression(spec, ctx, usize::MAX, Some(b));
                temp_spec = Expression::Identifier(tv);
            }
            let mut temp_opts = opts.clone().map(ExprBox::into_expression);
            if let Some(o) = opts
                && expr_has_suspension(o, ctx.is_async)
            {
                let tv = ctx.new_temp_var("imp_opts");
                let b = SentValueBindingKind::Variable(tv.clone());
                transform_yielding_expression(o, ctx, usize::MAX, Some(b));
                temp_opts = Some(Expression::Identifier(tv));
            }
            let boxed_opts = temp_opts.map(ExprBox::new);
            let combined = match expr {
                Expression::Import(_, _) => Expression::Import(ExprBox::new(temp_spec), boxed_opts),
                Expression::ImportDefer(_, _) => {
                    Expression::ImportDefer(ExprBox::new(temp_spec), boxed_opts)
                }
                Expression::ImportSource(_, _) => {
                    Expression::ImportSource(ExprBox::new(temp_spec), boxed_opts)
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
            ExprBox::new(Expression::Identifier(base_var.to_string())),
            MemberProperty::Dot(name.clone()),
            PropSiteId::UNASSIGNED,
        ),
        Expression::Member(inner, prop, _) => {
            let inner_expr = oc_chain_to_regular_expr(inner, base_var);
            Expression::Member(
                ExprBox::new(inner_expr),
                prop.clone(),
                PropSiteId::UNASSIGNED,
            )
        }
        Expression::Call(callee, args, _) => {
            let callee_expr = oc_chain_to_regular_expr(callee, base_var);
            Expression::Call(
                ExprBox::new(callee_expr),
                args.clone(),
                CallSiteId::UNASSIGNED,
            )
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
    let temp_callee = if matches!(callee, Expression::Member(..)) {
        lower_reference_operand(callee, false, ctx)
    } else {
        hoist_suspending_expr(callee, "call_callee", ctx).unwrap_or_else(|| callee.clone())
    };

    let mut temp_args = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if let Expression::Spread(inner) = arg {
            if expr_has_suspension(inner, ctx.is_async) {
                let temp_var = ctx.new_temp_var(&format!("call_arg_{}", i));
                let arg_binding = SentValueBindingKind::Variable(temp_var.clone());
                transform_yielding_expression(inner, ctx, usize::MAX, Some(arg_binding));
                temp_args.push(Expression::Spread(ExprBox::new(Expression::Identifier(
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

    let combined = Expression::Call(ExprBox::new(temp_callee), temp_args, CallSiteId::UNASSIGNED);
    emit_expression_with_binding(&combined, &binding, ctx);
}

fn emit_expression_with_binding(
    expr: &Expression,
    binding: &Option<SentValueBindingKind>,
    ctx: &mut TransformContext,
) {
    match binding {
        Some(SentValueBindingKind::Variable(name)) => {
            // Assigning an anonymous function/class straight to an internal temp
            // would apply NamedEvaluation and leak the temp's name as `.name`.
            let value =
                if expr.is_anonymous_function_definition() && ctx.generated_temps.contains(name) {
                    Expression::Sequence(vec![
                        Expression::Literal(Literal::Number(0.0)),
                        expr.clone(),
                    ])
                } else {
                    expr.clone()
                };
            let assign = Expression::Assign(
                AssignOp::Assign,
                ExprBox::new(Expression::Identifier(name.clone())),
                ExprBox::new(value),
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

/// Evaluate `expr` into the temp `var`, suspending first if it contains a
/// suspension point.
fn bind_expression_to_temp(expr: &Expression, var: &str, ctx: &mut TransformContext) {
    let binding = Some(SentValueBindingKind::Variable(var.to_string()));
    if expr_has_suspension(expr, ctx.is_async) {
        transform_yielding_expression(expr, ctx, usize::MAX, binding);
    } else {
        emit_expression_with_binding(expr, &binding, ctx);
    }
}

fn emit_pattern_binding(kind: VarKind, pattern: Pattern, source: &str, ctx: &mut TransformContext) {
    ctx.emit_statement(Statement::Variable(VariableDeclaration {
        kind,
        declarations: vec![VariableDeclarator {
            pattern,
            init: Some(Expression::Identifier(source.to_string())),
        }],
    }));
}

fn emit_temp_assignment(temp: &str, value: Expression, ctx: &mut TransformContext) {
    ctx.emit_statement(Statement::Expression(Expression::Assign(
        AssignOp::Assign,
        ExprBox::new(Expression::Identifier(temp.to_string())),
        ExprBox::new(value),
    )));
}

/// Property read of a pattern key off the destructuring source, standing in
/// for the `GetV` of `KeyedBindingInitialization`.
fn pattern_key_read(source: &str, key: &PropertyKey) -> Expression {
    let key_expr = match key {
        PropertyKey::Identifier(name) => {
            Expression::Literal(Literal::String(name.encode_utf16().collect()))
        }
        PropertyKey::String(units) => Expression::Literal(Literal::String(units.clone())),
        PropertyKey::Number(n) => Expression::Literal(Literal::Number(*n)),
        PropertyKey::Computed(e) => e.clone().into_expression(),
        PropertyKey::Private(_) => unreachable!("private names are not pattern keys"),
    };
    Expression::Member(
        ExprBox::new(Expression::Identifier(source.to_string())),
        MemberProperty::Computed(ExprBox::new(key_expr)),
        PropSiteId::UNASSIGNED,
    )
}

/// Binds `pattern` from the value held in temp `source`, suspending at every
/// `await` the pattern reaches. Only the parts of a pattern that reach an
/// `await` are broken up; everything else is bound by the tree-walker through
/// a sub-pattern, so naming, TDZ and nested-pattern semantics are unchanged.
///
/// For an object pattern this follows `KeyedBindingInitialization`: the
/// source is coerced once, then each property in source order evaluates its
/// computed key, performs exactly one `GetV`, and evaluates its default only
/// when that value is `undefined` (a present property costs no extra tick).
/// Callers only pass patterns `pattern_needs_lowering` accepts.
fn lower_pattern_binding(
    kind: VarKind,
    pattern: &Pattern,
    source: &str,
    ctx: &mut TransformContext,
) {
    let Pattern::Object(props) = pattern.clone() else {
        emit_pattern_binding(kind, pattern.clone(), source, ctx);
        return;
    };
    if !pattern_contains_suspension(pattern) {
        emit_pattern_binding(kind, pattern.clone(), source, ctx);
        return;
    }
    emit_pattern_binding(kind, Pattern::Object(Vec::new()), source, ctx);
    for prop in props {
        match prop {
            ObjectPatternProperty::KeyValue(key, value) => {
                lower_pattern_property(kind, key, value, source, ctx);
            }
            other => emit_pattern_binding(kind, Pattern::Object(vec![other]), source, ctx),
        }
    }
}

fn lower_pattern_property(
    kind: VarKind,
    key: PropertyKey,
    value: Pattern,
    source: &str,
    ctx: &mut TransformContext,
) {
    let key = match key {
        PropertyKey::Computed(e) if expr_has_suspension(&e, ctx.is_async) => {
            let key_temp = ctx.new_temp_var("dstr_key");
            transform_yielding_expression(
                &e,
                ctx,
                usize::MAX,
                Some(SentValueBindingKind::Variable(key_temp.clone())),
            );
            PropertyKey::Computed(ExprBox::new(Expression::Identifier(key_temp)))
        }
        other => other,
    };
    if !pattern_contains_suspension(&value) {
        emit_pattern_binding(
            kind,
            Pattern::Object(vec![ObjectPatternProperty::KeyValue(key, value)]),
            source,
            ctx,
        );
        return;
    }

    let value_temp = ctx.new_temp_var("dstr_val");
    emit_temp_assignment(&value_temp, pattern_key_read(source, &key), ctx);
    let target = match value {
        Pattern::Assign(target, default) => {
            let default_state = ctx.new_state();
            let join_state = ctx.new_state();
            ctx.finalize_current_state(StateTerminator::ConditionalGoto {
                condition: Expression::Binary(
                    BinaryOp::StrictEq,
                    ExprBox::new(Expression::Typeof(ExprBox::new(Expression::Identifier(
                        value_temp.clone(),
                    )))),
                    ExprBox::new(Expression::Literal(Literal::String(
                        "undefined".encode_utf16().collect(),
                    ))),
                ),
                true_state: default_state,
                false_state: join_state,
            });
            ctx.current_state_id = default_state;
            if expr_has_suspension(&default, ctx.is_async) {
                transform_yielding_expression(
                    &default,
                    ctx,
                    usize::MAX,
                    Some(SentValueBindingKind::Variable(value_temp.clone())),
                );
            } else {
                emit_temp_assignment(&value_temp, default.into_expression(), ctx);
            }
            ctx.finalize_current_state(StateTerminator::Goto(join_state));
            ctx.current_state_id = join_state;
            *target
        }
        other => other,
    };
    lower_pattern_binding(kind, &target, &value_temp, ctx);
}

/// `var === null || var === void 0` — true exactly when `var` is undefined or
/// null (unlike `== null`, false for `document.all`-style objects).
fn nullish_test(var: &str) -> Expression {
    let is = |rhs: Expression| {
        Expression::Binary(
            BinaryOp::StrictEq,
            ExprBox::new(Expression::Identifier(var.to_string())),
            ExprBox::new(rhs),
        )
    };
    Expression::Logical(
        LogicalOp::Or,
        ExprBox::new(is(Expression::Literal(Literal::Null))),
        ExprBox::new(is(Expression::Void(ExprBox::new(Expression::Literal(
            Literal::Number(0.0),
        ))))),
    )
}

/// The condition under which `var <op> rhs` goes on to evaluate `rhs`.
fn short_circuit_continue_test(op: LogicalOp, var: &str) -> Expression {
    match op {
        LogicalOp::And => Expression::Identifier(var.to_string()),
        LogicalOp::Or => Expression::Unary(
            UnaryOp::Not,
            ExprBox::new(Expression::Identifier(var.to_string())),
        ),
        LogicalOp::NullishCoalescing => nullish_test(var),
    }
}

/// Lower an optional chain whose chain part suspends: the base is evaluated
/// once into a temp, a nullish base yields `skip_value`, and otherwise the chain
/// runs as a regular expression on that temp, passed through `wrap`.
fn lower_optional_chain(
    base: &Expression,
    chain: &Expression,
    skip_value: Expression,
    wrap: impl FnOnce(Expression) -> Expression,
    binding: Option<SentValueBindingKind>,
    ctx: &mut TransformContext,
) {
    let base_var = ctx.new_temp_var("oc_bv");
    bind_expression_to_temp(base, &base_var, ctx);
    let after_oc = ctx.new_state();
    let eval_chain_state = ctx.new_state();
    let skip_state = ctx.new_state();
    ctx.finalize_current_state(StateTerminator::ConditionalGoto {
        condition: nullish_test(&base_var),
        true_state: skip_state,
        false_state: eval_chain_state,
    });
    ctx.current_state_id = skip_state;
    emit_expression_with_binding(&skip_value, &binding, ctx);
    ctx.finalize_current_state(StateTerminator::Goto(after_oc));
    ctx.current_state_id = eval_chain_state;
    let regular = wrap(oc_chain_to_regular_expr(chain, &base_var));
    if expr_has_suspension(&regular, ctx.is_async) {
        transform_yielding_expression(&regular, ctx, usize::MAX, binding);
    } else {
        emit_expression_with_binding(&regular, &binding, ctx);
    }
    ctx.finalize_current_state(StateTerminator::Goto(after_oc));
    ctx.current_state_id = after_oc;
}

/// An operand whose value cannot change across a suspension, so capturing it
/// into another temp would only add a copy.
fn is_settled_operand(expr: &Expression, ctx: &TransformContext) -> bool {
    match expr {
        Expression::Super | Expression::Literal(_) => true,
        Expression::Identifier(name) => ctx.generated_temps.contains(name),
        _ => false,
    }
}

/// Lower the suspension points out of a member Reference (the operand of
/// `delete`, `++`/`--`, a call callee, or an assignment target) while keeping it
/// a Reference: the base value and then the raw computed key are captured in
/// temps, in source order, and the returned member expression is suspension-free.
///
/// The base and key are captured only when the key or base suspends, or when
/// `capture_all` is set (the caller is about to suspend on something else, e.g.
/// an assignment's right-hand side, so both must be evaluated first). Other
/// expressions are returned unchanged.
fn lower_reference_operand(
    expr: &Expression,
    capture_all: bool,
    ctx: &mut TransformContext,
) -> Expression {
    let Expression::Member(obj, prop, _) = expr else {
        return expr.clone();
    };
    let key_suspends =
        matches!(prop, MemberProperty::Computed(e) if expr_has_suspension(e, ctx.is_async));
    if !capture_all && !key_suspends && !expr_has_suspension(obj, ctx.is_async) {
        return expr.clone();
    }
    let new_obj = if is_settled_operand(obj, ctx) {
        obj.clone().into_expression()
    } else {
        let temp = ctx.new_temp_var("ref_obj");
        bind_expression_to_temp(obj, &temp, ctx);
        Expression::Identifier(temp)
    };
    let new_prop = match prop {
        MemberProperty::Computed(e)
            if key_suspends || (capture_all && !is_settled_operand(e, ctx)) =>
        {
            let temp = ctx.new_temp_var("ref_key");
            bind_expression_to_temp(e, &temp, ctx);
            MemberProperty::Computed(ExprBox::new(Expression::Identifier(temp)))
        }
        other => other.clone(),
    };
    Expression::Member(ExprBox::new(new_obj), new_prop, PropSiteId::UNASSIGNED)
}

fn transform_variable_declaration(
    decl: &VariableDeclaration,
    ctx: &mut TransformContext,
    _after_state: usize,
) {
    for declarator in &decl.declarations {
        if let Some(init) = &declarator.init
            && pattern_needs_lowering(&declarator.pattern)
        {
            let source = ctx.new_temp_var("dstr_src");
            if expr_has_suspension(init, ctx.is_async) {
                transform_yielding_expression(
                    init,
                    ctx,
                    usize::MAX,
                    Some(SentValueBindingKind::Variable(source.clone())),
                );
            } else {
                emit_temp_assignment(&source, init.clone(), ctx);
            }
            lower_pattern_binding(decl.kind, &declarator.pattern, &source, ctx);
        } else if let Some(init) = &declarator.init {
            if expr_has_suspension(init, ctx.is_async) {
                let binding = match &declarator.pattern {
                    Pattern::Identifier(name) => {
                        // Ensure the variable is declared as a temp var so it exists
                        // in strict mode (the original let/const/var decl is replaced
                        // by a plain assignment)
                        if !ctx.temp_vars.contains(name) {
                            ctx.temp_vars.push(name.clone());
                        }
                        SentValueBindingKind::Variable(name.clone())
                    }
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
        if stmt_has_suspension(&if_stmt.consequent, ctx.is_async, ctx.detect_for_await)
            || (!ctx.break_targets.is_empty() && stmt_has_break_or_continue(&if_stmt.consequent))
        {
            transform_yielding_statement(&if_stmt.consequent, ctx, after_if);
            if ctx.current_state_id != after_if {
                ctx.finalize_current_state(StateTerminator::Goto(after_if));
            }
        } else {
            ctx.emit_statement(*if_stmt.consequent.clone());
            ctx.finalize_current_state(StateTerminator::Goto(after_if));
        }

        if let Some(alt) = &if_stmt.alternate {
            ctx.current_state_id = false_state;
            if stmt_has_suspension(alt, ctx.is_async, ctx.detect_for_await)
                || (!ctx.break_targets.is_empty() && stmt_has_break_or_continue(alt))
            {
                transform_yielding_statement(alt, ctx, after_if);
                if ctx.current_state_id != after_if {
                    ctx.finalize_current_state(StateTerminator::Goto(after_if));
                }
            } else {
                ctx.emit_statement(*alt.clone());
                ctx.finalize_current_state(StateTerminator::Goto(after_if));
            }
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
        if stmt_has_suspension(&if_stmt.consequent, ctx.is_async, ctx.detect_for_await)
            || (!ctx.break_targets.is_empty() && stmt_has_break_or_continue(&if_stmt.consequent))
        {
            transform_yielding_statement(&if_stmt.consequent, ctx, after_if);
            if ctx.current_state_id != after_if {
                ctx.finalize_current_state(StateTerminator::Goto(after_if));
            }
        } else {
            ctx.emit_statement(*if_stmt.consequent.clone());
            ctx.finalize_current_state(StateTerminator::Goto(after_if));
        }

        if let Some(alt) = &if_stmt.alternate {
            ctx.current_state_id = false_state;
            if stmt_has_suspension(alt, ctx.is_async, ctx.detect_for_await)
                || (!ctx.break_targets.is_empty() && stmt_has_break_or_continue(alt))
            {
                transform_yielding_statement(alt, ctx, after_if);
                if ctx.current_state_id != after_if {
                    ctx.finalize_current_state(StateTerminator::Goto(after_if));
                }
            } else {
                ctx.emit_statement(*alt.clone());
                ctx.finalize_current_state(StateTerminator::Goto(after_if));
            }
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

    let iteration_labels = std::mem::take(&mut ctx.iteration_labels);
    let break_target = ctx.loop_control_target(after_loop, ctx.for_of_depth);
    let continue_target = ctx.loop_control_target(test_state, ctx.for_of_depth);
    let prev_break = ctx.break_targets.insert(None, break_target);
    let prev_continue = ctx.continue_targets.insert(None, continue_target);
    let labeled_continues =
        ctx.install_labeled_continue_targets(&iteration_labels, continue_target);

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
    if stmt_has_suspension(&while_stmt.body, ctx.is_async, ctx.detect_for_await)
        || stmt_has_break_or_continue(&while_stmt.body)
    {
        transform_yielding_statement(&while_stmt.body, ctx, test_state);
        if ctx.current_state_id != test_state {
            ctx.finalize_current_state(StateTerminator::Goto(test_state));
        }
    } else {
        ctx.emit_statement(*while_stmt.body.clone());
        ctx.finalize_current_state(StateTerminator::Goto(test_state));
    }

    if let Some(prev) = prev_break {
        ctx.break_targets.insert(None, prev);
    } else {
        ctx.break_targets.remove(&None);
    }
    if let Some(prev) = prev_continue {
        ctx.continue_targets.insert(None, prev);
    } else {
        ctx.continue_targets.remove(&None);
    }
    ctx.restore_labeled_continue_targets(labeled_continues);
    ctx.iteration_labels = iteration_labels;

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

    let iteration_labels = std::mem::take(&mut ctx.iteration_labels);
    let break_target = ctx.loop_control_target(after_loop, ctx.for_of_depth);
    let continue_target = ctx.loop_control_target(test_state, ctx.for_of_depth);
    let prev_break = ctx.break_targets.insert(None, break_target);
    let prev_continue = ctx.continue_targets.insert(None, continue_target);
    let labeled_continues =
        ctx.install_labeled_continue_targets(&iteration_labels, continue_target);

    ctx.current_state_id = body_state;
    if stmt_has_suspension(&do_while_stmt.body, ctx.is_async, ctx.detect_for_await)
        || stmt_has_break_or_continue(&do_while_stmt.body)
    {
        transform_yielding_statement(&do_while_stmt.body, ctx, test_state);
        if ctx.current_state_id != test_state {
            ctx.finalize_current_state(StateTerminator::Goto(test_state));
        }
    } else {
        ctx.emit_statement(*do_while_stmt.body.clone());
        ctx.finalize_current_state(StateTerminator::Goto(test_state));
    }

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

    if let Some(prev) = prev_break {
        ctx.break_targets.insert(None, prev);
    } else {
        ctx.break_targets.remove(&None);
    }
    if let Some(prev) = prev_continue {
        ctx.continue_targets.insert(None, prev);
    } else {
        ctx.continue_targets.remove(&None);
    }
    ctx.restore_labeled_continue_targets(labeled_continues);
    ctx.iteration_labels = iteration_labels;

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

    // §14.7.4.2/§14.7.4.3 CreatePerIterationEnvironment: a lexical head gets a
    // fresh per-iteration frame, copying the bound names' current values
    // forward, both before the first test and again after each body (before
    // the update runs). `test_state`/`body_state`/`update_state` all share
    // this *one* frame at a constant depth — mirrors `exec_for`'s
    // `per_iteration_bindings` (`exec.rs`).
    let per_iteration_bindings: Vec<(String, bool)> = if let Some(ForInit::Variable(decl)) =
        &for_stmt.init
        && matches!(decl.kind, VarKind::Let | VarKind::Const)
    {
        let is_const = decl.kind == VarKind::Const;
        let mut names = Vec::new();
        for d in &decl.declarations {
            d.pattern.bound_names(&mut names);
        }
        names.into_iter().map(|n| (n, is_const)).collect()
    } else {
        Vec::new()
    };
    let has_per_iteration_env = !per_iteration_bindings.is_empty();

    if has_per_iteration_env {
        // §14.7.4.2 step 2's `NewDeclarativeEnvironment` for the
        // LexicalDeclaration itself: the head's own `let i = 0` must not run
        // in whatever environment is currently active (it could collide with
        // an unrelated outer `var i`), so give it a fresh state/frame before
        // emitting it — mirrors the `Statement::Block` arm's `entry_state`
        // bridge, for the same reason (this state may otherwise still be
        // accumulating unrelated preceding content).
        let init_state = ctx.new_state();
        ctx.finalize_current_state(StateTerminator::Goto(init_state));
        ctx.current_state_id = init_state;
        ctx.scope_depth += 1;
        ctx.states[init_state].scope_action = Some(ScopeAction::OpenBlock);
    }

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

    let iteration_labels = std::mem::take(&mut ctx.iteration_labels);
    let break_target = ctx.loop_control_target(after_loop, ctx.for_of_depth);
    let continue_target = ctx.loop_control_target(update_state, ctx.for_of_depth);
    let prev_break = ctx.break_targets.insert(None, break_target);
    let prev_continue = ctx.continue_targets.insert(None, continue_target);
    let labeled_continues =
        ctx.install_labeled_continue_targets(&iteration_labels, continue_target);

    if has_per_iteration_env {
        ctx.states[test_state].scope_action =
            Some(ScopeAction::CopyForward(per_iteration_bindings.clone()));
        ctx.states[update_state].scope_action =
            Some(ScopeAction::CopyForward(per_iteration_bindings));
    }

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
    if stmt_has_suspension(&for_stmt.body, ctx.is_async, ctx.detect_for_await)
        || stmt_has_break_or_continue(&for_stmt.body)
    {
        transform_yielding_statement(&for_stmt.body, ctx, update_state);
        if ctx.current_state_id != update_state {
            ctx.finalize_current_state(StateTerminator::Goto(update_state));
        }
    } else {
        ctx.emit_statement(*for_stmt.body.clone());
        ctx.finalize_current_state(StateTerminator::Goto(update_state));
    }

    ctx.current_state_id = update_state;
    if let Some(update) = &for_stmt.update {
        if expr_has_suspension(update, ctx.is_async) {
            transform_yielding_expression(update, ctx, test_state, None);
        } else {
            ctx.emit_statement(Statement::Expression(update.clone()));
        }
    }
    ctx.finalize_current_state(StateTerminator::Goto(test_state));

    if has_per_iteration_env {
        ctx.scope_depth -= 1;
    }

    if let Some(prev) = prev_break {
        ctx.break_targets.insert(None, prev);
    } else {
        ctx.break_targets.remove(&None);
    }
    if let Some(prev) = prev_continue {
        ctx.continue_targets.insert(None, prev);
    } else {
        ctx.continue_targets.remove(&None);
    }
    ctx.restore_labeled_continue_targets(labeled_continues);
    ctx.iteration_labels = iteration_labels;

    ctx.current_state_id = after_loop;
}

fn transform_for_in_statement(
    for_in_stmt: &ForInStatement,
    ctx: &mut TransformContext,
    after_state: usize,
) {
    // Annex B.3.5: `for (var x = init in obj)` evaluates the initializer once,
    // before the head's RHS.
    let mut left = for_in_stmt.left.clone();
    if let ForInOfLeft::Variable(decl) = &mut left
        && decl.kind == VarKind::Var
        && decl.declarations.first().is_some_and(|d| d.init.is_some())
    {
        transform_variable_declaration(decl, ctx, usize::MAX);
        for declarator in &mut decl.declarations {
            declarator.init = None;
        }
    }
    transform_for_in_of_loop(
        &left,
        &for_in_stmt.right,
        &for_in_stmt.body,
        false,
        true,
        ctx,
        after_state,
    );
}

fn transform_for_of_statement(
    for_of_stmt: &ForOfStatement,
    ctx: &mut TransformContext,
    after_state: usize,
) {
    transform_for_in_of_loop(
        &for_of_stmt.left,
        &for_of_stmt.right,
        &for_of_stmt.body,
        for_of_stmt.is_await,
        false,
        ctx,
        after_state,
    );
}

/// Lowers for-in, for-of and for-await-of onto the same `ForOfInit`/`ForOfHead`
/// pair. A for-in loop is a for-of over the spec's internal enumerator
/// (§14.7.5.10), so `is_for_in` only changes how `ForOfInit` builds the iterator.
fn transform_for_in_of_loop(
    left: &ForInOfLeft,
    right: &Expression,
    body: &Statement,
    is_await: bool,
    is_for_in: bool,
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

    let iteration_labels = std::mem::take(&mut ctx.iteration_labels);
    let break_target = ctx.loop_control_target(after_loop, ctx.for_of_depth);
    let continue_target = ctx.loop_control_target(head_state, ctx.for_of_depth + 1);
    let prev_break = ctx.break_targets.insert(None, break_target);
    let prev_continue = ctx.continue_targets.insert(None, continue_target);
    let labeled_continues =
        ctx.install_labeled_continue_targets(&iteration_labels, continue_target);

    // If the iterable expression contains a suspension point, evaluate it first
    let iterable_expr = if expr_has_suspension(right, ctx.is_async) {
        let temp_var = ctx.new_temp_var("forof_iterable");
        let iterable_binding = SentValueBindingKind::Variable(temp_var.clone());
        transform_yielding_expression(right, ctx, usize::MAX, Some(iterable_binding));
        Expression::Identifier(temp_var)
    } else {
        right.clone()
    };

    ctx.finalize_current_state(StateTerminator::ForOfInit {
        iterable: iterable_expr,
        iter_var: iter_var.clone(),
        label_set: iteration_labels.clone(),
        next_var: next_var.clone(),
        left: left.clone(),
        head_state,
        after_state: after_loop,
        is_await,
        is_for_in,
    });

    ctx.current_state_id = head_state;
    ctx.finalize_current_state(StateTerminator::ForOfHead {
        iter_var: iter_var.clone(),
        next_var: next_var.clone(),
        left: left.clone(),
        body_state,
        after_state: after_loop,
        is_await,
    });

    ctx.current_state_id = body_state;
    ctx.for_of_depth += 1;
    if stmt_has_suspension(body, ctx.is_async, ctx.detect_for_await) {
        transform_yielding_statement(body, ctx, head_state);
        if ctx.current_state_id != head_state {
            ctx.finalize_current_state(StateTerminator::Goto(head_state));
        }
    } else {
        ctx.emit_statement(body.clone());
        ctx.finalize_current_state(StateTerminator::Goto(head_state));
    }
    ctx.for_of_depth -= 1;

    if let Some(prev) = prev_break {
        ctx.break_targets.insert(None, prev);
    } else {
        ctx.break_targets.remove(&None);
    }
    if let Some(prev) = prev_continue {
        ctx.continue_targets.insert(None, prev);
    } else {
        ctx.continue_targets.remove(&None);
    }
    ctx.restore_labeled_continue_targets(labeled_continues);
    ctx.iteration_labels = iteration_labels;

    ctx.current_state_id = after_loop;
}

/// Lowers a `try`/`catch`/`finally` clause's own statement list. A clause
/// body that directly declares `await using` (no extra `{ }`) gets its own
/// scope, exactly like a nested block that does — its resource must dispose
/// at the clause's own exit, before control reaches `Catch`/`Finally`, not at
/// function exit (issue #683). Every other clause body lowers as it always
/// has: flattened into the enclosing state graph.
fn transform_clause_body(stmts: &[Statement], ctx: &mut TransformContext, after_state: usize) {
    if ctx.is_async && ctx.detect_for_await && block_has_await_using(stmts) {
        transform_scope_block(stmts, ctx, after_state);
    } else {
        transform_statements(stmts, ctx, after_state);
    }
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
    // A finally-less try/catch still needs a `TryExit` on its normal-completion
    // path: `TryEnter` unconditionally pushes a runtime `TryContextInfo`, and
    // only `TryExit` pops it. Without this, a finally-less try/catch's context
    // leaks on the runtime stack forever, desyncing every depth computed
    // afterwards (`try_depth`/`for_of_depth` on later `LoopControlTarget`s,
    // and exception-handler search) from this transform's own `try_stack`
    // bookkeeping, which pops on every try regardless of `finally`.
    let no_finally_exit_state = finally_entry_state.is_none().then(|| ctx.new_state());
    let clause_completion_state = finally_entry_state
        .or(no_finally_exit_state)
        .expect("exactly one of finally_entry_state/no_finally_exit_state is set");

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

    // try/catch/finally clauses are each a `Block` per grammar
    // (`sec-try-statement-runtime-semantics-evaluation`), so each gets its
    // own fresh scope exactly like a plain nested block. `try_body_state` and
    // `finally_body_state` already are fresh, dedicated entry states (unlike
    // the generic `Statement::Block` case), so no bridge state is needed
    // here — just bump/restore `scope_depth` and mark the entry.
    ctx.current_state_id = try_body_state;
    if ctx.is_async && ctx.detect_for_await && block_has_await_using(&try_stmt.block) {
        transform_scope_block(&try_stmt.block, ctx, clause_completion_state);
    } else {
        ctx.states[try_body_state].scope_action = Some(ScopeAction::OpenBlock);
        ctx.scope_depth += 1;
        transform_statements(&try_stmt.block, ctx, clause_completion_state);
        if ctx.current_state_id != clause_completion_state {
            ctx.finalize_current_state(StateTerminator::Goto(clause_completion_state));
        }
        ctx.scope_depth -= 1;
    }

    if let Some(ref info) = catch_info {
        let catch_body_state = ctx.new_state();
        ctx.current_state_id = info.state;
        ctx.finalize_current_state(StateTerminator::EnterCatch {
            body_state: catch_body_state,
            param: info.param.clone(),
        });

        // The catch parameter gets its own environment
        // (`sec-runtime-semantics-catchclauseevaluation`), pushed by the
        // driver's `EnterCatch` handling (it needs the thrown value, not
        // known until runtime) — so `catch_body_state` only needs the depth
        // bump, no `OpenBlock` marker; the generic reconciliation sees the
        // frame is already there.
        ctx.current_state_id = catch_body_state;
        ctx.scope_depth += 1;
        if let Some(handler) = &try_stmt.handler {
            transform_clause_body(&handler.body, ctx, clause_completion_state);
        }
        if ctx.current_state_id != clause_completion_state {
            ctx.finalize_current_state(StateTerminator::Goto(clause_completion_state));
        }
        ctx.scope_depth -= 1;
    }

    if let Some(fin_entry_state) = finally_entry_state {
        let finally_body_state = ctx.new_state();
        let finally_exit_state = ctx.new_state();
        ctx.current_state_id = fin_entry_state;
        ctx.finalize_current_state(StateTerminator::EnterFinally {
            body_state: finally_body_state,
        });

        ctx.current_state_id = finally_body_state;
        if let Some(finalizer) = &try_stmt.finalizer {
            if ctx.is_async && ctx.detect_for_await && block_has_await_using(finalizer) {
                transform_scope_block(finalizer, ctx, finally_exit_state);
            } else {
                ctx.states[finally_body_state].scope_action = Some(ScopeAction::OpenBlock);
                ctx.scope_depth += 1;
                transform_statements(finalizer, ctx, finally_exit_state);
                if ctx.current_state_id != finally_exit_state {
                    ctx.finalize_current_state(StateTerminator::Goto(finally_exit_state));
                }
                ctx.scope_depth -= 1;
            }
        }
        ctx.current_state_id = finally_exit_state;
        ctx.finalize_current_state(StateTerminator::TryExit {
            after_state: after_try,
        });
    } else if let Some(exit_state) = no_finally_exit_state {
        ctx.current_state_id = exit_state;
        ctx.finalize_current_state(StateTerminator::TryExit {
            after_state: after_try,
        });
    }

    ctx.try_stack.pop();
    ctx.current_state_id = after_try;
}

fn allocate_case_states(switch_stmt: &SwitchStatement, ctx: &mut TransformContext) -> Vec<usize> {
    switch_stmt.cases.iter().map(|_| ctx.new_state()).collect()
}

fn default_case_state(switch_stmt: &SwitchStatement, case_states: &[usize]) -> Option<usize> {
    switch_stmt
        .cases
        .iter()
        .zip(case_states)
        .find_map(|(case, &state)| case.test.is_none().then_some(state))
}

/// Lowers a switch whose case tests contain a suspension into a chain of
/// `ConditionalGoto` states, since `SwitchDispatch` evaluates its tests inside
/// the terminator where a `yield`/`await` cannot suspend. The discriminant is
/// captured once so a selector cannot change the value being compared.
/// Returns the case body states.
fn lower_switch_dispatch_with_suspending_tests(
    switch_stmt: &SwitchStatement,
    ctx: &mut TransformContext,
    after_switch: usize,
) -> Vec<usize> {
    let disc_var = ctx.new_temp_var("switch_disc");
    let disc_binding = Some(SentValueBindingKind::Variable(disc_var.clone()));
    if expr_has_suspension(&switch_stmt.discriminant, ctx.is_async) {
        transform_yielding_expression(&switch_stmt.discriminant, ctx, usize::MAX, disc_binding);
    } else {
        emit_expression_with_binding(&switch_stmt.discriminant, &disc_binding, ctx);
    }

    let case_states = allocate_case_states(switch_stmt, ctx);
    let case_var = ctx.new_temp_var("switch_case");
    for (case, &case_state) in switch_stmt.cases.iter().zip(&case_states) {
        let Some(test) = &case.test else { continue };
        let selector = if expr_has_suspension(test, ctx.is_async) {
            let case_binding = SentValueBindingKind::Variable(case_var.clone());
            transform_yielding_expression(test, ctx, usize::MAX, Some(case_binding));
            Expression::Identifier(case_var.clone())
        } else {
            test.clone()
        };
        let next_test_state = ctx.new_state();
        ctx.finalize_current_state(StateTerminator::ConditionalGoto {
            condition: Expression::Binary(
                BinaryOp::StrictEq,
                ExprBox::new(Expression::Identifier(disc_var.clone())),
                ExprBox::new(selector),
            ),
            true_state: case_state,
            false_state: next_test_state,
        });
        ctx.current_state_id = next_test_state;
    }
    let fallback = default_case_state(switch_stmt, &case_states).unwrap_or(after_switch);
    ctx.finalize_current_state(StateTerminator::Goto(fallback));
    case_states
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

    let break_target = ctx.loop_control_target(after_switch, ctx.for_of_depth);
    let prev_break = ctx.break_targets.insert(None, break_target);

    let tests_suspend = switch_stmt.cases.iter().any(|case| {
        case.test
            .as_ref()
            .is_some_and(|test| expr_has_suspension(test, ctx.is_async))
    });

    let case_states = if tests_suspend {
        lower_switch_dispatch_with_suspending_tests(switch_stmt, ctx, after_switch)
    } else {
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

        let case_states = allocate_case_states(switch_stmt, ctx);
        let case_targets = switch_stmt
            .cases
            .iter()
            .zip(&case_states)
            .filter_map(|(case, &state)| {
                case.test.as_ref().map(|test| SwitchCaseTarget {
                    test: test.clone(),
                    state,
                })
            })
            .collect();

        ctx.finalize_current_state(StateTerminator::SwitchDispatch {
            discriminant: temp_discriminant,
            cases: case_targets,
            default_state: default_case_state(switch_stmt, &case_states),
            after_state: after_switch,
        });
        case_states
    };

    for (i, case) in switch_stmt.cases.iter().enumerate() {
        ctx.current_state_id = case_states[i];
        let next_state = if i + 1 < case_states.len() {
            case_states[i + 1]
        } else {
            after_switch
        };

        // A yield-free body is emitted verbatim, where a `break`/`continue` would
        // surface as a raw completion the state driver drops, falling through.
        if case.consequent.iter().any(|s| {
            stmt_has_suspension(s, ctx.is_async, ctx.detect_for_await)
                || stmt_has_break_or_continue(s)
        }) {
            transform_statements(&case.consequent, ctx, next_state);
        } else {
            for stmt in &case.consequent {
                ctx.emit_statement(stmt.clone());
            }
        }
        if ctx.current_state_id != next_state {
            ctx.finalize_current_state(StateTerminator::Goto(next_state));
        }
    }

    if let Some(prev) = prev_break {
        ctx.break_targets.insert(None, prev);
    } else {
        ctx.break_targets.remove(&None);
    }
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

    let break_target = ctx.loop_control_target(after_labeled, ctx.for_of_depth);
    let previous_break = ctx
        .break_targets
        .insert(Some(label.to_string()), break_target);
    let labels_iteration = labels_iteration_statement(stmt);
    if labels_iteration {
        ctx.iteration_labels.push(label.to_string());
    }

    if stmt_has_suspension(stmt, ctx.is_async, ctx.detect_for_await) {
        transform_yielding_statement(stmt, ctx, after_labeled);
    } else {
        ctx.emit_statement(stmt.clone());
    }

    if labels_iteration {
        ctx.iteration_labels.pop();
    }
    if let Some(previous_break) = previous_break {
        ctx.break_targets
            .insert(Some(label.to_string()), previous_break);
    } else {
        ctx.break_targets.remove(&Some(label.to_string()));
    }
    ctx.finalize_current_state(StateTerminator::Goto(after_labeled));
    ctx.current_state_id = after_labeled;
}

pub(crate) fn transform_async_function(
    body: &[Statement],
    params: &[Pattern],
) -> GeneratorStateMachine {
    let rewritten = rewrite_stmts_await_to_yield(body);
    let mut machine = transform_generator_inner_opts(&rewritten, params, true, true);
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
        Expression::Await(inner) => {
            Expression::Yield(Some(ExprBox::new(rewrite_expr(inner))), false)
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
        Expression::Binary(op, l, r) => Expression::Binary(
            *op,
            ExprBox::new(rewrite_expr(l)),
            ExprBox::new(rewrite_expr(r)),
        ),
        Expression::Logical(op, l, r) => Expression::Logical(
            *op,
            ExprBox::new(rewrite_expr(l)),
            ExprBox::new(rewrite_expr(r)),
        ),
        Expression::Update(op, prefix, e) => {
            Expression::Update(*op, *prefix, ExprBox::new(rewrite_expr(e)))
        }
        Expression::Assign(op, l, r) => Expression::Assign(
            *op,
            ExprBox::new(rewrite_expr(l)),
            ExprBox::new(rewrite_expr(r)),
        ),
        Expression::Conditional(t, c, a) => Expression::Conditional(
            ExprBox::new(rewrite_expr(t)),
            ExprBox::new(rewrite_expr(c)),
            ExprBox::new(rewrite_expr(a)),
        ),
        Expression::Call(callee, args, _) => Expression::Call(
            ExprBox::new(rewrite_expr(callee)),
            args.iter().map(rewrite_expr).collect(),
            CallSiteId::UNASSIGNED,
        ),
        Expression::New(callee, args, _) => Expression::New(
            ExprBox::new(rewrite_expr(callee)),
            args.iter().map(rewrite_expr).collect(),
            CallSiteId::UNASSIGNED,
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
                expressions: tl.expressions.iter().map(rewrite_expr).collect(),
            },
        ),
        Expression::Template(tl) => Expression::Template(TemplateLiteral {
            id: tl.id,
            quasis: tl.quasis.clone(),
            raw_quasis: tl.raw_quasis.clone(),
            expressions: tl.expressions.iter().map(rewrite_expr).collect(),
        }),
        Expression::Typeof(e) => Expression::Typeof(ExprBox::new(rewrite_expr(e))),
        Expression::Void(e) => Expression::Void(ExprBox::new(rewrite_expr(e))),
        Expression::Delete(e) => Expression::Delete(ExprBox::new(rewrite_expr(e))),
        Expression::Sequence(exprs) => {
            Expression::Sequence(exprs.iter().map(rewrite_expr).collect())
        }
        Expression::Import(spec, opts) => Expression::Import(
            ExprBox::new(rewrite_expr(spec)),
            opts.as_ref().map(|o| ExprBox::new(rewrite_expr(o))),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_yield() -> Expression {
        Expression::Yield(None, false)
    }

    fn make_yield_expr(val: f64) -> Expression {
        Expression::Yield(
            Some(ExprBox::new(Expression::Literal(Literal::Number(val)))),
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

    fn empty_function_expr() -> FunctionExpr {
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

    #[test]
    fn test_yield_in_class_computed_key_is_decomposed() {
        // The computed key is hoisted into its own state rather than replaying
        // the whole class declaration on resume.
        let body = vec![Statement::ClassDeclaration(ClassDecl {
            name: "C".to_string(),
            super_class: None,
            body: vec![ClassElement::Method(ClassMethod {
                key: PropertyKey::Computed(ExprBox::new(make_yield())),
                kind: ClassMethodKind::Method,
                value: empty_function_expr(),
                is_static: false,
                computed: true,
            })],
            source_text: None,
        })];
        let sm = transform_generator(&body, &[]);

        assert_eq!(sm.num_yields, 1);
        assert!(sm.states.len() > 1);
    }

    fn plain_class_decl() -> Statement {
        Statement::ClassDeclaration(ClassDecl {
            name: "C".to_string(),
            super_class: Some(Box::new(Expression::Identifier("Base".to_string()))),
            body: vec![ClassElement::Method(ClassMethod {
                key: PropertyKey::Identifier("method".to_string()),
                kind: ClassMethodKind::Method,
                value: empty_function_expr(),
                is_static: false,
                computed: false,
            })],
            source_text: None,
        })
    }

    #[test]
    fn test_class_without_suspension_takes_simple_machine_fast_path() {
        let sm = transform_generator(&[plain_class_decl()], &[]);

        assert_eq!(sm.num_yields, 0);
        assert_eq!(sm.states.len(), 1);
    }

    #[test]
    fn test_class_without_suspension_is_not_hoisted_in_decomposed_generator() {
        // The leading yield forces full decomposition; the class itself has no
        // suspending heritage/key, so it must go through the plain statement
        // path and create no hoisting temp vars.
        let body = vec![Statement::Expression(make_yield()), plain_class_decl()];
        let sm = transform_generator(&body, &[]);

        assert_eq!(sm.num_yields, 1);
        assert!(sm.temp_vars.is_empty());
        let class_emitted = sm.states.iter().any(|s| {
            s.body
                .as_slice()
                .iter()
                .any(|stmt| matches!(stmt, Statement::ClassDeclaration(_)))
        });
        assert!(class_emitted);
    }

    #[test]
    fn test_only_new_temp_var_names_are_generated_temps() {
        let mut ctx = TransformContext::new(analyze_generator_body(&[], &[]), false);
        let generated = ctx.new_temp_var("call_arg_0");
        ctx.temp_vars.push("$a_1".to_string());

        assert!(ctx.generated_temps.contains(&generated));
        assert!(!ctx.generated_temps.contains("$a_1"));
    }

    #[test]
    fn test_yield_in_class_expression_heritage_is_decomposed() {
        let body = vec![Statement::Variable(VariableDeclaration {
            kind: VarKind::Let,
            declarations: vec![VariableDeclarator {
                pattern: Pattern::Identifier("C".to_string()),
                init: Some(Expression::Class(ClassExpr {
                    name: None,
                    super_class: Some(Box::new(make_yield())),
                    body: vec![],
                    source_text: None,
                })),
            }],
        })];
        let sm = transform_generator(&body, &[]);

        assert_eq!(sm.num_yields, 1);
        assert!(sm.states.len() > 1);
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

    fn switch_body(discriminant: Expression, tests: Vec<Expression>) -> Vec<Statement> {
        vec![Statement::Switch(SwitchStatement {
            discriminant,
            cases: tests
                .into_iter()
                .map(|test| SwitchCase {
                    test: Some(test),
                    consequent: vec![Statement::Expression(make_yield())],
                })
                .collect(),
        })]
    }

    fn has_switch_dispatch(sm: &GeneratorStateMachine) -> bool {
        sm.states
            .iter()
            .any(|s| matches!(s.terminator, StateTerminator::SwitchDispatch { .. }))
    }

    #[test]
    fn test_switch_with_suspending_case_test_is_lowered() {
        let one = Expression::Literal(Literal::Number(1.0));
        let body = switch_body(one.clone(), vec![make_yield_expr(5.0), one]);
        let sm = transform_generator(&body, &[]);

        assert!(!has_switch_dispatch(&sm));
    }

    #[test]
    fn test_switch_without_suspending_case_test_keeps_dispatch() {
        let one = Expression::Literal(Literal::Number(1.0));
        let plain = switch_body(one.clone(), vec![one.clone()]);
        assert!(has_switch_dispatch(&transform_generator(&plain, &[])));

        let yielding_discriminant = switch_body(make_yield_expr(5.0), vec![one]);
        assert!(has_switch_dispatch(&transform_generator(
            &yielding_discriminant,
            &[]
        )));
    }

    fn parse_fn_body(src: &str) -> Vec<Statement> {
        let mut parser = crate::parser::Parser::new(src).expect("parser init");
        let program = parser.parse_program().expect("parse");
        match program.body.as_slice().first() {
            Some(Statement::FunctionDeclaration(f)) => f.body.as_slice().to_vec(),
            other => panic!("expected a function declaration, got {other:?}"),
        }
    }

    /// The first statement of the innermost loop/label/block wrapper, i.e. the
    /// statement under test once wrapped in `<wrapper> { STMT }`.
    fn wrapped_stmt(stmt: &Statement) -> Statement {
        match stmt {
            Statement::Labeled(_, inner) => wrapped_stmt(inner),
            Statement::While(w) => wrapped_stmt(&w.body),
            Statement::For(f) => wrapped_stmt(&f.body),
            Statement::Block(stmts) => stmts.first().cloned().expect("non-empty block"),
            other => other.clone(),
        }
    }

    fn escaping_in(wrapper_src: &str) -> Vec<(JumpKind, Option<String>)> {
        let body = parse_fn_body(&format!("function f() {{ {wrapper_src} }}"));
        escaping_jumps(&wrapped_stmt(&body[0]))
    }

    fn brk(label: Option<&str>) -> (JumpKind, Option<String>) {
        (JumpKind::Break, label.map(str::to_owned))
    }

    fn cont(label: Option<&str>) -> (JumpKind, Option<String>) {
        (JumpKind::Continue, label.map(str::to_owned))
    }

    #[test]
    fn test_escaping_jumps_reports_try_and_with_bodies() {
        assert_eq!(
            escaping_in("while (1) { try { break; } finally {} }"),
            vec![brk(None)]
        );
        assert_eq!(
            escaping_in("while (1) { try { throw 0; } catch (e) { continue; } }"),
            vec![cont(None)]
        );
        assert_eq!(
            escaping_in("while (1) { try { } finally { break; } }"),
            vec![brk(None)]
        );
        assert_eq!(
            escaping_in("while (1) { with (o) { break; } }"),
            vec![brk(None)]
        );
    }

    #[test]
    fn test_escaping_jumps_native_targets_consume_jumps() {
        assert_eq!(escaping_in("x: { while (1) { break; } }"), vec![]);
        assert_eq!(escaping_in("x: { while (1) { continue; } }"), vec![]);
        assert_eq!(escaping_in("x: { switch (y) { case 0: break; } }"), vec![]);
        assert_eq!(
            escaping_in("while (1) { switch (y) { case 0: continue; } }"),
            vec![cont(None)]
        );
        assert_eq!(
            escaping_in("while (1) { for (;;) { switch (y) { case 0: continue; } } }"),
            vec![]
        );
    }

    #[test]
    fn test_escaping_jumps_are_label_aware() {
        assert_eq!(
            escaping_in("while (1) { outer: while (1) { break outer; } }"),
            vec![]
        );
        assert_eq!(
            escaping_in("while (1) { outer: while (1) { continue outer; } }"),
            vec![]
        );
        assert_eq!(
            escaping_in("outer: while (1) { while (1) { break outer; } }"),
            vec![brk(Some("outer"))]
        );
        assert_eq!(
            escaping_in("outer: while (1) { while (1) { continue outer; } }"),
            vec![cont(Some("outer"))]
        );
        assert_eq!(
            escaping_in("outer: while (1) { blk: { break blk; } }"),
            vec![]
        );
    }

    #[test]
    fn test_escaping_jumps_cover_branches_and_skip_functions() {
        assert_eq!(
            escaping_in("while (1) { if (a) { break; } else { continue; } }"),
            vec![brk(None), cont(None)]
        );
        assert_eq!(
            escaping_in("while (1) { function g() { while (1) { break; } } }"),
            vec![]
        );
    }

    fn state_with_inline_jump(sm: &GeneratorStateMachine, kind: JumpKind) -> Vec<&GeneratorState> {
        sm.states
            .iter()
            .filter(|s| s.inline_jumps.iter().any(|j| j.kind == kind))
            .collect()
    }

    #[test]
    fn test_yield_free_try_break_in_switch_case_records_inline_jump() {
        let body = parse_fn_body(
            "function* g(x) { switch (x) { case 1: try { break; } finally { f(); } case 2: g(); break; case 3: yield 0; } }",
        );
        let sm = transform_generator(&body, &[]);
        let Some(after_switch) = sm.states.iter().find_map(|s| match &s.terminator {
            StateTerminator::SwitchDispatch { after_state, .. } => Some(*after_state),
            _ => None,
        }) else {
            panic!("no switch dispatch state");
        };

        let with_jump = state_with_inline_jump(&sm, JumpKind::Break);
        assert_eq!(with_jump.len(), 1);
        let jump = &with_jump[0].inline_jumps[0];
        assert_eq!(jump.label, None);
        assert!(
            matches!(&jump.terminator, StateTerminator::LoopControl(t) if t.target_state == after_switch)
        );
        assert_eq!(with_jump[0].inline_jumps.len(), 1);
    }

    fn loop_control_targets(sm: &GeneratorStateMachine) -> Vec<LoopControlTarget> {
        sm.states
            .iter()
            .filter_map(|s| match &s.terminator {
                StateTerminator::LoopControl(target) => Some(*target),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_break_in_yielding_try_of_sync_generator_lowers_to_loop_control() {
        let body = parse_fn_body(
            "function* g() { for (;;) { try { yield 1; break; } finally { f(); } } }",
        );
        let sm = transform_generator(&body, &[]);
        let targets = loop_control_targets(&sm);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].try_depth, 0);
        assert_eq!(targets[0].for_of_depth, 0);
    }

    #[test]
    fn test_loop_control_target_records_enclosing_try_depth() {
        let body = parse_fn_body(
            "function* g() { try { for (;;) { try { yield 1; continue; } finally { f(); } } } finally { h(); } }",
        );
        let sm = transform_generator(&body, &[]);
        let targets = loop_control_targets(&sm);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].try_depth, 1);
    }

    #[test]
    fn test_natively_consumed_jump_records_nothing() {
        let body = parse_fn_body(
            "function* g(x) { switch (x) { case 1: while (1) { try { break; } finally {} } case 2: yield 0; } }",
        );
        let sm = transform_generator(&body, &[]);
        assert!(sm.states.iter().all(|s| s.inline_jumps.is_empty()));
    }

    #[test]
    fn test_labeled_continue_out_of_yield_free_try_records_inline_jump() {
        let body = parse_fn_body(
            "function* g() { outer: for (var i = 0; i < 3; i++) { for (;;) { try { continue outer; } finally {} } yield i; } }",
        );
        let sm = transform_generator(&body, &[]);
        let with_jump = state_with_inline_jump(&sm, JumpKind::Continue);
        assert_eq!(with_jump.len(), 1);
        assert_eq!(with_jump[0].inline_jumps[0].label.as_deref(), Some("outer"));
    }

    #[test]
    fn test_async_function_inline_jump_uses_loop_control() {
        let body = parse_fn_body(
            "async function f(x) { for (var i = 0; i < 3; i++) { try { if (i == 1) break; } finally {} await i; } }",
        );
        let sm = transform_async_function(&body, &[]);
        let with_jump = state_with_inline_jump(&sm, JumpKind::Break);
        assert_eq!(with_jump.len(), 1);
        assert!(matches!(
            with_jump[0].inline_jumps[0].terminator,
            StateTerminator::LoopControl(_)
        ));
    }

    #[test]
    fn test_plain_await_using_for_of_head_is_lowered() {
        // The only "suspension" here is the `await using` ForDeclaration's
        // per-iteration disposal Await, not the (non-`for await`) iteration
        // protocol or anything in the body. It must still take the state
        // machine, not the `create_simple_machine` fast path.
        let body = parse_fn_body("async function f(y) { for (await using x of y) {} }");
        let sm = transform_async_function(&body, &[]);
        assert!(
            sm.states.len() > 1,
            "expected a real state machine, got the simple-machine fast path"
        );
        assert!(
            sm.states
                .iter()
                .any(|s| matches!(s.terminator, StateTerminator::ForOfHead { .. })),
            "expected a ForOfHead terminator"
        );
    }

    fn async_machine(body_src: &str) -> GeneratorStateMachine {
        let body = parse_fn_body(&format!("async function f() {{ {body_src} }}"));
        transform_async_function(&body, &[])
    }

    fn count_terminators(
        sm: &GeneratorStateMachine,
        is_kind: impl Fn(&StateTerminator) -> bool,
    ) -> usize {
        sm.states.iter().filter(|s| is_kind(&s.terminator)).count()
    }

    fn state_reads_property_of_source(state: &GeneratorState) -> bool {
        state.body.as_slice().iter().any(|stmt| {
            matches!(
                stmt,
                Statement::Expression(Expression::Assign(_, _, rhs))
                    if matches!(&**rhs, Expression::Member(..))
            )
        })
    }

    #[test]
    fn test_awaiting_default_lowers_to_conditional_await_state() {
        let sm = async_machine("var { a = await 1 } = {}; return a;");

        assert_eq!(
            count_terminators(&sm, |t| matches!(t, StateTerminator::Await { .. })),
            1,
            "the default's await is a real Await state"
        );
        assert_eq!(
            count_terminators(&sm, |t| matches!(
                t,
                StateTerminator::ConditionalGoto { .. }
            )),
            1,
            "the default only runs when the property is undefined"
        );
        let reader = sm
            .states
            .iter()
            .find(|s| state_reads_property_of_source(s))
            .expect("a state reads the property once");
        assert!(
            !matches!(reader.terminator, StateTerminator::Await { .. }),
            "the property read must not share a state with the await"
        );
    }

    #[test]
    fn test_present_property_pattern_without_await_takes_simple_machine() {
        let sm = async_machine("var { a = 1 } = {}; let [b = 2] = []; a;");

        assert_eq!(sm.states.len(), 1);
        assert!(sm.temp_vars.is_empty());
    }

    #[test]
    fn test_awaiting_computed_key_is_lowered_at_its_own_position() {
        let sm = async_machine("var { a, [await k]: b } = o;");

        assert_eq!(
            count_terminators(&sm, |t| matches!(t, StateTerminator::Await { .. })),
            1
        );
        let first_await = sm
            .states
            .iter()
            .position(|s| matches!(s.terminator, StateTerminator::Await { .. }))
            .expect("an await state");
        let a_bound_before_await = sm.states[..=first_await].iter().any(|s| {
            s.body.as_slice().iter().any(|stmt| match stmt {
                Statement::Variable(v) => v.declarations.iter().any(|d| {
                    matches!(&d.pattern, Pattern::Object(props) if props.iter().any(|p| matches!(p,
                        ObjectPatternProperty::Shorthand(n) if n == "a")))
                }),
                _ => false,
            })
        });
        assert!(
            a_bound_before_await,
            "the earlier property is read before the key's await suspends"
        );
    }

    #[test]
    fn test_unsupported_pattern_shapes_stay_on_the_tree_walker() {
        for src in [
            "var [a = await 1] = [];",
            "var { a = await 1, ...rest } = {};",
            "var { x: [a = await 1] } = {};",
        ] {
            let sm = async_machine(src);
            assert_eq!(sm.states.len(), 1, "expected the simple machine for: {src}");
        }
    }

    #[test]
    fn test_async_generator_yield_only_pattern_is_not_lowered() {
        let body = parse_fn_body("async function* g() { var { a = yield 1 } = {}; }");
        let sm = transform_async_generator(&body, &[]);

        assert_eq!(sm.states.len(), 1);
    }
}
