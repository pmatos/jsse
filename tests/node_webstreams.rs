//! Web streams under `--node` (#1360): `ReadableStream`, `WritableStream`,
//! `TransformStream`, the queuing strategies, the text and compression
//! streams, `stream/web`, `stream/consumers` and the Node stream interop,
//! ported from Node v26.8.2. Runs Node's own tests (scripts/webstreams-fixtures,
//! verbatim) and pins the digest of the differential corpus that
//! `scripts/run-node-webstreams-oracle.sh` diffs against Node.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixtures() -> PathBuf {
    root().join("scripts/webstreams-fixtures")
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

fn eval(source: &str, node: bool) -> String {
    let args: Vec<&str> = if node {
        vec!["--node", "-e", source]
    } else {
        vec!["-e", source]
    };
    let out = run(&args, None);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

fn manifest() -> String {
    std::fs::read_to_string(fixtures().join("manifest.txt")).unwrap()
}

fn node_tests() -> Vec<String> {
    let tests: Vec<String> = manifest()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!'))
        .map(|l| l.split_whitespace().next().unwrap().to_string())
        .collect();
    assert!(tests.len() >= 20, "manifest too short: {}", tests.len());
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
    let stream = root().join("scripts/stream-fixtures");
    let read = |dir: &Path, file: &str| std::fs::read_to_string(dir.join(file)).unwrap();
    let path = dir.join(format!("{name}.js"));
    let script = format!(
        "{}{}{}(function (require, __filename, __dirname) {{\n{}\n}})(__sRequire, {:?}, {:?});\n",
        read(&stream, "harness-assert.js"),
        read(&stream, "harness.js"),
        read(&fixtures(), "harness.js"),
        read(&fixtures(), &format!("{name}.js")),
        path.to_str().unwrap(),
        dir.to_str().unwrap(),
    );
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
    let dir =
        std::env::temp_dir().join(format!("jsse-node-webstreams-{tag}-{}", std::process::id()));
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
    for (index, name) in node_tests().into_iter().enumerate() {
        if !select(index, &name) {
            continue;
        }
        let script = compose(&name, &dir.0);
        let mut args = vec!["--node", "--expose-internals"];
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
fn nodes_own_web_stream_tests_pass_shard_0() {
    run_node_tests("default0", &[], None, &|i, _| i % 3 == 0);
}

#[test]
fn nodes_own_web_stream_tests_pass_shard_1() {
    run_node_tests("default1", &[], None, &|i, _| i % 3 == 1);
}

#[test]
fn nodes_own_web_stream_tests_pass_shard_2() {
    run_node_tests("default2", &[], None, &|i, _| i % 3 == 2);
}

#[test]
fn nodes_own_web_stream_tests_pass_under_the_tree_walker() {
    run_node_tests("walker", &["--no-bytecode"], None, &|i, _| i % 2 == 0);
}

#[test]
fn web_stream_tests_pass_under_gc_stress() {
    let only = [
        "test-whatwg-readablebytestreambyob",
        "test-whatwg-transformstream",
        "test-webstreams-compose",
        "test-webstreams-finished",
        "test-stream-duplex-from",
    ];
    let names = node_tests();
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
    let vendored = node_tests();
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
        assert_eq!(
            stdout(&out).lines().last().unwrap(),
            want.trim_end(),
            "{flags:?}"
        );
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
fn web_stream_globals_are_absent_without_node() {
    let names = [
        "ReadableStream",
        "ReadableStreamDefaultReader",
        "ReadableStreamBYOBReader",
        "ReadableStreamBYOBRequest",
        "ReadableByteStreamController",
        "ReadableStreamDefaultController",
        "WritableStream",
        "WritableStreamDefaultWriter",
        "WritableStreamDefaultController",
        "TransformStream",
        "TransformStreamDefaultController",
        "ByteLengthQueuingStrategy",
        "CountQueuingStrategy",
        "TextEncoderStream",
        "TextDecoderStream",
        "CompressionStream",
        "DecompressionStream",
    ];
    let source = format!(
        "print([{}].map((n) => typeof globalThis[n]).join(' '))",
        names.map(|n| format!("'{n}'")).join(",")
    );
    assert_eq!(eval(&source, false), ["undefined"; 17].join(" "));
    assert_eq!(eval(&source, true), ["function"; 17].join(" "));
}

#[test]
fn stream_web_ids_resolve_and_internals_stay_private() {
    let source = r#"
      const web = require("stream/web");
      print(require("node:stream/web") === web, web.ReadableStream === ReadableStream);
      print(web.TransformStream === TransformStream, web.CountQueuingStrategy === CountQueuingStrategy);
      print(typeof require("stream/consumers").text, require("node:stream/consumers") === require("stream/consumers"));
      for (const id of ["internal/webstreams/readablestream", "node:internal/webstreams/util", "internal/webstreams/adapters"]) {
        try { require(id); print("no throw " + id); }
        catch (e) { print(e.code); }
      }
    "#;
    assert_eq!(
        eval(source, true),
        "true true\ntrue true\nfunction true\nMODULE_NOT_FOUND\nERR_UNKNOWN_BUILTIN_MODULE\nMODULE_NOT_FOUND"
    );
}

#[test]
fn stream_web_imports_from_an_es_module() {
    let dir = scratch("esm");
    let path = dir.0.join("main.mjs");
    std::fs::write(
        &path,
        "import { ReadableStream as RS, TransformStream } from 'node:stream/web';\n\
         import web from 'stream/web';\n\
         const chunks = [];\n\
         for await (const c of new RS({ start(ctl) { ctl.enqueue(1); ctl.close(); } })) chunks.push(c);\n\
         console.log(chunks.join(), RS === web.ReadableStream, RS === globalThis.ReadableStream, typeof TransformStream);\n",
    )
    .unwrap();
    let out = run(&["--node", path.to_str().unwrap()], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "1 true true function");
}

#[test]
fn vendored_sources_stay_under_the_file_budget() {
    let mut files = vec![];
    let mut stack = vec![root().join("node-lib/internal/webstreams")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    assert!(files.len() >= 15, "{files:?}");
    for file in files {
        let len = std::fs::metadata(&file).unwrap().len();
        assert!(len < 20_000, "{}: {len} bytes", file.display());
    }
}
