//! The realm an async generator's steps run in.

use super::*;

impl Interpreter {
    /// Runs `f` with the realm of async generator `gen_id` current, restoring
    /// the caller's afterwards.
    pub(super) fn with_generator_realm<R>(
        &mut self,
        gen_id: u64,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let realm = self
            .get_object_cell(gen_id)
            .and_then(|cell| cell.borrow().generator_realm_id());
        self.with_realm(realm, f)
    }
}
