//! The embedded `assert` module under `--node` (#1225), reached through
//! `require` (#1219). Runs Node's own assert tests (scripts/assert-fixtures,
//! verbatim from nodejs/node v26.8.2, some split into `.partN` slices) and pins
//! the digest of the differential corpus that `scripts/run-node-assert-oracle.sh`
//! diffs against Node.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/assert-fixtures")
}

fn run(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let child = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(args)
        .envs(envs.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run jsse");
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(Duration::from_secs(300)) {
        Ok(out) => out.expect("wait for jsse"),
        Err(_) => {
            let _ = Command::new("kill").arg(pid.to_string()).status();
            panic!("jsse {args:?} timed out after 300s");
        }
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim_end().to_string()
}

fn read(name: &str) -> String {
    std::fs::read_to_string(fixtures().join(name)).unwrap()
}

fn manifest() -> String {
    read("manifest.txt")
}

fn node_tests() -> Vec<String> {
    let tests: Vec<String> = manifest()
        .lines()
        .filter(|l| !l.is_empty() && !"#!@~".contains(&l[..1]))
        .map(|l| l.split_whitespace().next().unwrap().to_string())
        .collect();
    assert!(tests.len() >= 10, "manifest too short: {}", tests.len());
    tests
}

fn field(prefix: char, name: &str, index: usize) -> Vec<String> {
    let key = format!("{prefix}{name}|");
    manifest()
        .lines()
        .filter(|l| l.starts_with(&key))
        .map(|l| l.split('|').nth(index).unwrap().to_string())
        .collect()
}

fn source(name: &str) -> String {
    let whole = fixtures().join(format!("{name}.js"));
    let mut text = String::new();
    if whole.exists() {
        text = std::fs::read_to_string(whole).unwrap();
    } else {
        for part in 1.. {
            let path = fixtures().join(format!("{name}.js.part{part}"));
            if !path.exists() {
                break;
            }
            text += &std::fs::read_to_string(path).unwrap();
        }
    }
    assert!(!text.is_empty(), "no fixture named {name}");
    let want = manifest()
        .lines()
        .find_map(|l| l.strip_prefix(name)?.strip_prefix(' '))
        .unwrap_or_else(|| panic!("{name} is not in the manifest"))
        .trim()
        .to_string();
    assert_eq!(
        format!("{:x}", Sha256::digest(text.as_bytes())),
        want,
        "{name}: vendored text does not match its sha256"
    );
    text
}

fn patched(name: &str) -> String {
    let mut text = source(name);
    for expr in field('~', name, 1) {
        let body = expr.strip_prefix("s#").and_then(|e| e.strip_suffix('#'));
        let (from, to) = body.and_then(|b| b.split_once('#')).expect("s#from#to#");
        assert!(text.contains(from), "{name}: patch does not apply: {from}");
        text = text.replace(from, to);
    }
    text
}

fn compose(name: &str, dir: &Path) -> PathBuf {
    let script = format!(
        "{}(function (require, __filename, __dirname) {{\n{}\n}})(__aRequire, \"/node/test/parallel/{name}.js\", \"/node/test/parallel\");\n__aStart();\n",
        read("harness.js"),
        patched(name),
    );
    let path = dir.join(format!("{name}.js"));
    std::fs::write(&path, script).unwrap();
    path
}

fn compose_corpus(dir: &Path) -> PathBuf {
    let path = dir.join("corpus.js");
    std::fs::write(
        &path,
        read("corpus-a.js") + &read("corpus-b.js") + &read("corpus-run.js"),
    )
    .unwrap();
    path
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("jsse-node-assert-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

fn run_node_tests(tag: &str, flags: &[&str], select: &dyn Fn(usize, &str) -> bool) {
    let dir = scratch(tag);
    let fixture_dir = fixtures();
    for (index, name) in node_tests().into_iter().enumerate() {
        if !select(index, &name) {
            continue;
        }
        let script = compose(&name, &dir.0);
        let skip = field('@', &name, 1).join("\n");
        let mut args = vec!["--node"];
        args.extend_from_slice(flags);
        args.push(script.to_str().unwrap());
        let out = run(
            &args,
            &[
                ("ASSERT_FIXTURE_DIR", fixture_dir.to_str().unwrap()),
                ("ASSERT_FIXTURE_SKIP", &skip),
            ],
        );
        assert!(
            out.status.success(),
            "{name}: {}\n{}",
            stdout(&out),
            stderr(&out)
        );
        assert!(
            stdout(&out)
                .lines()
                .any(|l| l.starts_with("assert-fixture: ok")),
            "{name}: {}",
            stdout(&out)
        );
    }
}

#[test]
fn nodes_own_assert_tests_pass_shard_0() {
    run_node_tests("default0", &[], &|i, _| i % 3 == 0);
}

#[test]
fn nodes_own_assert_tests_pass_shard_1() {
    run_node_tests("default1", &[], &|i, _| i % 3 == 1);
}

#[test]
fn nodes_own_assert_tests_pass_shard_2() {
    run_node_tests("default2", &[], &|i, _| i % 3 == 2);
}

#[test]
fn nodes_own_assert_tests_pass_under_the_tree_walker() {
    run_node_tests("walker", &["--no-bytecode"], &|i, _| i % 2 == 0);
}

#[test]
fn manifest_documents_every_unvendored_test() {
    let manifest = manifest();
    let vendored = node_tests();
    for name in &vendored {
        source(name);
    }
    for line in manifest.lines().filter(|l| l.starts_with('!')) {
        let words: Vec<&str> = line.split_whitespace().collect();
        assert!(words.len() >= 3, "no reason: {line}");
        let name = words[0].trim_start_matches('!').trim_end_matches(':');
        assert!(!vendored.iter().any(|n| n == name), "{line}");
        assert!(!fixtures().join(format!("{name}.js")).exists(), "{line}");
    }
    for line in manifest.lines().filter(|l| l.starts_with(['@', '~'])) {
        assert_eq!(line.split('|').count(), 3, "needs a reason: {line}");
        let name = line[1..].split('|').next().unwrap();
        assert!(vendored.iter().any(|n| n == name), "{line}");
    }
}

#[test]
fn corpus_digest_matches_node() {
    let want = read("corpus.digest");
    let dir = scratch("corpus");
    let corpus = compose_corpus(&dir.0);
    for flags in [&[][..], &["--no-bytecode"][..]] {
        let mut args = vec!["--node"];
        args.extend_from_slice(flags);
        args.push(corpus.to_str().unwrap());
        let out = run(&args, &[]);
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
    let want = read("corpus.digest");
    let dir = scratch("corpus-stress");
    let corpus = compose_corpus(&dir.0);
    let out = run(
        &["--node", corpus.to_str().unwrap()],
        &[("JSSE_GC_STRESS", "7")],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out).lines().last().unwrap(), want.trim_end());
}

#[test]
fn assert_is_absent_without_node() {
    let out = run(&["-e", "print(typeof require, typeof AssertionError)"], &[]);
    assert_eq!(stdout(&out), "undefined undefined");
}

#[test]
fn assert_module_ids_resolve_and_internals_stay_private() {
    let source = r#"
      const assert = require("assert");
      print(require("node:assert") === assert, require("assert/strict") === assert.strict);
      print(require("node:assert/strict") === assert.strict, assert.strict.strict === assert.strict);
      print(typeof assert.CallTracker, assert.AssertionError === require("node:assert").AssertionError);
      for (const id of ["internal/assert/utils", "node:internal/assert/assertion_error", "internal/errors/error_source"]) {
        try { require(id); print("no throw " + id); }
        catch (e) { print(e.code); }
      }
    "#;
    let out = run(&["--node", "-e", source], &[]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "true true\ntrue true\nundefined true\nMODULE_NOT_FOUND\nERR_UNKNOWN_BUILTIN_MODULE\nMODULE_NOT_FOUND"
    );
}
