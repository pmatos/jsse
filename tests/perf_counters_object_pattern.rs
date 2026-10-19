//! Plain-name `let`/`const` object binding patterns run as VM ops, not
//! tree-walker hand-offs (#1101); defaults, rest, computed keys and nested
//! patterns keep their tree-walker lowering.
#![cfg(feature = "perf-counters")]

use std::collections::HashMap;
use std::process::Command;

fn counters(source: &str) -> (HashMap<String, u64>, HashMap<String, u64>) {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["-e", source])
        .output()
        .expect("run jsse");
    assert!(
        out.status.success(),
        "jsse failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut perf = HashMap::new();
    let mut ops = HashMap::new();
    for line in stderr.lines() {
        let mut cols = line.split('\t');
        match (cols.next(), cols.next(), cols.next()) {
            (Some("PERF"), Some(key), Some(value)) => {
                if let Ok(v) = value.parse::<u64>() {
                    perf.insert(key.to_string(), v);
                }
            }
            (Some("OP"), Some(op), Some(count)) => {
                ops.insert(op.to_string(), count.parse().expect("op count"));
            }
            _ => {}
        }
    }
    (perf, ops)
}

fn loop_of(declaration: &str) -> String {
    format!(
        "var src = {{ a: 1, b: 2, c: 3, \"d e\": 4 }};\n\
         function main() {{\n\
           var s = 0;\n\
           for (var i = 0; i < 50; i++) {{ {declaration} s += 1; }}\n\
           return s;\n\
         }}\n\
         main();"
    )
}

#[test]
fn plain_name_patterns_stay_in_the_vm() {
    for (declaration, dups) in [
        ("const { a } = src;", 0),
        ("const { a, b } = src;", 1),
        ("let { a, b: bb, \"d e\": de } = src;", 2),
        ("const { a, b, c } = src, { b: bb } = src;", 2),
        ("{ const { a, b } = src; s += b; }", 1),
    ] {
        let (perf, ops) = counters(&loop_of(declaration));
        assert_eq!(perf["ast_fallback_execs"], 0, "{declaration}: hand-offs");
        assert_eq!(
            ops.get("DupN").copied().unwrap_or(0),
            50 * dups,
            "{declaration}: duplicated sources per iteration"
        );
    }
}

#[test]
fn complex_patterns_stay_on_the_tree_walker() {
    for declaration in [
        "const {} = src;",
        "const { a = 1 } = src;",
        "const { a, ...rest } = src;",
        "const { [\"a\"]: a } = src;",
        "const { b: { x } = { x: 0 }, a } = src;",
        "const { e: [x] = [0], a } = src;",
    ] {
        let (perf, ops) = counters(&loop_of(declaration));
        assert_eq!(
            perf["ast_fallback_execs"], 50,
            "{declaration}: one per iteration"
        );
        assert!(!ops.contains_key("DupN"), "{declaration}");
    }
}
