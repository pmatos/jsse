//! `--node` stack traces (issue #1548): the position of an `at async` frame
//! parked where no expression has one of its own. A `yield*` is the keyword, a
//! disposal after a `yield` is that `yield`, a declaration leaves the start of
//! its initializer and a `break` closes a `for await` at the keyword, in the
//! bytecode VM and the tree-walker. The fixtures are run on Node under
//! `scripts/v8-error-fixtures/`; their output is pinned in `expected-*.txt`.

use std::path::PathBuf;
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

fn replay(name: &str, extra: &[&str], stress: Option<&str>) {
    let dir = root().join("scripts/v8-error-fixtures");
    let mut args = vec!["--node"];
    args.extend_from_slice(extra);
    let fixture = dir.join(format!("{name}.js"));
    args.push(fixture.to_str().expect("utf-8 path"));
    let out = jsse(&args, stress);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected =
        std::fs::read_to_string(dir.join(format!("expected-{name}.txt"))).expect("snapshot");
    assert_eq!(
        stdout(&out),
        expected.trim_end(),
        "{name} {extra:?} GC stress {stress:?}"
    );
}

const FIXTURES: [&str; 2] = ["generators-1529-gaps", "positions-1548"];

#[test]
fn positions_match_node_under_the_bytecode_vm() {
    for name in FIXTURES {
        replay(name, &[], None);
    }
}

#[test]
fn positions_match_node_under_the_tree_walker() {
    for name in FIXTURES {
        replay(name, &["--no-bytecode"], None);
    }
}

#[test]
fn positions_match_node_under_gc_stress() {
    for name in FIXTURES {
        replay(name, &[], Some("7"));
        replay(name, &["--no-bytecode"], Some("7"));
    }
}

#[test]
fn default_mode_runs_the_same_shapes_without_stacks() {
    let source = "function* inner() { yield 1; yield 2; }\n\
                  function* outer() { yield* inner(); }\n\
                  async function* closes() { try { yield 1; yield 2; } finally { await null; } }\n\
                  class R { async [Symbol.asyncDispose]() { await null; } }\n\
                  async function disposes() { await using one = new R(); const x = 1 + 2; return x; }\n\
                  (async () => {\n\
                    const seen = [...outer()];\n\
                    for await (const v of closes()) { seen.push(v); break; }\n\
                    seen.push(await disposes());\n\
                    let a = 1, b = 2;\n\
                    print(seen.join('|') + '|' + (a + b) + '|' + new Error('x').stack.length);\n\
                  })();\n";
    for extra in [&[][..], &["--no-bytecode"][..]] {
        let mut args = extra.to_vec();
        args.extend(["-e", source]);
        let out = jsse(&args, None);
        assert!(out.status.success(), "{extra:?}");
        assert_eq!(stdout(&out), "1|2|1|3|3|0", "{extra:?}");
    }
}
