//! The small self-compile guard (#1249): `scripts/run-tsc-selfcompile-oracle.py --smoke` compiles a
//! generated 60-module program with tsc 6.0.3 on Node and on `jsse --node` and requires the emitted
//! JS, declarations and maps to be byte-identical (and within the budget of
//! scripts/tsc-fixtures/selfcompile-budget.json, if one is set). The full cases (the compiler
//! project and all of src/, minutes on jsse) are the same script without `--smoke` and need
//! TS_SRC_DIR. Like tests/node_tsc.rs this guard needs TSC_DIR (an installed typescript 6.0.3) and
//! passes with a notice without it. Run it with `--profile release-nolto`.
#![cfg(unix)]

use std::path::Path;
use std::process::Command;

#[test]
fn generated_large_program_matches_node() {
    if std::env::var_os("TSC_DIR").is_none() {
        eprintln!(
            "SKIP: set TSC_DIR to an installed typescript 6.0.3 package to run the self-compile guard"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-tsc-selfcompile-oracle.py"))
        .args(["--smoke", "--ci"])
        .env("JSSE", env!("CARGO_BIN_EXE_jsse"))
        .output()
        .expect("run python3");
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains("SKIP:") {
        eprintln!("{stdout}");
        return;
    }
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("byte-identical"), "{stdout}");
}
