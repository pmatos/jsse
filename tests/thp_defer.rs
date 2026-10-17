//! On a host with THP `enabled=always` the shipped binary starts with
//! transparent huge pages off and turns them back on once the heap has grown
//! (#1075). The kernel reports the state per process as `THP_enabled`.
#![cfg(all(feature = "jemalloc", target_os = "linux"))]

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn host_uses_huge_pages_unasked() -> bool {
    std::fs::read_to_string("/sys/kernel/mm/transparent_hugepage/enabled")
        .is_ok_and(|mode| mode.contains("[always]"))
}

fn thp_enabled(child: &Child) -> Option<bool> {
    let status = std::fs::read_to_string(format!("/proc/{}/status", child.id())).ok()?;
    let line = status.lines().find(|l| l.starts_with("THP_enabled:"))?;
    Some(line.split_whitespace().nth(1)? == "1")
}

fn stop(mut child: Child) {
    child.kill().expect("kill jsse");
    child.wait().expect("reap jsse");
}

fn spawn(script: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["-e", script])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn jsse")
}

const SPIN: &str = "var t = Date.now(); while (Date.now() - t < 30000);";

#[test]
fn a_small_process_runs_without_huge_pages() {
    if !host_uses_huge_pages_unasked() {
        return;
    }
    let child = spawn(SPIN);
    std::thread::sleep(Duration::from_millis(500));
    let state = thp_enabled(&child);
    stop(child);
    if state.is_some() {
        assert_eq!(state, Some(false));
    }
}

#[test]
fn huge_pages_return_once_the_heap_has_grown() {
    if !host_uses_huge_pages_unasked() {
        return;
    }
    let script = format!(
        "var keep = []; for (var i = 0; i < 400000; i++) keep.push({{ a: i, b: [i] }}); {SPIN}"
    );
    let child = spawn(&script);
    if thp_enabled(&child).is_none() {
        stop(child);
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut enabled = false;
    while Instant::now() < deadline {
        enabled = thp_enabled(&child) == Some(true);
        if enabled {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    stop(child);
    assert!(
        enabled,
        "huge pages were not re-enabled after the heap grew"
    );
}
