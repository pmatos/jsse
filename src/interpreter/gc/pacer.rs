use crate::interpreter::types::{
    GC_GROWTH_FACTOR, GC_INITIAL_THRESHOLD_BYTES, GC_MAJOR_MIN_THRESHOLD_BYTES,
    GC_NURSERY_THRESHOLD_BYTES, GC_OBJECT_OVERHEAD,
};

/// Allocation-pressure pacing for the garbage collector.
///
/// Owns the accounting that decides *when* to collect: how many objects and
/// bytes have been charged since the last collection, how many off-heap bytes
/// are currently live, and the adaptive byte budget that grows with the live
/// set. The mark/sweep mechanics stay in [`Interpreter::gc_safepoint`]; this is
/// purely the "should we collect now?" heuristic, exercised through the small
/// interface below rather than by poking counters on the interpreter.
pub(crate) struct GcPacer {
    /// Object allocations currently charged to the nursery.
    nursery_alloc_count: usize,
    /// Approximate bytes currently charged to the nursery.
    nursery_bytes: usize,
    /// Heap + off-heap allocation debt since the last major collection.
    major_bytes_since_gc: usize,
    /// Live off-heap bytes currently tracked (e.g. ArrayBuffer backing stores).
    external_bytes: usize,
    /// Adaptive major byte budget; exceeding it requests a full collection.
    major_threshold_bytes: usize,
    /// A minor collection has been requested and not yet performed.
    minor_requested: bool,
    /// A full collection has been requested and not yet performed.
    major_requested: bool,
    /// A dense remembered set made routine minor collection counterproductive.
    /// Run a major after the next nursery budget.
    minor_suppressed: bool,
    /// Debug stress mode: force a collection every `stress_period` safepoints
    /// that would otherwise not collect. Zero disables it.
    stress_period: u32,
    /// `major_requested | minor_requested | (stress_period != 0)`, kept in
    /// step by every writer so the VM's safepoint poll is one load and test.
    poll: bool,
    /// A step budget is installed, so every safepoint must tick it.
    budget_active: bool,
    /// Safepoints seen while stress mode is on. Every `stress_period`th one
    /// collects, alternating major (finds missing roots) and minor (finds
    /// missing write barriers).
    pub(crate) stress_count: u64,
    /// Every `begin_collection()` call, unconditionally — one per production
    /// `Interpreter::gc_safepoint()`, plus one per direct `begin_collection()`
    /// call from this module's own unit tests below. Unlike `stress_count`
    /// (which `begin_collection` skips incrementing whenever a major/minor
    /// collection is already pending), this never resets and is never
    /// skipped, so tests can assert exact deltas across calls. Test-only.
    #[cfg(test)]
    safepoint_calls: u64,
}

/// Environment variable that enables the GC stress mode: a collection is
/// forced at every Nth safepoint. Unset, `0` or unparsable leaves it off.
pub(crate) const GC_STRESS_ENV: &str = "JSSE_GC_STRESS";

