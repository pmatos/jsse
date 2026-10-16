//! The shipped binary runs on jemalloc with `thp:never` compiled in (#1047).
//! The setting travels through the `_rjem_malloc_conf` symbol, which jemalloc
//! reads only while the crate keeps its `_rjem_` prefix; without this check a
//! prefix change would silently bring back the +17% test262 slowdown that
//! default THP behaviour causes on THP=always hosts.
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
fn compiled_in_config_disables_transparent_huge_pages() {
    let out = jsse()
        .env("_RJEM_MALLOC_CONF", "stats_print:true")
        .output()
        .expect("spawn jsse");
    assert!(out.status.success());
    let stats = String::from_utf8_lossy(&out.stderr);
    assert!(
        stats.contains("opt.thp: \"never\""),
        "jemalloc did not pick up the compiled-in malloc_conf:\n{stats}"
    );
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
