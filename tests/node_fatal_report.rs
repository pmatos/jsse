//! The `--node` fatal-error report (#1229): the stderr and exit status of an
//! uncaught exception or unhandled rejection match Node's for every
//! `scripts/event-loop-fixtures/*.stderr` expectation, which
//! `scripts/run-node-event-loop-oracle.sh` generates from Node itself and
//! normalises the same way as `normalise` below.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/event-loop-fixtures")
        .canonicalize()
        .unwrap()
}

fn normalise(stderr: &str, dir: &Path) -> String {
    let text = stderr.replace(dir.to_str().unwrap(), "<dir>");
    let mut out = String::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let trimmed = trimmed
            .strip_prefix("\x1b[90m")
            .unwrap_or(trimmed)
            .trim_start();
        let internal = trimmed.strip_prefix("at ").is_some_and(|frame| {
            let frame = frame.strip_prefix("async ").unwrap_or(frame);
            frame.starts_with("node:") || frame.contains(" (node:")
        });
        if internal {
            continue;
        }
        if line.starts_with("Node.js v") {
            out.push_str("Node.js <version>\n");
        } else if let Some(at) = line.find("... ")
            && line[at + 4..].contains(" lines matching cause")
        {
            let rest = line[at + 4..].trim_start_matches(|c: char| c.is_ascii_digit());
            out.push_str(&format!("{}... N{rest}\n", &line[..at]));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn header(text: &str, key: &str) -> Vec<String> {
    let prefix = format!("// {key}:");
    text.lines()
        .filter_map(|l| l.strip_prefix(&prefix))
        .map(|v| v.trim().to_string())
        .collect()
}

fn expected_status(text: &str) -> i32 {
    header(text, "RUN")[0].parse().unwrap()
}

fn check_all(args: &[&str], gc_stress: bool) {
    let dir = fixture_dir();
    let mut checked = 0;
    let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".stderr"))
        else {
            continue;
        };
        let fixture = ["js", "mjs"]
            .map(|ext| dir.join(format!("{name}.fixture.{ext}")))
            .into_iter()
            .find(|path| path.exists())
            .unwrap_or_else(|| panic!("no fixture for {name}.stderr"));
        let text = std::fs::read_to_string(&fixture).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
        cmd.arg("--node").args(args).current_dir(&dir);
        if header(&text, "MODE").iter().any(|mode| mode == "eval") {
            cmd.arg("-e").arg(text.trim_end_matches('\n'));
        } else {
            cmd.arg(&fixture);
        }
        for name in ["FORCE_COLOR", "NO_COLOR", "NODE_DISABLE_COLORS"] {
            cmd.env_remove(name);
        }
        for pair in header(&text, "ENV") {
            let (name, value) = pair.split_once('=').unwrap();
            cmd.env(name, value);
        }
        if gc_stress {
            cmd.env("JSSE_GC_STRESS", "7");
        }
        let out = cmd.output().unwrap();
        let expected = std::fs::read_to_string(&path).unwrap();
        let got = normalise(&String::from_utf8_lossy(&out.stderr), &dir);
        assert_eq!(got, expected, "{name} {args:?}");
        assert_eq!(
            out.status.code(),
            Some(expected_status(&text)),
            "{name} {args:?}"
        );
        checked += 1;
    }
    assert!(checked > 40, "only {checked} fixtures with expectations");
}

#[test]
fn fatal_reports_match_node() {
    check_all(&[], false);
}

#[test]
fn fatal_reports_match_node_on_the_tree_walker() {
    check_all(&["--no-bytecode"], false);
}

#[test]
fn fatal_reports_match_node_under_gc_stress() {
    check_all(&[], true);
}

#[test]
fn default_mode_has_no_throw_site_primitive() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args([
            "-e",
            "print(typeof __host_stack_throw_site); try { throw 1 } catch (e) {}",
        ])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "undefined");
}
