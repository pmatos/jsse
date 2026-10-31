//! The jsse pass list of TypeScript's own test suite (#1296): `scripts/run-ts-pass-list.py` lists the test
//! universe of a built TypeScript 6.0.3 checkout with Node (checked against `ts-suite/oracle.json`), runs the
//! selected tests on `jsse --node` and diffs them against `ts-suite/jsse-pass-1296.json`. The cases below run the
//! `transpile` runner (37 tests) and the universe check when TS_SRC_DIR points at such a checkout; the whole suite
//! is `uv run python scripts/run-ts-pass-list.py`.

use std::path::Path;
use std::process::Command;

fn gate(extra: &[&str]) -> (bool, String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-pass-list.py"))
        .args([
            "--require",
            "--jobs",
            "2",
            "--baseline-ref",
            "HEAD",
            "--jsse",
        ])
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
fn the_dry_run_universe_matches_the_pinned_oracle() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to check the pass list universe"
        );
        return;
    }
    let (ok, text) = gate(&["--check-universe"]);
    assert!(ok, "{text}");
    assert!(text.contains("pass list universe: PASS"), "{text}");
}

#[test]
fn transpile_tests_have_no_regression_against_the_pass_list() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to run the pass list gate"
        );
        return;
    }
    let (ok, text) = gate(&["transpile"]);
    assert!(ok, "{text}");
    assert!(text.contains("pass list: PASS"), "{text}");
    assert!(text.contains("\"regression\": 0"), "{text}");
}
