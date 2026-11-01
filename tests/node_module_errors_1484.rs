//! `jsse --node` output for an ES module entry point that does not parse or link
//! (issue #1484) against Node v26's stderr in `scripts/node-stack-fixtures-1484/`
//! (docs/agents/library-tests-typescript-conformance-module-errors-1484.md).

use std::path::PathBuf;
use std::process::Command;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/node-stack-fixtures-1484")
}

fn normalise(stderr: &[u8], dir: &str) -> String {
    let text = String::from_utf8_lossy(stderr)
        .replace(&format!("file://{dir}/"), "")
        .replace(&format!("{dir}/"), "");
    text.lines()
        .filter(|line| !line.contains("node:internal") && !line.contains("node:diagnostics"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn run(entry: &str, node: bool, flags: &[&str], stress: Option<&str>) -> (Option<i32>, String) {
    let dir = fixtures();
    let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if node {
        command.arg("--node");
    }
    command.args(flags).arg(dir.join(format!("{entry}.mjs")));
    if let Some(period) = stress {
        command.env("JSSE_GC_STRESS", period);
    }
    let out = command.output().expect("run jsse");
    (
        out.status.code(),
        normalise(&out.stderr, &dir.to_string_lossy()),
    )
}

fn check(entry: &str) {
    let want = std::fs::read_to_string(fixtures().join(format!("expected-{entry}.txt")))
        .expect("snapshot");
    for (flags, stress) in [
        (&[][..], None),
        (&["--no-bytecode"][..], None),
        (&[][..], Some("7")),
        (&["--no-bytecode"][..], Some("7")),
    ] {
        let (code, got) = run(entry, true, flags, stress);
        assert_eq!(
            got.trim_end(),
            want.trim_end(),
            "{entry} {flags:?} GC stress {stress:?}"
        );
        assert_eq!(code, Some(1), "{entry} {flags:?} exit status");
    }
}

#[test]
fn entry_module_that_does_not_parse_reports_like_node() {
    for entry in [
        "parse",
        "parse-eof",
        "parse-wide",
        "parse-crlf",
        "parse-ls",
        "parse-multiline",
        "parse-unicode",
        "parse-tab",
    ] {
        check(entry);
    }
}

#[test]
fn dependency_that_does_not_parse_reports_like_node() {
    check("syntax-dependency");
}

#[test]
fn entry_module_link_errors_are_not_bracketed() {
    for entry in [
        "missing-export",
        "missing-export-multiline",
        "missing-default",
        "reexport-missing",
        "star-conflict",
    ] {
        check(entry);
    }
}

#[test]
fn entry_module_that_throws_is_unchanged() {
    check("throwing-module");
}

#[test]
fn default_mode_keeps_the_engines_own_report() {
    let (code, stderr) = run("parse", false, &[], None);
    assert_eq!(code, Some(2));
    assert!(
        stderr.starts_with("SyntaxError:") && !stderr.contains("file://"),
        "{stderr}"
    );
}
