//! Minor-collection profile of the execution counters (`gc_minor_*` rows of
//! the `PERF` report): where each minor collection spends its time, and the
//! sizes that drive it. Added for #1490, where a registry probe per environment
//! was 85% of a 4 MiB nursery's collection.

use std::fmt::Write as _;

/// The phases of `gc_collect_minor`, in the order of `GcMinorProfile::phase_nanos`.
pub(crate) const PHASES: [&str; 6] = [
    "roots",
    "registry",
    "remembered",
    "trace",
    "sweep",
    "settle",
];

/// Per-collection sizes summed over every minor collection, in the order of
/// `GcMinorProfile::sizes`.
pub(crate) const SIZES: [&str; 5] = [
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
    pub(crate) fn report(&self, out: &mut String) {
        for (name, nanos) in PHASES.iter().zip(&self.phase_nanos) {
            let _ = writeln!(out, "PERF\tgc_minor_{name}_us\t{}", nanos / 1_000);
        }
        for (name, total) in SIZES.iter().zip(&self.sizes) {
            let _ = writeln!(out, "PERF\tgc_minor_{name}\t{total}");
        }
    }
}
