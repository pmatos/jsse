//! Node host mode `fs.watch`, `fs.watchFile` and `fs.promises.watch` (#1235):
//! liveness of watchers, the scripted create/modify/delete/rename fixture, GC
//! rooting of watcher callbacks and the polling fallback. The same fixture is
//! diffed against Node by `scripts/run-node-fs-oracle.sh`; its output here is
//! compared with the recorded Node output in `scripts/fs-fixtures/`.
#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(180);

fn scratch_in(base: &Path, name: &str) -> PathBuf {
    let dir = base.join(format!("jsse-node-fs-watch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn scratch(name: &str) -> PathBuf {
    scratch_in(&std::env::temp_dir(), name)
}

/// A scratch directory on a file system that gives a removed directory's inode
/// number to the next directory created (ext4 does, btrfs and tmpfs do not),
/// when the temp dir or the build tree is on one. A directory replaced there
/// looks like the old one to anything comparing inodes (#1377).
fn scratch_reusing_inodes(name: &str) -> PathBuf {
    let candidates = [
        std::env::temp_dir(),
        PathBuf::from("/var/tmp"),
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")),
    ];
    for base in &candidates {
        let probe = scratch_in(base, &format!("{name}-probe"));
        let reuses = inode_of_new_dir(&probe.join("d")) == inode_of_new_dir(&probe.join("d"));
        let _ = std::fs::remove_dir_all(&probe);
        if reuses {
            return scratch_in(base, name);
        }
    }
    scratch(name)
}

fn inode_of_new_dir(path: &Path) -> u64 {
    std::fs::create_dir(path).unwrap();
    let ino = std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(path).unwrap());
    std::fs::remove_dir(path).unwrap();
    ino
}

struct Session {
    child: Child,
    lines: Receiver<String>,
}

impl Session {
    fn start(script: &str, dir: &Path, env: &[(&str, &str)]) -> Session {
        let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
        command
            .args(["--node", "-e", script])
            .env("DIR", dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for (key, value) in env {
            command.env(key, value);
        }
        let mut child = command.spawn().expect("run jsse");
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        Session { child, lines }
    }

    fn next_line(&mut self, deadline: Instant) -> Option<String> {
        let left = deadline.saturating_duration_since(Instant::now());
        match self.lines.recv_timeout(left) {
            Ok(line) => Some(line),
            Err(RecvTimeoutError::Disconnected) => None,
            Err(RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                panic!("timed out waiting for the process");
            }
        }
    }

    fn wait_for(&mut self, expected: &str) {
        let deadline = Instant::now() + DEADLINE;
        while let Some(line) = self.next_line(deadline) {
            if line == expected {
                return;
            }
        }
        panic!("process ended before printing {expected:?}");
    }

    /// Everything the process prints until it exits by itself, and its status.
    fn finish(mut self) -> (Vec<String>, i32) {
        let deadline = Instant::now() + DEADLINE;
        let mut out = Vec::new();
        while let Some(line) = self.next_line(deadline) {
            out.push(line);
        }
        let status = self.child.wait().unwrap();
        (out, status.code().unwrap_or(-1))
    }
}

fn run_script(script: &str, dir: &Path) -> (Vec<String>, i32) {
    Session::start(script, dir, &[]).finish()
}

#[test]
fn the_watch_floor_is_absent_without_node_and_private_with_it() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args([
            "-e",
            "print(typeof __host_watch_add, typeof __host_watch_remove)",
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "undefined undefined"
    );
    let dir = scratch("gate");
    let (out, rc) = run_script(
        "print(typeof __host_watch_add, typeof __host_watch_remove, typeof require('fs').watch, typeof require('fs').watchFile, typeof require('fs/promises').watch)",
        &dir,
    );
    assert_eq!(
        (out.join("\n").as_str(), rc),
        ("undefined undefined function function function", 0)
    );
}

#[test]
fn a_persistent_watcher_keeps_the_process_alive_until_its_event() {
    let dir = scratch("alive");
    let mut session = Session::start(
        "const fs = require('fs'); const w = fs.watch(process.env.DIR, (type, name) => { console.log('event', type, name); w.close(); }); console.log('ready');",
        &dir,
        &[],
    );
    session.wait_for("ready");
    std::fs::write(dir.join("created"), "x").unwrap();
    let (out, rc) = session.finish();
    assert_eq!((out, rc), (vec!["event rename created".to_string()], 0));
}

#[test]
fn unref_close_and_non_persistent_release_the_loop() {
    let dir = scratch("release");
    for body in [
        "const w = fs.watch(dir); w.unref();",
        "fs.watch(dir, { persistent: false });",
        "const w = fs.watch(dir); w.close();",
        "const w = fs.watch(dir); w.unref(); w.ref(); w.unref(); w.unref();",
        "const w = fs.watch(dir, { recursive: true }); w.unref();",
        "fs.watch(dir, { signal: AbortSignal.abort() });",
        "fs.watchFile(dir + '/missing', { persistent: false }, () => {});",
        "fs.watchFile(dir + '/missing', () => {}).unref();",
        "fs.watchFile(dir + '/missing', () => {}); fs.unwatchFile(dir + '/missing');",
    ] {
        let script = format!(
            "const fs = require('fs'), fsp = require('fs/promises'), dir = process.env.DIR; {body} console.log('end');"
        );
        let (out, rc) = run_script(&script, &dir);
        assert_eq!((out, rc), (vec!["end".to_string()], 0), "{body}");
    }
}

#[test]
fn ref_after_unref_holds_the_loop_again() {
    let dir = scratch("reref");
    let mut session = Session::start(
        "const fs = require('fs'); const w = fs.watch(process.env.DIR, () => { console.log('event'); w.close(); }); w.unref(); w.ref(); console.log('ready');",
        &dir,
        &[],
    );
    session.wait_for("ready");
    std::fs::write(dir.join("x"), "x").unwrap();
    assert_eq!(session.finish(), (vec!["event".to_string()], 0));
}

#[test]
fn watch_file_keeps_the_process_alive_until_unwatched() {
    let dir = scratch("poll-alive");
    let file = dir.join("polled");
    std::fs::write(&file, "1").unwrap();
    let mut session = Session::start(
        "const fs = require('fs'); const f = process.env.DIR + '/polled'; fs.watchFile(f, { interval: 10 }, (cur, prev) => { console.log('change', cur.size > prev.size); fs.unwatchFile(f); }); console.log('ready');",
        &dir,
        &[],
    );
    session.wait_for("ready");
    // The first poll only records the baseline, so keep appending to the file
    // until the process reports a change. Appending never shrinks the file, so
    // no poll can see a truncated size.
    let deadline = Instant::now() + DEADLINE;
    let line = loop {
        let mut appender = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        std::io::Write::write_all(&mut appender, b"x").unwrap();
        drop(appender);
        match session.lines.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => break line,
            Err(RecvTimeoutError::Timeout) if Instant::now() < deadline => {}
            Err(error) => panic!("no change reported: {error:?}"),
        }
    };
    assert_eq!(line, "change true");
    assert_eq!(session.finish(), (vec![], 0));
}

#[test]
fn an_iterating_promises_watch_ends_with_its_loop_when_broken_out_of() {
    let dir = scratch("promises");
    let mut session = Session::start(
        "const fsp = require('fs/promises'); (async () => { const it = fsp.watch(process.env.DIR); setImmediate(() => console.log('ready')); for await (const e of it) { console.log(e.eventType, e.filename); break; } console.log('left'); })();",
        &dir,
        &[],
    );
    session.wait_for("ready");
    std::fs::write(dir.join("p"), "x").unwrap();
    assert_eq!(
        session.finish(),
        (vec!["rename p".to_string(), "left".to_string()], 0)
    );
}

#[test]
fn a_listener_that_throws_reaches_uncaught_exception_and_the_watcher_survives() {
    let dir = scratch("throws");
    let mut session = Session::start(
        "const fs = require('fs'); let n = 0; const w = fs.watch(process.env.DIR, () => { if (++n === 1) throw new Error('boom'); console.log('second'); w.close(); }); process.on('uncaughtException', (e) => console.log('caught', e.message)); console.log('ready');",
        &dir,
        &[],
    );
    session.wait_for("ready");
    std::fs::write(dir.join("a"), "x").unwrap();
    std::fs::write(dir.join("b"), "x").unwrap();
    let (out, rc) = session.finish();
    assert_eq!(rc, 0);
    assert_eq!(&out[..2], ["caught boom", "second"]);
}

#[test]
fn closing_one_watcher_from_a_sibling_listener_stops_its_delivery() {
    let dir = scratch("cross-close");
    let mut session = Session::start(
        "const fs = require('fs'); const d = process.env.DIR; let b; const a = fs.watch(d, () => { b.close(); a.close(); console.log('a saw'); }); b = fs.watch(d, () => console.log('b saw')); b.on('close', () => console.log('b closed')); console.log('ready');",
        &dir,
        &[],
    );
    session.wait_for("ready");
    std::fs::write(dir.join("a"), "x").unwrap();
    let (out, rc) = session.finish();
    assert_eq!(rc, 0);
    assert!(!out.contains(&"b saw".to_string()), "{out:?}");
    assert!(out.contains(&"a saw".to_string()), "{out:?}");
}

#[cfg(target_os = "linux")]
fn run_fixture(name: &str, env: &[(&str, &str)]) -> String {
    let dir = scratch(name);
    let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
    command
        .args([
            "--node",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/fs-fixtures/watch.fixture.js"
            ),
        ])
        .env("FS_ORACLE_DIR", &dir);
    for (key, value) in env {
        command.env(key, value);
    }
    let out = command.output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    text
}

