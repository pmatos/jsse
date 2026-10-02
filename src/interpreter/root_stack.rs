/// The temporary GC root stack: object ids that stay live across a native
/// call's safepoints until they are popped or the enclosing frame truncates.
///
/// The vocabulary is deliberately narrow — push, remove-by-id, truncate, and a
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

    /// Remove the most recently pushed entry equal to `id`, preserving the
    /// order of the rest. Returns whether an entry was found.
    #[inline]
    pub(super) fn remove_last(&mut self, id: u64) -> bool {
        match self.ids.iter().rposition(|&rid| rid == id) {
            Some(pos) => {
                self.ids.remove(pos);
                true
            }
            None => false,
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
