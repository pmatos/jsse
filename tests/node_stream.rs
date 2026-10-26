//! The embedded `stream` module under `--node` (#1233), reached through
//! `require` (#1219). Runs Node's own stream tests (scripts/stream-fixtures,
//! verbatim from nodejs/node v26.8.2) and pins the digest of the differential
//! corpus that `scripts/run-node-stream-oracle.sh` diffs against Node.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/stream-fixtures")
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

fn probe(source: &str) -> String {
    stdout(&run(&["--node", "-e", source], None))
}

struct Needs {
    string_decoder: bool,
    webstreams: bool,
}

fn manifest() -> String {
    std::fs::read_to_string(fixtures().join("manifest.txt")).unwrap()
}

fn node_tests() -> Vec<(String, Needs)> {
    let tests: Vec<(String, Needs)> = manifest()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!'))
        .map(|l| {
            let mut words = l.split_whitespace();
            let name = words.next().unwrap().to_string();
            let flags: Vec<&str> = words.collect();
            let needs = Needs {
                string_decoder: flags.contains(&"needs-string-decoder"),
                webstreams: flags.contains(&"needs-webstreams"),
            };
            (name, needs)
        })
        .collect();
    assert!(tests.len() >= 150, "manifest too short: {}", tests.len());
    tests
}

fn compose_corpus(dir: &Path) -> PathBuf {
    let read = |name: &str| std::fs::read_to_string(fixtures().join(name)).unwrap();
    let path = dir.join("corpus.js");
    std::fs::write(
        &path,
        read("corpus-a.js") + &read("corpus-b.js") + &read("corpus-run.js"),
    )
    .unwrap();
    path
}

