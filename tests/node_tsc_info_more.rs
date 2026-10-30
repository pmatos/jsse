//! The second informational-flag set of the tsc project oracle (#1247), on jsse alone:
//! `scripts/tsc-fixtures/cases-info-more.txt` runs `--listFiles`, `--explainFiles`,
//! `--traceResolution`, `--showConfig`, `--init`, `--extendedDiagnostics`, `--generateTrace` and
//! `--generateCpuProfile` (also under `--build`, with project references, package.json
//! imports/exports/typesVersions, response files and `--locale`) and compares stdout, stderr, exit
//! codes and written files with the digests pinned from Node in `expected-info-more.txt`. Like
//! tests/node_tsc_projects.rs it needs TSC_DIR (an installed typescript 6.0.3) and passes with a
//! notice without it; the Node-vs-jsse diff is `scripts/run-tsc-projects-oracle.py`.
//! Run it with `--profile release-nolto`: `infm-trace-dom` parses `lib.dom.d.ts`.
#![cfg(unix)]

use std::path::Path;
use std::process::Command;

fn case_names(root: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(root.join("scripts/tsc-fixtures/cases-info-more.txt"))
        .expect("read cases-info-more.txt");
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| l.split(" | ").next().unwrap().trim().to_string())
        .collect()
}

#[test]
fn informational_flag_cases_match_pinned_node_output() {
    if std::env::var_os("TSC_DIR").is_none() {
        eprintln!(
            "SKIP: set TSC_DIR to an installed typescript 6.0.3 package to run the informational-flag cases"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-tsc-projects-oracle.py"))
        .args(["--no-node", "--ci", "--jobs", "4"])
        .args(case_names(root))
        .env("JSSE", env!("CARGO_BIN_EXE_jsse"))
        .output()
        .expect("run python3");
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains("SKIP:") {
        eprintln!("{stdout}");
        return;
    }
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("passed"), "{stdout}");
}
