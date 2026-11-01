//! `for (... in ...)` and `for (... of ...)` statements; the shared lowering
//! is `compile_for_in_of`.

use super::{CompileError, Compiler};

impl Compiler {
    pub(super) fn compile_for_in(
        &mut self,
        for_in: &crate::ast::ForInStatement,
        labels: Vec<String>,
    ) -> Result<(), CompileError> {
        self.compile_for_in_of(
            &for_in.left,
            &for_in.right,
            &for_in.body,
            false,
            for_in.may_capture_iteration_scope(),
            (0, 0),
            labels,
        )
    }

    pub(super) fn compile_for_of(
        &mut self,
        for_of: &crate::ast::ForOfStatement,
        labels: Vec<String>,
    ) -> Result<(), CompileError> {
        if for_of.is_await {
            return Err(CompileError::Unsupported("for await"));
        }
        self.compile_for_in_of(
            &for_of.left,
            &for_of.right,
            &for_of.body,
            true,
            for_of.may_capture_iteration_scope(),
            (for_of.at, for_of.right.tail_pos(for_of.right_at)),
            labels,
        )
    }
}
