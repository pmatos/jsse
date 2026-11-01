//! The environments a lexical for-in/of head needs (§14.7.5.12).

use super::*;

impl Interpreter {
    /// The declaration and `BindingKind` of a lexical for-in/of head, or
    /// `None` when the head is not lexical (`var`, or an assignment pattern).
    /// A lexical head is exactly the case that needs a TDZ head environment
    /// and a fresh per-iteration environment.
    pub(super) fn for_of_head_lexical(
        left: &ForInOfLeft,
    ) -> Option<(&VariableDeclaration, BindingKind)> {
        let ForInOfLeft::Variable(decl) = left else {
            return None;
        };
        let kind = match decl.kind {
            VarKind::Var => return None,
            VarKind::Let => BindingKind::Let,
            VarKind::Const | VarKind::Using | VarKind::AwaitUsing => BindingKind::Const,
        };
        Some((decl, kind))
    }

    /// §14.7.5.12 ForIn/OfHeadEvaluation steps 1-2: the lexical head's names
    /// are in TDZ while the iterable expression is evaluated, in a temporary
    /// environment that no loop iteration ever uses.
    pub(super) fn for_of_head_tdz_env(left: &ForInOfLeft, env: &EnvRef) -> EnvRef {
        let Some((decl, binding_kind)) = Self::for_of_head_lexical(left) else {
            return env.clone();
        };
        let head_env = Environment::new(Some(env.clone()));
        let mut tdz_names = Vec::new();
        if let Some(d) = decl.declarations.first() {
            d.pattern.bound_names(&mut tdz_names);
        }
        for name in &tdz_names {
            head_env.borrow_mut().declare(name, binding_kind);
        }
        head_env
    }
}
