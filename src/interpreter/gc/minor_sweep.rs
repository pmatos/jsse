use super::*;

/// How many nursery objects ahead of the sweep cursor the allocation of an
/// object is prefetched: far enough to cover a main-memory miss at the
/// sweep's per-object cost, near enough to stay cached until used.
const PREFETCH_OBJECT_AHEAD: usize = 12;

/// How many ahead the heap blocks an object owns are prefetched. Reading the
/// object to find them needs its allocation to have arrived already.
const PREFETCH_OWNED_AHEAD: usize = 4;

impl Interpreter {
    /// A minor collection's sweep: frees the nursery objects the trace left
    /// unmarked, promotes survivors that reached `GC_PROMOTION_AGE`, and
    /// returns the ids that stay in the nursery.
    ///
    /// Dropping a dead object (its property storage, element vector, closure
    /// and so on) is what the time goes to, per object and independent of the
    /// heap (#1490), and it starts with cache misses because only survivors
    /// were visited by the trace. Hence the prefetch of upcoming objects and
    /// of what they own.
    pub(super) fn sweep_nursery(&mut self, nursery: &[u64], young: &YoungIds) -> Vec<u64> {
        let mut survivors = Vec::with_capacity(nursery.len());
        for (index, &id) in nursery.iter().enumerate() {
            if let Some(&ahead) = nursery.get(index + PREFETCH_OBJECT_AHEAD) {
                self.objects.prefetch_object(ahead);
            }
            if let Some(&ahead) = nursery.get(index + PREFETCH_OWNED_AHEAD) {
                self.objects.prefetch_owned(ahead);
            }
            let Some(handle) = self.objects.get_cell(id) else {
                continue;
            };
            if !handle.is_young_marked() {
                self.free_gc_object(id);
            } else if handle.age_survivor(GC_PROMOTION_AGE) {
                handle.promote();
                if young.has_young_edge(id) || Self::has_untraceable_young_edges(handle) {
                    handle.remember_if_old();
                }
            } else {
                handle.clear_young_mark();
                survivors.push(id);
            }
        }
        survivors
    }
}
