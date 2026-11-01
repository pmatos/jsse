//! V8's wording of the destructuring, rest, `let`, `const`, module-export and
//! regular-expression early errors under `--node` (issue #1523), in the
//! bytecode VM, the tree-walker and under GC stress.
//! `scripts/node-stack-fixtures-1523-tail/expected-tail-N.txt` is Node v26's own
//! output for `syntax-tail.mjs tail-N.tsv` (one source per line, `script` or
//! `module` goal); see docs/agents/library-tests-typescript-conformance-parser-messages-1523.md.

use std::path::PathBuf;
use std::process::Command;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/node-stack-fixtures-1523-tail")
}

fn run(part: usize, flags: &[&str], stress: Option<&str>) -> String {
    let dir = fixtures();
    let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
    command
        .arg("--node")
        .args(flags)
        .arg(dir.join("syntax-tail.mjs"))
        .arg(dir.join(format!("tail-{part}.tsv")));
    if let Some(period) = stress {
        command.env("JSSE_GC_STRESS", period);
    }
    let out = command.output().expect("run jsse");
    assert!(
        out.status.success(),
        "part {part} {flags:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn check(part: usize) {
    let want = std::fs::read_to_string(fixtures().join(format!("expected-tail-{part}.txt")))
        .expect("snapshot");
    for (flags, stress) in [
        (&[][..], None),
        (&["--no-bytecode"][..], None),
        (&[][..], Some("7")),
        (&["--no-bytecode"][..], Some("7")),
    ] {
        assert_eq!(
            run(part, flags, stress).trim_end(),
            want.trim_end(),
            "part {part} {flags:?} GC stress {stress:?}"
        );
    }
}

#[test]
fn pattern_and_rest_wording_matches_v8() {
    check(1);
}

#[test]
fn arrow_object_and_class_wording_matches_v8() {
    check(2);
}

#[test]
fn statement_and_expression_wording_matches_v8() {
    check(3);
}

#[test]
fn module_and_class_wording_matches_v8() {
    check(4);
}

#[test]
fn regexp_template_and_strict_wording_matches_v8() {
    check(5);
}

fn default_messages(source: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["-e", source])
        .output()
        .expect("run jsse");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn default_mode_keeps_its_own_wording() {
    let text = default_messages(
        r#"
        for (const s of [
          "var [a.b] = c", "var {1}", "function f(...a, b) {}", "(...a) + 1",
          "for (let x, y of c);", "for (let x, y in c);", "class let {}", "import()", "export 1",
          "const a, b = 1", "/(?ii:a)/", "/[\\d-a]/u",
        ]) {
          try { new Function(s); } catch (e) { console.log(e.message); }
        }
        try { new RegExp("(?<=a)+"); } catch (e) { console.log(e.message); }
        try { new RegExp("a{1", "u"); } catch (e) { console.log(e.message); }
        "#,
    );
    for own in [
        "Expected RightBracket, got Dot",
        "Expected identifier for shorthand pattern",
        "Expected RightParen, got Comma",
        "Expected Arrow, got Plus",
        "for-of loop variable declaration may not have an initializer",
        "for-in loop variable declaration may not have an initializer",
        "Invalid flags in modifier group",
        "Invalid class escape in range",
        "Expected class name",
        "Unexpected token: RightParen",
        "Nothing to repeat",
        "Lone quantifier brackets",
    ] {
        assert!(text.contains(own), "{own}: {text}");
    }
    for v8 in [
        "Illegal property in declaration context",
        "Rest parameter must be last formal parameter",
        "Must have a single binding",
        "Repeated flag in flag group",
        "Invalid character class",
        "Unexpected strict mode reserved word",
        "import() requires a specifier",
        "Invalid quantifier",
        "Incomplete quantifier",
    ] {
        assert!(!text.contains(v8), "{v8}: {text}");
    }
}
