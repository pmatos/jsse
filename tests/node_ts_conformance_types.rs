//! TypeScript `conformance` runner, directory `types` (#1274): `scripts/run-ts-conformance-types-oracle.py` runs
//! tests/cases/conformance/types through the compiler oracle's machinery on Node and `jsse --node` against
//! `ts-suite/conformance-types-known-failures.txt`. The case below checks, with Node alone, that the slice lists the
//! 857 suites (5142 tests) the `types` bucket of ts-suite/oracle.json pins, and that its thirty subdirectory rows
//! add up to them, when TS_SRC_DIR points at a built TypeScript 6.0.3 checkout; the whole directory is
//! `uv run python scripts/run-ts-conformance-types-oracle.py`.

use std::path::Path;
use std::process::Command;

fn listing(spec: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-conformance-types-oracle.py"))
        .args(["--list", "--no-jsse", spec])
        .output()
        .expect("run python3");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    text.lines().next().unwrap_or_default().to_string()
}

#[test]
fn conformance_types_slice_lists_the_pinned_suites_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to list the conformance suites"
        );
        return;
    }
    let head = listing("types");
    assert!(
        head.contains(": 857 of ") && head.contains("(5142 tests)"),
        "{head}"
    );
    let by_dir = [
        "any",
        "asyncGenerators",
        "conditional",
        "contextualTypes",
        "forAwait",
        "import",
        "intersection",
        "keyof",
        "literal",
        "localTypes",
        "mapped",
        "members",
        "namedTypes",
        "never",
        "nonPrimitive",
        "objectTypeLiteral",
        "primitives",
        "rest",
        "specifyingTypes",
        "spread",
        "stringLiteral",
        "thisType",
        "tuple",
        "typeAliases",
        "typeParameters",
        "typeRelationships",
        "union",
        "uniqueSymbol",
        "unknown",
        "witness",
    ];
    let total: usize = by_dir
        .iter()
        .map(|d| {
            let line = listing(d);
            let n = line.split(": ").nth(1).and_then(|s| s.split(' ').next());
            n.and_then(|n| n.parse::<usize>().ok()).expect(&line)
        })
        .sum();
    assert_eq!(total, 857);
}
