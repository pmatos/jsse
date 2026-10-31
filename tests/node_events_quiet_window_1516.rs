//! The events test harness (scripts/events-fixtures/harness.js) must not
//! verify `mustCall` expectations while a call is still owed (#1516).
//! It used to verify after 60 ms without activity, so on a slow build (GC stress
//! on a coverage-instrumented binary) `test-events-on-async-iterator` was
//! verified between two steps and failed with "expected 1 calls, got 0".

use std::path::Path;
use std::process::{Command, Output};

fn run_with_harness(body: &str) -> Output {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/events-fixtures");
    let harness = std::fs::read_to_string(dir.join("harness.js")).unwrap();
    let script = format!(
        "{harness}try {{ (function (require, __filename, __dirname) {{\n{body}\n}})(__eventsRequire, \"/node/test/parallel/quiet.js\", \"/node/test/parallel\"); }} finally {{ __eventsFinish(); }}\n"
    );
    let path =
        std::env::temp_dir().join(format!("jsse-events-quiet-1516-{}.js", std::process::id()));
    std::fs::write(&path, script).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["--node", path.to_str().unwrap()])
        .output()
        .expect("run jsse");
    let _ = std::fs::remove_file(&path);
    out
}

const SLOW_STEPS: &str = r#"
const common = require("../common");
const done = common.mustCall();
let steps = 0;
function step() {
  const end = Date.now() + 40;
  while (Date.now() < end) {}
  if (++steps < 6) setImmediate(step);
  else done();
}
setImmediate(step);
"#;

#[test]
fn a_slow_test_is_not_verified_before_its_owed_call_happens() {
    let out = run_with_harness(SLOW_STEPS);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.lines().any(|l| l == "events-fixture: ok"), "{text}");
}
