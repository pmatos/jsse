/// A GC root stack: object ids that stay live until they are released by id
/// or the enclosing frame truncates. Backs both `Interpreter::gc_temp_roots`
/// (object ids that stay live across a native call's safepoints) and
/// `Interpreter::gc_bytecode_roots` (the bytecode VM's operand-stack roots).
///
/// The vocabulary is deliberately narrow — push, pop-expected, truncate, and a
/// read-only slice for the collector. There is no `retain`, `clear`, `extend`,
/// `swap_remove` or index mutation, so every production mutation goes through
/// [`Interpreter::gc_root_id`]/[`Interpreter::gc_unroot_id`] or the frame
/// helpers for `gc_temp_roots`, and through `bytecode::vm`'s own push/pop
/// helpers for `gc_bytecode_roots` — each stack has one place to hook a
/// stack-discipline check.
#[derive(Debug, Default)]
pub(crate) struct RootStack {
    ids: Vec<u64>,
}

impl RootStack {
    #[inline]
    pub(super) fn push(&mut self, id: u64) {
        self.ids.push(id);
    }

    /// Pop `id`, which must be the top entry: the stack is strictly LIFO, so a
    /// root is always released in reverse order of its push.
    ///
    /// A mismatch is a bug in the caller's rooting discipline. Debug and
    /// checked builds assert on it; release builds fall back to removing the
    /// most recent matching entry, which can leak a root but never drops one a
    /// still-running caller depends on.
    #[inline]
    pub(super) fn pop_expected(&mut self, id: u64) {
        if self.ids.last() == Some(&id) {
            self.ids.pop();
            return;
        }
        debug_assert!(
            false,
            "root {id} released out of LIFO order (top of {:?})",
            self.ids.last()
        );
        if let Some(pos) = self.ids.iter().rposition(|&rid| rid == id) {
            self.ids.remove(pos);
        }
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.ids.len()
    }

    #[inline]
    pub(super) fn truncate(&mut self, depth: usize) {
        debug_assert!(
            depth <= self.ids.len(),
            "root frame {depth} outlived its roots (stack is {} deep)",
            self.ids.len()
        );
        self.ids.truncate(depth);
    }

    /// Debug-assert the stack is exactly `depth` deep. Placed at boundaries
    /// where every root pushed by the code in between must already have been
    /// released.
    #[inline(always)]
    pub(super) fn assert_depth(&self, depth: usize, boundary: &str) {
        debug_assert_eq!(
            self.ids.len(),
            depth,
            "root stack unbalanced after {boundary}"
        );
    }

    #[inline]
    pub(super) fn as_slice(&self) -> &[u64] {
        &self.ids
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, id: &u64) -> bool {
        self.ids.contains(id)
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}
