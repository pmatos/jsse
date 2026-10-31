//! TypeScript `conformance` runner, `es5`, `es7`, `es2016` to `es2025` and `esnext` (#1278):
//! `scripts/run-ts-conformance-es-oracle.py` cuts the 226 suites (1356 tests) into four chunks and runs each through
//! `scripts/run-ts-compiler-oracle.py` (pointed at the conformance runner) against
//! `ts-suite/conformance-es-known-failures.txt`. The case below checks, with Node alone, that the chunks partition the
//! group exactly when TS_SRC_DIR points at a built TypeScript 6.0.3 checkout; the whole group is
//! `uv run python scripts/run-ts-conformance-es-oracle.py`.

use std::path::Path;
use std::process::Command;

#[test]
fn conformance_es_chunks_partition_the_group_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to list the conformance suites"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-conformance-es-oracle.py"))
        .arg("--check-partition")
        .output()
        .expect("run python3");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("partition OK"), "{text}");
}
