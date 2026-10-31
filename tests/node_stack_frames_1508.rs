//! `--node` stack traces (issue #1508): `at async Promise.all (index N)` frames
//! (also `allSettled`, `any`), V8's ` [as method]` suffix and Node's own timer
//! frame names, in the bytecode VM and the tree-walker. The fixtures under
//! `scripts/v8-error-fixtures` are run on Node; their output is pinned in
//! `expected-async-1508.txt` and `expected-timers-1508.txt`.

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

#[test]
fn combinator_and_method_frames_match_node_under_the_bytecode_vm() {
    replay("async-1508", &[], None);
}

#[test]
fn combinator_and_method_frames_match_node_under_the_tree_walker() {
    replay("async-1508", &["--no-bytecode"], None);
}

#[test]
fn combinator_and_method_frames_match_node_under_gc_stress() {
    replay("async-1508", &[], Some("7"));
    replay("async-1508", &["--no-bytecode"], Some("7"));
}

#[test]
fn timer_frames_match_node_in_both_engines() {
    replay("timers-1508", &[], None);
    replay("timers-1508", &["--no-bytecode"], None);
    replay("timers-1508", &[], Some("7"));
}

#[test]
fn call_sites_describe_combinator_frames_like_v8() {
    let source = r#"
        Error.prepareStackTrace = (e, sites) => sites;
        async function leaf() { await null; return new Error("x"); }
        async function main() {
            const rows = [];
            for (const name of ["all", "allSettled", "any"]) {
                const error = name === "any" ? await Promise.any([leaf()]) : (await Promise[name]([1, leaf()]))[1];
                const site = (error.value ?? error).stack.find((s) => s.getTypeName() === "Promise");
                rows.push([String(site), site.getFunctionName(), site.getMethodName(), site.getPromiseIndex(),
                    site.isPromiseAll(), site.isAsync(), site.getFileName(), site.getLineNumber(), site.isToplevel()].map(String).join(","));
            }
            console.log(rows.join("\n"));
        }
        main();
    "#;
    let expected = [
        "async Promise.all (index 1),all,all,1,true,true,null,null,false",
        "async Promise.allSettled (index 1),allSettled,allSettled,1,false,true,null,null,false",
        "async Promise.any (index 0),any,any,0,false,true,null,null,false",
    ]
    .join("\n");
    for extra in [&[][..], &["--no-bytecode"][..]] {
        let mut args = vec!["--node"];
        args.extend_from_slice(extra);
        args.extend(["-e", source]);
        let out = jsse(&args, None);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(stdout(&out), expected, "{extra:?}");
    }
}

#[test]
fn default_mode_has_no_stacks_and_combinators_still_work() {
    let source = "async function leaf() { await null; return new Error('x').stack; }\n\
                  Promise.all([Promise.race([leaf()]), 2]).then((v) => print((v[0] ? 'stack' : 'empty') + '|' + v[1]));\n\
                  ";
    for extra in [&[][..], &["--no-bytecode"][..]] {
        let mut args = extra.to_vec();
        args.extend(["-e", source]);
        let out = jsse(&args, None);
        assert!(out.status.success());
        assert_eq!(stdout(&out), "empty|2", "{extra:?}");
    }
}
