//! Loop bodies that run in one environment for the whole loop.
//!
//! A loop body block that declares `let`/`const` normally pays `PushEnv`/`PopEnv`
//! on every iteration. When nothing can observe the identity of the per-iteration
//! environment, the block's bindings can live in one environment created before
//! the loop (merged with the loop head's own scope) instead, each iteration
//! re-initialising them through `InitLexical`.
//!
//! That is unobservable only when:
//!
//! - nothing in the body can capture the environment: no function, arrow, class,
//!   `with` or `eval` mention (the scan `for (let ...)` uses to skip
//!   CreatePerIterationEnvironment);
//! - no name the head, test or update mentions is shadowed by the body's
//!   bindings, since those parts now run inside the merged environment;
//! - no read can see the previous iteration's value in place of the TDZ: every
//!   mention of a body binding comes after its declarator has initialised it, in
//!   source order, which is also the execution order of a statement list;
//! - every top-level declaration is one the compiler lowers to `InitLexical`
//!   (an identifier, or a plain object pattern with an initializer), never a
//!   tree-walker hand-off that would re-declare an initialised binding.

use super::{CompileGoal, Compiler, ScopeDecls, lexical::simple_object_pattern};
use crate::ast::{CaptureWalk, Expression, ForInOfLeft, ForInit, Pattern, Statement, VarKind};
use crate::interpreter::bytecode::compiler::CompileError;
use crate::interpreter::hoisting::{block_declares_lexical, collect_lexical_names};
use std::collections::HashSet;

/// The parts of a loop that run outside its body block.
#[derive(Default)]
pub(super) struct LoopHead<'a> {
    pub init: Option<&'a ForInit>,
    pub left: Option<&'a ForInOfLeft>,
    pub test: Option<&'a Expression>,
    pub update: Option<&'a Expression>,
}

/// What `open_loop_scope` did, for the loop to finish with.
pub(super) struct LoopScope {
    /// A scope was opened and the loop must `close_scope` it.
    pub opened: bool,
    /// The body block's bindings live in it.
    pub body_merged: bool,
}

/// The `let`/`const` names of `body`, in declaration order, when its block can
/// share one environment across iterations with `head`.
fn mergeable_body_lexicals(body: &Statement, head: &LoopHead) -> Option<Vec<(String, bool)>> {
    let Statement::Block(stmts) = body else {
        return None;
    };
    if !block_declares_lexical(stmts) {
        return None;
    }
    let mut outside = CaptureWalk::collecting();
    outside.push_for_init(head.init);
    if let Some(left) = head.left {
        outside.push_for_in_of_left(left);
    }
    outside.push_opt_expr(head.test);
    outside.push_opt_expr(head.update);
    if outside.run() {
        return None;
    }
    let outside_names: HashSet<&str> = outside.mentions.iter().copied().collect();

    let lexicals = collect_lexical_names(stmts);
    let mut uninitialized: HashSet<&str> = HashSet::new();
    for (name, _) in &lexicals {
        if outside_names.contains(name.as_str()) || !uninitialized.insert(name.as_str()) {
            return None;
        }
    }
    for stmt in stmts {
        match stmt {
            Statement::Variable(decl) if matches!(decl.kind, VarKind::Let | VarKind::Const) => {
                for declarator in &decl.declarations {
                    let lowered = match &declarator.pattern {
                        Pattern::Identifier(_) => true,
                        pattern => {
                            declarator.init.is_some() && simple_object_pattern(pattern).is_some()
                        }
                    };
                    if !lowered {
                        return None;
                    }
                    let mut walk = CaptureWalk::collecting();
                    walk.push_opt_expr(declarator.init.as_ref());
                    if walk.run() || walk.mentions.iter().any(|m| uninitialized.contains(m)) {
                        return None;
                    }
                    let mut bound = Vec::new();
                    declarator.pattern.bound_names(&mut bound);
                    for name in &bound {
                        uninitialized.remove(name.as_str());
                    }
                }
            }
            Statement::Variable(decl) if decl.kind != VarKind::Var => return None,
            other => {
                let mut walk = CaptureWalk::collecting();
                walk.push_stmt(other);
                if walk.run() || walk.mentions.iter().any(|m| uninitialized.contains(m)) {
                    return None;
                }
            }
        }
    }
    Some(lexicals)
}

impl Compiler {
    /// Opens the scope a loop runs in: `head_lexicals` (the loop head's own
    /// bindings) plus, when `mergeable` and the body block qualifies, the body
    /// block's bindings. Opens nothing for a loop with neither.
    pub(super) fn open_loop_scope(
        &mut self,
        mut head_lexicals: Vec<(String, bool)>,
        body: &Statement,
        head: &LoopHead,
        mergeable: bool,
    ) -> Result<LoopScope, CompileError> {
        let body_lexicals = if mergeable && self.goal == CompileGoal::Function {
            mergeable_body_lexicals(body, head)
        } else {
            None
        };
        let body_merged = body_lexicals.is_some();
        head_lexicals.extend(body_lexicals.unwrap_or_default());
        let opened = !head_lexicals.is_empty();
        if opened {
            self.open_scope(ScopeDecls {
                lexicals: head_lexicals,
                ..ScopeDecls::default()
            })?;
        }
        Ok(LoopScope {
            opened,
            body_merged,
        })
    }

    /// The loop body: a merged block's statements run in the loop's own scope.
    pub(super) fn compile_loop_body(
        &mut self,
        body: &Statement,
        scope: &LoopScope,
    ) -> Result<(), CompileError> {
        match body {
            Statement::Block(stmts) if scope.body_merged => {
                self.compile_statements(stmts, false, true)
            }
            _ => self.compile_statement(body),
        }
    }
}
