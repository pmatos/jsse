//! Node event-loop semantics (#1229). The default event loop (no `--node`) must
//! behave exactly as before: no Node globals, numeric timer ids, throwing timer
//! callbacks and unhandled rejections ignored, a throwing main script still
//! draining timers. Under `--node` the ordering and lifecycle checks run the
//! binary, because the observable result is stdout and the exit status. The
//! Node-diffed ordering fixtures live in `scripts/event-loop-fixtures/`.

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
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn default_run(code: &str) -> (String, i32) {
    let out = jsse(&["-e", code]);
    (stdout(&out), out.status.code().unwrap())
}

fn node_run(code: &str) -> (String, i32, String) {
    let out = jsse(&["--node", "-e", code]);
    (stdout(&out), out.status.code().unwrap(), stderr(&out))
}

#[test]
fn default_mode_has_no_node_event_loop_surface() {
    let (out, rc) = default_run(
        r#"var interval = setInterval(function () {}, 1);
        clearInterval(interval);
        print([typeof setImmediate, typeof clearImmediate, typeof queueMicrotask,
          typeof process, typeof setTimeout(function () {}, 0), typeof interval].join())"#,
    );
    assert_eq!(
        (out.as_str(), rc),
        ("undefined,undefined,undefined,undefined,number,number", 0)
    );
}

#[test]
fn default_mode_ignores_a_throwing_timer_and_keeps_firing_later_ones() {
    let (out, rc) = default_run(
        r#"setTimeout(function () { throw new Error("x"); }, 1);
        setTimeout(function () { print("later"); }, 5);"#,
    );
    assert_eq!((out.as_str(), rc), ("later", 0));
}

#[test]
fn default_mode_does_not_fail_on_an_unhandled_rejection() {
    let (out, rc) = default_run(
        r#"Promise.reject(new Error("x")); setTimeout(function () { print("later"); }, 5);"#,
    );
    assert_eq!((out.as_str(), rc), ("later", 0));
}

