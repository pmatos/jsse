//! The `global` alias of `globalThis` under `--node` (#1372): Node defines it as a
//! writable, enumerable, configurable data property; the default environment has none.

use std::process::Command;

fn run(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(args)
        .output()
        .expect("run jsse");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

#[test]
fn global_is_the_global_object_under_node() {
    let src = "const d = Object.getOwnPropertyDescriptor(globalThis, 'global');\
        console.log(typeof global, global === globalThis, d.writable, d.enumerable, d.configurable,\
        Object.keys(globalThis).includes('global'), typeof global.gc);";
    assert_eq!(
        run(&["--node", "-e", src]),
        "object true true true true true undefined"
    );
}

#[test]
fn global_is_absent_without_node() {
    assert_eq!(run(&["-e", "print(typeof global)"]), "undefined");
}
