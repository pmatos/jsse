//! `child_process` and process signals under `--node` (#1236), reached through
//! `require`. `scripts/run-node-child-process-oracle.sh` diffs the same
//! behaviour against Node; these tests need no Node and add the GC-stress,
//! large-transfer and event-loop liveness checks.

use std::process::{Command, Output};
use std::time::{Duration, Instant};

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

fn node_with(source: &str, gc_stress: Option<&str>) -> String {
    let out = run(&["--node", "-e", source], gc_stress);
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stderr),
        stdout(&out)
    );
    stdout(&out)
}

fn node(source: &str) -> String {
    node_with(source, None)
}

#[test]
fn surface_is_absent_without_node() {
    let source = "print([typeof __host_cp_spawn, typeof __host_cp_spawn_sync, typeof __host_kill, \
                  typeof __host_signal_watch, typeof __host_getpid, typeof require].join(' '))";
    let out = run(&["-e", source], None);
    assert_eq!(
        stdout(&out),
        "undefined undefined undefined undefined undefined undefined"
    );
}

#[test]
fn floor_is_private_under_node() {
    let source = r#"
      print([typeof __host_cp_spawn, typeof __host_kill, typeof __host_cp_start,
             typeof globalThis[Symbol.for("jsse.node.child")],
             typeof require("child_process").spawn].join(" "));
      try { require("internal/child_process"); } catch (e) { print(e.code); }
    "#;
    assert_eq!(
        node(source),
        "undefined undefined undefined undefined function\nMODULE_NOT_FOUND"
    );
}

const SMALL: &str = r#"
  const cp = require("child_process");
  const log = [];
  const c = cp.spawn("sh", ["-c", "read line; echo got:$line; echo err >&2; exit 3"]);
  c.stdout.on("data", (d) => log.push("out " + String(d).trim()));
  c.stderr.on("data", (d) => log.push("err " + String(d).trim()));
  c.on("spawn", () => log.push("spawn"));
  c.on("exit", (code, signal) => log.push("exit " + code + " " + signal));
  c.on("close", (code) => {
    log.push("close " + code);
    const r = cp.spawnSync("cat", { input: "sync" });
    log.push(String(r.stdout) + " " + r.status);
    cp.execFile("sh", ["-c", "echo $0; exit 2"], { encoding: "buffer" }, (error, stdout) => {
      log.push("exec " + error.code + " " + JSON.stringify(String(stdout)));
      print(log.join("|"));
    });
  });
  c.stdin.end("hello\n");
"#;

const SMALL_EXPECTED: &str =
    "spawn|out got:hello|err err|exit 3 null|close 3|sync 0|exec 2 \"sh\\n\"";

fn same_events(got: &str) -> bool {
    let mut got: Vec<_> = got.split('|').collect();
    let mut want: Vec<_> = SMALL_EXPECTED.split('|').collect();
    got.sort_unstable();
    want.sort_unstable();
    got == want && got.len() == want.len()
}

#[test]
fn spawn_pipes_events_and_exec() {
    assert!(same_events(&node(SMALL)), "{}", node(SMALL));
}

