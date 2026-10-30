//! TypeScript `conformance` runner, directory `types` (#1274): `scripts/run-ts-conformance-types-oracle.py` runs
//! tests/cases/conformance/types through the compiler oracle's machinery on Node and `jsse --node` against
//! `ts-suite/conformance-types-known-failures.txt`. The case below checks, with Node alone, that the slice lists the
//! 857 suites (5142 tests) the `types` bucket of ts-suite/oracle.json pins, spread over the thirty subdirectories,
//! when TS_SRC_DIR points at a built TypeScript 6.0.3 checkout; the whole directory is
//! `uv run python scripts/run-ts-conformance-types-oracle.py`.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

#[test]
fn conformance_types_slice_lists_the_pinned_suites_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to list the conformance suites"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-conformance-types-oracle.py"))
        .args(["--list", "--no-jsse", "types"])
        .output()
        .expect("run python3");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut lines = text.lines();
    let head = lines.next().unwrap_or_default();
    assert!(
        head.contains(": 857 of ") && head.contains("(5142 tests)"),
        "{head}"
    );
    let prefix = "tests/cases/conformance/types/";
    let dirs: BTreeSet<&str> = lines
        .filter_map(|l| l.split(prefix).nth(1))
        .filter_map(|rest| rest.split('/').next())
        .collect();
    assert_eq!(dirs.len(), 30, "{dirs:?}");
}
