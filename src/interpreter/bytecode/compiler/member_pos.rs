//! Error positions of member ops for `--node` (#1419): where V8 points a
//! `Cannot read properties of undefined` TypeError. Recorded in
//! `Chunk::member_pos` only once the prelude installed the stack hooks, so a
//! default chunk keeps the same bytes and an empty table.

use super::{CompileError, Compiler};
use crate::ast::PropSiteId;
use crate::interpreter::builtins::node_host_iter_operand::{operand_call_pos, unparenthesized};
use crate::interpreter::builtins::node_host_stack::call_positions_enabled;
use crate::interpreter::bytecode::op::Op;

impl Compiler {
    /// Remembers `init`'s text for the object pattern read that starts at the
    /// current offset (`--node` only, see `Chunk::pattern_src`).
    pub(super) fn note_pattern_source(&mut self, init: &crate::ast::Expression) {
        if call_positions_enabled() {
            let at = u32::try_from(self.code.len()).unwrap_or(u32::MAX);
            let text = crate::interpreter::builtins::node_host_expr_text::expr_text(init);
            self.pattern_src.push((at, text.into()));
        }
    }

    /// Remembers the text of a call spread's operand for the spread op that
    /// is emitted next.
    pub(super) fn note_spread_source(&mut self, operand: &crate::ast::Expression) {
        if call_positions_enabled() {
            let at = u32::try_from(self.code.len()).unwrap_or(u32::MAX);
            let text = crate::interpreter::builtins::node_host_expr_text::expr_text(operand);
            self.pattern_src.push((at, text.into()));
        }
    }

    /// `note_spread_source` for a spread among several call arguments, which
    /// V8 words only when it is the one spread and the last argument.
    pub(super) fn note_call_spread_source(
        &mut self,
        args: &[crate::ast::Expression],
        operand: &crate::ast::Expression,
    ) {
        if crate::interpreter::builtins::node_host_spread_errors::is_lone_last_spread(args, operand)
        {
            self.note_spread_source(operand);
        }
    }

    /// Remembers the message for a for-of operand that is not iterable, and
    /// the position of the operand when it is a call (#1557). A tagged
    /// template or array assignment operand stays on the tree-walker, which
    /// words its error.
    pub(super) fn note_iterable_source(
        &mut self,
        right: &crate::ast::Expression,
        is_of: bool,
    ) -> Result<(), CompileError> {
        use crate::ast::{AssignOp, Expression};
        if !is_of || !call_positions_enabled() {
            return Ok(());
        }
        if matches!(right, Expression::TaggedTemplate(..))
            || matches!(unparenthesized(right), Expression::Assign(AssignOp::Assign, t, ..)
                if matches!(**t, Expression::Array(..)))
        {
            return Err(CompileError::Unsupported("for-of over a tagged template"));
        }
        self.iter_calls.extend(operand_call_pos(right));
        if let Some(text) = crate::interpreter::Interpreter::iterable_message(right) {
            let at = u32::try_from(self.code.len()).unwrap_or(u32::MAX);
            self.pattern_src.push((at, text.into()));
        }
        Ok(())
    }

    /// `emit(op)` for a member op, remembering the site's position.
    pub(super) fn emit_member(&mut self, op: Op, site: PropSiteId) {
        self.emit_member_at(op, site.1);
    }

    /// The load of `name` read at `at` (offset plus one, 0: unknown), which a
    /// missing or uninitialised binding reports there (`place_ident_error`).
    pub(super) fn compile_ident_load(&mut self, name: &str, at: u32) -> Result<(), CompileError> {
        if at != 0 && call_positions_enabled() {
            let pc = u32::try_from(self.code.len()).unwrap_or(u32::MAX);
            while self.member_pos.last().is_some_and(|entry| entry.0 >= pc) {
                self.member_pos.pop();
            }
            self.member_pos.push((pc, at));
        }
        self.compile_load_name(name)
    }

    /// `emit(op)` for a member op that reports `pos` (0: none).
    pub(super) fn emit_member_at(&mut self, op: Op, pos: u32) {
        if call_positions_enabled() {
            let at = u32::try_from(self.code.len()).unwrap_or(u32::MAX);
            while self.member_pos.last().is_some_and(|entry| entry.0 >= at) {
                self.member_pos.pop();
            }
            if pos != 0 {
                self.member_pos.push((at, pos));
            }
        }
        self.emit(op);
    }
}
