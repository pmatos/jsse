//! TypeScript's `compiler` runner (#1268) run on `jsse --node` and Node, compared per test, per error
//! message and per local baseline file by `scripts/run-ts-compiler-oracle.py`. The case below runs the first
//! two suites of tests/cases/compiler plus the corrupted-reference control when TS_SRC_DIR points at a built
//! TypeScript 6.0.3 checkout; the first 200 suites are
//! `uv run python scripts/run-ts-compiler-oracle.py first200`.

use std::path::Path;
use std::process::Command;

#[test]
fn compiler_tests_match_node_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to run the compiler oracle"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-compiler-oracle.py"))
        .args(["--require", "--jobs", "2", "--batch-size", "2", "--jsse"])
        .arg(env!("CARGO_BIN_EXE_jsse"))
        .arg("first:2")
        .output()
        .expect("run python3");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("compiler oracle: PASS"), "{text}");
}
