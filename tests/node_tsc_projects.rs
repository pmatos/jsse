//! The tsc project set (#1243) on jsse alone: `scripts/run-tsc-projects-oracle.py --no-node`
//! compiles every case in scripts/tsc-fixtures/cases.txt and compares stdout, stderr, exit
//! codes and emitted files with the digests pinned from Node in expected-projects.txt. Like
//! tests/node_tsc.rs it needs TSC_DIR (an installed typescript 6.0.3) and passes with a
//! notice without it; the Node-vs-jsse diff itself is `scripts/run-tsc-projects-oracle.py`.
#![cfg(unix)]

use std::path::Path;
use std::process::Command;

#[test]
fn project_set_matches_pinned_node_output() {
    if std::env::var_os("TSC_DIR").is_none() {
        eprintln!(
            "SKIP: set TSC_DIR to an installed typescript 6.0.3 package to run the tsc project set"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-tsc-projects-oracle.py"))
        .args(["--no-node", "--require", "--ci"])
        .env("JSSE", env!("CARGO_BIN_EXE_jsse"))
        .output()
        .expect("run python3");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("passed"), "{stdout}");
}
