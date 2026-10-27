//! The real TypeScript 6.0.3 compiler entry under `--node` (#1242). The sources are not
//! vendored: set TSC_DIR to an installed `typescript` package and these tests run
//! `lib/_tsc.js` and `bin/tsc` on jsse; without it (or with another version) each test
//! prints a notice and passes. `scripts/run-tsc-oracle.sh` diffs the same scenarios
//! against Node and pins the sha256 of Node's `--version`/`--help`/`--all` stdout in
//! scripts/tsc-fixtures/expected-sha256.txt, which these tests check without Node.
#![cfg(unix)]

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/tsc-fixtures")
}

fn tsc_dir() -> Option<PathBuf> {
    let dir = std::fs::canonicalize(std::env::var_os("TSC_DIR")?).ok()?;
    let manifest = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let version = manifest
        .split("\"version\"")
        .nth(1)
        .and_then(|rest| rest.split('"').nth(1));
    (dir.join("lib/_tsc.js").is_file() && version == Some("6.0.3")).then_some(dir)
}

fn skip() -> bool {
    if tsc_dir().is_none() {
        eprintln!(
            "SKIP: set TSC_DIR to an installed typescript 6.0.3 package to run the tsc tests"
        );
        return true;
    }
    false
}

fn tsc(entry: &str, args: &[&str], cwd: &Path) -> Output {
    let script = tsc_dir().unwrap().join(entry);
    Command::new(env!("CARGO_BIN_EXE_jsse"))
        .env_remove("NODE_OPTIONS")
        .env_remove("FORCE_COLOR")
        .env_remove("NO_COLOR")
        .env_remove("COLUMNS")
        .current_dir(cwd)
        .arg("--node")
        .arg(script)
        .args(args)
        .output()
        .expect("run jsse")
}

struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch_tree(name: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("jsse-tsc-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for entry in std::fs::read_dir(fixtures().join("proj")).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
    }
    Scratch(dir)
}

fn pinned(tag: &str) -> String {
    let text = std::fs::read_to_string(fixtures().join("expected-sha256.txt")).unwrap();
    text.lines()
        .find_map(|l| l.strip_prefix(tag).and_then(|r| r.strip_prefix(' ')))
        .unwrap_or_else(|| panic!("no pinned digest for {tag}"))
        .to_string()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn version_matches_node() {
    if skip() {
        return;
    }
    let cwd = scratch_tree("version");
    for flag in ["--version", "-v"] {
        let out = tsc("lib/_tsc.js", &[flag], &cwd);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        assert_eq!(text(&out.stdout), "Version 6.0.3\n");
    }
    let out = tsc("lib/_tsc.js", &["--version"], &cwd);
    assert_eq!(sha256(&out.stdout), pinned("version"));
}

#[test]
fn help_and_all_match_node() {
    if skip() {
        return;
    }
    let cwd = scratch_tree("help");
    for (flag, tag) in [("--help", "help"), ("--all", "all")] {
        let out = tsc("lib/_tsc.js", &[flag], &cwd);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        assert_eq!(
            sha256(&out.stdout),
            pinned(tag),
            "{flag} stdout differs from Node's"
        );
        assert!(out.stderr.is_empty(), "{}", text(&out.stderr));
    }
}

#[test]
fn bin_entry_runs_lib_tsc() {
    if skip() {
        return;
    }
    let cwd = scratch_tree("bin");
    let out = tsc("bin/tsc", &["--version"], &cwd);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "Version 6.0.3\n");
}

#[test]
fn unknown_flag_exits_with_one() {
    if skip() {
        return;
    }
    let cwd = scratch_tree("flag");
    let out = tsc("lib/_tsc.js", &["--bogus"], &cwd);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stdout).contains("error TS5023: Unknown compiler option '--bogus'."));
}

#[test]
fn nolib_compile_emits_javascript() {
    if skip() {
        return;
    }
    let cwd = scratch_tree("emit");
    let args = [
        "--noLib",
        "--target",
        "es2020",
        "--module",
        "commonjs",
        "--pretty",
        "false",
        "--outDir",
        "out",
        "ok.ts",
        "globals.d.ts",
    ];
    let out = tsc("lib/_tsc.js", &args, &cwd);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stdout));
    assert!(out.stdout.is_empty());
    let js = std::fs::read_to_string(cwd.join("out/ok.js")).unwrap();
    assert!(js.contains("class Shape"), "{js}");
    assert!(js.contains("Color[Color[\"Red\"] = 0] = \"Red\""), "{js}");
    assert!(!js.contains("interface"), "{js}");
}

#[test]
fn nolib_compile_reports_diagnostics() {
    if skip() {
        return;
    }
    let cwd = scratch_tree("diag");
    let args = [
        "--noLib",
        "--pretty",
        "false",
        "--noEmit",
        "bad.ts",
        "globals.d.ts",
    ];
    let out = tsc("lib/_tsc.js", &args, &cwd);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        text(&out.stdout),
        "bad.ts(1,14): error TS2322: Type 'string' is not assignable to type 'number'.\n\
         bad.ts(2,47): error TS2322: Type 'number' is not assignable to type 'string'.\n\
         bad.ts(3,17): error TS2304: Cannot find name 'unknownName'.\n"
    );
}
