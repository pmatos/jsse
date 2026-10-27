//! The embedded `events` module, `EventTarget` and `AbortSignal` under `--node`
//! (#1223), reached through `require` (#1219). Runs Node's own event tests
//! (scripts/events-fixtures, verbatim from nodejs/node v26.8.2) and pins the
//! digest of the differential corpus that `scripts/run-node-events-oracle.sh`
//! diffs against Node.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/events-fixtures")
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

fn manifest() -> String {
    std::fs::read_to_string(fixtures().join("manifest.txt")).unwrap()
}

fn node_tests() -> Vec<String> {
    let tests: Vec<String> = manifest()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with(['#', '!', '~']))
        .map(|l| l.split_whitespace().next().unwrap().to_string())
        .collect();
    assert!(tests.len() >= 40, "manifest too short: {tests:?}");
    tests
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
        let want = manifest()
            .lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix(' '))
            .unwrap_or_else(|| panic!("{name} has no sha256 in the manifest"))
            .trim()
            .to_string();
        let got: String = Sha256::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(got, want, "{name}: vendored slices do not match the sha256");
    }
    assert!(!text.is_empty(), "no fixture named {name}");
    text
}

fn patched(name: &str) -> String {
    let mut text = source(name);
    let prefix = format!("~{name}|");
    for line in manifest().lines().filter_map(|l| l.strip_prefix(&prefix)) {
        let expr = line.split('|').next().unwrap();
        let body = expr.strip_prefix("s#").and_then(|e| e.strip_suffix('#'));
        let (from, to) = body.and_then(|b| b.split_once('#')).expect("s#from#to#");
        assert!(text.contains(from), "{name}: patch does not apply: {from}");
        text = text.replace(from, to);
    }
    text
}

fn exposes_internals(name: &str) -> bool {
    !name.ends_with(".mjs")
        && source(name)
            .lines()
            .any(|l| l.starts_with("// Flags:") && l.contains("--expose-internals"))
}

fn compose_corpus(dir: &Path) -> PathBuf {
    let read = |name: &str| std::fs::read_to_string(fixtures().join(name)).unwrap();
    let path = dir.join("corpus.js");
    std::fs::write(&path, read("corpus-emitter.js") + &read("corpus-web.js")).unwrap();
    path
}

