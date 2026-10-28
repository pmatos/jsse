//! TypeScript's top-level and services unit tests (`src/testRunner/unittests/*.ts`, issue #1259; `services/`,
//! #1261) on `jsse --node` against Node, test by test: `scripts/run-ts-unittests-oracle.py` runs mocha on
//! `built/local/run.js` per group of files and fails on a test that passes on Node and not on jsse unless
//! `ts-suite/unittests-core-known-failures.txt` lists it. The cases below run a few small groups when TS_SRC_DIR
//! points at a built TypeScript 6.0.3 checkout, one at a time (each oracle run already uses up to four processes
//! per engine); the whole set (42 groups, 73 with the 31 `tsbuild-*` groups of #1264) is run by hand (the
//! nightly TypeScript-suite job is #1298).

use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

static ONE_ORACLE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn run_groups(groups: &[&str]) {
    let _serial = ONE_ORACLE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to run the unit-test oracle"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-unittests-oracle.py"))
        .args(["--ci", "--require", "--jsse", env!("CARGO_BIN_EXE_jsse")])
        .args(groups)
        .output()
        .expect("run python3");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let want = format!("{0}/{0} groups match", groups.len());
    assert!(text.contains(&want), "{text}");
}

#[test]
fn small_unit_test_groups_match_node_when_a_typescript_checkout_is_given() {
    run_groups(&[
        "asserts",
        "base64",
        "builder",
        "comments",
        "convertToBase64",
        "debugDeprecation",
        "paths",
        "semver",
    ]);
}

#[test]
fn small_services_groups_match_node_when_a_typescript_checkout_is_given() {
    run_groups(&[
        "services-cancellable",
        "services-colorization",
        "services-documentRegistry",
        "services-hostNewLineSupport",
        "services-languageService",
        "services-patternMatcher",
        "services-preProcessFile",
        "services-utilities",
    ]);
}

#[test]
fn small_tsbuild_groups_match_node_when_a_typescript_checkout_is_given() {
    run_groups(&[
        "tsbuild-clean",
        "tsbuild-configFileExtends",
        "tsbuild-demo",
        "tsbuild-emptyFiles",
        "tsbuild-exitCodeOnBogusFile",
        "tsbuild-extends",
        "tsbuild-graphOrdering",
        "tsbuild-lateBoundSymbol",
        "tsbuild-libraryResolution",
        "tsbuild-moduleSpecifiers",
        "tsbuild-publicApi",
        "tsbuild-transitiveReferences",
    ]);
}
