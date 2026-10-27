//! The esbuild JS API under `--node` (#1252). esbuild spawns its native binary once and talks to it
//! over stdio pipes with a binary protocol, so the first test reproduces that process shape with
//! jsse as the stand-in service and needs nothing installed. The others run
//! `scripts/esbuild-fixtures/*.fixture.js` against a real esbuild 0.27.3: set ESBUILD_DIR to the
//! installed `esbuild` package (its platform binary next to it) and they compare stdout with the
//! digests Node produced (`scripts/run-node-esbuild-oracle.sh --pin`); without it each prints a
//! notice and passes. TS_SRC_DIR adds the real TypeScript bundle.
#![cfg(unix)]

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const SERVICE: &str = r#"
const chunks = [];
process.stdin.on('data', (chunk) => { chunks.push(chunk); process.stdout.write(chunk); });
process.stdin.on('end', () => {
  require('fs').writeFileSync(process.argv[2], String(Buffer.concat(chunks).length));
  process.exit(0);
});
"#;

const CLIENT: &str = r#"
const cp = require('child_process');
const crypto = require('crypto');
const [serviceFile, doneFile, size] = process.argv.slice(2);
const child = cp.spawn(process.execPath, ['--node', serviceFile, doneFile], { stdio: ['pipe', 'pipe', 'inherit'] });
let refCount = 0;
child.unref();
child.stdin.unref();
child.stdout.unref();
const refs = {
  ref() { if (++refCount === 1) child.ref(); },
  unref() { if (--refCount === 0) child.unref(); },
};
let received = 0;
const hash = crypto.createHash('sha256');
let finish;
child.stdout.on('data', (chunk) => { received += chunk.length; hash.update(chunk); if (received === total) finish(); });
const total = Number(size);
const payload = Buffer.alloc(total);
for (let i = 0; i < total; i++) payload[i] = (i * 31 + 7) & 255;
const sent = crypto.createHash('sha256').update(payload).digest('hex');
refs.ref();
const done = new Promise((resolve) => { finish = resolve; });
child.stdin.write(payload);
done.then(() => {
  refs.unref();
  console.log(received, hash.digest('hex') === sent);
});
"#;

fn jsse() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    cmd.env_remove("NODE_OPTIONS")
        .env_remove("FORCE_COLOR")
        .env_remove("NO_COLOR")
        .env_remove("COLUMNS");
    cmd
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("jsse-esbuild-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stand_in_service(size: usize, gc_stress: Option<&str>) {
    let dir = Scratch::new(&format!("service-{size}-{}", gc_stress.unwrap_or("0")));
    let service = dir.0.join("service.js");
    let client = dir.0.join("client.js");
    let done = dir.0.join("done");
    std::fs::write(&service, SERVICE).unwrap();
    std::fs::write(&client, CLIENT).unwrap();
    let mut cmd = jsse();
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    let out = cmd
        .arg("--node")
        .arg(&client)
        .arg(&service)
        .arg(&done)
        .arg(size.to_string())
        .output()
        .expect("run jsse");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}\n{stdout}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout.trim_end(), format!("{size} true"));
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        std::fs::read_to_string(&done).expect("the service saw EOF and exited"),
        size.to_string()
    );
}

#[test]
fn long_lived_unref_service_child_echoes_large_messages_and_sees_eof() {
    stand_in_service(6 * 1024 * 1024, None);
}

#[test]
fn long_lived_unref_service_child_survives_gc_stress() {
    stand_in_service(160 * 1024, Some("23"));
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/esbuild-fixtures")
}

fn esbuild_dir() -> Option<PathBuf> {
    let dir = std::fs::canonicalize(std::env::var_os("ESBUILD_DIR")?).ok()?;
    let manifest = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let version = manifest
        .split("\"version\"")
        .nth(1)
        .and_then(|rest| rest.split('"').nth(1));
    let binary = std::env::var_os("ESBUILD_BINARY_PATH").is_some()
        || dir
            .parent()
            .and_then(|modules| std::fs::read_dir(modules.join("@esbuild")).ok())
            .is_some_and(|mut platforms| platforms.next().is_some());
    (dir.join("lib/main.js").is_file() && version == Some("0.27.3") && binary).then_some(dir)
}

fn skip() -> bool {
    if esbuild_dir().is_none() {
        eprintln!(
            "SKIP: set ESBUILD_DIR to an installed esbuild 0.27.3 package to run the esbuild tests"
        );
        return true;
    }
    false
}

fn pinned() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(fixtures().join("expected-sha256.txt")).unwrap();
    text.lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(name, digest)| (name.to_string(), digest.to_string()))
        .collect()
}

fn expected_status(source: &str) -> i32 {
    source
        .lines()
        .find_map(|line| line.strip_prefix("// RUN: "))
        .and_then(|status| status.trim().parse().ok())
        .expect("fixture RUN line")
}

fn run_fixture(name: &str, gc_stress: Option<&str>) -> (Output, i32) {
    let source = std::fs::read_to_string(fixtures().join(format!("{name}.fixture.js"))).unwrap();
    let dir = Scratch::new(&format!("{name}-{}", gc_stress.unwrap_or("0")));
    copy_tree(&fixtures().join("project"), &dir.0);
    let mut cmd = jsse();
    cmd.current_dir(&dir.0)
        .env("ESBUILD_DIR", esbuild_dir().unwrap())
        .arg("--node")
        .arg(fixtures().join(format!("{name}.fixture.js")));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    (cmd.output().expect("run jsse"), expected_status(&source))
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn fixtures_match_the_digests_pinned_from_node() {
    if skip() {
        return;
    }
    for (name, expected) in pinned() {
        if name == "typescript-bundle" && std::env::var_os("TS_SRC_DIR").is_none() {
            continue;
        }
        let (out, status) = run_fixture(&name, None);
        assert_eq!(
            out.status.code(),
            Some(status),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            digest(&out.stdout),
            expected,
            "{name} stdout:\n{}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

#[test]
fn fixtures_survive_gc_stress() {
    if skip() {
        return;
    }
    let pins = pinned();
    for name in [
        "build-cjs",
        "plugins",
        "context",
        "lifecycle-exit",
        "transform",
    ] {
        let expected = &pins.iter().find(|(n, _)| n == name).unwrap().1;
        let (out, status) = run_fixture(name, Some("7"));
        assert_eq!(out.status.code(), Some(status), "{name}");
        assert_eq!(
            &digest(&out.stdout),
            expected,
            "{name} under JSSE_GC_STRESS=7"
        );
    }
}
