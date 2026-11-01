//! Minor-collection profile of the execution counters (`gc_minor_*` rows of
//! the `PERF` report): where each minor collection spends its time, and the
//! sizes that drive it. Added for #1490, where a registry probe per environment
//! was 85% of a 4 MiB nursery's collection.

use std::fmt::Write as _;
use std::time::Instant;

/// The phases of `gc_collect_minor`; each name is the index of its slot in
/// `GcMinorProfile::phase_nanos`.
pub(crate) const ROOTS: usize = 0;
pub(crate) const REGISTRY: usize = 1;
pub(crate) const REMEMBERED: usize = 2;
pub(crate) const TRACE: usize = 3;
pub(crate) const SWEEP: usize = 4;
pub(crate) const SETTLE: usize = 5;
const PHASES: [&str; 6] = [
    "roots",
    "registry",
    "remembered",
    "trace",
    "sweep",
    "settle",
];

/// Per-collection sizes summed over every minor collection; each name is the
/// index of its slot in `GcMinorProfile::sizes`.
pub(crate) const REGISTRY_TOTAL: usize = 0;
pub(crate) const REMEMBERED_TOTAL: usize = 1;
pub(crate) const NURSERY_TOTAL: usize = 2;
pub(crate) const SURVIVORS_TOTAL: usize = 3;
pub(crate) const SCANNED_ENVS_TOTAL: usize = 4;
const SIZES: [&str; 5] = [
    "registry_total",
    "remembered_total",
    "nursery_total",
    "survivors_total",
    "scanned_envs_total",
];

#[derive(Default)]
pub(crate) struct GcMinorProfile {
    pub(crate) phase_nanos: [u128; PHASES.len()],
    pub(crate) sizes: [u64; SIZES.len()],
}

impl GcMinorProfile {
    /// Charge the time since `since`, less what the registry probe took since
    /// `registry_before`, to the roots phase.
    pub(crate) fn lap_roots(&mut self, since: Instant, registry_before: u128) -> Instant {
        let registry = self.phase_nanos[REGISTRY] - registry_before;
        self.phase_nanos[ROOTS] += since.elapsed().as_nanos().saturating_sub(registry);
        Instant::now()
    }

    /// Charge the time since `since` to `phase`; returns the next lap's start.
    pub(crate) fn lap(&mut self, phase: usize, since: Instant) -> Instant {
        self.phase_nanos[phase] += since.elapsed().as_nanos();
        Instant::now()
    }

    pub(crate) fn report(&self, out: &mut String) {
        for (name, nanos) in PHASES.iter().zip(&self.phase_nanos) {
            let _ = writeln!(out, "PERF\tgc_minor_{name}_us\t{}", nanos / 1_000);
        }
        for (name, total) in SIZES.iter().zip(&self.sizes) {
            let _ = writeln!(out, "PERF\tgc_minor_{name}\t{total}");
        }
    }
}
