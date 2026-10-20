//! A loop body block whose `let`/`const` bindings nothing can capture or read
//! before they are initialised shares one environment for the whole loop, so
//! `PushEnv` runs once per loop entry instead of once per iteration (#1132).
//! A body that could observe the difference keeps its fresh environment per
//! iteration. These are child-process checks because the counters are
//! per-process.
#![cfg(feature = "perf-counters")]

use std::collections::HashMap;
use std::process::Command;

fn push_env_ops(source: &str) -> u64 {
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
    let ops: HashMap<&str, u64> = stderr
        .lines()
        .filter_map(|line| {
            let mut cols = line.split('\t');
            match (cols.next(), cols.next(), cols.next()) {
                (Some("OP"), Some(op), Some(count)) => Some((op, count.parse().ok()?)),
                _ => None,
            }
        })
        .collect();
    ops.get("PushEnv").copied().unwrap_or(0)
}

fn in_function(iterations: u32, loop_source: &str) -> String {
    format!(
        "function f(n) {{ var s = 0; var xs = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]; {loop_source} return s; }} \
         for (var k = 0; k < {iterations}; k++) f(10);",
    )
}

const MERGEABLE: &[&str] = &[
    "for (let i = 0; i < n; i++) { let t = i + 1; s = s + t; }",
    "for (var i = 0; i < n; i++) { const t = i + 1; s = s + t; }",
    "var i = 0; while (i < n) { let t = i + 1; s = s + t; i++; }",
    "var i = 0; do { let t = i + 1; s = s + t; i++; } while (i < n);",
    "for (const v of xs) { let t = v + 1; s = s + t; }",
    "for (var v of xs) { const o = { a: v }; const { a } = o; s = s + a; }",
];

const FRESH_PER_ITERATION: &[&str] = &[
    "for (let i = 0; i < n; i++) { let t = i + 1; if (s < 0) (() => t); s = s + t; }",
    "for (let i = 0; i < n; i++) { try { s = s + typeof t; } catch (e) {} let t = i; }",
    "for (let i = 0; i < n; i++) { try { let t = t; } catch (e) { s = s + 1; } }",
    "var t = 1; for (let i = 0; i < n && t; i++) { let t = i; s = s + t; }",
    "for (let i = 0; i < n; i++) { const [a, b] = [i, i]; s = s + a + b; }",
];

#[test]
fn mergeable_bodies_enter_their_environment_once_per_loop() {
    for loop_source in MERGEABLE {
        let small = push_env_ops(&in_function(10, loop_source));
        let large = push_env_ops(&in_function(30, loop_source));
        let per_call = (large - small) / 20;
        assert!(
            per_call <= 3,
            "`{loop_source}` enters {per_call} environments per call; each iteration must not"
        );
    }
}

#[test]
fn observable_bodies_keep_a_fresh_environment_per_iteration() {
    for loop_source in FRESH_PER_ITERATION {
        let small = push_env_ops(&in_function(10, loop_source));
        let large = push_env_ops(&in_function(30, loop_source));
        let per_call = (large - small) / 20;
        assert!(
            per_call >= 10,
            "`{loop_source}` must keep one environment per iteration, saw {per_call} per call"
        );
    }
}
