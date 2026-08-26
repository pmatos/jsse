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

/// Generator and `eval` bodies bypass `dispatch_body`, so without frames of
/// their own their work units are credited to the calling function — the
/// `BODY` ranking's whole purpose is defeated (#537 review). Measured before
/// the fix, `caller` reported 16,020 exclusive units; its real total is 13.
#[test]
fn generator_and_eval_work_is_not_credited_to_the_caller() {
    let dir = scratch("attribution");
    fs::write(
        dir.join("main.js"),
        "function* gen(n) { var s = 0; for (var i = 0; i < n; i++) { s += i; } yield s; }\n\
         function caller() { var t = 0; for (var g of gen(2000)) { t += g; } return t; }\n\
         function evalCaller() { var r = 0; for (var i = 0; i < 200; i++) { r += eval(\"i + 1\"); } return r; }\n\
         caller(); evalCaller();\n",
    )
    .expect("write main");
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(&dir)
        .args(["main.js"])
        .output()
        .expect("run jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);

    let row = |name: &str| -> Option<u64> {
        stderr
            .lines()
            .find(|l| l.starts_with(&format!("BODY\t{name}\t")))
            .and_then(|l| l.split('\t').nth(2))
            .and_then(|n| n.parse().ok())
    };

    let generator = row("<generator/async/script body>")
        .unwrap_or_else(|| panic!("no generator BODY row; stderr:\n{stderr}"));
    let eval_body =
        row("<eval>").unwrap_or_else(|| panic!("no <eval> BODY row; stderr:\n{stderr}"));
    let caller = row("caller").unwrap_or_else(|| panic!("no caller BODY row; stderr:\n{stderr}"));

    assert!(
        generator > 1000,
        "the generator's own loop must be credited to it, got {generator}"
    );
    assert!(eval_body > 0, "eval'd code must be credited to <eval>");
    assert!(
        caller < 100,
        "caller must not absorb the generator's work, got {caller}"
    );
}
