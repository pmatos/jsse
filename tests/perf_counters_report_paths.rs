//! Every instrumented run must write the counter report to stderr, not only
//! the runs that happen to go through `execute_code` (#537 review finding).
//!
//! Before the fix, `--prelude` and REPL invocations bypassed `execute_code`
//! entirely and exited silently, contradicting the guarantee documented in
//! AGENTS.md. These are child-process checks because the condition is which
//! `main.rs` exit path the process takes.
#![cfg(feature = "perf-counters")]

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("jsse-perf-report-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Counts complete counter reports by their first line, so a path emitting the
/// report twice is as much a failure as one emitting it not at all.
fn report_count(stderr: &str) -> usize {
    stderr
        .lines()
        .filter(|l| l.starts_with("PERF\tvm_ops\t"))
        .count()
}

#[test]
fn prelude_path_emits_exactly_one_report() {
    let dir = scratch("prelude");
    fs::write(dir.join("pre.js"), "var a = 1;\n").expect("write prelude");
    fs::write(dir.join("main.js"), "var b = a + 1;\n").expect("write main");
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(&dir)
        .args(["--prelude", "pre.js", "main.js"])
        .output()
        .expect("run jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(report_count(&stderr), 1, "stderr was:\n{stderr}");
}

#[test]
fn file_path_emits_exactly_one_report() {
    let dir = scratch("file");
    fs::write(dir.join("main.js"), "var b = 2;\n").expect("write main");
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(&dir)
        .args(["main.js"])
        .output()
        .expect("run jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(report_count(&stderr), 1, "stderr was:\n{stderr}");
}

#[test]
fn eval_path_emits_exactly_one_report() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["-e", "1 + 1"])
        .output()
        .expect("run jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(report_count(&stderr), 1, "stderr was:\n{stderr}");
}

#[test]
fn repl_path_emits_exactly_one_report() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn jsse");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"1 + 1\n")
        .expect("write to repl");
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait for jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(report_count(&stderr), 1, "stderr was:\n{stderr}");
}
