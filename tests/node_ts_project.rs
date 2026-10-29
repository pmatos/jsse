//! TypeScript's `project` runner (#1285) run on `jsse --node` and Node, compared per test, per error message
//! and per local baseline file by `scripts/run-ts-project-oracle.py`. The case below runs three JSON cases
//! (resolution, declarations, privacy check; both module kinds) plus the corrupted-input control when
//! TS_SRC_DIR points at a built TypeScript 6.0.3 checkout; the whole `core` slice (88 cases, 704 tests) is
//! `uv run python scripts/run-ts-project-oracle.py core`.

use std::path::Path;
use std::process::Command;

fn oracle(extra: &[&str]) -> (bool, String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-project-oracle.py"))
        .args(["--require", "--jobs", "2", "--batch-size", "3", "--jsse"])
        .arg(env!("CARGO_BIN_EXE_jsse"))
        .args(extra)
        .output()
        .expect("run python3");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn project_cases_match_node_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to run the project oracle"
        );
        return;
    }
    let (ok, text) = oracle(&[
        "files:baseline.json,declarationsSimpleImport.json,privacyCheckOnImportedModuleSimpleReference.json",
    ]);
    assert!(ok, "{text}");
    assert!(text.contains("project oracle: PASS"), "{text}");
    assert!(text.contains("jsse: 24 passing, 0 failing"), "{text}");
}

#[test]
fn known_failures_file_lists_only_titles_of_the_core_slice() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("ts-suite/project-1285-known-failures.txt"))
        .expect("known-failures file");
    for line in text
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let (title, issue) = line.split_once('\t').expect("title<TAB>issue");
        assert!(
            title.starts_with("projects tests Compiling project for "),
            "{line}"
        );
        assert!(issue.starts_with('#'), "{line}");
    }
}
