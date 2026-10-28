//! TypeScript's tsserver unit tests (unittests/tsserver, #1266 and #1267) run
//! on `jsse --node` and Node, compared per test title by
//! `scripts/run-ts-unittests-tsserver-oracle.py`. The cases below drive the
//! oracle on small groups of each half when TS_SRC_DIR points at a built TypeScript
//! 6.0.3 checkout; the files a-m (421 tests, 15 groups) are
//! `uv run python scripts/run-ts-unittests-tsserver-oracle.py` and the files n-z (819 tests,
//! 12 groups) `... --half n-z`.

use std::path::Path;
use std::process::Command;

fn run_oracle(groups: &[&str]) {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to run the unit-test oracle"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-unittests-tsserver-oracle.py"))
        .args([
            "--require",
            "--jobs",
            "2",
            "--jsse",
            env!("CARGO_BIN_EXE_jsse"),
        ])
        .args(groups)
        .output()
        .expect("run python3");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("unit-test oracle: PASS"), "{text}");
}

#[test]
fn tsserver_unit_tests_match_node_when_a_typescript_checkout_is_given() {
    run_oracle(&[
        "tsserver-cachingFileSystemInformation",
        "tsserver-exportMapCache-extends",
        "tsserver-documentRegistry-duplicate-dynamic",
    ]);
}

#[test]
fn tsserver_unit_tests_n_to_z_match_node_when_a_typescript_checkout_is_given() {
    run_oracle(&[
        "tsserver-refactors-reload-rename",
        "tsserver-resolutionCache-session",
    ]);
}