#[test]
fn default_mode_still_drains_timers_after_a_throwing_main_script() {
    let out = jsse(&[
        "-e",
        r#"setTimeout(function () { print("t"); }, 1); throw new Error("x");"#,
    ]);
    assert_eq!(stdout(&out), "t");
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn default_mode_job_order_is_unchanged() {
    let (out, rc) = default_run(
        r#"Promise.resolve().then(function () { print("p"); });
        setTimeout(function () { print("t"); }, 0);
        print("s");"#,
    );
    assert_eq!((out.as_str(), rc), ("s\np\nt", 0));
}

#[test]
fn prelude_mode_never_enables_the_node_event_loop() {
    let dir = std::env::temp_dir().join(format!("jsse-node-event-loop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let prelude = dir.join("empty.js");
    std::fs::write(&prelude, "void 0;").unwrap();
    let out = jsse(&[
        "--prelude",
        prelude.to_str().unwrap(),
        "-e",
        r#"print(typeof setImmediate, typeof setTimeout(function () {}, 0), typeof __host_loop_install)"#,
    ]);
    assert_eq!(stdout(&out), "undefined number undefined");
}

#[test]
fn node_floor_is_private_after_the_prelude() {
    let (out, rc, _) = node_run(
        r#"console.log(["__host_loop_install", "__host_loop_set_immediate", "__host_tick_scheduled",
          "__host_run_microtasks", "__host_checkpoint", "__host_immediate_info", "__host_timer_ref"]
          .map(function (n) { return typeof globalThis[n]; }).join());
        console.log(Object.keys(globalThis).filter(function (k) { return /^(setImmediate|queueMicrotask|setTimeout)$/.test(k); }).join());"#,
    );
    assert_eq!(rc, 0);
    assert_eq!(
        out.lines().next().unwrap(),
        "undefined,undefined,undefined,undefined,undefined,undefined,undefined"
    );
    let mut keys: Vec<&str> = out.lines().nth(1).unwrap().split(',').collect();
    keys.sort();
    assert_eq!(keys, ["queueMicrotask", "setImmediate", "setTimeout"]);
}

#[test]
fn node_ticks_run_before_promise_jobs_and_timers_return_objects() {
    let (out, rc, _) = node_run(
        r#"Promise.resolve().then(function () { console.log("promise"); });
        process.nextTick(function () { console.log("tick"); });
        var t = setTimeout(function () {}, 1);
        console.log(typeof t, typeof +t, t.hasRef());
        clearTimeout(t);"#,
    );
    assert_eq!((out.as_str(), rc), ("object number true\ntick\npromise", 0));
}

#[test]
fn node_uncaught_exception_is_fatal_with_exit_event_and_status_one() {
    let (out, rc, err) = node_run(
        r#"process.on("exit", function (c) { console.log("exit", c); });
        setTimeout(function () { console.log("never"); }, 30);
        setTimeout(function () { throw new Error("boom"); }, 1);"#,
    );
    assert_eq!((out.as_str(), rc), ("exit 1", 1));
    assert!(err.contains("boom"), "{err}");
}

#[test]
fn node_unhandled_rejection_is_fatal_unless_handled() {
    let (_, rc, err) = node_run(r#"Promise.reject(new Error("rejected-boom"));"#);
    assert_eq!(rc, 1);
    assert!(err.contains("rejected-boom"), "{err}");
    let (out, rc, _) = node_run(
        r#"process.on("unhandledRejection", function (r) { console.log("seen", r); });
        Promise.reject("why");"#,
    );
    assert_eq!((out.as_str(), rc), ("seen why", 0));
    let (_, rc, _) = node_run(
        r#"var p = Promise.reject(1); process.nextTick(function () { p.catch(function () {}); });"#,
    );
    assert_eq!(rc, 0);
}

#[test]
fn node_loop_exits_when_only_unref_handles_remain() {
    let (out, rc, _) = node_run(
        r#"setTimeout(function () { console.log("never"); }, 50).unref();
        setImmediate(function () { console.log("never"); }).unref();
        process.on("beforeExit", function () { console.log("beforeExit"); });"#,
    );
    assert_eq!((out.as_str(), rc), ("beforeExit", 0));
}

#[test]
fn node_before_exit_may_schedule_more_work() {
    let (out, rc, _) = node_run(
        r#"var n = 0;
        process.on("beforeExit", function () {
          console.log("beforeExit", n);
          if (n++ < 1) setImmediate(function () { console.log("more"); });
        });
        process.on("exit", function () { console.log("exit"); });"#,
    );
    assert_eq!(
        (out.as_str(), rc),
        ("beforeExit 0\nmore\nbeforeExit 1\nexit", 0)
    );
}

#[test]
fn node_process_exit_skips_before_exit_and_pending_work() {
    let (out, rc, _) = node_run(
        r#"process.on("exit", function (c) { console.log("exit", c); process.exitCode = 5; });
        process.on("beforeExit", function () { console.log("never"); });
        setTimeout(function () { console.log("never"); }, 20);
        setImmediate(function () { process.exit(0); });"#,
    );
    assert_eq!((out.as_str(), rc), ("exit 0", 5));
}

#[test]
fn node_ordering_is_stable_under_gc_stress() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .env("JSSE_GC_STRESS", "1")
        .args([
            "--node",
            "-e",
            r#"var log = [];
            process.on("unhandledRejection", function (r) { log.push("rej " + r); });
            setImmediate(function (a) { log.push("imm " + a); process.nextTick(function () { log.push("imm-tick"); }); }, 7);
            setTimeout(function () { log.push("timer"); Promise.reject("late"); }, 1);
            process.nextTick(function () { log.push("tick"); });
            Promise.resolve().then(function () { log.push("promise"); });
            process.on("exit", function () { console.log(log.join(",")); });"#,
        ])
        .output()
        .expect("run jsse");
    assert_eq!(stdout(&out), "tick,promise,imm 7,imm-tick,timer,rej late");
    assert_eq!(out.status.code(), Some(0));
}
