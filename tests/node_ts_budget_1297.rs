//! Wall-time and peak-RSS budget of TypeScript's own test suite on `jsse --node` (#1297):
//! `scripts/run-ts-budget-oracle.py` runs selections of the suite (`scripts/ts_budget_cases.py`) in one mocha
//! process per engine and checks jsse's peak RSS against `max_rss_ratio` (6x Node) of `ts-suite/budget-1297.json`;
//! wall time is recorded, never capped. The first case checks the budget file itself and needs nothing; the
//! second runs the two cheapest cases (the 13 MB runner bundle load and the transpile runner, under a minute on
//! jsse) and needs TS_SRC_DIR, a built TypeScript 6.0.3 checkout. Run it with `--profile release-nolto`.
#![cfg(unix)]

use std::path::Path;
use std::process::Command;

fn oracle() -> Command {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut cmd = Command::new("python3");
    cmd.arg(root.join("scripts/run-ts-budget-oracle.py"))
        .env("JSSE", env!("CARGO_BIN_EXE_jsse"));
    cmd
}

#[test]
fn budget_file_covers_every_case_with_the_ruled_limits() {
    let out = oracle()
        .arg("--check-budget")
        .output()
        .expect("run python3");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("budget OK"), "{text}");
}

#[test]
fn bundle_load_and_transpile_match_node_within_budget_when_a_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to run the budget cases"
        );
        return;
    }
    let out = oracle()
        .args(["--timeout", "1200", "bundle-load", "transpile"])
        .output()
        .expect("run python3");
    let text = String::from_utf8_lossy(&out.stdout);
    if text.contains("SKIP:") {
        eprintln!("{text}");
        return;
    }
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("within the budget"), "{text}");
}
