//! `MessageChannel`, `MessagePort` and the `structuredClone` transfer protocol
//! under `--node` (#1528). The Node-diffed fixtures live in
//! `scripts/messaging-fixtures/` (`scripts/run-node-messaging-oracle.sh`; the
//! `.stdout` files are Node's output). Without `--node` nothing of this exists
//! and `structuredClone` behaves as it always did.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/messaging-fixtures")
}

/// Run jsse and kill it after `limit`: a port that wrongly keeps the loop
/// alive or a starved timer must fail the test, not hang it.
fn run(args: &[&str], gc_stress: Option<&str>, limit: Duration) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    let mut child = cmd
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn jsse");
    let started = Instant::now();
    while child.try_wait().expect("wait").is_none() {
        if started.elapsed() > limit {
            child.kill().expect("kill");
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    child.wait_with_output().expect("collect output")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim_end().to_string()
}

fn eval(args: &[&str], source: &str) -> String {
    let mut full = args.to_vec();
    full.extend(["-e", source]);
    let out = run(&full, None, Duration::from_secs(60));
    assert!(out.status.success(), "{}", text(&out.stderr));
    text(&out.stdout)
}

fn fixture_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixtures())
        .unwrap()
        .filter_map(|e| {
            let name = e.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".fixture.js").map(str::to_string)
        })
        .collect();
    names.sort();
    assert!(names.len() >= 4, "fixtures missing: {names:?}");
    names
}

fn check_fixtures(flags: &[&str], gc_stress: Option<&str>) {
    for name in fixture_names() {
        let file = fixtures().join(format!("{name}.fixture.js"));
        let mut args = vec!["--node"];
        args.extend_from_slice(flags);
        args.push(file.to_str().unwrap());
        let out = run(&args, gc_stress, Duration::from_secs(120));
        assert!(
            out.status.success(),
            "{name} {flags:?}: {}",
            text(&out.stderr)
        );
        let want = std::fs::read_to_string(fixtures().join(format!("{name}.stdout"))).unwrap();
        assert_eq!(text(&out.stdout), want.trim_end(), "{name} {flags:?}");
    }
}

#[test]
fn fixtures_match_node() {
    check_fixtures(&[], None);
}

#[test]
fn fixtures_match_node_in_the_tree_walker() {
    check_fixtures(&["--no-bytecode"], None);
}

#[test]
fn fixtures_match_node_under_gc_stress() {
    check_fixtures(&[], Some("20"));
}

#[test]
fn nothing_exists_without_node() {
    let out = eval(
        &[],
        r#"print([typeof MessageChannel, typeof MessagePort, typeof MessageEvent,
          typeof ReadableStream].join())"#,
    );
    assert_eq!(out, "undefined,undefined,undefined,undefined");
}

#[test]
fn structured_clone_is_unchanged_without_node() {
    let out = eval(
        &[],
        r#"class A { constructor() { this.x = 1; } }
        var plain = structuredClone(new A());
        var b = new ArrayBuffer(4);
        var moved = structuredClone({ b: b }, { transfer: [b] });
        var errors = [];
        for (var list of [[{}], [new Map()]]) {
          try { structuredClone(1, { transfer: list }); } catch (e) { errors.push(e.name + ':' + e.message); }
        }
        print([Object.getPrototypeOf(plain) === Object.prototype, plain.x, b.byteLength,
          moved.b.byteLength, errors.join('|')].join())"#,
    );
    assert_eq!(
        out,
        "true,1,0,4,DataCloneError:Cannot transfer object of unsupported type.|\
         DataCloneError:Cannot transfer object of unsupported type."
    );
}

#[test]
fn the_mark_is_invisible_and_kclone_deserializes() {
    let source = r#"
        const { markTransferMode, markAsUncloneable, kClone, kDeserialize } =
          require('internal/worker/js_transferable');
        const { MessageChannel } = require('internal/worker/io');
        MessageChannel.prototype[kDeserialize] = function (data) { this.restored = data; };
        class Box {
          constructor(v) { this.v = v; markTransferMode(this, true); }
          [kClone]() {
            return { data: { v: this.v, at: new Date(0) },
                     deserializeInfo: 'internal/worker/io:MessageChannel' };
          }
        }
        const box = new Box(7);
        const copy = structuredClone(box);
        console.log(Reflect.ownKeys(box).join(), Object.getOwnPropertySymbols(box).length);
        console.log(copy instanceof MessageChannel, copy.restored.v, copy.restored.at.getTime());
        markAsUncloneable(box);
        try { structuredClone(box); } catch (e) { console.log(e.name, e.message); }
        const plain = {};
        markTransferMode(plain, false, false);
        try { structuredClone(plain); } catch (e) { console.log(e.name, e.message); }
    "#;
    assert_eq!(
        eval(&["--node", "--expose-internals"], source),
        "v 0\ntrue 7 0\nDataCloneError Cannot clone object of unsupported type.\n\
         DataCloneError Cannot clone object of unsupported type."
    );
}

#[test]
fn ports_answering_each_other_do_not_starve_timers() {
    let out = eval(
        &["--node"],
        r#"const { port1, port2 } = new MessageChannel();
        let count = 0;
        let stop = false;
        port1.onmessage = () => { if (!stop) port1.postMessage(0); };
        port2.onmessage = () => { count++; if (!stop) port2.postMessage(0); };
        port2.postMessage(0);
        setTimeout(() => {
          stop = true;
          port1.close();
          console.log('timer ran', count > 10);
        }, 20);"#,
    );
    assert_eq!(out, "timer ran true");
}

#[test]
fn a_message_runs_before_an_immediate() {
    let out = eval(
        &["--node"],
        r#"const { port1, port2 } = new MessageChannel();
        port2.onmessage = () => { console.log('message'); port2.close(); };
        setImmediate(() => console.log('immediate'));
        port1.postMessage(1);"#,
    );
    assert_eq!(out, "message\nimmediate");
}

#[test]
fn a_ref_d_idle_port_holds_the_loop_and_close_releases_it() {
    let source = r#"const { port1, port2 } = new MessageChannel();
        port2.onmessage = () => {};
        setTimeout(() => { console.log('closing'); port2.close(); }, 30);"#;
    assert_eq!(eval(&["--node"], source), "closing");
    let source = r#"const { port2 } = new MessageChannel();
        port2.onmessage = () => {};
        setTimeout(() => { console.log('timer'); }, 30);
        setTimeout(() => { console.log('alive'); process.exit(0); }, 400).unref();"#;
    assert_eq!(eval(&["--node"], source), "timer\nalive");
}
