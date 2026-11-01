//! Statement positions in `--node` stacks (issue #1548). V8 reports the last
//! position it recorded for a frame parked where no expression has one of its
//! own (the `await` of a disposer, of an iterator's `return()`): a declaration
//! leaves the start of its initializer, a `break` its keyword.

use crate::interpreter::Interpreter;

impl Interpreter {
    /// Records `at` (0: none) as the position the next implicit `await` reports.
    #[inline]
    pub(crate) fn node_note_statement_pos(&mut self, at: u32) {
        if at != 0 {
            self.node_set_call_pos(at);
        }
    }
}