/// In-crate unit tests never read the variable, so their exact-result pacer
/// assertions hold under `JSSE_GC_STRESS`; they opt in with `set_stress_period`.
fn stress_period_from_env() -> u32 {
    if cfg!(test) {
        return 0;
    }
    std::env::var(GC_STRESS_ENV)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CollectionKind {
    Minor,
    Major,
}

impl GcPacer {
    pub(crate) fn new() -> Self {
        let stress_period = stress_period_from_env();
        GcPacer {
            nursery_alloc_count: 0,
            nursery_bytes: 0,
            major_bytes_since_gc: 0,
            external_bytes: 0,
            major_threshold_bytes: GC_INITIAL_THRESHOLD_BYTES,
            minor_requested: false,
            major_requested: false,
            minor_suppressed: false,
            stress_period,
            poll: stress_period != 0,
            stress_count: 0,
            budget_active: false,
            #[cfg(test)]
            safepoint_calls: 0,
        }
    }

    #[inline(always)]
    fn sync_poll(&mut self) {
        self.poll = self.wanted_poll();
    }

    fn wanted_poll(&self) -> bool {
        self.major_requested | self.minor_requested | (self.stress_period != 0) | self.budget_active
    }

    pub(crate) fn set_budget_active(&mut self, active: bool) {
        self.budget_active = active;
        self.sync_poll();
    }

    #[inline(always)]
    fn mark_major(&mut self) {
        self.major_requested = true;
        self.poll = true;
    }

    #[inline(always)]
    fn mark_minor(&mut self) {
        self.minor_requested = true;
        self.poll = true;
    }

    pub(crate) fn set_stress_period(&mut self, period: u32) {
        self.stress_period = period;
        self.stress_count = 0;
        self.sync_poll();
    }

    /// Charge one object allocation. Reused logical slots still hold a full
    /// live payload and therefore carry full major-collection pressure.
    pub(crate) fn charge_object(&mut self, _reused: bool) {
        self.nursery_alloc_count += 1;
        self.nursery_bytes += GC_OBJECT_OVERHEAD;
        self.major_bytes_since_gc += GC_OBJECT_OVERHEAD;
        if self.nursery_bytes >= GC_NURSERY_THRESHOLD_BYTES {
            if self.minor_suppressed {
                self.mark_major();
            } else {
                self.mark_minor();
            }
        }
        if self.major_bytes_since_gc >= self.major_threshold_bytes {
            self.mark_major();
        }
    }

    /// Charge newly tracked off-heap bytes against the budget.
    pub(crate) fn track_external(&mut self, bytes: usize) {
        self.external_bytes += bytes;
        self.major_bytes_since_gc += bytes;
        if self.major_bytes_since_gc >= self.major_threshold_bytes {
            self.mark_major();
        }
    }

    /// Release previously tracked off-heap bytes (saturating at zero).
    pub(crate) fn release_external(&mut self, bytes: usize) {
        self.external_bytes = self.external_bytes.saturating_sub(bytes);
    }

    /// Force a collection at the next safepoint.
    pub(crate) fn request(&mut self) {
        self.mark_major();
    }

    #[cfg(test)]
    pub(crate) fn request_minor(&mut self) {
        self.mark_minor();
    }

    /// Whether a safepoint has anything to do: a pending request, or stress
    /// mode, whose counter must tick at every safepoint. Under `cfg(test)`
    /// it is always true so `safepoint_calls` still counts every poll.
    #[inline(always)]
    pub(crate) fn safepoint_pending(&self) -> bool {
        debug_assert_eq!(self.poll, self.wanted_poll());
        cfg!(test) || self.poll
    }

    /// Consume the highest-priority pending request at a safepoint.
    pub(crate) fn begin_collection(&mut self) -> Option<CollectionKind> {
        #[cfg(test)]
        {
            self.safepoint_calls += 1;
        }
        if self.major_requested {
            self.major_requested = false;
            self.minor_requested = false;
            self.sync_poll();
            Some(CollectionKind::Major)
        } else if self.minor_requested {
            self.minor_requested = false;
            self.sync_poll();
            Some(CollectionKind::Minor)
        } else if self.stress_period != 0 {
            self.stress_count += 1;
            let period = u64::from(self.stress_period);
            if !self.stress_count.is_multiple_of(period) {
                return None;
            }
            Some(if (self.stress_count / period) % 2 == 1 {
                CollectionKind::Major
            } else {
                CollectionKind::Minor
            })
        } else {
            None
        }
    }

    /// Reset the nursery cycle after a minor collection. High survival does not
    /// switch to major pacing: a minor costs about the same per nursery
    /// whatever the live-set size, while forcing a major every nursery traced
    /// the whole growing heap per 8,192 allocations (#908). Majors stay on the
    /// live-set-proportional byte budget.
    pub(crate) fn end_minor_collection(&mut self) {
        self.nursery_alloc_count = 0;
        self.nursery_bytes = 0;
    }

    /// Pause minor collection after an unproductive scavenge. Nursery objects
    /// remain allocated until the next budget requests a major collection.
    pub(crate) fn suppress_minor_temporarily(&mut self) {
        self.minor_requested = false;
        self.sync_poll();
        self.minor_suppressed = true;
        self.nursery_alloc_count = 0;
        self.nursery_bytes = 0;
    }

    /// Re-pace after a full collection: grow the next byte budget from the
    /// surviving live set and reset both allocation cycles.
    pub(crate) fn end_major_collection(&mut self, live_object_count: usize) {
        let live_bytes = live_object_count * GC_OBJECT_OVERHEAD + self.external_bytes;
        let growth_debt = live_bytes.saturating_mul(GC_GROWTH_FACTOR.saturating_sub(1));
        self.major_threshold_bytes = std::cmp::max(GC_MAJOR_MIN_THRESHOLD_BYTES, growth_debt);
        self.major_bytes_since_gc = 0;
        self.nursery_alloc_count = 0;
        self.nursery_bytes = 0;
        self.minor_suppressed = false;
    }

    #[cfg(test)]
    pub(crate) fn is_requested(&self) -> bool {
        self.minor_requested || self.major_requested
    }

    #[cfg(test)]
    pub(crate) fn safepoint_calls(&self) -> u64 {
        self.safepoint_calls
    }

    #[cfg(test)]
    pub(crate) fn alloc_count(&self) -> usize {
        self.nursery_alloc_count
    }

    #[cfg(test)]
    pub(crate) fn bytes_since_gc(&self) -> usize {
        self.major_bytes_since_gc
    }

    #[cfg(test)]
    pub(crate) fn external_bytes(&self) -> usize {
        self.external_bytes
    }

    #[cfg(test)]
    pub(crate) fn threshold_bytes(&self) -> usize {
        self.major_threshold_bytes
    }
}

#[cfg(test)]
#[path = "pacer_tests.rs"]
mod tests;
