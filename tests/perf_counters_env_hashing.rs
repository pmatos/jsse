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
