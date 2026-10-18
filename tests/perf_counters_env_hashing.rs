//! A compiled user-function call must not hash or intern identifier text, and
//! frame-local names must resolve through slot ops (#898).
//!
//! The atom interner is the only place script-controlled names are hashed, so
//! `atom_intern_calls` / `atom_lookup_calls` staying flat while the call count
//! grows is the observable proof that the call path is hash-free. These are
//! child-process checks because the counters are per-process.
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

fn call_loop(iterations: u32) -> String {
    format!(
        "function add(a, b) {{ var t = a + b; return t; }}\n\
         function main() {{ var s = 0; for (var i = 0; i < {iterations}; i++) {{ s = add(s, i & 7); }} return s; }}\n\
         main();"
    )
}

#[test]
fn call_path_interns_and_looks_up_nothing_per_call() {
    let (small, _) = counters(&call_loop(1_000));
    let (large, _) = counters(&call_loop(2_000));
    for key in ["atom_intern_calls", "atom_lookup_calls"] {
        assert_eq!(
            small[key], large[key],
            "{key} must not scale with the number of calls"
        );
    }
    assert!(
        large["calls_to_user"] > small["calls_to_user"],
        "the larger run must actually make more calls"
    );
}

#[test]
fn frame_locals_resolve_through_slots() {
    let (perf, ops) = counters(&call_loop(1_000));
    assert!(
        perf["local_slot_hits"] > 2_000,
        "hits: {}",
        perf["local_slot_hits"]
    );
    assert_eq!(perf["local_slot_misses"], 0, "no hint should miss");
    assert!(ops.contains_key("LoadLocal"));
    assert!(ops.contains_key("StoreLocalPop"));
    assert!(!ops.contains_key("ResolveName"));
    assert!(!ops.contains_key("StoreResolvedName"));
    assert!(!ops.contains_key("StoreResolvedNamePop"));
}

/// Dispatch budget for fused effect-context ops (#903): the `arith` loop of
/// `benchmarks/scripts/bench_opmix.js` must dispatch at most 25 counted VM ops
/// per iteration (43 before statement-context assignment/update fusion,
/// constant folding and compare+branch fusion). The per-iteration figure is the
/// difference between two run lengths, so setup and teardown ops cancel out.
#[test]
fn arith_loop_dispatches_at_most_25_ops_per_iteration() {
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
        counters(&source).0["vm_ops"]
    }
    let (n, m) = (1_000u32, 2_000u32);
    let per_iteration = (vm_ops(m) - vm_ops(n)) / u64::from(m - n);
    assert!(
        per_iteration <= 25,
        "arith loop dispatches {per_iteration} ops per iteration"
    );
}

/// Function-top-level `let`/`const`/destructured `const` names are interned
/// once per Body, not once per call (#1052). A class declaration is left out:
/// evaluating one still interns its binding name on the tree-walker.
#[test]
fn function_top_level_lexicals_intern_nothing_per_call() {
    fn lexical_loop(iterations: u32) -> String {
        format!(
            "function f(o, a) {{\n\
               let r = a + 1;\n\
               const s = r * 2;\n\
               const {{ x, y }} = o;\n\
               return s + x + y;\n\
             }}\n\
             function main() {{ var o = {{ x: 1, y: 2 }}, t = 0;\n\
               for (var i = 0; i < {iterations}; i++) {{ t = (t + f(o, i & 7)) | 0; }}\n\
               return t; }}\n\
             main();"
        )
    }
    let (small, _) = counters(&lexical_loop(1_000));
    let (large, _) = counters(&lexical_loop(2_000));
    let extra_interns = large["atom_intern_calls"] - small["atom_intern_calls"];
    assert!(
        extra_interns <= 8,
        "{extra_interns} extra interns over 1,000 extra calls of a body with four lexical names"
    );
    assert!(large["calls_to_user"] > small["calls_to_user"]);
}

/// Function-top-level `let`/`const` bindings are part of the frame layout, so
/// reading and writing them never resolves a name (#1022), even beside hoisted
/// function declarations.
#[test]
fn function_top_level_lexicals_resolve_through_slots() {
    let (perf, ops) = counters(
        "function main(n) {\n\
           function twice(v) { return v * 2; }\n\
           let sum = 0;\n\
           const mask = 31;\n\
           for (let i = 0; i < n; i++) { sum += i & mask; }\n\
           sum += mask;\n\
           return sum;\n\
         }\n\
         main(1000);",
    );
    assert_eq!(perf["local_slot_misses"], 0, "no hint should miss");
    for op in ["LoadName", "ResolveName", "StoreResolvedNamePop"] {
        assert!(
            !ops.contains_key(op),
            "{op} executed for a frame-local name"
        );
    }
}

/// Parameters with defaults or a rest element bind through the literal's
/// atoms, and a body that shares the parameter environment reads them through
/// frame slots, so repeated calls intern nothing and parameter reads are
/// `LoadLocal` exactly as for simple parameters (#1024).
#[test]
fn default_and_rest_parameters_intern_nothing_and_resolve_through_slots() {
    fn default_loop(iterations: u32) -> String {
        format!(
            "function add(a, b = 1, ...rest) {{ var t = a + b; return t + rest.length; }}\n\
             function main() {{ var s = 0; for (var i = 0; i < {iterations}; i++) {{ s = (s + add(i & 7) + add(i & 3, 2, 5)) | 0; }} return s; }}\n\
             main();"
        )
    }
    let (small, small_ops) = counters(&default_loop(1_000));
    let (large, _) = counters(&default_loop(2_000));
    let extra_interns = large["atom_intern_calls"] - small["atom_intern_calls"];
    assert!(
        extra_interns <= 8,
        "{extra_interns} extra interns over 1,000 extra iterations of two calls with default and rest parameters"
    );
    assert!(large["calls_to_user"] > small["calls_to_user"]);
    assert_eq!(small["local_slot_misses"], 0, "no hint should miss");
    assert!(
        !small_ops.contains_key("LoadName"),
        "a parameter or var was read by name"
    );
}
