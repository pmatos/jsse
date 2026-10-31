//! TypeScript `conformance` runner, directories classes, decorators, esDecorators and override (#1276):
//! `scripts/run-ts-conformance-oracle.py` runs them through the compiler oracle's machinery against
//! `ts-suite/conformance-1276-known-failures.txt`. The case below checks, with Node alone, that the four directories
//! partition the `issue1276` row exactly when TS_SRC_DIR points at a built TypeScript 6.0.3 checkout; the whole group
//! is `uv run python scripts/run-ts-conformance-oracle.py`.

use std::path::Path;
use std::process::Command;

#[test]
fn conformance_directories_partition_the_group_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to list the conformance suites"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-conformance-oracle.py"))
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
