//! Async function frames and `at async` frames of `--node` stacks in the
//! tree-walker as well as the bytecode VM (issues #1333, #1358, #1368, #1379).
//! `scripts/v8-error-fixtures/async.js` is Node's own output pinned in
//! `expected-async.txt` (`scripts/run-node-v8-error-oracle.sh` cross-checks it).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn jsse(args: &[&str], stress: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
    command.args(args);
    if let Some(period) = stress {
        command.env("JSSE_GC_STRESS", period);
    }
    command.output().expect("run jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn normalise(text: &str, dir: &Path) -> String {
    let text = text.replace(dir.to_string_lossy().as_ref(), "<dir>");
    let mut kept = Vec::new();
    for line in text.lines() {
        let frame = line.starts_with(char::is_whitespace) && line.trim_start().starts_with("at ");
        if frame && (!line.contains("<dir>/") || line.contains("Object.<anonymous>")) {
            continue;
        }
        kept.push(line);
    }
    kept.join("\n")
}

fn run_async_fixture(extra: &[&str], stress: Option<&str>) {
    let dir = root().join("scripts/v8-error-fixtures");
    let mut args = vec!["--node"];
    args.extend_from_slice(extra);
    let fixture = dir.join("async.js");
    args.push(fixture.to_str().expect("utf-8 path"));
    let out = jsse(&args, stress);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected = std::fs::read_to_string(dir.join("expected-async.txt")).expect("snapshot");
    assert_eq!(
        normalise(&stdout(&out), &dir),
        expected.trim_end(),
        "{extra:?} GC stress {stress:?}"
    );
}

#[test]
fn async_frames_match_node_under_the_bytecode_vm() {
    run_async_fixture(&[], None);
}

#[test]
fn async_frames_match_node_under_the_tree_walker() {
    run_async_fixture(&["--no-bytecode"], None);
}

#[test]
fn async_frames_match_node_under_gc_stress() {
    run_async_fixture(&[], Some("7"));
    run_async_fixture(&["--no-bytecode"], Some("7"));
}

#[test]
fn module_chain_matches_node_under_the_tree_walker() {
    let dir = root().join("scripts/event-loop-fixtures");
    let fixture = dir.join("async-stack.fixture.mjs");
    let expected = std::fs::read_to_string(dir.join("async-stack.stdout")).expect("snapshot");
    let dir = dir.to_string_lossy();
    for stress in [None, Some("7")] {
        let out = jsse(
            &[
                "--node",
                "--no-bytecode",
                fixture.to_str().expect("utf-8 path"),
            ],
            stress,
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let actual = stdout(&out).replace(dir.as_ref(), "<dir>");
        assert_eq!(actual, expected.trim_end(), "GC stress {stress:?}");
    }
}

#[test]
fn async_frames_do_not_exist_in_default_mode_in_either_engine() {
    let source = "async function f(a = new Error('d')) { await 1; return [a.stack, new Error('x').stack] }\n\
                  f().then((s) => print(s.join('|') === '|' ? 'empty' : s.join('|')))";
    for extra in [&[][..], &["--no-bytecode"][..]] {
        let mut args = extra.to_vec();
        args.extend(["-e", source]);
        let out = jsse(&args, None);
        assert!(out.status.success());
        assert_eq!(stdout(&out), "empty", "{extra:?}");
    }
}