/// `exec` with a string encoding goes through `Readable.setEncoding`, which
/// needs the `string_decoder` module (#1337); this test runs once it exists.
#[test]
fn exec_decodes_output_once_string_decoder_exists() {
    if node("try { require('string_decoder'); print('yes'); } catch { print('no'); }") != "yes" {
        return;
    }
    let source = r#"
      const cp = require("child_process");
      cp.exec("echo out; echo err >&2; exit 4", (error, stdout, stderr) =>
        print(error.code, JSON.stringify(stdout), JSON.stringify(stderr)));
    "#;
    assert_eq!(node(source), r#"4 "out\n" "err\n""#);
    for period in ["1", "7"] {
        assert_eq!(node_with(source, Some(period)), r#"4 "out\n" "err\n""#);
    }
}

#[test]
fn children_survive_gc_stress() {
    for period in ["1", "7"] {
        let got = node_with(SMALL, Some(period));
        assert!(same_events(&got), "period {period}: {got}");
    }
}

#[test]
fn signals_and_kill_survive_gc_stress() {
    let source = r#"
      const cp = require("child_process");
      let hits = 0;
      process.on("SIGUSR2", (name, number) => {
        hits++;
        if (hits === 2) {
          process.removeAllListeners("SIGUSR2");
          const sleeper = cp.spawn("sleep", ["30"]);
          sleeper.on("exit", (code, signal) => print("sleeper " + code + " " + signal));
          sleeper.on("spawn", () => print("kill " + process.kill(sleeper.pid, "SIGKILL")));
        }
      });
      process.kill(process.pid, "SIGUSR2");
      setTimeout(() => process.kill(process.pid, "SIGUSR2"), 20);
      setTimeout(() => {}, 100);
    "#;
    for period in [None, Some("1")] {
        let got = node_with(source, period);
        assert!(got.ends_with("sleeper null SIGKILL"), "{got}");
        assert!(got.contains("kill true"), "{got}");
    }
}

#[test]
fn large_outputs_stream_without_deadlock() {
    let source = r#"
      const cp = require("child_process");
      const counts = { out: 0, err: 0, back: 0 };
      const c = cp.spawn("sh", ["-c", "head -c 4000000 /dev/zero & head -c 3000000 /dev/zero >&2; wait"]);
      c.stdout.on("data", (d) => { counts.out += d.length; });
      c.stderr.on("data", (d) => { counts.err += d.length; });
      c.on("close", () => {
        const echo = cp.spawn("cat");
        echo.stdout.on("data", (d) => { counts.back += d.length; });
        echo.on("close", () => print(JSON.stringify(counts)));
        echo.stdin.end(new Uint8Array(5 * 1024 * 1024));
      });
    "#;
    assert_eq!(
        node(source),
        r#"{"out":4000000,"err":3000000,"back":5242880}"#
    );
}

#[test]
fn spawn_sync_moves_large_input_and_output_at_once() {
    let source = r#"
      const cp = require("child_process");
      const input = new Uint8Array(4 * 1024 * 1024).fill(7);
      const r = cp.spawnSync("sh", ["-c", "cat; head -c 2000000 /dev/zero >&2"],
                             { input, maxBuffer: Infinity });
      print(r.status, r.stdout.length, r.stderr.length, r.stdout[1000]);
      const limited = cp.spawnSync("head", ["-c", "100000", "/dev/zero"], { maxBuffer: 1000 });
      print(limited.error.code, limited.stdout.length > 1000);
      const slow = cp.spawnSync("sleep", ["5"], { timeout: 50 });
      print(slow.error.code, slow.signal, slow.status);
      const unlimited = cp.spawnSync("echo", ["x"], { maxBuffer: 0 });
      print(unlimited.error, String(unlimited.stdout).trim());
      const lingering = cp.spawnSync("sh", ["-c", "sleep 3 & echo x"], { timeout: 300 });
      print(lingering.error.code, lingering.status);
      const t0 = Date.now();
      const orphaned = cp.spawnSync("sh", ["-c", "sleep 4 & sleep 4"], { timeout: 100 });
      print(orphaned.error.code, orphaned.signal, Date.now() - t0 < 2000);
      const unread = cp.spawnSync("true", { input: new Uint8Array(8e6) });
      print(unread.error.code, unread.status);
    "#;
    assert_eq!(
        node(source),
        "0 4194304 2000000 7\nENOBUFS true\nETIMEDOUT SIGTERM null\nundefined x\nETIMEDOUT 0\nETIMEDOUT SIGTERM true\nEPIPE 0"
    );
}

#[test]
fn write_in_flight_when_the_child_exits_fails_with_epipe() {
    let source = r#"
      const c = require("child_process").spawn("true");
      c.stdin.on("error", (e) => print("stdin error", e.code));
      c.stdin.write(new Uint8Array(5e6), (e) => print("write callback", e && e.code));
      c.on("close", () => print("close"));
    "#;
    assert_eq!(
        node(source),
        "write callback EPIPE\nstdin error EPIPE\nclose"
    );
}

#[test]
fn child_keeps_the_loop_alive_only_while_ref() {
    let source = r#"
      const cp = require("child_process");
      const started = Date.now();
      const kept = cp.spawn("sleep", ["0.3"]);
      kept.on("exit", () => print("kept exited after >=250ms: " + (Date.now() - started >= 250)));
      const unrefed = cp.spawn("sleep", ["30"], { stdio: "ignore" });
      unrefed.unref();
      process.on("exit", () => { process.kill(unrefed.pid, "SIGKILL"); });
    "#;
    let begin = Instant::now();
    assert_eq!(node(source), "kept exited after >=250ms: true");
    assert!(begin.elapsed() < Duration::from_secs(20));
}

#[test]
fn spawn_failures_and_errors() {
    let source = r#"
      const cp = require("child_process");
      const missing = cp.spawn("no-such-command-xyz", ["a"]);
      missing.on("error", (e) => print(e.code, e.errno, e.syscall, e.path, JSON.stringify(e.spawnargs)));
      missing.on("close", (code) => print("close", code, missing.exitCode, missing.pid));
      try { cp.spawn("x", [], { stdio: ["pipe", "pipe", "pipe", "ipc"] }); }
      catch (e) { print(e.code); }
      try { process.kill(process.pid, "SIGNOPE"); } catch (e) { print(e.code); }
      try { process.kill(2 ** 22, 0); } catch (e) { print(e.code); }
      try { process.on("SIGKILL", () => {}); } catch (e) { print(e.syscall, e.code); }
      print(process.listenerCount("SIGKILL"));
    "#;
    let got = node(source);
    let lines: Vec<&str> = got.lines().collect();
    assert_eq!(
        lines[..5],
        [
            "ERR_FEATURE_UNAVAILABLE_ON_PLATFORM",
            "ERR_UNKNOWN_SIGNAL",
            "ESRCH",
            "uv_signal_start EINVAL",
            "0"
        ]
    );
    assert_eq!(
        lines[5..],
        [
            r#"ENOENT -2 spawn no-such-command-xyz no-such-command-xyz ["a"]"#,
            "close -2 -2 undefined"
        ]
    );
}