fn compose(name: &str, dir: &Path) -> PathBuf {
    let read = |name: &str| std::fs::read_to_string(fixtures().join(name)).unwrap();
    let script = format!(
        "{}{}(function (require, __filename, __dirname) {{\n{}\n}})(__sRequire, \"/node/test/parallel/{name}.js\", \"/node/test/parallel\");\n",
        read("harness-assert.js"),
        read("harness.js"),
        read(&format!("{name}.js")),
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
    let dir = std::env::temp_dir().join(format!("jsse-node-stream-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

fn run_node_tests(
    tag: &str,
    flags: &[&str],
    gc_stress: Option<&str>,
    select: &dyn Fn(usize, &str) -> bool,
) {
    let dir = scratch(tag);
    let string_decoder =
        probe(r#"print(typeof require("string_decoder").StringDecoder)"#) == "function";
    let webstreams = probe("print(typeof ReadableStream)") == "function";
    for (index, (name, needs)) in node_tests().into_iter().enumerate() {
        if !select(index, &name) {
            continue;
        }
        if (needs.string_decoder && !string_decoder) || (needs.webstreams && !webstreams) {
            continue;
        }
        let script = compose(&name, &dir.0);
        let mut args = vec!["--node"];
        args.extend_from_slice(flags);
        args.push(script.to_str().unwrap());
        let out = run(&args, gc_stress);
        assert!(out.status.success(), "{name}: {}", stderr(&out));
        assert!(
            stdout(&out).lines().any(|l| l == "stream-fixture: ok"),
            "{name}: {}",
            stdout(&out)
        );
    }
}

#[test]
fn nodes_own_stream_tests_pass_shard_0() {
    run_node_tests("default0", &[], None, &|i, _| i % 4 == 0);
}

#[test]
fn nodes_own_stream_tests_pass_shard_1() {
    run_node_tests("default1", &[], None, &|i, _| i % 4 == 1);
}

#[test]
fn nodes_own_stream_tests_pass_shard_2() {
    run_node_tests("default2", &[], None, &|i, _| i % 4 == 2);
}

#[test]
fn nodes_own_stream_tests_pass_shard_3() {
    run_node_tests("default3", &[], None, &|i, _| i % 4 == 3);
}

#[test]
fn nodes_own_stream_tests_pass_under_the_tree_walker() {
    run_node_tests("walker", &["--no-bytecode"], None, &|i, _| i % 3 == 0);
}

#[test]
fn stream_tests_pass_under_gc_stress() {
    let only = [
        "test-stream-backpressure",
        "test-stream-pipe-await-drain",
        "test-stream-transform-split-objectmode",
        "test-stream-writable-final-async",
        "test-stream-readable-destroy",
        "test-stream-pipeline-queued-end-in-destroy",
    ];
    let names: Vec<String> = node_tests().into_iter().map(|(n, _)| n).collect();
    for wanted in only {
        assert!(
            names.iter().any(|n| n == wanted),
            "no fixture named {wanted}"
        );
    }
    run_node_tests("stress", &[], Some("50"), &|_, name| only.contains(&name));
}

#[test]
fn manifest_documents_every_unvendored_test() {
    let manifest = manifest();
    let vendored: Vec<String> = node_tests().into_iter().map(|(n, _)| n).collect();
    for name in &vendored {
        assert!(fixtures().join(format!("{name}.js")).exists(), "{name}");
    }
    for line in manifest.lines().filter(|l| l.starts_with('!')) {
        let words: Vec<&str> = line.split_whitespace().collect();
        assert!(words.len() >= 3, "no reason: {line}");
        assert!(!vendored.iter().any(|n| n == words[1]), "{line}");
        assert!(
            !fixtures().join(format!("{}.js", words[1])).exists(),
            "{line}"
        );
    }
}

#[test]
fn corpus_digest_matches_node() {
    let want = std::fs::read_to_string(fixtures().join("corpus.digest")).unwrap();
    let dir = scratch("corpus");
    let corpus = compose_corpus(&dir.0);
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
fn corpus_survives_gc_stress() {
    let want = std::fs::read_to_string(fixtures().join("corpus.digest")).unwrap();
    let dir = scratch("corpus-stress");
    let corpus = compose_corpus(&dir.0);
    let out = run(&["--node", corpus.to_str().unwrap()], Some("7"));
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out).lines().last().unwrap(), want.trim_end());
}

#[test]
fn stream_is_absent_without_node() {
    let out = run(
        &[
            "-e",
            "print(typeof require, typeof Readable, typeof Writable)",
        ],
        None,
    );
    assert_eq!(stdout(&out), "undefined undefined undefined");
}

#[test]
fn stream_module_ids_resolve_and_internals_stay_private() {
    let source = r#"
      const stream = require("stream");
      print(require("node:stream") === stream, stream.Stream === stream);
      print(require("stream/promises") === stream.promises, require("node:stream/promises") === stream.promises);
      print(Object.getPrototypeOf(stream.Readable) === stream.Stream, new stream.PassThrough() instanceof stream.Duplex);
      for (const id of ["internal/streams/readable", "node:internal/streams/utils", "internal/webstreams/adapters"]) {
        try { require(id); print("no throw " + id); }
        catch (e) { print(e.code); }
      }
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "true true\ntrue true\ntrue true\nMODULE_NOT_FOUND\nERR_UNKNOWN_BUILTIN_MODULE\nMODULE_NOT_FOUND"
    );
}

#[test]
fn web_stream_interop_throws_until_web_streams_exist() {
    let source = r#"
      const { Readable, Writable, Duplex } = require("stream");
      const attempts = [
        () => Readable.toWeb(new Readable()),
        () => Readable.fromWeb({}),
        () => Writable.toWeb(new Writable()),
        () => Writable.fromWeb({}),
        () => Duplex.toWeb(new Duplex()),
        () => Duplex.fromWeb({}),
      ];
      for (const fn of attempts) {
        try { fn(); print("no throw"); } catch (e) { print(e.code); }
      }
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), ["ERR_METHOD_NOT_IMPLEMENTED"; 6].join("\n"));
}
