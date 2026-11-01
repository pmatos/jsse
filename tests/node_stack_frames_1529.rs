//! `--node` stack traces (issue #1529): a generator body is a frame of its
//! function, an async generator parked at an `await` is an `at async` frame, and
//! a `for` / `for await` loop reports its head, in the bytecode VM and the
//! tree-walker. `scripts/v8-error-fixtures/generators-1529.js` is run on Node;
//! its output is pinned in `expected-generators-1529.txt`.

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

fn run_both_engines(source: &str) -> Vec<String> {
    [&[][..], &["--no-bytecode"][..]]
        .iter()
        .map(|extra| {
            let mut args = vec!["--node"];
            args.extend_from_slice(extra);
            args.extend(["-e", source]);
            let out = jsse(&args, None);
            assert!(
                out.status.success(),
                "{extra:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            stdout(&out)
        })
        .collect()
}

#[test]
fn generator_frames_match_node_under_the_bytecode_vm() {
    replay("generators-1529", &[], None);
}

#[test]
fn generator_frames_match_node_under_the_tree_walker() {
    replay("generators-1529", &["--no-bytecode"], None);
}

#[test]
fn generator_frames_match_node_under_gc_stress() {
    replay("generators-1529", &[], Some("7"));
    replay("generators-1529", &["--no-bytecode"], Some("7"));
}

#[test]
fn call_sites_describe_generator_frames_like_v8() {
    let source = r#"
        Error.prepareStackTrace = (e, sites) => sites;
        function* sync() { yield new Error("x"); }
        async function* parked() { await null; yield new Error("y"); }
        const row = (site) => [site.getFunctionName(), site.isAsync(), site.isToplevel(), site.isConstructor()].join(",");
        async function main() {
            const rows = [];
            for (const e of sync()) rows.push(row(e.stack[0]));
            for await (const e of parked()) {
                rows.push(row(e.stack[0]));
                const sites = e.stack.filter((s) => s.isAsync());
                rows.push(sites.map(row).join(";"));
            }
            console.log(rows.join("\n"));
        }
        main();
    "#;
    let expected = "sync,false,true,false\nparked,false,true,false\nmain,true,true,false";
    for output in run_both_engines(source) {
        assert_eq!(output, expected);
    }
}

#[test]
fn default_mode_has_no_stacks_and_generators_still_work() {
    let source = "function* g() { yield new Error('x').stack; }\n\
                  async function* ag() { await null; yield 1; }\n\
                  (async () => {\n\
                    const seen = [];\n\
                    for (const s of g()) seen.push(s.length === 0 ? 'empty' : 'stack');\n\
                    for await (const v of ag()) seen.push(v);\n\
                    print(seen.join('|'));\n\
                  })();\n";
    for extra in [&[][..], &["--no-bytecode"][..]] {
        let mut args = extra.to_vec();
        args.extend(["-e", source]);
        let out = jsse(&args, None);
        assert!(out.status.success());
        assert_eq!(stdout(&out), "empty|1", "{extra:?}");
    }
}
