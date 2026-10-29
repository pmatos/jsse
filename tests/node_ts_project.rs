//! TypeScript's `project` runner (#1286) run on `jsse --node` and Node, compared per test, per error
//! message and per local baseline file by `scripts/run-ts-project-oracle.py`. The case below runs a few
//! suites of the out/sourcemap/mapRoot/sourceRoot matrix plus the corrupted-reference control when TS_SRC_DIR
//! points at a built TypeScript 6.0.3 checkout; the whole matrix is
//! `uv run python scripts/run-ts-project-oracle.py matrix`.

use std::path::Path;
use std::process::Command;

fn oracle(extra: &[&str]) -> (bool, String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-project-oracle.py"))
        .args(["--require", "--jobs", "2", "--batch-size", "4", "--jsse"])
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
fn project_matrix_matches_node_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to run the project oracle"
        );
        return;
    }
    let (ok, text) = oracle(&[
        "files:outSimpleNoOutdir.json+files:sourcemapSubfolderSpecifyOutputFile.json+files:maprootUrlsourcerootUrlMixedSubfolderSpecifyOutputDirectory.json",
    ]);
    assert!(ok, "{text}");
    assert!(text.contains("project oracle: PASS"), "{text}");
}
