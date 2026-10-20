//! The shipped binary runs on jemalloc (#1047). Its huge-page policy is set at
//! runtime by `thp::disable_at_startup` and `thp::after_major_collection`
//! (#1075, covered by `tests/thp_defer.rs`), not by an embedded `malloc_conf`.
// The counters build writes its report to stderr at exit by design, so the
// empty-stderr check only holds without it.
#![cfg(all(
    feature = "jemalloc",
    target_os = "linux",
    not(feature = "perf-counters")
))]

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
