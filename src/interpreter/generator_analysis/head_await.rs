use super::pattern_needs_await_lowering;
use crate::ast::ForInOfLeft;

/// Like `for_in_of_head_contains_yield`, but `await`-only and shape-gated via
/// `pattern_needs_await_lowering` -- an unsupported shape (an object rest
/// beside a suspending sibling, issue #771; array patterns are supported
/// since issue #774) must not force the compiled state machine. The
/// `Pattern` (assignment-form) arm mirrors the `Variable` arm since issue
/// #788: before that fix this unconditionally returned `false`, so an
/// assignment-form head's only `await` (e.g. `for ([a = await x] of it)`)
/// was invisible to `contains_suspension`, leaving the whole statement
/// tree-walked through the blocking `await_value` fallback with no
/// suspend/resume awareness -- the same gap ADR-2026-09-30-2230 closed for a
/// `yield` in this position, left open for `await` pending issue #725/#788.
pub(super) fn for_in_of_variable_head_contains_await(left: &ForInOfLeft) -> bool {
    match left {
        ForInOfLeft::Variable(decl) => decl
            .declarations
            .iter()
            .any(|d| pattern_needs_await_lowering(&d.pattern)),
        ForInOfLeft::Pattern(pattern) => pattern_needs_await_lowering(pattern),
        ForInOfLeft::Expression(_) => false,
    }
}
