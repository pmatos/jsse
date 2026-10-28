//! TypeScript's config, sys and debugDeprecation unit tests (#1260) run on
//! `jsse --node` and Node, compared per test title by
//! `scripts/run-ts-unittests-config-sys-oracle.py`. The case below drives the
//! oracle on three small groups (debugDeprecation, initTSConfig,
//! tsconfigParsing) when TS_SRC_DIR points at a built TypeScript
//! 6.0.3 checkout; the full set (541 tests, `symlinkWatching` included) is
//! `uv run python scripts/run-ts-unittests-config-sys-oracle.py`.

use std::path::Path;
use std::process::Command;

#[test]
fn config_sys_unit_tests_match_node_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to run the unit-test oracle"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-unittests-config-sys-oracle.py"))
        .args([
            "--require",
            "--jobs",
            "2",
            "--jsse",
            env!("CARGO_BIN_EXE_jsse"),
        ])
        .args([
            "debugDeprecation",
            "config-initTSConfig",
            "config-tsconfigParsing",
        ])
        .output()
        .expect("run python3");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("unit-test oracle: PASS"), "{text}");
}