#[cfg(target_os = "linux")]
fn golden() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/fs-fixtures/watch.golden.txt"
    ))
    .unwrap()
}

#[test]
#[cfg(target_os = "linux")]
fn the_scripted_fixture_matches_the_recorded_node_output() {
    assert_eq!(run_fixture("fixture", &[]), golden());
}

#[test]
#[cfg(target_os = "linux")]
fn the_fixture_survives_gc_stress() {
    assert_eq!(
        run_fixture("fixture-gc", &[("JSSE_GC_STRESS", "5")]),
        golden()
    );
}

#[test]
fn a_watcher_callback_is_rooted_for_the_watchers_lifetime() {
    let dir = scratch("rooting");
    let mut session = Session::start(
        "const fs = require('fs'); const d = process.env.DIR;
         (function () { const held = { tag: 'kept' }; const w = fs.watch(d, () => { console.log('callback', held.tag); w.close(); }); })();
         const junk = []; for (let i = 0; i < 2000; i++) junk.push({ i, s: 'x'.repeat(i % 7) });
         console.log('ready');",
        &dir,
        &[("JSSE_GC_STRESS", "1")],
    );
    session.wait_for("ready");
    std::fs::write(dir.join("a"), "x").unwrap();
    assert_eq!(session.finish(), (vec!["callback kept".to_string()], 0));
}

