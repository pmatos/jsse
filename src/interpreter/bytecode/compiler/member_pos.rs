//! Error positions of member ops for `--node` (#1419): where V8 points a
//! `Cannot read properties of undefined` TypeError. Recorded in
//! `Chunk::member_pos` only once the prelude installed the stack hooks, so a
//! default chunk keeps the same bytes and an empty table.

use super::Compiler;
use crate::ast::PropSiteId;
use crate::interpreter::builtins::node_host_stack::call_positions_enabled;
use crate::interpreter::bytecode::op::Op;

impl Compiler {
    /// `emit(op)` for a member op, remembering the site's position.
    pub(super) fn emit_member(&mut self, op: Op, site: PropSiteId) {
        if call_positions_enabled() {
            let at = u32::try_from(self.code.len()).unwrap_or(u32::MAX);
            while self.member_pos.last().is_some_and(|entry| entry.0 >= at) {
                self.member_pos.pop();
            }
            if site.1 != 0 {
                self.member_pos.push((at, site.1));
            }
        }
        self.emit(op);
    }
}
