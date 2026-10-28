//! `child_process.fork` and the IPC channel under `--node` (#1237).
//! `scripts/run-node-child-process-oracle.sh` diffs the fork fixtures against
//! Node (including jsse parent with a Node child and the reverse); these tests
//! need no Node and add GC-stress, large-message, parent-death and liveness
//! checks. The children are the fixtures' `fork-child.js` and `fork-big-child.js`.

use std::process::{Command, Output};
use std::time::{Duration, Instant};

fn fixture(name: &str) -> String {
    format!(
        "{}/scripts/child-process-fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn run_with(source: &str, gc_stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    cmd.args(["--node", "-e", source])
        .output()
        .expect("run jsse")
}

fn node_with(source: &str, gc_stress: Option<&str>) -> String {
    let source = source
        .replace("CHILD", &format!("{:?}", fixture("fork-child.js")))
        .replace("BIG", &format!("{:?}", fixture("fork-big-child.js")));
    let out = run_with(&source, gc_stress);
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn node(source: &str) -> String {
    node_with(source, None)
}

const PING_PONG: &str = r#"
  const { fork } = require("child_process");
  const count = COUNT;
  const c = fork(CHILD, ["bulk"], { serialization: "MODE" });
  let next = 0, wrong = 0;
  c.on("message", (m) => {
    if (m.n !== next || m.echo !== "x".repeat(next % 50)) wrong++;
    next++;
    if (next === count) c.send("done"); else c.send({ n: next });
  });
  c.on("close", (code, signal) => print("received", next, "wrong", wrong, "code", code, signal));
  c.send({ n: 0 });
"#;

fn ping_pong(mode: &str, count: u32, gc_stress: Option<&str>) -> String {
    let source = PING_PONG
        .replace("MODE", mode)
        .replace("COUNT", &count.to_string());
    node_with(&source, gc_stress)
}

#[test]
fn ping_pong_is_ordered_and_exit_propagates() {
    let begin = Instant::now();
    for mode in ["json", "advanced"] {
        assert_eq!(
            ping_pong(mode, 5000, None),
            "received 5000 wrong 0 code 0 null"
        );
    }
    assert!(begin.elapsed() < Duration::from_secs(120));
}

#[test]
fn ping_pong_under_gc_stress() {
    for mode in ["json", "advanced"] {
        assert_eq!(
            ping_pong(mode, 150, Some("3")),
            "received 150 wrong 0 code 0 null"
        );
    }
}

#[test]
fn large_messages_in_both_directions_at_once() {
    let source = r#"
      const { fork } = require("child_process");
      const size = 8 * 1024 * 1024;
      const c = fork(BIG, [String(size)], { serialization: "MODE" });
      const got = [];
      c.on("message", (m) => {
        got.push(typeof m === "string" ? [m.length, m.slice(-4)] : [m.length, m.head, m.tail]);
        if (got.length === 2) c.send("done");
      });
      c.on("close", (code) => print(code, JSON.stringify(got)));
      c.send("abcdefghij".repeat(size / 10) + "TAIL");
    "#;
    let expect = "0 [[8388604,\"END!\"],[8388604,\"abcd\",\"TAIL\"]]";
    for mode in ["json", "advanced"] {
        assert_eq!(node(&source.replace("MODE", mode)), expect);
    }
}

#[test]
fn child_exit_code_and_signal_reach_the_parent() {
    let source = r#"
      const { fork } = require("child_process");
      const done = [];
      const finish = (line) => { done.push(line); if (done.length === 2) print(done.sort().join(" | ")); };
      const a = fork(CHILD, ["exit"]);
      a.on("close", (code, signal) => finish("exit " + code + " " + signal));
      a.send(7);
      const b = fork(CHILD, ["hold"]);
      b.once("message", () => b.kill("SIGKILL"));
      b.on("close", (code, signal) => finish("kill " + code + " " + signal));
    "#;
    assert_eq!(node(source), "exit 7 null | kill null SIGKILL");
}

#[test]
fn an_aborted_child_and_a_signalled_worker_report_their_signal() {
    let source = r#"
      const { spawn, fork } = require("child_process");
      const done = [];
      const finish = (line) => { done.push(line); if (done.length === 2) print(done.sort().join(" | ")); };
      const abort = spawn(process.execPath, ["-e", "process.abort()"]);
      abort.on("exit", (code, signal) => finish("abort " + code + " " + signal));
      const worker = fork(CHILD, ["hold"]);
      worker.on("message", () => { worker.send("work"); setTimeout(() => worker.kill("SIGABRT"), 50); });
      let disconnected = false;
      worker.on("disconnect", () => { disconnected = true; });
      worker.on("exit", (code, signal) => finish("worker " + code + " " + signal + " " + (disconnected || worker.connected === false)));
    "#;
    let source = source.replace("CHILD", &format!("{:?}", fixture("fork-child.js")));
    let out = Command::new("sh")
        .args(["-c", r#"ulimit -c 0; exec "$0" --node -e "$1""#])
        .args([env!("CARGO_BIN_EXE_jsse"), &source])
        .output()
        .expect("run jsse");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "abort null SIGABRT | worker null SIGABRT true"
    );
}

#[test]
fn child_sees_disconnect_when_the_parent_dies() {
    let source = r#"
      const { fork } = require("child_process");
      const middle = fork(CHILD, ["middle"], { silent: true });
      let printed = "";
      middle.stdout.on("data", (d) => { printed += d; });
      middle.stdout.on("end", () => print(JSON.stringify(printed)));
      middle.once("message", () => middle.kill("SIGKILL"));
    "#;
    assert_eq!(node(source), r#""leaf: parent gone\n""#);
}

#[test]
fn a_channel_without_listeners_does_not_keep_the_child_alive() {
    let source = r#"
      const { fork } = require("child_process");
      for (const mode of ["noop", "addremove"]) {
        const c = fork(CHILD, [mode]);
        c.on("message", (m) => print(mode, m));
        c.on("close", (code) => { print(mode, "closed", code); });
      }
    "#;
    let got = node(source);
    let mut lines: Vec<&str> = got.lines().collect();
    lines.sort_unstable();
    assert_eq!(
        lines,
        [
            "addremove closed 0",
            "addremove listener removed",
            "noop closed 0",
            "noop hello"
        ]
    );
}

#[test]
fn unref_lets_the_parent_exit_with_a_live_child() {
    let source = r#"
      const { fork } = require("child_process");
      const c = fork(CHILD, ["hold"]);
      c.once("message", () => {
        c.channel.unref();
        c.unref();
        print("unref'd", c.connected);
      });
      process.on("exit", () => c.kill("SIGKILL"));
    "#;
    let begin = Instant::now();
    assert_eq!(node(source), "unref'd true");
    assert!(begin.elapsed() < Duration::from_secs(60));
}

#[test]
fn options_and_failures() {
    let source = r#"
      const { fork } = require("child_process");
      const flags = fork(CHILD, ["info"], { execArgv: ["--node"], silent: true });
      print(flags.spawnargs.filter((a) => a === "--node").length, flags.spawnargs.length);
      const plain = fork(CHILD, ["info", "x"], { silent: true });
      plain.once("message", (m) => print(m.fd, m.serialization, m.args.join(), m.connected));
      const none = fork(CHILD, [], { execPath: "/no/such/jsse", stdio: ["ignore", "ignore", "ignore", "ipc"] });
      none.on("error", (e) => print("error", e.code));
      none.on("close", (code) => print("close", code, none.connected));
      try { fork(CHILD, [], { stdio: ["pipe", "pipe", "pipe"] }); }
      catch (e) { print(e.code); }
      try { fork(CHILD, [], { serialization: "v8" }); } catch (e) { print(e.code); }
      try { flags.send("x", {}); } catch (e) { print(e.code); }
      print(typeof process.send, process.connected, typeof process.channel);
    "#;
    let got = node(source);
    let mut lines: Vec<&str> = got.lines().collect();
    let first = lines.remove(0);
    assert_eq!(first, "1 4");
    lines.sort_unstable();
    assert_eq!(
        lines,
        [
            "ERR_CHILD_PROCESS_IPC_REQUIRED",
            "ERR_INVALID_ARG_VALUE",
            "ERR_INVALID_HANDLE_TYPE",
            "close -2 false",
            "error ENOENT",
            "undefined undefined info,x true",
            "undefined undefined undefined",
        ]
    );
}

#[test]
fn surface_without_a_channel_is_node_like() {
    let source = r#"
      print(typeof process.send, process.connected, process.channel,
            process.env.NODE_CHANNEL_FD, typeof require("child_process").fork);
    "#;
    assert_eq!(
        node(source),
        "undefined undefined undefined undefined function"
    );
}
