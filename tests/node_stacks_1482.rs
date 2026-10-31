//! Builtin frames, `import()` call sites and module error text of `--node`
//! stacks (issues #1482, #1483, #1484), in the bytecode VM, the tree-walker
//! and under GC stress. `scripts/node-stack-fixtures-1482/expected-*.txt` is
//! Node v26's own output for each fixture, with the fixture directory removed
//! from paths; regenerate one by running the fixture on Node and applying the
//! same normalisation. `scripts/run-hereby-stacks-oracle.py` cross-checks the
//! hereby cases against a live Node.

use std::path::PathBuf;
use std::process::Command;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/node-stack-fixtures-1482")
}

fn run(fixture: &str, flags: &[&str], stress: Option<&str>) -> String {
    let dir = fixtures();
    let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
    command.arg("--node").args(flags).arg(dir.join(fixture));
    if let Some(period) = stress {
        command.env("JSSE_GC_STRESS", period);
    }
    let out = command.output().expect("run jsse");
    assert!(
        out.status.success(),
        "{fixture} {flags:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let dir = dir.to_string_lossy().into_owned();
    String::from_utf8_lossy(&out.stdout)
        .replace(&format!("file://{dir}/"), "")
        .replace(&format!("{dir}/"), "")
}

fn check(fixture: &str, expected: &str) {
    let want = std::fs::read_to_string(fixtures().join(expected)).expect("snapshot");
    for (flags, stress) in [
        (&[][..], None),
        (&["--no-bytecode"][..], None),
        (&[][..], Some("7")),
        (&["--no-bytecode"][..], Some("7")),
    ] {
        assert_eq!(
            run(fixture, flags, stress).trim_end(),
            want.trim_end(),
            "{fixture} {flags:?} GC stress {stress:?}"
        );
    }
}

#[test]
fn builtin_frames_match_node() {
    check("native.mjs", "expected-native.txt");
}

#[test]
fn import_call_sites_and_link_errors_match_node() {
    check("import-site.mjs", "expected-import-site.txt");
}

#[test]
fn syntax_error_wording_matches_v8() {
    check("syntax.mjs", "expected-syntax.txt");
}

#[test]
fn default_mode_has_no_builtin_frames_and_keeps_its_parser_wording() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args([
            "-e",
            r#"
            try { [1].map(() => { throw new Error("x"); }); } catch (e) { console.log(JSON.stringify(e.stack)); }
            try { new Function("const = 1"); } catch (e) { console.log(e.message); }
            "#,
        ])
        .output()
        .expect("run jsse");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("\"\""), "{text}");
    assert!(
        text.contains("Expected binding pattern, got Assign"),
        "{text}"
    );
}
