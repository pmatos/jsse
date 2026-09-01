use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "jsse-import-defer-async-cycle-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("failed to create scratch directory");
        Self(path)
    }

    fn write(&self, name: &str, source: &str) {
        fs::write(self.0.join(name), source).expect("failed to write module fixture");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn deferred_module_waits_for_an_in_flight_async_cycle() {
    let dir = ScratchDir::new();
    dir.write(
        "setup.js",
        r#"
globalThis.evaluations = [];
export const blocker = Promise.withResolvers();
export const aStarted = Promise.withResolvers();
"#,
    );
    dir.write(
        "a.js",
        r#"
import { blocker, aStarted } from "./setup.js";
import "./b.js";
globalThis.evaluations.push("A-before-await");
aStarted.resolve();
await blocker.promise;
globalThis.evaluations.push("A-after-await");
"#,
    );
    dir.write(
        "b.js",
        r#"
import "./a.js";
globalThis.evaluations.push("B");
"#,
    );
    dir.write(
        "d.js",
        r#"
import "./b.js";
globalThis.evaluations.push("D");
"#,
    );
    dir.write(
        "middle.js",
        r#"
import defer * as nsD from "./d.js";
globalThis.evaluations.push("Middle-before-nsD.z");
nsD.z;
globalThis.evaluations.push("Middle-after-nsD.z");
"#,
    );
    dir.write(
        "resolve-blocker.js",
        r#"
import { blocker } from "./setup.js";
globalThis.evaluations.push("resolve-blocker");
blocker.resolve();
"#,
    );
    dir.write(
        "c.js",
        r#"
import "./middle.js";
import "./resolve-blocker.js";
globalThis.evaluations.push("C");
"#,
    );
    dir.write(
        "main.js",
        r#"
import { aStarted } from "./setup.js";
const pA = import("./a.js");
await aStarted.promise;
const pC = import("./c.js");
await Promise.all([pA, pC]);

const actual = globalThis.evaluations.join("|");
const expected = "B|A-before-await|resolve-blocker|A-after-await|Middle-before-nsD.z|D|Middle-after-nsD.z|C";
if (actual !== expected) {
    throw new Error("unexpected evaluation order: " + actual);
}
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(dir.path())
        .args(["--module", "main.js"])
        .output()
        .expect("failed to run jsse");
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "deferred evaluation did not wait for the async cycle: {combined}"
    );
}
