//! An async generator whose body the compiler can host runs as a suspending
//! bytecode chunk (#1105): its yields, awaits and resumptions do no tree-walker work,
//! while a body it cannot host keeps the state machine and is reported as a
//! bail. The counters are per-process, so these are child-process checks.
#![cfg(feature = "perf-counters")]

use std::collections::HashMap;
use std::process::Command;

struct Report {
    perf: HashMap<String, u64>,
    bails: Vec<String>,
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
            _ => {}
        }
    }
    report
}

const ITERATIONS: u64 = 500;

fn keeps_state_machine(bail: &str) -> bool {
    bail.contains("async generator state machine") || bail.contains("tree-walked")
}

fn driver(generator: &str) -> String {
    format!(
        "{generator}\n\
         async function main(n) {{\n\
           var it = g(n), s = 0;\n\
           while (true) {{ var r = await it.next(); if (r.done) break; s += r.value; }}\n\
           return s;\n\
         }}\n\
         main({ITERATIONS});"
    )
}

#[test]
fn hosted_async_generator_runs_without_tree_walker_work_in_its_own_body() {
    let report = run(&driver(
        "async function* g(n) { for (let i = 0; i < n; i++) { const sq = i * i; yield sq & 7; await i; } }",
    ));
    assert_eq!(report.perf["ast_work_units"], 0, "no tree-walker work");
    assert!(
        report.perf["body_non_function_execs"] > 2 * ITERATIONS,
        "every resumption is an execution of the chunk"
    );
    assert!(
        !report
            .bails
            .iter()
            .any(|b| b.contains("async generator state machine")),
        "{:?}",
        report.bails
    );
}

#[test]
fn async_generator_with_yield_star_keeps_the_state_machine() {
    let report = run(&driver(
        "async function* g(n) { for (let i = 0; i < n; i++) { yield* [i & 7]; } }",
    ));
    assert!(report.perf["ast_work_units"] > 0);
    assert_eq!(
        report
            .bails
            .iter()
            .filter(|b| keeps_state_machine(b))
            .count(),
        1,
        "{:?}",
        report.bails
    );
}
