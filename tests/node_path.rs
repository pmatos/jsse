//! The embedded `path` module under `--node` (#1221), reached through the
//! hidden `__node_builtin` accessor until the CommonJS loader (#1219) adds
//! `require`. Runs Node's own path tests (scripts/path-fixtures, verbatim from
//! nodejs/node v26.8.2) and pins the digest of the differential corpus that
//! `scripts/run-node-path-oracle.sh` diffs against Node.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/path-fixtures")
}

fn run(args: &[&str], gc_stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    cmd.args(args).output().expect("run jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim_end().to_string()
}

fn node_tests() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixtures())
        .expect("fixtures dir")
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("test-path") && n.ends_with(".js"))
        .map(|n| n.trim_end_matches(".js").to_string())
        .collect();
    names.sort();
    assert!(names.len() >= 15, "missing path tests: {names:?}");
    names
}

fn compose(name: &str, dir: &Path) -> PathBuf {
    let harness = std::fs::read_to_string(fixtures().join("harness.js")).unwrap();
    let test = std::fs::read_to_string(fixtures().join(format!("{name}.js"))).unwrap();
    let script = format!(
        "{harness}(function (require, __filename, __dirname) {{\n{test}\n}})(__nodePathRequire, \"/node/test/parallel/{name}.js\", \"/node/test/parallel\");\nconsole.log(\"ok\");\n"
    );
    let path = dir.join(format!("{name}.js"));
    std::fs::write(&path, script).unwrap();
    path
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("jsse-node-path-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

fn run_node_tests(tag: &str, flags: &[&str], gc_stress: Option<&str>, only: &[&str]) {
    let dir = scratch(tag);
    for name in node_tests() {
        if !only.is_empty() && !only.contains(&name.as_str()) {
            continue;
        }
        let script = compose(&name, &dir.0);
        let mut args = vec!["--node"];
        args.extend_from_slice(flags);
        args.push(script.to_str().unwrap());
        let out = run(&args, gc_stress);
        assert!(out.status.success(), "{name}: {}", stderr(&out));
        assert_eq!(stdout(&out), "ok", "{name}");
    }
}

#[test]
fn nodes_own_path_tests_pass() {
    run_node_tests("default", &[], None, &[]);
}

#[test]
fn nodes_own_path_tests_pass_under_the_tree_walker() {
    run_node_tests("walker", &["--no-bytecode"], None, &[]);
}

#[test]
fn path_tests_pass_under_gc_stress() {
    run_node_tests(
        "stress",
        &[],
        Some("50"),
        &[
            "test-path-glob",
            "test-path-join",
            "test-path-resolve",
            "test-path-parse-format",
            "test-path-relative",
            "test-path-win32-normalize-device-names",
        ],
    );
}

#[test]
fn corpus_digest_matches_node() {
    let want = std::fs::read_to_string(fixtures().join("corpus.digest")).unwrap();
    let corpus = fixtures().join("corpus.js");
    for flags in [&[][..], &["--no-bytecode"][..]] {
        let mut args = vec!["--node"];
        args.extend_from_slice(flags);
        args.push(corpus.to_str().unwrap());
        let out = run(&args, None);
        assert!(out.status.success(), "{}", stderr(&out));
        let text = stdout(&out);
        assert_eq!(text.lines().last().unwrap(), want.trim_end(), "{flags:?}");
    }
}

#[test]
fn builtin_accessor_is_hidden_and_loads_path() {
    let source = r#"
      const d = Object.getOwnPropertyDescriptor(globalThis, "__node_builtin");
      const path = __node_builtin("path");
      print([
        typeof d.value, d.enumerable, d.configurable,
        Object.keys(globalThis).includes("__node_builtin"),
        typeof __host_builtin_source,
        __node_builtin("node:path") === path,
        __node_builtin("path/posix") === path.posix,
        __node_builtin("node:path/win32") === path.win32,
        path.posix.posix === path.posix && path.win32.win32 === path.win32,
      ].join());
      for (const id of ["internal/errors", "node:internal/util", "nope"]) {
        try { __node_builtin(id); print("no throw " + id); }
        catch (e) { print(id, e.code, e.message); }
      }
      try { path.join(1); } catch (e) { print(e.name, e.code, e instanceof TypeError, e.message); }
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "function,false,true,false,undefined,true,true,true,true\n\
         internal/errors ERR_UNKNOWN_BUILTIN_MODULE No such built-in module: internal/errors\n\
         node:internal/util ERR_UNKNOWN_BUILTIN_MODULE No such built-in module: node:internal/util\n\
         nope ERR_UNKNOWN_BUILTIN_MODULE No such built-in module: nope\n\
         TypeError ERR_INVALID_ARG_TYPE true The \"path\" argument must be of type string. Received type number (1)"
    );
}

#[test]
fn builtin_modules_are_absent_without_node() {
    let probe = r#"print(typeof __node_builtin, typeof __host_builtin_source, typeof require)"#;
    let out = run(&["-e", probe], None);
    assert_eq!(stdout(&out), "undefined undefined undefined");
    let dir = scratch("gate");
    let empty = dir.0.join("empty.js");
    std::fs::write(&empty, "void 0;").unwrap();
    let out = run(&["--prelude", empty.to_str().unwrap(), "-e", probe], None);
    assert_eq!(stdout(&out), "undefined undefined undefined");
}

#[test]
fn builtin_loader_validates_ids_and_survives_intrinsic_tampering() {
    let source = r#"
      const load = __node_builtin;
      Array.prototype.join = function () { return "tampered"; };
      String.prototype.slice = function () { return "tampered"; };
      Array.prototype[Symbol()] = 1;
      delete globalThis.Map;
      for (const id of [undefined, {}, 7]) {
        try { load(id); print("no throw"); }
        catch (e) { print(e.code, e instanceof TypeError); }
      }
      try { load("nope"); } catch (e) { print(e.code); }
      try { load("nope"); } catch (e) { print(e.code); }
      const path = load("path");
      print(path.join("a", "b"), path.resolve("/a", "b"), path.win32.join("a", "b"));
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "ERR_INVALID_ARG_TYPE true\nERR_INVALID_ARG_TYPE true\nERR_INVALID_ARG_TYPE true\n\
         ERR_UNKNOWN_BUILTIN_MODULE\nERR_UNKNOWN_BUILTIN_MODULE\na/b /a/b a\\b"
    );
}
