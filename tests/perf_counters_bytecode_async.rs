//! An async function whose body the compiler can host runs as a suspending
//! bytecode chunk (#1105): its awaits and resumptions do no tree-walker work,
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
    bail.contains("async state machine") || bail.contains("tree-walked")
}

fn driver(function: &str) -> String {
    format!(
        "{function}\n\
         var total = 0;\n\
         main({ITERATIONS}).then(function (v) {{ total = v; }});"
    )
}

#[test]
fn hosted_async_function_resumes_without_tree_walker_work() {
    let report = run(&driver(
        "async function main(n) { var s = 0; for (let i = 0; i < n; i++) { s += await i; } return s; }",
    ));
    assert_eq!(report.perf["ast_work_units"], 0, "no tree-walker work");
    assert!(
        report.perf["body_non_function_execs"] > ITERATIONS,
        "every resumption is an execution of the chunk"
    );
    assert!(
        !report.bails.iter().any(|b| keeps_state_machine(b)),
        "{:?}",
        report.bails
    );
}

#[test]
fn async_function_with_for_await_keeps_the_state_machine() {
    let report = run(&driver(
        "async function main(n) { var s = 0; for await (const v of [1, 2, 3]) { s += v; } return s; }",
    ));
    assert!(
        report.perf["ast_work_units"] > 0,
        "the state machine's payloads run on the tree-walker"
    );
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

#[test]
fn hosted_async_function_with_a_hand_off_node_counts_only_that_node() {
    let report = run(&driver(
        "async function main(n) { class C {} var s = 0; for (let i = 0; i < n; i++) { s += await i; } return s; }",
    ));
    assert!(
        report.perf["ast_work_units"] < ITERATIONS / 10,
        "hand-off node cost only: {}",
        report.perf["ast_work_units"]
    );
}