fn compose(name: &str, dir: &Path) -> PathBuf {
    let read = |file: &str| std::fs::read_to_string(fixtures().join(file)).unwrap();
    if name.ends_with(".mjs") {
        std::fs::write(dir.join("esm-shim.mjs"), read("esm-shim.mjs")).unwrap();
        let path = dir.join(name);
        let mut test = read(name);
        for quote in ["'", "\""] {
            for specifier in ["../common/index.mjs", "node:test"] {
                test = test.replace(
                    &format!("{quote}{specifier}{quote}"),
                    &format!("{quote}./esm-shim.mjs{quote}"),
                );
            }
        }
        std::fs::write(&path, test).unwrap();
        return path;
    }
    let harness = read("harness.js");
    let test = patched(name);
    let script = format!(
        "{harness}try {{ (function (require, __filename, __dirname) {{\n{test}\n}})(__eventsRequire, \"/node/test/parallel/{name}.js\", \"/node/test/parallel\"); }} finally {{ __eventsFinish(); }}\n"
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
    let dir = std::env::temp_dir().join(format!("jsse-node-events-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

fn run_node_tests(tag: &str, flags: &[&str], gc_stress: Option<&str>, only: &[&str]) {
    let dir = scratch(tag);
    let tests = node_tests();
    for wanted in only {
        assert!(
            tests.iter().any(|name| name == wanted),
            "no fixture named {wanted} in the manifest"
        );
    }
    for name in tests {
        if !only.is_empty() && !only.contains(&name.as_str()) {
            continue;
        }
        let script = compose(&name, &dir.0);
        let mut args = vec!["--node"];
        args.extend_from_slice(flags);
        if exposes_internals(&name) {
            args.push("--expose-internals");
        }
        args.push(script.to_str().unwrap());
        let out = run(&args, gc_stress);
        assert!(out.status.success(), "{name}: {}", stderr(&out));
        assert!(
            stdout(&out).lines().any(|l| l == "events-fixture: ok"),
            "{name}: {}",
            stdout(&out)
        );
    }
}

#[test]
fn nodes_own_events_tests_pass() {
    run_node_tests("default", &[], None, &[]);
}

#[test]
fn nodes_own_events_tests_pass_under_the_tree_walker() {
    run_node_tests("walker", &["--no-bytecode"], None, &[]);
}

#[test]
fn events_tests_pass_under_gc_stress() {
    run_node_tests(
        "stress",
        &[],
        Some("50"),
        &[
            "test-event-emitter-modify-in-emit",
            "test-event-emitter-remove-all-listeners",
            "test-abortsignal-any.mjs",
            "test-events-once",
            "test-event-target",
            "test-abortcontroller",
            "test-eventtarget",
            "test-events-on-async-iterator",
        ],
    );
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
fn web_globals_are_lazy_non_enumerable_and_settle_to_data_properties() {
    let source = r#"
      const names = ["Event", "EventTarget", "CustomEvent", "AbortController", "AbortSignal", "DOMException"];
      const before = names.map((n) => {
        const d = Object.getOwnPropertyDescriptor(globalThis, n);
        return [typeof d.get, typeof d.set, d.enumerable, d.configurable].join();
      });
      print(before[0], before.every((s) => s === before[0]));
      print(Object.keys(globalThis).filter((k) => names.includes(k)).length);
      print(typeof EventTarget, typeof Event, typeof CustomEvent, typeof AbortController, typeof AbortSignal, typeof DOMException);
      const d = Object.getOwnPropertyDescriptor(globalThis, "EventTarget");
      print(typeof d.value, d.writable, d.enumerable, d.configurable);
      print(EventTarget === globalThis.EventTarget, new CustomEvent("x") instanceof Event);
      globalThis.AbortSignal = 1;
      const e = Object.getOwnPropertyDescriptor(globalThis, "AbortSignal");
      print(e.value, e.writable, e.enumerable);
      print(new AbortController().signal instanceof EventTarget);
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "function,function,false,true true\n0\n\
         function function function function function function\n\
         function true false true\n\
         true true\n\
         1 true false\n\
         true"
    );
}

#[test]
fn web_globals_and_events_are_absent_without_node() {
    let probe = r#"print(typeof EventTarget, typeof Event, typeof CustomEvent, typeof AbortController, typeof AbortSignal, typeof DOMException, typeof __node_builtin)"#;
    let out = run(&["-e", probe], None);
    assert_eq!(
        stdout(&out),
        "undefined undefined undefined undefined undefined undefined undefined"
    );
    let dir = scratch("gate");
    let empty = dir.0.join("empty.js");
    std::fs::write(&empty, "void 0;").unwrap();
    let out = run(&["--prelude", empty.to_str().unwrap(), "-e", probe], None);
    assert_eq!(
        stdout(&out),
        "undefined undefined undefined undefined undefined undefined undefined"
    );
}

#[test]
fn events_module_ids_resolve_and_internals_stay_private() {
    let source = r#"
      const EE = require("events");
      print(require("node:events") === EE, EE.EventEmitter === EE);
      for (const id of ["internal/event_target", "node:internal/events/statics", "internal/abort_controller"]) {
        try { require(id); print("no throw " + id); }
        catch (e) { print(e.code); }
      }
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "true true\nMODULE_NOT_FOUND\nERR_UNKNOWN_BUILTIN_MODULE\nMODULE_NOT_FOUND"
    );
}

#[test]
fn expose_internals_opens_internal_ids_under_node_only() {
    let source = r#"
      const t = require("internal/event_target");
      print(require("node:internal/event_target") === t, typeof t.NodeEventTarget, typeof t.initNodeEventTarget);
      print(Object.keys(t).join());
      try { require("internal/no_such_module"); } catch (e) { print(e.code); }
      print(typeof __host_expose_internals, typeof __host_builtin_source);
    "#;
    let out = run(&["--node", "--expose-internals", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "true function function\n\
         Event,CustomEvent,EventTarget,NodeEventTarget,defineEventHandler,initEventTarget,\
         initNodeEventTarget,kCreateEvent,kNewListener,kTrustEvent,kRemoveListener,kEvents,\
         kWeakHandler,kResistStopPropagation,isEventTarget\n\
         MODULE_NOT_FOUND\n\
         undefined undefined"
    );
}

#[test]
fn expose_internals_has_no_effect_without_node() {
    let probe = r#"print(Object.getOwnPropertyNames(globalThis).sort().join())"#;
    let plain = run(&["-e", probe], None);
    let flagged = run(&["--expose-internals", "-e", probe], None);
    assert!(flagged.status.success(), "{}", stderr(&flagged));
    assert_eq!(stdout(&plain), stdout(&flagged));
    assert!(!stdout(&flagged).contains("__host"));
    let dir = scratch("expose-gate");
    let empty = dir.0.join("empty.js");
    std::fs::write(&empty, "void 0;").unwrap();
    let with_prelude = run(
        &[
            "--expose-internals",
            "--prelude",
            empty.to_str().unwrap(),
            "-e",
            probe,
        ],
        None,
    );
    assert_eq!(stdout(&plain), stdout(&with_prelude));
}

#[test]
fn a_handled_throwing_tick_does_not_strand_the_rest_of_the_queue() {
    let source = r#"
      process.on("uncaughtException", (e) => print("uncaught " + e.message));
      for (let i = 0; i < 3; i++) process.nextTick(() => { throw new Error("t" + i); });
      Promise.resolve().then(() => {
        for (let i = 0; i < 2; i++) process.nextTick(() => { throw new Error("p" + i); });
      });
      setTimeout(() => print("timer"), 50);
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "uncaught t0\nuncaught t1\nuncaught t2\nuncaught p0\nuncaught p1\ntimer"
    );
}

#[test]
fn events_survive_intrinsic_tampering() {
    let source = r#"
      const EE = require("events");
      const e = new EE();
      const seen = [];
      Array.prototype.slice = function () { throw new Error("tampered slice"); };
      Array.prototype.join = function () { throw new Error("tampered join"); };
      Reflect.apply = function () { throw new Error("tampered apply"); };
      for (let i = 0; i < 8; i++) e.on("x", (a) => seen.push(a + i));
      e.once("x", (a) => seen.push(a * 10));
      e.emit("x", 2);
      print(JSON.stringify(seen), e.listenerCount("x"), e.listeners("x").length, e.rawListeners("x").length, e.eventNames().length);
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "[2,3,4,5,6,7,8,9,20] 8 8 8 1");
}

#[test]
fn max_listeners_warning_reaches_stderr_with_a_trace_hint() {
    let source = r#"
      const EE = require("events");
      const e = new EE();
      e.setMaxListeners(1);
      e.on("a", function () {});
      e.on("a", function () {});
      e.on("a", function () {});
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    let err = stderr(&out);
    assert_eq!(
        err.matches("MaxListenersExceededWarning").count(),
        1,
        "{err}"
    );
    assert!(err.contains("2 a listeners added to"), "{err}");
    assert!(err.contains("--trace-warnings"), "{err}");
}

#[test]
fn abort_signal_timeout_and_any_ride_the_timer_queue() {
    let source = r#"
      const timed = AbortSignal.timeout(5);
      const composite = AbortSignal.any([timed, new AbortController().signal]);
      const seen = [];
      composite.addEventListener("abort", () => seen.push("composite"));
      timed.addEventListener("abort", () => seen.push("timed"));
      print(timed.aborted, composite.aborted);
      setTimeout(() => {
        print(timed.aborted, composite.aborted, timed.reason.name, timed.reason.code, composite.reason === timed.reason);
        print(seen.sort().join());
      }, 40);
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "false false\ntrue true TimeoutError 23 true\ncomposite,timed"
    );
}

#[test]
fn once_and_on_helpers_settle_across_event_loop_turns() {
    let source = r#"
      const EE = require("events");
      (async () => {
        const e = new EE();
        setTimeout(() => e.emit("go", 1, 2), 5);
        print(JSON.stringify(await EE.once(e, "go")));
        const ac = new AbortController();
        const it = EE.on(e, "tick", { signal: ac.signal });
        setTimeout(() => { e.emit("tick", "a"); e.emit("tick", "b"); ac.abort(); }, 5);
        const got = [];
        try { for await (const [v] of it) got.push(v); } catch (err) { got.push(err.name); }
        print(JSON.stringify(got));
      })();
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "[1,2]\n[\"a\",\"b\",\"AbortError\"]");
}
