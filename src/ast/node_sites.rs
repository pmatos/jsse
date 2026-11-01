//! Source positions the `--node` host reads off the tree (issue #1548): V8
//! reports the last position it recorded for a frame parked where no expression
//! has one of its own, such as a generator at a `yield*` or a function that
//! disposes resources.

use super::*;

/// Whether a `yield` delegates, and the offset plus one of its keyword (0 when
/// unknown).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct YieldKind {
    pub delegate: bool,
    pub at: u32,
}

impl YieldKind {
    pub(crate) const PLAIN: Self = Self {
        delegate: false,
        at: 0,
    };
}

impl VariableDeclarator {
    /// A declarator the parser did not read from source: no initializer position.
    pub(crate) fn new(pattern: Pattern, init: Option<Expression>) -> Self {
        Self {
            pattern,
            init,
            init_at: 0,
        }
    }
}
