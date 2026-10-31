//! `hereby tests` (#1254) builds the TypeScript test runner bundle `built/local/run.js` on `jsse --node`, with
//! and without `--no-typecheck`. The build and the test listings run under
//! `scripts/run-hereby-tests-oracle.py`; the cases here run it when TS_SRC_DIR points at a TypeScript 6.0.3
//! checkout (npm ci done) and JSSE_HEREBY_TESTS_ORACLE is set (the type-checked build takes minutes).

use std::path::Path;
use std::process::{Command, Output};

fn oracle(args: &[&str]) -> Option<Output> {
    std::env::var_os("JSSE_HEREBY_TESTS_ORACLE")?;
    let ts = std::env::var_os("TS_SRC_DIR")?;
    if !Path::new(&ts).join("node_modules").is_dir() {
        return None;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    Some(
        Command::new("uv")
            .args(["run", "python", "scripts/run-hereby-tests-oracle.py"])
            .arg("--jsse")
            .arg(env!("CARGO_BIN_EXE_jsse"))
            .arg("--require")
            .args(args)
            .current_dir(root)
            .output()
            .expect("run the hereby tests oracle"),
    )
}

fn check(out: Output) {
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn the_runner_bundle_without_typecheck_matches_node_when_a_checkout_is_given() {
    if let Some(out) = oracle(&["--variants", "no-typecheck"]) {
        check(out);
    }
}

#[test]
fn the_type_checked_runner_build_matches_node_when_a_checkout_is_given() {
    if let Some(out) = oracle(&["--variants", "typecheck", "--runners", "transpile,project"]) {
        check(out);
    }
}

#[test]
fn the_oracle_skips_without_a_checkout() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("uv")
        .args(["run", "python", "scripts/run-hereby-tests-oracle.py"])
        .env_remove("TS_SRC_DIR")
        .current_dir(root)
        .output();
    let Ok(out) = out else { return };
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("SKIP"));
}
