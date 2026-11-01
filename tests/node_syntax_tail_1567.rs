//! V8's wording of the early errors #1567 closes under `--node`, in the
//! bytecode VM, the tree-walker and under GC stress. `tail-1.tsv` holds one
//! source per line (`script` or `module` goal); `expected-tail-1.txt` is Node
//! v26's own output for `syntax-tail.mjs tail-1.tsv`. The driver is the one of
//! `scripts/node-stack-fixtures-1523-tail/`; the rows that still differ are in
//! docs/agents/library-tests-typescript-conformance-wording-1567.md.

use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts")
}

fn run(flags: &[&str], stress: Option<&str>) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
    command
        .arg("--node")
        .args(flags)
        .arg(root().join("node-stack-fixtures-1523-tail/syntax-tail.mjs"))
        .arg(root().join("node-stack-fixtures-1567/tail-1.tsv"));
    if let Some(period) = stress {
        command.env("JSSE_GC_STRESS", period);
    }
    let out = command.output().expect("run jsse");
    assert!(
        out.status.success(),
        "{flags:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn early_error_wording_matches_v8() {
    let want = std::fs::read_to_string(root().join("node-stack-fixtures-1567/expected-tail-1.txt"))
        .expect("snapshot");
    for (flags, stress) in [
        (&[][..], None),
        (&["--no-bytecode"][..], None),
        (&[][..], Some("7")),
        (&["--no-bytecode"][..], Some("7")),
    ] {
        assert_eq!(
            run(flags, stress).trim_end(),
            want.trim_end(),
            "{flags:?} GC stress {stress:?}"
        );
    }
}

#[test]
fn default_mode_keeps_the_engine_wording() {
    let probe = "var r = [];\n\
        function t(f) { try { f(); } catch (e) { r.push(e.message); } }\n\
        t(() => new Function('({ 1n })'));\n\
        t(() => new Function('async function f() { for await (var a = 1 of b); }'));\n\
        t(() => new Function('x = /[\\\\c]/u'));\n\
        t(() => new Function('x = async a\\n=> 1'));\n\
        print(r.join('|'));";
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["-e", probe])
        .output()
        .unwrap();
    let got = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(
        got,
        "Expected Colon, got RightBrace|for-of loop variable declaration may not have an \
         initializer|Invalid regular expression: /[\\c]/ : Invalid escape|Expected semicolon"
    );
}
