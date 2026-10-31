//! V8 names an anonymous function assigned to a member after its target
//! (`a.x = function () {}` prints `at a.x`). An arrow whose parameter list is
//! re-parsed as FormalParameters (#1547) must record that name once, not once
//! per pass (`at a.x.a.x`).
use std::process::Command;

fn first_frame(source: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .arg("--node")
        .args(["-e", source])
        .output()
        .expect("run jsse");
    assert!(
        output.status.success(),
        "jsse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf-8 stdout")
        .trim()
        .to_string()
}

#[test]
fn member_assignment_in_arrow_default_is_named_once() {
    let source = r#"
var a = {};
var g = (p = (a.x = function () { throw new Error('boom') })) => p;
try { g()(); } catch (e) { console.log(e.stack.split('\n')[1].trim().split(' ')[1]); }
"#;
    assert_eq!(first_frame(source), "a.x");
}

#[test]
fn chained_member_assignment_in_async_arrow_default_is_named_once() {
    let source = r#"
var a = {}, b = {};
var g = async (p = (a.x = b.y = function () { throw new Error('boom') })) => p;
g().then((f) => {
  try { f(); } catch (e) { console.log(e.stack.split('\n')[1].trim().split(' ')[1]); }
});
"#;
    assert_eq!(first_frame(source), "a.x.b.y");
}
