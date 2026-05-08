//! test262 random-sample oracle for cargo-mutants. Gated on the
//! `JSSE_MUTANTS_ORACLE` env var so a normal `cargo test` stays fast.
//!
//! Each invocation samples a fresh random subset of test262 (no `--seed`),
//! so mutation verdicts are intentionally non-deterministic across mutants
//! and shards. Over many runs this exercises a broader cross-section of
//! the suite than a fixed smoke set would.

use std::process::Command;

#[test]
fn test262_smoke_oracle() {
    if std::env::var_os("JSSE_MUTANTS_ORACLE").is_none() {
        return;
    }

    let jsse = env!("CARGO_BIN_EXE_jsse");
    let workspace = env!("CARGO_MANIFEST_DIR");
    let runner = format!("{workspace}/scripts/run-test262.py");
    let test262 = format!("{workspace}/test262");

    let status = Command::new("uv")
        .args([
            "run",
            "python",
            &runner,
            "--fail-on-failures",
            "--jsse",
            jsse,
            "--test262",
            &test262,
            "--sample",
            "0.005",
            "-j",
            "8",
        ])
        .current_dir(workspace)
        .status()
        .expect("failed to invoke run-test262.py via uv");
    assert!(status.success(), "test262 random sample failed");
}
