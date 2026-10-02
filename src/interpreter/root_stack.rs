/// The temporary GC root stack: object ids that stay live across a native
/// call's safepoints until they are released by id or the enclosing frame
/// truncates.
///
/// The vocabulary is deliberately narrow — push, pop-expected, truncate, and a
/// read-only slice for the collector. There is no `retain`, `clear`, `extend`,
/// `swap_remove` or index mutation, so every production mutation goes through
/// [`Interpreter::gc_root_id`]/[`Interpreter::gc_unroot_id`] or the frame
/// helpers, and a stack-discipline check has one place to hook.
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
            "temp root {id} released out of LIFO order (top of {:?})",
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
        self.ids.truncate(depth);
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
