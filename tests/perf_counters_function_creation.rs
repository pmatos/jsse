//! Creating a function object must not intern or hash property keys (#901).
//!
//! `length`, `name`, `caller`, `arguments`, `prototype` and `constructor` are
//! interned once per interpreter, so `key_intern_calls` staying flat while the
//! number of closures grows is the observable proof that closure creation is
//! hash-free. These are child-process checks because the counters are
//! per-process.
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

fn closure_loop(iterations: u32) -> String {
    format!(
        "function makeAdder(x) {{ return function (y) {{ return x + y; }}; }}\n\
         function main() {{ var s = 0; for (var i = 0; i < {iterations}; i++) {{ s += makeAdder(i & 15)(i & 255); }} return s; }}\n\
         main();"
    )
}

#[test]
fn closure_creation_interns_no_property_keys() {
    let small = counters(&closure_loop(1_000));
    let large = counters(&closure_loop(2_000));
    assert_eq!(
        small["key_intern_calls"], large["key_intern_calls"],
        "key_intern_calls must not scale with the number of closures"
    );
    assert!(
        large["calls_to_user"] > small["calls_to_user"],
        "the larger run must actually create and call more closures"
    );
}

#[test]
fn promise_capability_creation_interns_no_property_keys() {
    let promise_loop = |n: u32| {
        format!("for (var i = 0; i < {n}; i++) {{ new Promise(function (res) {{ res(i); }}); }}")
    };
    let small = counters(&promise_loop(500));
    let large = counters(&promise_loop(1_000));
    assert_eq!(
        small["key_intern_calls"], large["key_intern_calls"],
        "key_intern_calls must not scale with the number of promises"
    );
}

#[test]
fn named_function_expressions_and_hoisted_declarations_intern_no_names() {
    let named = |n: u32| {
        format!(
            "function mk(x) {{ return function inner(y) {{ return x + y; }}; }}\n\
             function main() {{ var s = 0; for (var i = 0; i < {n}; i++) {{ s += mk(i & 7)(i & 3); }} return s; }}\n\
             main();"
        )
    };
    let hoisted = |n: u32| {
        format!(
            "function mk(x) {{ function decl(y) {{ return x + y; }} return decl; }}\n\
             function main() {{ var s = 0; for (var i = 0; i < {n}; i++) {{ s += mk(i & 7)(i & 3); }} return s; }}\n\
             main();"
        )
    };
    for (label, make) in [
        (
            "named function expression",
            &named as &dyn Fn(u32) -> String,
        ),
        ("hoisted declaration", &hoisted),
    ] {
        let small = counters(&make(1_000));
        let large = counters(&make(2_000));
        assert_eq!(
            small["atom_intern_calls"], large["atom_intern_calls"],
            "{label}: atom_intern_calls must not scale with the number of closures"
        );
        assert!(large["calls_to_user"] > small["calls_to_user"]);
    }
}
