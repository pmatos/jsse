//! The stream corpus digest (scripts/stream-fixtures/corpus.digest) must not
//! depend on how fast the build under test is (#1516). The nightly coverage
//! workflow runs `corpus_survives_gc_stress` (tests/node_stream.rs) on an
//! llvm-cov instrumented binary, where the "operators" scenario alone takes
//! more than 5 s under JSSE_GC_STRESS=7. A per-scenario wall-clock budget of 3 s
//! then fired, added a `TIMEOUT operators` line and changed the digest.

use std::path::Path;

fn corpus_run() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/stream-fixtures/corpus-run.js");
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn scenario_budget_is_a_hang_guard_not_a_performance_bound() {
    let source = corpus_run();
    let budget: u64 = source
        .lines()
        .find_map(|l| l.trim().strip_prefix("var BUDGET_MS = "))
        .and_then(|rest| rest.trim_end_matches(';').parse().ok())
        .expect("corpus-run.js declares `var BUDGET_MS = <ms>;`");
    assert!(
        budget >= 30_000,
        "BUDGET_MS = {budget} ms is short enough to fire under GC stress with coverage"
    );
    assert!(
        source.contains("}, BUDGET_MS);"),
        "the scenario timer must use BUDGET_MS"
    );
}
