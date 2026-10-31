//! #1334: under `--node`, `util.isDeepStrictEqual` and `assert.deepStrictEqual`
//! reach a Proxy's own keys through its `ownKeys` and `getOwnPropertyDescriptor`
//! traps the way Node's `getOwnNonIndexProperties` does (V8 `GetPropertyNames`
//! with `kSkipIndices`), instead of reading the target's property map.
//! `EXPECTED` is Node v26.8.2's output for the same script.

use std::process::{Command, Stdio};

const SCRIPT: &str = include_str!("../scripts/util-fixtures/proxy-own-keys.js");

const EXPECTED: &str = include_str!("../scripts/util-fixtures/proxy-own-keys.expected");

fn eval(flags: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .arg("--node")
        .args(flags)
        .args(["-e", SCRIPT])
        .stdin(Stdio::null())
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
fn proxy_own_keys_traps_run_like_node() {
    for flags in [&[][..], &["--no-bytecode"][..]] {
        assert_eq!(eval(flags), EXPECTED.trim_end(), "flags {flags:?}");
    }
}
