//! The long tail of #1523 under `--node` (fourth pass): V8's wording of the delete of a
//! private name, escaped words, escapes that name no identifier, class heritage, `break`
//! and `continue`, patterns in strict code, regular-expression reasons and more, in the
//! bytecode VM, the tree-walker and under GC stress.
//! `scripts/node-stack-fixtures-1523-final/expected-final.txt` is Node v26's own output
//! for the driver of `scripts/node-stack-fixtures-1523-tail/` over `final.tsv`; see
//! docs/agents/library-tests-typescript-conformance-final-1523.md. Default mode keeps the
//! engine's own wording.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

const CONFIGS: [(&[&str], Option<&str>); 4] = [
    (&[], None),
    (&["--no-bytecode"], None),
    (&[], Some("7")),
    (&["--no-bytecode"], Some("7")),
];

#[test]
fn early_error_wording_matches_v8() {
    let want = std::fs::read_to_string(
        root().join("scripts/node-stack-fixtures-1523-final/expected-final.txt"),
    )
    .expect("snapshot");
    for (flags, stress) in CONFIGS {
        let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
        command
            .arg("--node")
            .args(flags)
            .arg(root().join("scripts/node-stack-fixtures-1523-tail/syntax-tail.mjs"))
            .arg(root().join("scripts/node-stack-fixtures-1523-final/final.tsv"));
        if let Some(period) = stress {
            command.env("JSSE_GC_STRESS", period);
        }
        let out = command.output().expect("run jsse");
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

fn default_mode(code: &str, flags: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(flags)
        .args(["-e", code])
        .output()
        .expect("run jsse");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn default_mode_keeps_its_own_wording_and_acceptance() {
    let probe = r#"
        var r = [];
        function early(src) {
          try { new Function(src); r.push("parses"); } catch (e) { r.push(e.message); }
        }
        early("break = 1");
        early("continue = 1");
        early("class C extends () => {} {}");
        early("class C extends (o) => [o.#foo] { #foo; }");
        early("class C { m() { delete (this.#m); } #m; }");
        early("class C { x = delete (this.#m); }");
        early("'use strict'; while (0) function f() {}");
        early("({var x})");
        early("for ('' in {} ? 0 : 0; false; ) ;");
        early("class C { #f; m() { for (#f in []) ; } }");
        early("var C = class l\\u0065t {};");
        early("function* g() { void yi\\u0065ld; }");
        early("/(?i)/");
        early("/(?<a>.)\\k<a/");
        early("'use strict'; ({ eval = 0 } = {})");
        early("'use strict'; for ([eval] of []) ;");
        early("\\u0023!");
        early("0\\u00620");
        early("var af = ()\n=> {};");
        early("with ({}) class C {}");
        early("async () => { class C { async * constructor() {} } }");
        print(r.join("|"));
    "#;
    let node_wording = [
        "Unexpected token '='",
        "Unexpected token '=>'",
        "Private field '#m' must be declared in an enclosing class",
        "Unexpected identifier 'x'",
        "Unexpected identifier '#field'",
        "Unexpected identifier 'yield'",
        "Invalid group",
        "Invalid capture group name",
        "Unexpected eval or arguments in strict mode",
        "Invalid or unexpected token",
        "Unexpected token 'class'",
        "In strict mode code, functions can only be declared at top level or inside a block.",
    ];
    for flags in [&[][..], &["--no-bytecode"][..]] {
        let got = default_mode(probe, flags);
        for v8 in node_wording {
            assert!(!got.contains(v8), "{v8}: {got}");
        }
        for own in [
            "Illegal break statement",
            "Illegal continue statement",
            "Arrow function is not allowed in class heritage",
            "Applying the 'delete' operator to a private name is not allowed",
            "Declaration not allowed in statement position of iteration statement",
            "Invalid modifier group",
            "Invalid named reference",
            "Invalid destructuring target",
            "Unexpected token #f",
            "Assignment to 'eval' in strict mode",
            "Invalid Unicode escape in identifier",
            "Identifier directly after number",
            "Class declaration not allowed in statement position",
        ] {
            assert!(got.contains(own), "{own}: {got}");
        }
        assert_eq!(
            got.matches("parses").count(),
            0,
            "every probe is a syntax error: {got}"
        );
    }
}
