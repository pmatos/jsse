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
    assert!(
        stderr.contains('x'),
        "stderr should contain 'x', got {stderr:?}"
    );
    assert!(
        stderr.contains('y'),
        "stderr should contain 'y', got {stderr:?}"
    );
}

#[test]
fn info_and_debug_write_to_stdout_only() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["-e", "console.info('a'); console.debug('b')"])
        .output()
        .expect("run jsse");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains('a'),
        "stdout should contain 'a', got {stdout:?}"
    );
    assert!(
        stdout.contains('b'),
        "stdout should contain 'b', got {stdout:?}"
    );
    assert_eq!(stderr, "", "stderr should be empty, got {stderr:?}");
}
