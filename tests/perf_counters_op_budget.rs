//! Dispatch budget for fused effect-context ops (#903): the `arith` loop of
//! `benchmarks/scripts/bench_opmix.js` must dispatch at most 25 counted VM ops
//! per iteration (43 before statement-context assignment/update fusion, constant
//! folding and compare+branch fusion).
//!
//! Child-process check because the counters are per-process; the per-iteration
//! figure is the difference between two run lengths, so setup and teardown ops
//! cancel out.
#![cfg(feature = "perf-counters")]

use std::process::Command;

fn vm_ops(iterations: u32) -> u64 {
    let source = format!(
        "function arith(n) {{\n\
           var r0 = 0, r1 = 1, r2 = 2;\n\
           for (var i = 0; i < n; i++) {{\n\
             r0 = (r0 + 4) | 0;\n\
             r1 = (r0 + r1) | 0;\n\
             r1 = r1 & 65535;\n\
             r2 = r1 >> 2;\n\
             r0 = (r2 + r0) | 0;\n\
             r0 = r0 & 65535;\n\
           }}\n\
           return r0 + r1 + r2;\n\
         }}\n\
         arith({iterations});"
    );
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["-e", &source])
        .output()
        .expect("run jsse");
    assert!(
        out.status.success(),
        "jsse failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .find_map(|line| {
            let mut cols = line.split('\t');
            match (cols.next(), cols.next(), cols.next()) {
                (Some("PERF"), Some("vm_ops"), Some(v)) => v.parse().ok(),
                _ => None,
            }
        })
        .expect("vm_ops counter")
}

#[test]
fn arith_loop_dispatches_at_most_25_ops_per_iteration() {
    let (n, m) = (1_000u32, 2_000u32);
    let per_iteration = (vm_ops(m) - vm_ops(n)) / u64::from(m - n);
    assert!(
        per_iteration <= 25,
        "arith loop dispatches {per_iteration} ops per iteration"
    );
}
