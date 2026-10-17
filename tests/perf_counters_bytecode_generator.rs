//! A sync generator whose body the compiler can host runs as a suspending
//! bytecode chunk (#1031): its resumptions do no tree-walker work, while a
//! body it cannot host keeps the state machine and is reported as a bail.
//! The counters are per-process, so these are child-process checks.
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

fn consumer(generator: &str) -> String {
    format!(
        "{generator}\n\
         function main(n) {{ var s = 0; for (const v of g(n)) s += v; return s; }}\n\
         main({ITERATIONS});"
    )
}

#[test]
fn hosted_generator_resumes_without_tree_walker_work() {
    let report = run(&consumer(
        "function* g(n) { for (let i = 0; i < n; i++) { const sq = i * i; yield sq & 7; } }",
    ));
    assert_eq!(report.perf["ast_work_units"], 0, "no tree-walker work");
    assert!(
        report.perf["body_non_function_execs"] > ITERATIONS,
        "every resumption is an execution of the chunk"
    );
    assert!(
        !report
            .bails
            .iter()
            .any(|b| b.contains("generator state machine")),
        "{:?}",
        report.bails
    );
}

#[test]
fn generator_with_a_yield_the_chunk_cannot_host_keeps_the_state_machine() {
    let report = run(&consumer(
        "function* g(n) { for (let i = 0; i < n; i++) { yield* [i & 7]; } }",
    ));
    assert!(
        report.perf["ast_work_units"] > ITERATIONS,
        "the state machine's payloads run on the tree-walker"
    );
    assert_eq!(
        report
            .bails
            .iter()
            .filter(|b| b.contains("generator state machine"))
            .count(),
        1,
        "{:?}",
        report.bails
    );
}

#[test]
fn hosted_generator_with_a_hand_off_node_counts_only_that_node() {
    // The class declaration has no native lowering and runs once on the
    // tree-walker; the loop and its yields stay native.
    let report = run(&consumer(
        "function* g(n) { class C {} for (let i = 0; i < n; i++) { yield i & 7; } }",
    ));
    assert!(
        report.perf["ast_work_units"] < ITERATIONS / 10,
        "hand-off node cost only: {}",
        report.perf["ast_work_units"]
    );
}
