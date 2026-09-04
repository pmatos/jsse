//! Support code for the `differential` fuzz target: runs the same source
//! through the release `jsse` binary and `node` as subprocesses and
//! classifies the result. Lives in the `fuzz` crate (not `jsse` itself)
//! since it's fuzz-harness-only: it shells out to compiled binaries rather
//! than linking the interpreter in-process (see the ADR for why).

use std::io::Read;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Matches `scripts/run-test262.py`'s `JsseAdapter.setup_preexec`.
pub const JSSE_AS_LIMIT: u64 = 512 * 1024 * 1024;

/// Matches `scripts/run-test262.py`'s `NodeAdapter.setup_preexec`: V8 needs
/// ~2 GiB just to start, so jsse's 512 MiB cap fatally OOMs it immediately
/// (verified: `prlimit --as=536870912 node -e 1` dies in `NewIsolate` before
/// running anything), independent of the fuzzed source.
pub const NODE_AS_LIMIT: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineOutcome {
    Exited { code: i32, stderr: String },
    Signaled { signal: i32, stderr: String },
    TimedOut,
}

impl EngineOutcome {
    fn error_class(&self) -> Option<&str> {
        match self {
            EngineOutcome::Exited { stderr, .. } | EngineOutcome::Signaled { stderr, .. } => {
                extract_error_class(stderr)
            }
            EngineOutcome::TimedOut => None,
        }
    }
}

/// Scans `stderr` line by line for the first `SomeError:`-shaped prefix.
/// Node prints the failing source line and a `^` caret *before* the actual
/// `Error: message` line, so "first line of stderr" is not the error class —
/// this has to scan every line. Requiring the candidate to end in `Error`
/// filters out unrelated `label:`/object-literal colons reflected from the
/// fuzzed source in the printed context lines (an exact false-positive would
/// need a source identifier that itself ends in `Error`, e.g. a label
/// `myError: for(...)`; accepted as noise per the Tier 3 design below).
fn extract_error_class(stderr: &str) -> Option<&str> {
    for line in stderr.lines() {
        let line = line.trim_start();
        if let Some(colon_idx) = line.find(':') {
            let candidate = &line[..colon_idx];
            if !candidate.is_empty()
                && candidate.ends_with("Error")
                && candidate.chars().all(|c| c.is_ascii_alphanumeric())
            {
                return Some(candidate);
            }
        }
    }
    None
}

fn status_to_outcome(status: ExitStatus, stderr: String) -> EngineOutcome {
    match status.signal() {
        Some(signal) => EngineOutcome::Signaled { signal, stderr },
        None => EngineOutcome::Exited {
            code: status.code().unwrap_or(-1),
            stderr,
        },
    }
}

fn run_with_timeout(mut cmd: Command, timeout: Duration) -> EngineOutcome {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(_) => return EngineOutcome::TimedOut,
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stderr = String::new();
                if let Some(mut s) = child.stderr.take() {
                    let _ = s.read_to_string(&mut stderr);
                }
                return status_to_outcome(status, stderr);
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return EngineOutcome::TimedOut;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => return EngineOutcome::TimedOut,
        }
    }
}

/// Runs `jsse_bin src_file` under a 512 MiB `RLIMIT_AS` via `prlimit(1)`
/// (matching `scripts/run-test262.py`) and a wall-clock `timeout`.
pub fn run_engine_subprocess(jsse_bin: &Path, src_file: &Path, timeout: Duration) -> EngineOutcome {
    let mut cmd = Command::new("prlimit");
    cmd.arg(format!("--as={JSSE_AS_LIMIT}"))
        .arg("--")
        .arg(jsse_bin)
        .arg(src_file);
    run_with_timeout(cmd, timeout)
}

/// Runs `node --expose-gc --require <node-test262-prelude.js> src_file`
/// under a 4 GiB `RLIMIT_AS` (matching `scripts/run-test262.py`'s
/// `NodeAdapter`) and a wall-clock `timeout`. The prelude defines `print`
/// (used by several corpus seeds) and gates `gc()` behind `--expose-gc`, so
/// seeds that call it don't spuriously diverge for lacking a host global.
pub fn run_node_subprocess(repo_root: &Path, src_file: &Path, timeout: Duration) -> EngineOutcome {
    let prelude = repo_root.join("scripts/node-test262-prelude.js");
    let mut cmd = Command::new("prlimit");
    cmd.arg(format!("--as={NODE_AS_LIMIT}"))
        .arg("--")
        .arg("node")
        .arg("--expose-gc")
        .arg("--require")
        .arg(prelude)
        .arg(src_file);
    run_with_timeout(cmd, timeout)
}

/// Locates the release `jsse` binary relative to this crate, following the
/// documented precondition that `cargo build --release` has already run.
pub fn jsse_release_binary() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/release/jsse")
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// jsse crashed (signal or the interpreter-panic exit code 101) while
    /// node did not crash the same way. An engine bug by definition,
    /// independent of what node does.
    Tier1(String),
    /// One side reports a parse error and the other parses successfully.
    /// Surfaces real syntax coverage gaps.
    Tier2(String),
    /// Both sides threw (possibly a different error class) or both timed
    /// out. Dominated by "jsse hasn't implemented X yet" and host-global
    /// differences — too noisy to gate on; recorded for the triage step
    /// instead of panicking.
    Recorded,
}

