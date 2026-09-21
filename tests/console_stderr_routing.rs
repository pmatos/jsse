//! `console.error`/`console.warn` must write to stderr, and `console.info`/
//! `console.debug` must write to stdout like `console.log` (#680). This is a
//! child-process check because the split is which OS stream receives the
//! write, not something observable from within JS.

use std::process::Command;

#[test]
fn error_and_warn_write_to_stderr_only() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["-e", "console.error('x'); console.warn('y')"])
        .output()
        .expect("run jsse");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(stdout, "", "stdout should be empty, got {stdout:?}");
    // A `perf-counters` build appends a PERF/BODY report to stderr at exit
    // (see CLAUDE.md), so only pin the exact bytes on a plain build.
    if cfg!(feature = "perf-counters") {
        assert!(
            stderr.starts_with("x\ny\n"),
            "stderr should start with \"x\\ny\\n\", got {stderr:?}"
        );
    } else {
        assert_eq!(stderr, "x\ny\n", "stderr mismatch, got {stderr:?}");
    }
}

#[test]
fn info_and_debug_write_to_stdout_only() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args([
            "-e",
            "console.info('info-marker'); console.debug('debug-marker')",
        ])
        .output()
        .expect("run jsse");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stdout, "info-marker\ndebug-marker\n",
        "stdout mismatch, got {stdout:?}"
    );
    // A `perf-counters` build appends a PERF/BODY report to stderr at exit
    // (see CLAUDE.md), so only require an exact-empty stderr on a plain
    // build; otherwise just confirm console output didn't leak there.
    if cfg!(feature = "perf-counters") {
        assert!(
            !stderr.contains("info-marker") && !stderr.contains("debug-marker"),
            "stderr should not contain console output, got {stderr:?}"
        );
    } else {
        assert_eq!(stderr, "", "stderr should be empty, got {stderr:?}");
    }
}
