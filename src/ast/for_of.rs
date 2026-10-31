//! Queries on a for-of head.

use super::*;

impl ForOfStatement {
    /// See [`ForStatement::may_capture_iteration_scope`].
    pub(crate) fn may_capture_iteration_scope(&self) -> bool {
        self.scope_capture
            .get_or_compute(|| for_in_of_may_capture(&self.left, &self.body))
    }

    /// Whether this loop can suspend the enclosing async function/generator at its head,
    /// either through the async iteration protocol (`is_await`) or through an `await using`
    /// ForDeclaration's per-iteration DisposeResources `Await`.
    pub(crate) fn awaits_at_head(&self) -> bool {
        self.is_await || self.disposes_at_head()
    }

    /// Whether the ForDeclaration is `await using`, so each iteration's
    /// DisposeResources may `Await`.
    pub(crate) fn disposes_at_head(&self) -> bool {
        matches!(
            &self.left,
            ForInOfLeft::Variable(decl) if decl.kind == VarKind::AwaitUsing
        )
    }
}
