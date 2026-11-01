//! The source position of a dynamic `import()` under `--node` (#1524): V8
//! reports the last position its bytecode generator emitted before the call, so
//! an `import()` nested in other expressions (`id(import(x))`,
//! `(await import(x))`, `() => import(x).then(...)`) reports the statement
//! start or the previous call, not the `import` keyword.
//! `tests/node-import-positions-1524/shapes.txt` holds each shape with the
//! position Node 26 reports for it; `scripts/run-node-import-positions-1524-oracle.py
//! --node` regenerates it. The shapes replay on the bytecode VM, the
//! tree-walker and under GC stress; default mode keeps parsing them.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/node-import-positions-1524")
}

struct Shape {
    name: String,
    expected: String,
    code: String,
}

fn shapes() -> Vec<Shape> {
    let text = std::fs::read_to_string(fixture_dir().join("shapes.txt")).unwrap();
    let mut shapes: Vec<Shape> = Vec::new();
    for line in text.split('\n') {
        if let Some(head) = line.strip_prefix("=== ") {
            let (name, expected) = head.split_once(" | ").unwrap();
            shapes.push(Shape {
                name: name.trim().to_string(),
                expected: expected.trim().to_string(),
                code: String::new(),
            });
        } else if let Some(shape) = shapes.last_mut() {
            shape.code.push_str(line);
            shape.code.push('\n');
        }
    }
    for shape in &mut shapes {
        shape.code = shape.code.trim_end_matches('\n').to_string();
    }
    shapes
}

fn position(shape: &Shape, args: &[&str], stress: bool, scratch: &Path) -> String {
    let prelude = std::fs::read_to_string(fixture_dir().join("prelude.mjs")).unwrap();
    let dir = scratch.join(&shape.name);
    std::fs::create_dir_all(&dir).unwrap();
    let source = shape.code.replace("IMP", "import(\"./nope.mjs\")");
    std::fs::write(dir.join("probe.mjs"), format!("{prelude}{source}\n")).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    cmd.arg("--node")
        .args(args)
        .arg("probe.mjs")
        .current_dir(&dir);
    if stress {
        cmd.env("JSSE_GC_STRESS", "7");
    }
    let out = cmd.output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let last = text.lines().last().unwrap_or("NOOUT").trim_end_matches(')');
    let mut parts = last.split(':');
    match (parts.next().map(str::parse::<usize>), parts.next()) {
        (Some(Ok(line)), Some(column)) => {
            format!("{}:{column}", line - prelude.matches('\n').count())
        }
        _ => last.to_string(),
    }
}

fn check_all(args: &[&str], stress: bool) {
    let shapes = shapes();
    assert!(shapes.len() > 300, "only {} shapes", shapes.len());
    let scratch = std::env::temp_dir().join(format!(
        "jsse-import-positions-{}-{}-{stress}",
        std::process::id(),
        args.len()
    ));
    let next = AtomicUsize::new(0);
    let failures = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(shape) = shapes.get(index) else {
                        break;
                    };
                    let got = position(shape, args, stress, &scratch);
                    if got != shape.expected {
                        failures.lock().unwrap().push(format!(
                            "{}: expected {}, got {got}",
                            shape.name, shape.expected
                        ));
                    }
                }
            });
        }
    });
    let _ = std::fs::remove_dir_all(&scratch);
    let failures = failures.into_inner().unwrap();
    assert!(failures.is_empty(), "{args:?}: {}", failures.join("\n"));
}

#[test]
fn import_positions_match_node() {
    check_all(&[], false);
}

#[test]
fn import_positions_match_node_on_the_tree_walker() {
    check_all(&["--no-bytecode"], false);
}

#[test]
fn import_positions_match_node_under_gc_stress() {
    check_all(&[], true);
}

#[test]
fn default_mode_still_parses_every_shape() {
    let prelude = "var id = (...a) => a[0], gl = { q: 0 }, o = { m() { return o; }, p: 1, f() { return 1; }, g: { h() { return 1; } } }, arr = { b: () => 1 }, b = 'b', C = class {}, tag = (s) => 1, fn = () => {}, n = 1, v = './nope.mjs', obj = { v: './nope.mjs' };\n";
    let shapes: Vec<Shape> = shapes()
        .into_iter()
        .filter(|shape| !shape.code.contains("export") && !shape.code.contains("import.meta"))
        .collect();
    let scratch = std::env::temp_dir().join(format!("jsse-import-default-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let mut checked = 0;
    for shape in shapes.iter().step_by(7) {
        let source = shape.code.replace("IMP", "import(\"./nope.mjs\")");
        let path = scratch.join("default.js");
        std::fs::write(
            &path,
            format!("{prelude}try {{ (async function () {{ {source}\n}}); }} catch (e) {{}}\n"),
        )
        .unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
            .arg(&path)
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!err.contains("SyntaxError"), "{}: {err}", shape.name);
        checked += 1;
    }
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(checked > 20, "only {checked} shapes");
}
