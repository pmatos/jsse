//! What hereby and its dependencies (#1251) needed from the `--node` host:
//! the `global` alias, the `constants` module, and `file:` URL names in the
//! stack frames of ES modules. The hereby/chokidar/glob run against Node is
//! `scripts/run-hereby-oracle.py`; the case below only drives it when
//! TS_SRC_DIR points at a TypeScript 6.0.3 checkout.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn jsse(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(args)
        .output()
        .expect("run jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim_end().to_string()
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("jsse-hereby-deps-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).expect("write fixture");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn global_is_the_global_object_under_node_only() {
    let out = jsse(&[
        "--node",
        "-e",
        r#"var d = Object.getOwnPropertyDescriptor(globalThis, "global");
console.log(global === globalThis, d.writable, d.enumerable, d.configurable, typeof global.process)"#,
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "true true true true object");

    let plain = jsse(&["-e", r#"print(typeof global)"#]);
    assert_eq!(stdout(&plain), "undefined");
}

#[test]
fn constants_module_merges_os_and_fs_tables_frozen() {
    let out = jsse(&[
        "--node",
        "-e",
        r#"var c = require("constants"), os = require("os"), fs = require("fs");
console.log(
  require("node:constants") === c,
  Object.isFrozen(c),
  c.SIGINT === os.constants.signals.SIGINT,
  c.ENOENT === os.constants.errno.ENOENT,
  c.O_RDONLY === fs.constants.O_RDONLY,
  c.RTLD_LAZY === os.constants.dlopen.RTLD_LAZY,
  c.PRIORITY_LOW === os.constants.priority.PRIORITY_LOW,
  Object.keys(c)[0],
  Object.getPrototypeOf(c) === Object.prototype
)"#,
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "true true true true true true true RTLD_LAZY true"
    );
}

#[test]
fn es_module_frames_are_named_by_file_url() {
    let dir = Scratch::new("frames");
    let helper = dir.write(
        "thrower.mjs",
        "export function explode() { return new Error('boom').stack; }\n",
    );
    let main = dir.write(
        "main with space.mjs",
        "import { explode } from './thrower.mjs';\nfunction here() { return new Error('here').stack; }\nconsole.log(explode());\nconsole.log(here());\n",
    );
    let out = jsse(&["--node", main.to_str().unwrap()]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    let url = |p: &Path| format!("file://{}", p.to_str().unwrap().replace(' ', "%20"));
    assert!(
        text.contains(&format!("at explode ({}:1:", url(&helper))),
        "{text}"
    );
    assert!(
        text.contains(&format!("at here ({}:2:", url(&main))),
        "{text}"
    );
}

#[test]
fn hereby_matches_node_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a TypeScript 6.0.3 checkout (npm ci done) to run the hereby oracle"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-hereby-oracle.py"))
        .args(["--ci", "--jsse", env!("CARGO_BIN_EXE_jsse")])
        .output()
        .expect("run python3");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("passed") || text.contains("SKIP:"), "{text}");
}
