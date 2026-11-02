use super::*;

/// How many nursery objects ahead of the sweep cursor the allocation of an
/// object is prefetched: far enough to cover a main-memory miss at the
/// sweep's per-object cost, near enough to stay cached until used.
const PREFETCH_OBJECT_AHEAD: usize = 12;

/// How many ahead the heap blocks an object owns are prefetched. Reading the
/// object to find them needs its allocation to have arrived already.
const PREFETCH_OWNED_AHEAD: usize = 4;

/// Live objects from which the sweep prefetches. Below it the blocks of a
/// nursery are recycled from a small address range and stay in cache between
/// collections, so the lookahead only costs instructions; a heap of this many
/// objects spans tens of megabytes, where a nursery's blocks are cold.
const PREFETCH_MIN_LIVE_OBJECTS: usize = 1 << 16;

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
        let prefetching =
            cfg!(target_arch = "x86_64") && self.objects.live_count() >= PREFETCH_MIN_LIVE_OBJECTS;
        for (index, &id) in nursery.iter().enumerate() {
            if prefetching {
                if let Some(&ahead) = nursery.get(index + PREFETCH_OBJECT_AHEAD) {
                    self.objects.prefetch_object(ahead);
                }
                if let Some(&ahead) = nursery.get(index + PREFETCH_OWNED_AHEAD) {
                    self.objects.prefetch_owned(ahead);
                }
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
