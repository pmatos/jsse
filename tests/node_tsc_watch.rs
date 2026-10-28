//! `tsc --watch` sessions (#1248) on jsse alone: `scripts/run-tsc-watch-oracle.py --no-node` drives
//! every session of scripts/tsc-fixtures/watch-cases.txt (scripted edits, creates, deletes, renames,
//! config changes and watch strategies) and compares the recorded diagnostics sequence, emitted files
//! and exit status with the digests pinned from Node in watch-expected.txt. Like tests/node_tsc.rs it
//! needs TSC_DIR (an installed typescript 6.0.3) and passes with a notice without it; the Node-vs-jsse
//! diff itself is `scripts/run-tsc-watch-oracle.py`. Run it with `--profile release-nolto`.
#![cfg(unix)]

use std::path::Path;
use std::process::Command;

#[test]
fn watch_sessions_match_pinned_node_output() {
    if std::env::var_os("TSC_DIR").is_none() {
        eprintln!(
            "SKIP: set TSC_DIR to an installed typescript 6.0.3 package to run the tsc watch sessions"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-tsc-watch-oracle.py"))
        .args(["--no-node", "--ci"])
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
    assert!(stdout.contains("passed"), "{stdout}");
}
