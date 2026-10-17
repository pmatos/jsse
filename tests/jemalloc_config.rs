//! The shipped binary runs on jemalloc (#1047); huge pages are handled by
//! `thp::defer_huge_pages` (#1075, see `tests/thp_defer.rs`).
#![cfg(all(feature = "jemalloc", target_os = "linux"))]

use std::process::Command;

fn jsse() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    cmd.args(["-e", "1"])
        .env_remove("MALLOC_CONF")
        .env_remove("_RJEM_MALLOC_CONF");
    cmd
}

#[test]
fn startup_writes_nothing_to_stderr() {
    let out = jsse().output().expect("spawn jsse");
    assert!(out.status.success());
    assert!(
        out.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