const INTERPRETER_PANIC_EXIT_CODE: i32 = 101;

fn is_jsse_crash(outcome: &EngineOutcome) -> bool {
    matches!(outcome, EngineOutcome::Signaled { .. })
        || matches!(
            outcome,
            EngineOutcome::Exited {
                code: INTERPRETER_PANIC_EXIT_CODE,
                ..
            }
        )
}

fn is_node_crash(outcome: &EngineOutcome) -> bool {
    matches!(outcome, EngineOutcome::Signaled { .. })
}

/// jsse's parser exits 2 on a syntax error (`exit_code_from_result` in
/// `src/main.rs`); node's prelude has no equivalent convention, so a
/// `SyntaxError` error class in its stderr is the signal instead.
fn is_syntax_error(outcome: &EngineOutcome, is_jsse: bool) -> bool {
    if is_jsse {
        matches!(outcome, EngineOutcome::Exited { code: 2, .. })
    } else {
        outcome.error_class() == Some("SyntaxError")
    }
}

pub fn classify(jsse: &EngineOutcome, node: &EngineOutcome) -> Verdict {
    if is_jsse_crash(jsse) && !is_node_crash(node) {
        return Verdict::Tier1(format!(
            "jsse crashed while node did not: jsse={jsse:?} node={node:?}"
        ));
    }
    let jsse_syntax_err = is_syntax_error(jsse, true);
    let node_syntax_err = is_syntax_error(node, false);
    if jsse_syntax_err != node_syntax_err {
        return Verdict::Tier2(format!(
            "parse accept/reject mismatch: jsse={jsse:?} node={node:?}"
        ));
    }
    Verdict::Recorded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exited(code: i32) -> EngineOutcome {
        EngineOutcome::Exited {
            code,
            stderr: String::new(),
        }
    }

    fn exited_with_stderr(code: i32, stderr: &str) -> EngineOutcome {
        EngineOutcome::Exited {
            code,
            stderr: stderr.to_string(),
        }
    }

    fn signaled(signal: i32) -> EngineOutcome {
        EngineOutcome::Signaled {
            signal,
            stderr: String::new(),
        }
    }

    #[test]
    fn both_succeed_is_recorded() {
        assert_eq!(classify(&exited(0), &exited(0)), Verdict::Recorded);
    }

    #[test]
    fn both_throw_same_class_is_recorded() {
        let jsse = exited_with_stderr(1, "TypeError: x is not a function");
        let node = exited_with_stderr(1, "TypeError: x is not a function");
        assert_eq!(classify(&jsse, &node), Verdict::Recorded);
    }

    #[test]
    fn both_throw_different_class_is_recorded_not_tier2() {
        let jsse = exited_with_stderr(1, "TypeError: x is not a function");
        let node = exited_with_stderr(1, "ReferenceError: x is not defined");
        assert_eq!(classify(&jsse, &node), Verdict::Recorded);
    }

    #[test]
    fn both_time_out_is_recorded() {
        assert_eq!(
            classify(&EngineOutcome::TimedOut, &EngineOutcome::TimedOut),
            Verdict::Recorded
        );
    }

    #[test]
    fn jsse_signal_crash_while_node_clean_is_tier1() {
        let jsse = signaled(11); // SIGSEGV
        let node = exited(0);
        assert!(matches!(classify(&jsse, &node), Verdict::Tier1(_)));
    }

    #[test]
    fn jsse_panic_exit_while_node_clean_is_tier1() {
        let jsse = exited(101);
        let node = exited(0);
        assert!(matches!(classify(&jsse, &node), Verdict::Tier1(_)));
    }

    #[test]
    fn jsse_crash_while_node_also_crashes_is_not_tier1() {
        let jsse = signaled(11);
        let node = signaled(11);
        assert_eq!(classify(&jsse, &node), Verdict::Recorded);
    }

    #[test]
    fn jsse_rejects_node_accepts_is_tier2() {
        let jsse = exited(2); // jsse SyntaxError exit code
        let node = exited(0);
        assert!(matches!(classify(&jsse, &node), Verdict::Tier2(_)));
    }

    #[test]
    fn node_rejects_jsse_accepts_is_tier2() {
        let jsse = exited(0);
        let node = exited_with_stderr(1, "SyntaxError: Unexpected token");
        assert!(matches!(classify(&jsse, &node), Verdict::Tier2(_)));
    }

    #[test]
    fn both_reject_is_recorded_not_tier2() {
        let jsse = exited(2);
        let node = exited_with_stderr(1, "SyntaxError: Unexpected token");
        assert_eq!(classify(&jsse, &node), Verdict::Recorded);
    }

    #[test]
    fn extract_error_class_skips_source_line_before_error_line() {
        let stderr = "file.js:21\n    throw new Error('boom');\n    ^\n\nError: boom\n    at foo\n";
        assert_eq!(extract_error_class(stderr), Some("Error"));
    }

    #[test]
    fn extract_error_class_none_when_absent() {
        assert_eq!(extract_error_class("nothing to see here\n"), None);
    }
}
