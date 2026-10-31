//! The global `TextDecoder` under `--node` (#1527) against Node v26.8.2:
//! `tests/node_textdecoder_1527/probe.js` prints one line per observation
//! (argument validation, error codes and messages, fatal and streaming state,
//! class shape, inspect output); `expected.txt` is its output on Node.

use std::path::PathBuf;
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/node_textdecoder_1527")
        .join(name)
}

fn run(extra: &[&str], gc_stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    cmd.arg("--node").args(extra).arg(fixture("probe.js"));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    cmd.output().expect("run jsse")
}

fn assert_matches_node(out: &Output) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "exit {:?}: {stderr}", out.status);
    let want = std::fs::read_to_string(fixture("expected.txt")).expect("expected.txt");
    let got = String::from_utf8_lossy(&out.stdout);
    for (n, (w, g)) in want.lines().zip(got.lines()).enumerate() {
        assert_eq!(g, w, "line {} differs from Node", n + 1);
    }
    assert_eq!(got.lines().count(), want.lines().count(), "line count");
}

#[test]
fn text_decoder_matches_node() {
    assert_matches_node(&run(&[], None));
}

#[test]
fn text_decoder_matches_node_under_the_tree_walker() {
    assert_matches_node(&run(&["--no-bytecode"], None));
}

#[test]
fn text_decoder_matches_node_under_gc_stress() {
    assert_matches_node(&run(&[], Some("50")));
}

#[test]
fn text_decoder_is_absent_without_node() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["-e", "print(typeof TextDecoder)"])
        .output()
        .expect("run jsse");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "undefined");
}
