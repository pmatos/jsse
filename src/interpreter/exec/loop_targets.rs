//! Binding one iteration value, independently of loop execution and cleanup.

use super::*;
use crate::interpreter::bytecode::chunk::HeadBinding;

/// Environments already created and published by the loop driver. Ordinary
/// heads select the outer environment for `var` and assignments, and the
/// iteration environment for lexical declarations. Lowered drivers have
/// selected their declaration environment and retain their own assignment scope.
pub(crate) enum LoopHeadContext<'a> {
    ForIn {
        outer: &'a EnvRef,
        iteration: &'a EnvRef,
        /// Prepared by `simple_head_binding` only for a shared environment.
        simple: Option<&'a (Atom, BindingKind)>,
    },
    ForOf {
        outer: &'a EnvRef,
        iteration: &'a EnvRef,
        simple: Option<&'a (Atom, BindingKind)>,
    },
    Lowered {
        declaration: &'a EnvRef,
        assignment: &'a EnvRef,
    },
}

#[derive(Clone, Copy, PartialEq)]
enum HeadFlavor {
    In,
    Of,
    Lowered,
}

/// AST heads and the VM's complex heads use the same pattern operation.
enum HeadTarget<'a> {
    Declare(&'a Pattern, BindingKind),
    Assign(&'a Pattern),
    Expression(&'a Expression),
}

impl Interpreter {
    /// §14.7.5 ForIn/OfBodyEvaluation's per-value binding operation. The
    /// driver owns iteration environments, suspension, disposal, IteratorClose
    /// and completion routing; this operation returns every completion intact.
    pub(crate) fn bind_loop_head(
        &mut self,
        left: &ForInOfLeft,
        value: JsValue,
        context: LoopHeadContext<'_>,
    ) -> Completion {
        let flavor = match &context {
            LoopHeadContext::ForIn { .. } => HeadFlavor::In,
            LoopHeadContext::ForOf { .. } => HeadFlavor::Of,
            LoopHeadContext::Lowered { .. } => HeadFlavor::Lowered,
        };
        let (declaration, assignment, simple) = match context {
            LoopHeadContext::ForIn {
                outer,
                iteration,
                simple,
            }
            | LoopHeadContext::ForOf {
                outer,
                iteration,
                simple,
            } => {
                let declaration = if matches!(left, ForInOfLeft::Variable(d) if d.kind == VarKind::Var)
                {
                    outer
                } else {
                    iteration
                };
                (declaration, outer, simple)
            }
            LoopHeadContext::Lowered {
                declaration,
                assignment,
            } => (declaration, assignment, None),
        };
        self.with_gc_root_scope(|interp| {
            interp.gc_root_value(&value);
            let target = match left {
                ForInOfLeft::Variable(decl) => {
                    let kind = match decl.kind {
                        VarKind::Var => BindingKind::Var,
                        VarKind::Let => BindingKind::Let,
                        VarKind::Const | VarKind::Using | VarKind::AwaitUsing => BindingKind::Const,
                    };
                    if flavor != HeadFlavor::In
                        && matches!(decl.kind, VarKind::Using | VarKind::AwaitUsing)
                    {
                        let hint = DisposeHint::for_var_kind(decl.kind);
                        propagate!(interp.add_disposable_resource(declaration, &value, hint));
                    }
                    if let Some((atom, kind)) = simple {
                        declaration
                            .borrow_mut()
                            .bindings
                            .push_unique(atom.clone(), Binding::new(value, *kind, true));
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                    let Some(d) = decl.declarations.first() else {
                        return Completion::Normal(JsValue::UNDEFINED);
                    };
                    HeadTarget::Declare(&d.pattern, kind)
                }
                ForInOfLeft::Pattern(pattern) => HeadTarget::Assign(pattern),
                ForInOfLeft::Expression(expr) => HeadTarget::Expression(expr),
            };
            interp.bind_iteration_target(target, value, declaration, assignment, flavor)
        })
    }

    /// The VM retains native simple-head stores. Its complex heads cross the
    /// same pattern seam as AST heads, while catch parameters remain distinct.
    pub(crate) fn bind_head_binding(
        &mut self,
        head: &HeadBinding,
        value: JsValue,
        env: &EnvRef,
    ) -> Completion {
        self.with_gc_root_scope(|interp| {
            interp.gc_root_value(&value);
            let target = match head {
                HeadBinding::Declare { pattern, kind } => HeadTarget::Declare(pattern, *kind),
                HeadBinding::Assign { pattern } => HeadTarget::Assign(pattern),
                HeadBinding::Catch { pattern } => return interp.bind_catch(pattern, value, env),
            };
            interp.bind_iteration_target(target, value, env, env, HeadFlavor::Of)
        })
    }

    fn bind_iteration_target(
        &mut self,
        target: HeadTarget<'_>,
        value: JsValue,
        declaration: &EnvRef,
        assignment: &EnvRef,
        flavor: HeadFlavor,
    ) -> Completion {
        match target {
            HeadTarget::Declare(pattern, kind) => {
                // Ordinary for-in keeps its existing diagnostic origin. Other
                // declaration heads use the Node adapter's `.for` origin.
                if flavor == HeadFlavor::In {
                    self.bind_pattern(pattern, value, kind, declaration)
                } else {
                    self.bind_head(pattern, value, kind, declaration)
                }
            }
            HeadTarget::Assign(pattern) => {
                if flavor == HeadFlavor::In {
                    match pattern {
                        Pattern::Identifier(name) => {
                            propagate!(self.assign_loop_identifier_target(name, value, assignment));
                            return Completion::Normal(JsValue::UNDEFINED);
                        }
                        Pattern::MemberExpression(expr) => {
                            propagate!(self.assign_to_expr(expr, value, assignment));
                            return Completion::Normal(JsValue::UNDEFINED);
                        }
                        _ => {}
                    }
                }
                self.assign_to_for_pattern(pattern, value, assignment)
            }
            HeadTarget::Expression(expr) => {
                if flavor != HeadFlavor::Lowered
                    && matches!(expr, Expression::Identifier(..) | Expression::Member(..))
                {
                    propagate!(self.assign_to_expr(expr, value, assignment));
                    return Completion::Normal(JsValue::UNDEFINED);
                }
                // The lowered drivers leave only web-compatible call targets
                // as expressions. Evaluate once, then reject the target.
                propagate!(self.eval_expr(expr, assignment));
                let kind = match flavor {
                    HeadFlavor::In => "in",
                    HeadFlavor::Of => "of",
                    HeadFlavor::Lowered => "in/of",
                };
                Completion::Throw(self.invalid_for_target(kind))
            }
        }
    }

    /// PutValue of one iteration's value to a for-in/of identifier target
    /// (§14.7.5.7 ForIn/OfBodyEvaluation): the reference is resolved afresh, so
    /// a binding still in its TDZ or a `const` throws and strict code cannot
    /// create an implicit global.
    pub(super) fn assign_loop_identifier_target(
        &mut self,
        name: &str,
        value: JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        let id_ref = self.resolve_identifier_ref(name, env)?;
        match self.put_value_by_ref(name, value, &id_ref, env) {
            Completion::Throw(e) => Err(e),
            _ => Ok(()),
        }
    }

    /// For a loop sharing one iteration environment, the `let`/`const`
    /// identifier its head binds, interned once so each iteration installs it
    /// without hashing the name again.
    pub(super) fn simple_head_binding(
        left: &ForInOfLeft,
        shared_env: Option<&EnvRef>,
    ) -> Option<(Atom, BindingKind)> {
        shared_env?;
        let ForInOfLeft::Variable(decl) = left else {
            return None;
        };
        let kind = match decl.kind {
            VarKind::Let => BindingKind::Let,
            VarKind::Const => BindingKind::Const,
            _ => return None,
        };
        match decl.declarations.first().map(|d| &d.pattern) {
            Some(Pattern::Identifier(name)) => Some((Atom::intern(name), kind)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
