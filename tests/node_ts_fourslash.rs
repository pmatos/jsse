//! TypeScript's `fourslash` runner (#1288) run on `jsse --node` and Node, compared per test, per error
//! message and per local baseline file by `scripts/run-ts-fourslash-oracle.py`. The case below runs the first
//! few tests of tests/cases/fourslash plus the corrupted-input control, and the first server tests, when
//! TS_SRC_DIR points at a built TypeScript 6.0.3 checkout; the first 200 tests are
//! `uv run python scripts/run-ts-fourslash-oracle.py first200`.

use std::path::Path;
use std::process::Command;

fn oracle(extra: &[&str]) -> (bool, String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-fourslash-oracle.py"))
        .args(["--require", "--jobs", "2", "--batch-size", "3", "--jsse"])
        .arg(env!("CARGO_BIN_EXE_jsse"))
        .args(extra)
        .output()
        .expect("run python3");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn fourslash_tests_match_node_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to run the fourslash oracle"
        );
        return;
    }
    let (ok, text) = oracle(&["first:3"]);
    assert!(ok, "{text}");
    assert!(text.contains("fourslash oracle: PASS"), "{text}");
    let (ok, text) = oracle(&["--runner", "fourslash-server", "--no-control", "first:2"]);
    assert!(ok, "{text}");
    assert!(text.contains("fourslash oracle: PASS"), "{text}");
}
