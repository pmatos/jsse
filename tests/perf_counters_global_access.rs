//! Script-level `var` reads and writes must take the global data fast path
//! (#905): one own-property probe instead of the generic Object Environment
//! Record sequence, observable as `global_prop_fast_hits` growing with the
//! iteration count while the generic fallback stays cold. Child-process
//! checks because the counters are per-process.
#![cfg(feature = "perf-counters")]

use std::collections::HashMap;
use std::process::Command;

fn counters(source: &str) -> HashMap<String, u64> {
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
    for line in stderr.lines() {
        let mut cols = line.split('\t');
        if let (Some("PERF"), Some(key), Some(value)) = (cols.next(), cols.next(), cols.next())
            && let Ok(v) = value.parse::<u64>()
        {
            perf.insert(key.to_string(), v);
        }
    }
    perf
}

fn global_loop(iterations: u32) -> String {
    format!(
        "var a = 1, b = 2, c = 0;\n\
         function step(n) {{ for (var i = 0; i < n; i++) {{ c = (a + b) & 255; a = c; b = b + 1; }} return c; }}\n\
         step({iterations});"
    )
}

#[test]
fn global_reads_and_writes_report_fast_path_counters() {
    let perf = counters(&global_loop(10));
    assert!(perf.contains_key("global_prop_fast_hits"));
    assert!(perf.contains_key("global_prop_fast_misses"));
}

#[test]
fn global_var_traffic_takes_the_fast_path() {
    let small = counters(&global_loop(1_000));
    let large = counters(&global_loop(2_000));
    let delta = large["global_prop_fast_hits"] - small["global_prop_fast_hits"];
    // Per iteration: reads of a, b, a(c = ...) and writes of c, a, b, plus b's
    // read-modify-write; at least five fast accesses.
    assert!(delta >= 5_000, "fast hits grew by only {delta}");
    assert_eq!(
        large["global_prop_fast_misses"], small["global_prop_fast_misses"],
        "the generic fallback must not scale with iterations"
    );
}

#[test]
fn accessor_global_never_counts_as_a_fast_hit() {
    let source = "var n = 0;\n\
         Object.defineProperty(globalThis, 'acc', { get() { return n; }, set(v) { n = v; }, configurable: true });\n\
         function step(k) { for (var i = 0; i < k; i++) { acc = acc + 1; } return n; }\n\
         step(500);";
    let before = counters("var n = 0; function step(k) { return k; } step(500);");
    let perf = counters(source);
    assert!(
        perf["global_prop_fast_misses"] >= before["global_prop_fast_misses"] + 1_000,
        "accessor reads and writes must fall back, misses: {}",
        perf["global_prop_fast_misses"]
    );
}
