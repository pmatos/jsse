//! `process.exit()` under `--node` ends the run at once even when it is called
//! from a callback of a builtin that does not forward `Completion::Exit`
//! (mocha's `--list-reporters` exits from inside `forEach`, #1256): nothing
//! after the call may run and the exit status is the code given.

use std::process::{Command, Output};

fn jsse_node(code: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["--node", "-e", code])
        .output()
        .expect("run jsse")
}

const CALLBACKS: &[(&str, &str)] = &[
    ("forEach", "[1, 2].forEach(x)"),
    ("sort comparator", "[2, 1].sort(x)"),
    ("toSorted comparator", "[2, 1].toSorted(x)"),
    ("typed array sort", "new Uint8Array([2, 1]).sort(x)"),
    ("JSON.stringify replacer", "JSON.stringify([1, 2], x)"),
    ("toJSON", "JSON.stringify({ toJSON: x })"),
    ("Promise executor", "new Promise(x)"),
    ("setter", "({ set a(v) { x(); } }).a = 1"),
    ("Symbol.toPrimitive", "`${{ [Symbol.toPrimitive]: x }}`"),
    ("valueOf", "+{ valueOf: x }"),
    ("proxy get", "new Proxy({}, { get: x }).a"),
    ("proxy has", "'a' in new Proxy({}, { has: x })"),
    ("proxy apply", "new Proxy(function () {}, { apply: x })()"),
    ("static block", "class A { static { x(); } }"),
    ("Object.groupBy", "Object.groupBy([1], x)"),
    ("iterator helper forEach", "[1].values().forEach(x)"),
    (
        "Object.entries getter",
        "Object.entries({ get a() { x(); } })",
    ),
    ("object spread getter", "({ ...{ get a() { x(); } } })"),
    (
        "structuredClone getter",
        "structuredClone({ get a() { x(); } })",
    ),
    ("class field", "class A { f = x(); } new A()"),
];

#[test]
fn process_exit_inside_a_builtin_callback_ends_the_run() {
    for (name, call) in CALLBACKS {
        let src = format!(
            "const x = () => {{ process.exit(3); }};\n\
             process.on('exit', c => console.log('exit event', c));\n\
             try {{ {call}; }} catch (e) {{ console.log('caught', e); }} finally {{ console.log('finally'); }}\n\
             console.log('after');"
        );
        let out = jsse_node(&src);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(3), "{name}: {stdout}");
        assert_eq!(stdout.trim_end(), "exit event 3", "{name}");
    }
}

#[test]
fn process_exit_in_the_first_callback_skips_the_rest() {
    let out = jsse_node(
        "[1, 2, 3].forEach(n => { console.log('item', n); process.exit(n); });\n\
         console.log('after');",
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), "item 1");
}

#[test]
fn process_exit_inside_a_loop_of_swallowing_callbacks_stops_the_loop() {
    let out = jsse_node(
        "let n = 0;\n\
         while (true) { n++; [1].forEach(() => { if (n === 3) process.exit(5); }); }",
    );
    assert_eq!(out.status.code(), Some(5));
}
