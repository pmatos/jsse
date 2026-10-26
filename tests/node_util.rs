//! The embedded `util` module under `--node` (#1224), reached through
//! `require` (#1219), and the `console` built on it. The Node-oracle diff is
//! `scripts/run-node-util-oracle.sh`; this pins the digest of that corpus and
//! checks the opt-in gate, the introspection floor and the console output.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/util-fixtures")
}

fn run(args: &[&str], gc_stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    cmd.env_remove("FORCE_COLOR")
        .env_remove("NO_COLOR")
        .env_remove("NODE_DISABLE_COLORS")
        .env_remove("NODE_DEBUG");
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

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("jsse-node-util-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

fn corpus(dir: &Path) -> PathBuf {
    let mut names: Vec<String> = std::fs::read_dir(fixtures())
        .expect("fixtures dir")
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("corpus-") && n.ends_with(".js"))
        .collect();
    names.sort();
    assert!(names.len() >= 10, "missing corpus parts: {names:?}");
    let mut script = String::new();
    for name in names {
        script.push_str(&std::fs::read_to_string(fixtures().join(name)).unwrap());
    }
    let path = dir.join("corpus.js");
    std::fs::write(&path, script).unwrap();
    path
}

fn eval(source: &str, flags: &[&str], gc_stress: Option<&str>) -> String {
    let mut args = vec!["--node"];
    args.extend_from_slice(flags);
    args.extend_from_slice(&["-e", source]);
    let out = run(&args, gc_stress);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

#[test]
fn corpus_digest_matches_node() {
    let want = std::fs::read_to_string(fixtures().join("corpus.digest")).unwrap();
    let dir = scratch("corpus");
    let script = corpus(&dir.0);
    for flags in [&[][..], &["--no-bytecode"][..]] {
        let mut args = vec!["--node"];
        args.extend_from_slice(flags);
        args.push(script.to_str().unwrap());
        let out = run(&args, None);
        assert!(out.status.success(), "{flags:?}: {}", stderr(&out));
        assert_eq!(stderr(&out), "", "{flags:?}");
        assert_eq!(
            stdout(&out).lines().last().unwrap(),
            want.trim_end(),
            "{flags:?}"
        );
    }
}

#[test]
fn introspection_floor_is_absent_without_node_and_private_with_it() {
    let names = [
        "__host_util_kind",
        "__host_util_promise_details",
        "__host_util_proxy_details",
        "__host_util_preview_entries",
        "__host_util_own_non_index",
        "__host_util_class_name",
    ];
    let probe = format!(
        "print({})",
        names
            .iter()
            .map(|n| format!("typeof {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let absent = vec!["undefined"; names.len()].join(" ");
    assert_eq!(stdout(&run(&["-e", &probe], None)), absent);
    let dir = scratch("gate");
    let empty = dir.0.join("empty.js");
    std::fs::write(&empty, "void 0;").unwrap();
    let out = run(&["--prelude", empty.to_str().unwrap(), "-e", &probe], None);
    assert_eq!(stdout(&out), absent);
    assert_eq!(eval(&probe, &[], None), absent);
}

#[test]
fn console_prints_like_node_for_buffers_and_typed_arrays() {
    let source = r#"
      console.log(Buffer.from("a"));
      console.log(Buffer.alloc(0), Buffer.from("hello"));
      const withProp = Buffer.from([1, 2]);
      withProp.tag = "x";
      console.log(withProp, Buffer.alloc(52));
      console.log(new Uint8Array([1, 2, 3]), new Float32Array(2), new BigInt64Array(1));
      console.log(new ArrayBuffer(3), new DataView(new ArrayBuffer(2)));
      console.log([Buffer.from("ab")], { b: Buffer.from("c") });
    "#;
    assert_eq!(
        eval(source, &[], None),
        "<Buffer 61>\n\
         <Buffer > <Buffer 68 65 6c 6c 6f>\n\
         <Buffer 01 02, tag: 'x'> <Buffer 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 ... 2 more bytes>\n\
         Uint8Array(3) [ 1, 2, 3 ] Float32Array(2) [ 0, 0 ] BigInt64Array(1) [ 0n ]\n\
         ArrayBuffer { [Uint8Contents]: <00 00 00>, [byteLength]: 3 } DataView {\n  [byteLength]: 2,\n  [byteOffset]: 0,\n  [buffer]: ArrayBuffer { [Uint8Contents]: <00 00>, [byteLength]: 2 }\n}\n\
         [ <Buffer 61 62> ] { b: <Buffer 63> }"
    );
}

#[test]
fn console_formats_primitives_and_format_strings() {
    let source = r#"
      console.log("a", 1, -0, null, undefined, true, "b%s");
      console.log("%s=%d %j %%", "x", 42, { k: [1] }, "tail", { t: 1 });
      console.log(1, "%s", "unformatted");
      console.group("outer");
      console.log("one\ntwo");
      console.group();
      console.log({ a: { b: { c: { d: 1 } } } });
      console.groupEnd();
      console.groupEnd();
      console.dir({ a: { b: { c: { d: 1 } } } }, { depth: 0 });
      console.dir({ [Symbol.for("nodejs.util.inspect.custom")]() { return "custom"; } });
      console.error("to stderr");
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "a 1 -0 null undefined true b%s\n\
         x=42 {\"k\":[1]} % tail { t: 1 }\n\
         1 %s unformatted\n\
         outer\n  one\n  two\n    { a: { b: { c: [Object] } } }\n\
         { a: [Object] }\n\
         {\n  Symbol(nodejs.util.inspect.custom): [Function: [nodejs.util.inspect.custom]]\n}"
    );
    assert_eq!(stderr(&out), "to stderr");
}

#[test]
fn inspect_reads_internal_slots_without_running_user_code() {
    let source = r#"
      const util = require("util");
      const trap = new Proxy({ a: 1 }, new Proxy({}, { get() { throw new Error("trap ran"); } }));
      const revoked = Proxy.revocable({}, {});
      revoked.revoke();
      const pending = new Promise(() => {});
      const rejected = Promise.reject(new Error("no"));
      rejected.catch(() => {});
      const it = new Map([[1, 2], [3, 4]]).entries();
      it.next();
      print(util.inspect(trap, { showProxy: true }));
      print(util.inspect(revoked.proxy, { showProxy: true }), util.inspect(revoked.proxy));
      print(util.inspect(pending), util.inspect(Promise.resolve([1])));
      print(util.inspect(it), util.inspect(new Set([1]).values()));
      print(util.inspect(new WeakMap([[{}, 1]]), { showHidden: true }));
      print(util.types.isProxy(trap), util.types.isPromise(rejected), util.types.isMapIterator(it));
      print(util.types.isGeneratorObject((function* () {})()), util.types.isAsyncFunction(async () => {}));
    "#;
    assert_eq!(
        eval(source, &[], None),
        "Proxy [ { a: 1 }, Proxy [ {}, { get: [Function: get] } ] ]\n\
         <Revoked Proxy> <Revoked Proxy>\n\
         Promise { <pending> } Promise { [ 1 ] }\n\
         [Map Entries] { [ 3, 4 ] } [Set Iterator] { 1 }\n\
         WeakMap { {} => 1 }\n\
         true true true\n\
         true true"
    );
}

#[test]
fn events_max_listeners_warning_names_the_emitter_class() {
    let source = r#"
      const EventEmitter = require("events");
      class FakeInput extends EventEmitter {}
      const e = new FakeInput();
      e.setMaxListeners(1);
      process.on("warning", (w) => print(w.name, w.message));
      e.on("event-type", () => {});
      e.on("event-type", () => {});
    "#;
    let out = eval(source, &[], None);
    assert!(
        out.contains("2 event-type listeners added to [FakeInput]. MaxListeners is 1."),
        "{out}"
    );
}

#[test]
fn util_module_surface_matches_node() {
    let source = r#"
      const util = require("util");
      print(Object.keys(util).sort().join());
      print(util.inspect.custom === Symbol.for("nodejs.util.inspect.custom"), util.types.isTypedArray(Buffer.alloc(1)));
      print(util.format("%s %d %i %f %j %o %O %%", "s", 1.5, 2.5, "3.5", { j: 1 }, [1], { O: 1 }));
      print(util.isDeepStrictEqual({ a: [1, new Map([[1, { b: 2 }]])] }, { a: [1, new Map([[1, { b: 2 }]])] }));
      print(JSON.stringify(util.parseArgs({ args: ["-f", "--bar", "b"], options: { f: { type: "boolean", short: "f" }, bar: { type: "string" } } })));
      print(new util.MIMEType("Text/HTML; Charset=utf-8").toString(), util.styleText("red", "x", { validateStream: false }));
      print(util.getSystemErrorName(-2), util.stripVTControlCharacters("\u001b[31mx\u001b[39m"));
    "#;
    assert_eq!(
        eval(source, &[], None),
        "MIMEParams,MIMEType,TextDecoder,TextEncoder,_errnoException,_exceptionWithHostPort,_extend,aborted,callbackify,convertProcessSignalToExitCode,debug,debuglog,deprecate,diff,format,formatWithOptions,getCallSites,getSystemErrorMap,getSystemErrorMessage,getSystemErrorName,inherits,inspect,isArray,isDeepStrictEqual,parseArgs,parseEnv,promisify,setTraceSigInt,stripVTControlCharacters,styleText,toUSVString,transferableAbortController,transferableAbortSignal,types\n\
         true true\n\
         s 1.5 2 3.5 {\"j\":1} [ 1, [length]: 1 ] { O: 1 } %\n\
         true\n\
         {\"values\":{\"f\":true,\"bar\":\"b\"},\"positionals\":[]}\n\
         text/html;charset=utf-8 \u{1b}[31mx\u{1b}[39m\n\
         ENOENT x"
    );
}

#[test]
fn debuglog_and_colors_honour_the_environment() {
    let source = r#"
      const util = require("util");
      const d = util.debuglog("foo");
      d("a", "b", "c");
      d("%s-%d", "x", 5);
      print(d.enabled, util.debuglog("bar").enabled);
      print(JSON.stringify(util.styleText("red", "x")), JSON.stringify(util.styleText("red", "x", { stream: process.stderr })));
    "#;
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    cmd.env_remove("NO_COLOR")
        .env_remove("NODE_DISABLE_COLORS")
        .env("NODE_DEBUG", "foo")
        .env("FORCE_COLOR", "1");
    let out = cmd.args(["--node", "-e", source]).output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "true false\n\"\\u001b[31mx\\u001b[39m\" \"\\u001b[31mx\\u001b[39m\""
    );
    let err = stderr(&out);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 2, "{err}");
    assert!(
        lines[0].starts_with("FOO ") && lines[0].ends_with(": a b c"),
        "{err}"
    );
    assert!(
        lines[1].starts_with("FOO ") && lines[1].ends_with(": x-5"),
        "{err}"
    );
    assert_eq!(
        eval(
            r#"print(JSON.stringify(require("util").styleText("red", "x")))"#,
            &[],
            None
        ),
        "\"x\""
    );
}

#[test]
fn module_namespace_objects_and_shadow_realms() {
    let dir = scratch("namespace");
    let module = dir.0.join("m.mjs");
    std::fs::write(&module, "export const a = 1;\n").unwrap();
    let main = dir.0.join("main.js");
    let source = format!(
        r#"
      const util = require("util");
      const hits = (v) => Object.keys(util.types).filter((k) => typeof util.types[k] === "function" && util.types[k](v)).join();
      import({:?}).then((ns) => {{
        print(hits(ns), util.inspect(ns));
        const realm = new ShadowRealm();
        print(JSON.stringify(hits(realm)), util.inspect(realm));
      }});
    "#,
        module.to_str().unwrap()
    );
    std::fs::write(&main, source).unwrap();
    let out = run(&["--node", main.to_str().unwrap()], None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "isModuleNamespaceObject [Module: null prototype] { a: 1 }\n\"\" ShadowRealm {}"
    );
}

#[test]
fn corpus_survives_gc_stress() {
    let dir = scratch("stress");
    let script = corpus(&dir.0);
    let want = std::fs::read_to_string(fixtures().join("corpus.digest")).unwrap();
    let out = run(&["--node", script.to_str().unwrap()], Some("997"));
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out).lines().last().unwrap(), want.trim_end());
}

#[test]
fn console_assert_indents_and_warns_like_node() {
    let source = r#"
      console.group('g');
      console.assert(false, 'ind');
      console.assert(false, { a: 1 });
      console.groupEnd();
      console.timeEnd('zz');
      console.countReset('q');
      console.time('d'); console.time('d');
    "#;
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.starts_with("  Assertion failed: ind\n  Assertion failed { a: 1 }\n"),
        "{err}"
    );
    for want in [
        "Warning: No such label 'zz' for console.timeEnd()",
        "Warning: Count for 'q' does not exist",
        "Warning: Label 'd' already exists for console.time()",
    ] {
        assert!(err.contains(want), "{err}");
    }
}
