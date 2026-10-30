//! TypeScript's `hereby lint` (ESLint 10 and typescript-eslint over the compiler and script sources, #1295)
//! run on `jsse --node` and Node, compared by `scripts/run-ts-lint-oracle.py`: exit status, stdout, stderr and
//! every file the run writes (`--fix`, `.eslintcache`). The case below runs the cheap cases (ESLint's version,
//! `--print-config`, the control files of scripts/ts-lint-fixtures in the scripts program) and `ctl-src` (the
//! type-aware program of src/) when TS_SRC_DIR points at a TypeScript 6.0.3 checkout with `npm ci` done; the whole
//! repository is `uv run python scripts/run-ts-lint-oracle.py full`.

use std::path::Path;
use std::process::Command;

#[test]
fn lint_matches_node_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!("SKIP: set TS_SRC_DIR to a TypeScript 6.0.3 checkout to run the lint oracle");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-lint-oracle.py"))
        .args(["--require", "--jobs", "4", "--jsse"])
        .arg(env!("CARGO_BIN_EXE_jsse"))
        .args(["cheap", "ctl-src"])
        .output()
        .expect("run python3");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{text}");
    assert!(text.contains("lint oracle: PASS"), "{text}");
}
