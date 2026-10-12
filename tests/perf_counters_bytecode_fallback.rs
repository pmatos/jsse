//! One construct the VM cannot lower natively must cost only that node, not
//! the whole function (#902).
//!
//! Each micro is the `bench_numeric_bail_*` shape: a hot numeric loop plus one
//! statement that never executes. Before per-node fallback every one of them
//! sent the whole `kernel` body to the tree-walker. The counters are
//! per-process, so these are child-process checks.
#![cfg(feature = "perf-counters")]

use std::collections::HashMap;
use std::process::Command;

struct Report {
    perf: HashMap<String, u64>,
    bails: Vec<String>,
    fallbacks: Vec<String>,
    bodies: HashMap<String, String>,
}

fn run(source: &str) -> Report {
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
    let mut report = Report {
        perf: HashMap::new(),
        bails: Vec::new(),
        fallbacks: Vec::new(),
        bodies: HashMap::new(),
    };
    for line in stderr.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        match cols.as_slice() {
            ["PERF", key, value] => {
                if let Ok(v) = value.parse::<u64>() {
                    report.perf.insert((*key).to_string(), v);
                }
            }
            ["BAIL", reason, _] => report.bails.push((*reason).to_string()),
            ["FALLBACK", reason, _] => report.fallbacks.push((*reason).to_string()),
            ["BODY", name, _, _, _, reason] => {
                report
                    .bodies
                    .insert((*name).to_string(), (*reason).to_string());
            }
            _ => {}
        }
    }
    report
}

fn kernel(extra: &str, iterations: u32) -> String {
    format!(
        "function kernel(n, o) {{\n\
           var s = 0, x = 1;\n\
           {extra}\n\
           for (var i = 0; i < n; i++) {{\n\
             x = (x * 1103515245 + 12345) & 0x7fffffff;\n\
             s = (s + (x >> 7) + (i & 15)) | 0;\n\
             s = s ^ (x >>> 3);\n\
             if (s > 100000000) s = s - 100000000;\n\
           }}\n\
           return s;\n\
         }}\n\
         kernel({iterations}, {{ t: 0 }});"
    )
}

/// The construct each `bench_numeric_bail_*` repro adds, plus constructs with
/// no native lowering yet. All must leave `kernel` compiled; those placed
/// outside a guard run their fallback node once, never per iteration.
const NEVER_EXECUTED: &[(&str, &str)] = &[
    ("switch", "switch (n) { case -1: s = 1; break; }"),
    ("throw", "if (n < 0) throw new Error('negative');"),
    ("typeof", "if (typeof n !== 'number') return 0;"),
    ("memberupdate", "if (n < 0) o.t++;"),
    ("letconst", "const k = 7;"),
    ("arrayliteral", "var tbl = [1, 2, 3];"),
    ("objectliteral", "var obj = { a: 1 };"),
    (
        "trycatch",
        "if (n < 0) { try { s = 1; } catch (e) { s = 2; } }",
    ),
    ("instanceof", "if (o instanceof Array) s = 1;"),
    ("in", "if ('t' in o) s = s | 0;"),
    ("bigint", "if (n < 0) s = Number(10n ** 3n);"),
    ("forof", "for (var v of [1, 2]) { s = v; }"),
];

#[test]
fn one_unsupported_node_does_not_send_the_function_to_the_tree_walker() {
    for (label, extra) in NEVER_EXECUTED {
        let r = run(&kernel(extra, 2_000));
        assert_eq!(
            r.perf["body_dispatch_ast"], 0,
            "{label}: kernel must not run on the tree-walker; bails: {:?}",
            r.bails
        );
        assert_eq!(
            r.perf["body_dispatch_compiled"], 1,
            "{label}: kernel must be dispatched as a compiled body"
        );
        assert!(
            r.perf["ast_fallback_execs"] <= 3,
            "{label}: a fallback node outside the loop runs a constant number of times, got {}",
            r.perf["ast_fallback_execs"]
        );
        let kernel_row = r.bodies.get("kernel").map(String::as_str);
        assert!(
            matches!(kernel_row, None | Some("-")),
            "{label}: kernel's BODY row must name no bail reason, got {kernel_row:?}"
        );
    }
}

#[test]
fn natively_lowered_constructs_leave_no_fallback_nodes() {
    for (label, extra) in [
        ("throw", "if (n < 0) throw new Error('negative');"),
        ("typeof", "if (typeof n !== 'number') return 0;"),
        ("memberupdate", "if (n < 0) { o.t++; o['t']--; --o.t; }"),
        ("instanceof", "if (o instanceof Array) s = 1;"),
        ("in", "if ('t' in o) s = s | 0;"),
        ("bigint", "if (n < 0) s = Number(10n ** 3n);"),
    ] {
        let r = run(&kernel(extra, 2_000));
        assert!(
            r.fallbacks.is_empty(),
            "{label}: expected a fully native body, got fallbacks {:?}",
            r.fallbacks
        );
    }
}

#[test]
fn a_hot_fallback_node_is_reported_as_fallback_work_not_an_ast_body() {
    let r = run(&kernel("", 10).replace(
        "return s;",
        "for (var k = 0; k < 50; k++) { try { s += k; } finally { s ^= 1; } } return s;",
    ));
    assert_eq!(r.perf["body_dispatch_ast"], 0);
    assert!(
        r.perf["ast_fallback_execs"] >= 50,
        "ast_fallback_execs: {}",
        r.perf["ast_fallback_execs"]
    );
    assert!(
        r.perf["ast_units_in_fallbacks"] > 0,
        "fallback work must be attributed"
    );
    assert_eq!(
        r.perf["ast_units_in_functions"], 0,
        "fallback nodes are not AST-body invocations"
    );
}

#[test]
fn never_executed_fallback_node_adds_no_per_call_hashing() {
    let call_loop = |calls: u32| {
        format!(
            "function f(a) {{ var t = a + 1; if (a < 0) {{ switch (a) {{ case 1: t = 2; }} }} return t; }}\n\
             function main() {{ var s = 0; for (var i = 0; i < {calls}; i++) {{ s = f(s & 7); }} return s; }}\n\
             main();"
        )
    };
    let small = run(&call_loop(1_000));
    let large = run(&call_loop(2_000));
    for key in ["atom_intern_calls", "atom_lookup_calls"] {
        assert_eq!(
            small.perf[key], large.perf[key],
            "{key} must not scale with the number of calls"
        );
    }
    assert!(large.perf["calls_to_user"] > small.perf["calls_to_user"]);
}

#[test]
fn constructs_that_need_a_tree_walked_frame_stay_ineligible() {
    for (label, extra) in [
        ("with", "with (o) { s = 1; }"),
        ("directeval", "if (n < 0) eval('s = 1');"),
    ] {
        let r = run(&kernel(extra, 100));
        assert_eq!(
            r.perf["body_dispatch_ast"], 1,
            "{label}: kernel must stay on the tree-walker"
        );
        assert!(
            r.bodies.get("kernel").is_some_and(|reason| reason != "-"),
            "{label}: BODY row must name the reason"
        );
    }
}
