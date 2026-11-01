//! What #1523 closes under `--node`: V8's early-error wording and acceptance for `using`,
//! `import defer`, a call as an assignment target, `for await` patterns, class names,
//! `#constructor` and more, in the bytecode VM, the tree-walker and under GC stress.
//! `scripts/node-stack-fixtures-1523-residual/expected-residual.txt` is Node v26's own
//! output for the driver of `scripts/node-stack-fixtures-1523-tail/` over `residual.tsv`;
//! `tests/node-v8-messages-1523/*.out` is Node's output for the fixtures beside it
//! (`scripts/run-node-v8-messages-1523-oracle.sh` regenerates and cross-checks both
//! kinds). Default mode keeps the engine's own wording.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn jsse(flags: &[&str], stress: Option<&str>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
    command.arg("--node").args(flags);
    if let Some(period) = stress {
        command.env("JSSE_GC_STRESS", period);
    }
    for name in ["FORCE_COLOR", "NO_COLOR", "NODE_DISABLE_COLORS"] {
        command.env_remove(name);
    }
    command
}

const CONFIGS: [(&[&str], Option<&str>); 4] = [
    (&[], None),
    (&["--no-bytecode"], None),
    (&[], Some("7")),
    (&["--no-bytecode"], Some("7")),
];

#[test]
fn early_error_wording_and_acceptance_match_v8() {
    let want = std::fs::read_to_string(
        root().join("scripts/node-stack-fixtures-1523-residual/expected-residual.txt"),
    )
    .expect("snapshot");
    for (flags, stress) in CONFIGS {
        let out = jsse(flags, stress)
            .arg(root().join("scripts/node-stack-fixtures-1523-tail/syntax-tail.mjs"))
            .arg(root().join("scripts/node-stack-fixtures-1523-residual/residual.tsv"))
            .output()
            .expect("run jsse");
        assert!(
            out.status.success(),
            "{flags:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim_end(),
            want.trim_end(),
            "{flags:?} GC stress {stress:?}"
        );
    }
}

#[test]
fn call_targets_throw_like_v8() {
    let dir = root().join("tests/node-v8-messages-1523");
    let want = std::fs::read_to_string(dir.join("call-targets.out")).unwrap();
    for (flags, stress) in CONFIGS {
        let out = jsse(flags, stress)
            .arg(dir.join("call-targets.js"))
            .current_dir(&dir)
            .output()
            .unwrap();
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        assert_eq!(text.trim_end(), want.trim_end(), "{flags:?} {stress:?}");
    }
}

fn default_mode(code: &str, flags: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(flags)
        .args(["-e", code])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn default_mode_keeps_its_own_behaviour() {
    let probe = r#"
        var r = [];
        function early(src) {
          try { new Function(src); r.push("parses"); } catch (e) { r.push(e.message); }
        }
        early("switch (1) { case 1: using x = null }");
        early("function f() { using x }");
        early("function f() { for (using x in []); }");
        early("'use strict'; f() = 1");
        early("'use strict'; f()++");
        early("'use strict'; for (f() of c);");
        early("var C = class yield {};");
        early("var C = class { #constructor() {} }");
        early("for (const let in {}) {}");
        early("async function f() { for await (const [...x = []] of y); }");
        early("return import.defer('./x.mjs')");
        function f() { return {}; }
        function run(fn) { try { fn(); r.push("no error"); } catch (e) { r.push(e.message); } }
        run(function () { f()++; });
        run(function () { --f(); });
        run(function () { for (f() of [1]); });
        run(function () { for (f() in { a: 1 }); });
        print(r.join("|"));
    "#;
    let node_wording = [
        "Unexpected identifier 'x'",
        "Missing initializer in using declaration",
        "Invalid 'using' in for-in loop",
        "Unexpected strict mode reserved word",
        "Class constructor may not be a private method",
        "let is disallowed as a lexically bound name",
        "Must have a single binding",
        "Invalid left-hand side in for-loop",
        "prefix operation",
        "postfix operation",
    ];
    for flags in [&[][..], &["--no-bytecode"][..]] {
        let got = default_mode(probe, flags);
        for v8 in node_wording {
            assert!(!got.contains(v8), "{v8}: {got}");
        }
        for own in [
            "using declaration is not allowed in this position",
            "using declaration requires an initializer",
            "Invalid left-hand side in assignment",
            "Invalid left-hand side expression in update expression",
            "Invalid left-hand side in for-of loop",
            "Invalid left-hand side in for-in loop",
            "Class fields and methods cannot be named '#constructor'",
        ] {
            assert!(got.contains(own), "{own}: {got}");
        }
        assert!(got.contains("parses"), "import.defer stays: {got}");
    }
}

#[test]
fn default_mode_suspending_loops_reject_a_call_target_like_plain_loops() {
    let probe = r#"
        var r = [];
        function f() { r.push("f"); return {}; }
        async function a() {
          try { for (f() of [1]) await 0; r.push("no error"); } catch (e) { r.push(e.message); }
          try { for (f() in { k: 1 }) await 0; r.push("no error"); } catch (e) { r.push(e.message); }
        }
        function* g() {
          try { for (f() of [1]) yield 0; r.push("no error"); } catch (e) { r.push(e.message); }
        }
        for (var _ of g());
        a().then(function () { print(r.join("|")); });
    "#;
    let expected = [
        "f",
        "Invalid left-hand side in for-in/of loop",
        "f",
        "Invalid left-hand side in for-in/of loop",
        "f",
        "Invalid left-hand side in for-in/of loop",
    ]
    .join("|");
    for flags in [&[][..], &["--no-bytecode"][..]] {
        assert_eq!(default_mode(probe, flags), expected, "{flags:?}");
    }
}
