//! Calls and `new` with spread arguments run as VM ops, not tree-walker
//! hand-offs (#1100); `super(...)`, optional calls, direct `eval` and strict
//! tail calls keep their tree-walker lowering.
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

fn loop_of(body: &str) -> String {
    format!(
        "function g(a, b, c) {{ return a; }}\n\
         function C(a) {{ this.a = a; }}\n\
         function main() {{\n\
           var args = [1, 2, 3], s = 0;\n\
           for (var i = 0; i < 50; i++) {{ {body} }}\n\
           return s;\n\
         }}\n\
         main();"
    )
}

#[test]
fn spread_calls_and_new_stay_in_the_vm() {
    for body in [
        "s += g(...args);",
        "s += g(i, ...args, i);",
        "s += g(...[], ...args);",
        "s += new C(...args).a;",
        "s += [].push(...args);",
    ] {
        let (perf, ops) = counters(&loop_of(body));
        assert_eq!(perf["ast_fallback_execs"], 0, "{body}: hand-offs");
        let spread_ops: u64 = [
            "CallSpread",
            "ConstructSpread",
            "CallSpreadSole",
            "ConstructSpreadSole",
        ]
        .iter()
        .filter_map(|op| ops.get(*op))
        .sum();
        assert_eq!(spread_ops, 50, "{body}: one spread call op per iteration");
    }
}

#[test]
fn unsupported_spread_forms_stay_on_the_tree_walker() {
    let source = "class A { constructor(...a) { this.n = a.length; } }\n\
                  class B extends A { constructor(...a) { super(...a); } }\n\
                  function f() { return 1; }\n\
                  function main() {\n\
                    var args = [1, 2], s = 0, o = { f: f };\n\
                    for (var i = 0; i < 20; i++) { s += new B(1, 2).n; s += o.f?.(...args); }\n\
                    return s;\n\
                  }\n\
                  main();";
    let (perf, ops) = counters(source);
    assert!(
        perf["ast_fallback_execs"] > 0,
        "super(...) and ?.(...) hand off"
    );
    for op in [
        "CallSpread",
        "ConstructSpread",
        "CallSpreadSole",
        "ConstructSpreadSole",
    ] {
        assert!(
            !ops.contains_key(op),
            "{op}: the only spreads are super(...) and ?.(...) forms"
        );
    }
}
