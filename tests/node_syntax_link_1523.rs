//! V8's wording of early errors and Node's module link and re-import behaviour
//! under `--node` (issues #1484, #1523, #1524), in the bytecode VM, the
//! tree-walker and under GC stress. `scripts/node-stack-fixtures-1523/expected-*.txt`
//! is Node v26's own output for each fixture, with the fixture directory
//! removed from paths; regenerate one by running the fixture on Node and
//! applying the same normalisation (docs/agents/node-host-syntax-link-1523.md).

use std::path::PathBuf;
use std::process::Command;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/node-stack-fixtures-1523")
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
fn script_syntax_error_wording_matches_v8_first_half() {
    check("syntax-scripts-1.mjs", "expected-syntax-scripts-1.txt");
}

#[test]
fn script_syntax_error_wording_matches_v8_second_half() {
    check("syntax-scripts-2.mjs", "expected-syntax-scripts-2.txt");
}

#[test]
fn module_and_eval_syntax_error_wording_matches_v8() {
    check(
        "syntax-modules-eval.mjs",
        "expected-syntax-modules-eval.txt",
    );
}

#[test]
fn export_from_link_errors_and_reimport_match_node() {
    check("link-errors.mjs", "expected-link-errors.txt");
}

#[test]
fn import_of_a_variable_reports_the_load_of_the_variable() {
    check("import-specifier.mjs", "expected-import-specifier.txt");
}

#[test]
fn fs_frames_read_node_fs_not_anonymous() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args([
            "--node",
            "-e",
            r#"
            try { require("fs").openSync(__filename + "/x.json", "w"); }
            catch (e) { console.log(e.stack.split("\n")[1]); }
            "#,
        ])
        .output()
        .expect("run jsse");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("at Object.openSync (node:fs:"), "{text}");
    assert!(!text.contains("<anonymous>:"), "{text}");
}

#[test]
fn default_mode_keeps_its_own_wording() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args([
            "-e",
            r#"
            for (const s of ["function () {}", "let let = 1", "@", "continue;", "1++", "var x = 1__0"]) {
              try { new Function(s); } catch (e) { console.log(e.message); }
            }
            "#,
        ])
        .output()
        .expect("run jsse");
    let text = String::from_utf8_lossy(&out.stdout);
    for own in [
        "SyntaxError: Expected function name",
        "SyntaxError: 'let' is not allowed as a variable name in lexical declarations",
        "SyntaxError: Expected decorator expression after '@'",
        "SyntaxError: Illegal continue statement",
        "SyntaxError: Invalid left-hand side in assignment",
        "SyntaxError: Numeric separator must be exactly one underscore",
    ] {
        assert!(text.contains(own), "{own}: {text}");
    }
}

#[test]
fn default_mode_keeps_its_own_module_link_errors() {
    for (module, own) in [
        ("star-import.mjs", "Ambiguous export 'y' in module"),
        ("reexport-missing.mjs", "has no export named 'absent'"),
        ("reexport-chain.mjs", "has no export named 'absent'"),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
            .arg(fixtures().join(module))
            .output()
            .expect("run jsse");
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(text.contains(own), "{module}: {text}");
        assert!(!text.contains("The requested module"), "{module}: {text}");
    }
}