#[test]
fn the_polling_fallback_reports_create_modify_and_delete() {
    let dir = scratch("poll-fallback");
    let script = "const fs = require('fs'); const d = process.env.DIR;
        const seen = []; let waiting = null;
        const w = fs.watch(d, (type, name) => { seen.push(type + ':' + name); if (waiting && waiting.test(type + ':' + name)) { const r = waiting; waiting = null; r.resolve(); } });
        const until = (pattern) => new Promise((resolve) => { waiting = Object.assign(pattern, { resolve }); });
        (async () => {
          let next = until(/^rename:f$/); fs.writeFileSync(d + '/f', 'a'); await next;
          next = until(/^change:f$/); fs.writeFileSync(d + '/f', 'abc'); await next;
          next = until(/^rename:f$/); fs.unlinkSync(d + '/f'); await next;
          w.close();
          console.log('polled ok', typeof w._handle);
        })();";
    let (out, rc) = Session::start(script, &dir, &[("JSSE_NODE_WATCH_POLL", "1")]).finish();
    assert_eq!((out, rc), (vec!["polled ok object".to_string()], 0));
}

#[test]
#[cfg(target_os = "linux")]
fn recursive_watchers_follow_new_directories_and_drop_removed_ones() {
    let dir = scratch("recursive");
    std::fs::create_dir_all(dir.join("a/b")).unwrap();
    let script = "const fs = require('fs'); const d = process.env.DIR;
        const w = fs.watch(d, { recursive: true }, (type, name) => console.log('ev', type, name));
        setImmediate(() => console.log('ready'));";
    let mut session = Session::start(script, &dir, &[]);
    session.wait_for("ready");
    std::fs::write(dir.join("a/b/leaf"), "x").unwrap();
    session.wait_for("ev rename a/b/leaf");
    std::fs::create_dir(dir.join("a/new")).unwrap();
    session.wait_for("ev rename a/new");
    std::fs::write(dir.join("a/new/probe"), "x").unwrap();
    session.wait_for("ev rename a/new/probe");
    std::fs::remove_dir_all(dir.join("a")).unwrap();
    session.wait_for("ev rename a");
    session.child.kill().unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn a_directory_replaced_before_its_events_are_handled_is_watched_again() {
    let dir = scratch_reusing_inodes("replaced");
    std::fs::create_dir_all(dir.join("a")).unwrap();
    let script = "const fs = require('fs'); const d = process.env.DIR;
        const w = fs.watch(d, { recursive: true }, (type, name) => console.log('ev', type, name));
        fs.rmSync(d + '/a', { recursive: true }); fs.mkdirSync(d + '/a');
        console.log('ready');";
    let mut session = Session::start(script, &dir, &[]);
    session.wait_for("ready");
    session.wait_for("ev rename a");
    std::fs::write(dir.join("a/probe"), "x").unwrap();
    session.wait_for("ev rename a/probe");
    session.child.kill().unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn a_replaced_directory_tree_is_watched_down_to_its_nested_directories() {
    let dir = scratch_reusing_inodes("replaced-tree");
    std::fs::create_dir_all(dir.join("a/sub")).unwrap();
    let script = "const fs = require('fs'); const d = process.env.DIR;
        const w = fs.watch(d, { recursive: true }, (type, name) => console.log('ev', type, name));
        fs.rmSync(d + '/a', { recursive: true }); fs.mkdirSync(d + '/a/sub', { recursive: true });
        console.log('ready');";
    let mut session = Session::start(script, &dir, &[]);
    session.wait_for("ready");
    session.wait_for("ev rename a/sub");
    std::fs::write(dir.join("a/sub/probe"), "x").unwrap();
    session.wait_for("ev rename a/sub/probe");
    session.child.kill().unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn a_moved_directory_announces_its_entries_once() {
    let dir = scratch("moved");
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::write(dir.join("a/x"), "x").unwrap();
    let script = "const fs = require('fs'); const d = process.env.DIR;
        const w = fs.watch(d, { recursive: true }, (type, name) => console.log('ev', type, name));
        fs.renameSync(d + '/a', d + '/b');
        setTimeout(() => w.close(), 1000);";
    let (out, rc) = run_script(script, &dir);
    assert_eq!(rc, 0);
    assert_eq!(
        out.iter().filter(|line| *line == "ev rename b/x").count(),
        1,
        "{out:?}"
    );
}
