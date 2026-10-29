//! TypeScript `compiler` runner files D-F (#1270): `scripts/run-ts-compiler-df-oracle.py` cuts the 1693 suites into
//! chunks and runs each through `scripts/run-ts-compiler-oracle.py` against `ts-suite/compiler-known-failures-d-f.txt`.
//! The case below checks, with Node alone, that the chunks partition D-F exactly when TS_SRC_DIR points at a built
//! TypeScript 6.0.3 checkout; the whole range is `uv run python scripts/run-ts-compiler-df-oracle.py`.

use std::path::Path;
use std::process::Command;

#[test]
fn compiler_d_to_f_chunks_partition_the_range_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to list the compiler suites"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-compiler-df-oracle.py"))
        .arg("--check-partition")
        .output()
        .expect("run python3");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("partition OK"), "{text}");
}
