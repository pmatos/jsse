//! `process.stdin`/`stdout`/`stderr`, `tty` and `readline` under `--node`
//! (#1232). The fixtures in scripts/tty-fixtures run over pipes, files and a
//! pseudo-terminal; `scripts/tty-fixtures/oracle.py` compares each against the
//! output recorded from Node (expected/), and `scripts/run-node-tty-oracle.sh`
//! cross-checks against a live Node.

#![cfg(unix)]

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn jsse() -> Command {
    Command::new(env!("CARGO_BIN_EXE_jsse"))
}

fn run_with_stdin(args: &[&str], input: &str, gc_stress: Option<&str>) -> Output {
    let mut cmd = jsse();
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    let mut child = cmd
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn jsse");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().expect("wait for jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|name| {
        Command::new(name)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

fn oracle(only: Option<&str>) {
    let Some(python) = python() else {
        eprintln!("python is not available: skipping the tty oracle");
        return;
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/tty-fixtures/oracle.py");
    let mut cmd = Command::new(python);
    cmd.arg(script)
        .args(["--no-cross-check", "--jsse", env!("CARGO_BIN_EXE_jsse")]);
    if let Some(case) = only {
        cmd.args(["--only", case]);
    }
    let out = cmd.output().expect("run the tty oracle");
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn fixtures_match_the_output_recorded_from_node() {
    oracle(None);
}

#[test]
fn lines_are_read_from_a_pipe_under_gc_stress() {
    let source = r#"
        const rl = require("readline").createInterface({ input: process.stdin });
        const seen = [];
        rl.on("line", (line) => seen.push(line));
        rl.on("close", () => console.log(JSON.stringify(seen)));
    "#;
    let out = run_with_stdin(&["--node", "-e", source], "a\nb\r\nc", Some("7"));
    assert_eq!(stdout(&out), "[\"a\",\"b\",\"c\"]\n");
}

#[test]
fn lines_are_read_by_the_tree_walker_too() {
    let source = r#"
        (async () => {
          const rl = require("readline").createInterface({ input: process.stdin });
          let n = 0;
          for await (const line of rl) n += line.length;
          console.log(n);
        })();
    "#;
    let out = run_with_stdin(
        &["--node", "--no-bytecode", "-e", source],
        "ab\ncde\n",
        None,
    );
    assert_eq!(stdout(&out), "5\n");
}

#[test]
fn an_open_stdin_keeps_the_loop_alive_until_it_ends_or_is_paused() {
    let source = r#"
        let n = 0;
        process.stdin.on("data", (c) => { n += c.length; process.stdin.pause(); });
        process.on("exit", () => console.log("exit", n));
    "#;
    let out = run_with_stdin(&["--node", "-e", source], "12345", None);
    assert_eq!(stdout(&out), "exit 5\n");
}

#[test]
fn an_unref_stdin_does_not_keep_the_loop_alive() {
    let source = r#"
        process.stdin.unref();
        process.stdin.on("data", () => {});
        process.stdout.ref();
        process.stderr.unref();
        process.on("exit", () => console.log("exit"));
    "#;
    let mut child = jsse()
        .args(["--node", "-e", source])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn jsse");
    let mut pipe = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut pipe, &mut text).unwrap();
        let _ = tx.send(text);
    });
    let result = rx.recv_timeout(std::time::Duration::from_secs(30));
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(result.expect("the process kept running"), "exit\n");
}

#[test]
fn a_broken_pipe_is_reported_on_the_stream_and_not_by_console() {
    let source = r#"
        process.stdout.on("error", (e) => process.stderr.write(e.code + "|" + e.message + "\n"));
        setTimeout(() => { process.stdout.write("x\n"); console.log("y"); }, 200);
    "#;
    let mut child = jsse()
        .args(["--node", "-e", source])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn jsse");
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stderr), "EPIPE|write EPIPE\n");
}

#[test]
fn uncaught_errors_reach_stderr_through_process_stderr() {
    let out = run_with_stdin(&["--node", "-e", "throw new Error('boom')"], "", None);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Error: boom"));
}

#[test]
fn stdio_and_tty_are_absent_without_node() {
    let out = run_with_stdin(
        &[
            "-e",
            "print(typeof process, typeof __host_tty, typeof require)",
        ],
        "",
        None,
    );
    assert_eq!(stdout(&out), "undefined undefined undefined\n");
}

#[test]
fn internal_stdio_modules_stay_private() {
    let source = r#"
        for (const id of ["internal/process/stdio", "internal/stdio/socket", "internal/readline/interface"]) {
          try { require(id); console.log("loaded", id); } catch (e) { console.log(e.code); }
        }
        console.log(typeof require("tty").isatty, typeof require("node:readline/promises").Interface);
    "#;
    let out = run_with_stdin(&["--node", "-e", source], "", None);
    assert_eq!(
        stdout(&out),
        "MODULE_NOT_FOUND\nMODULE_NOT_FOUND\nMODULE_NOT_FOUND\nfunction function\n"
    );
}

#[test]
fn a_tty_read_stream_of_another_descriptor_is_not_implemented() {
    let source = r#"
        try { new (require("tty").ReadStream)(3); } catch (e) { console.log(e.code); }
    "#;
    let out = run_with_stdin(&["--node", "-e", source], "", None);
    assert_eq!(stdout(&out), "ERR_METHOD_NOT_IMPLEMENTED\n");
}
