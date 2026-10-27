//! The minimal `inspector` module under `--node` (#1247): a `Session` whose Profiler
//! domain returns a profile without samples. The same script runs on Node in
//! `scripts/run-tsc-projects-oracle.py` (`info-cpuprofile*` cases) through tsc.

use std::process::Command;

fn run(source: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["--node", "-e", source])
        .output()
        .expect("run jsse");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

const PRELUDE: &str = "const i = require('inspector'); const s = new i.Session();\
    const log = (...a) => console.log(...a);\
    const err = (f) => { try { f(); } catch (e) { log(e.code, e.message); } };";

#[test]
fn it_is_a_builtin_reached_with_and_without_the_node_prefix() {
    assert_eq!(
        run(
            "console.log(require('node:inspector') === require('inspector'),\
             require('module').builtinModules.includes('inspector'),\
             typeof require('inspector').Session, require('inspector').url())"
        ),
        "true true function undefined"
    );
}

#[test]
fn profiler_lifecycle_and_protocol_errors_match_node() {
    let script = format!(
        "{PRELUDE}\
         err(() => s.post('Profiler.enable'));\
         s.connect();\
         err(() => s.connect());\
         s.post('Foo.bar', (e, r) => log(e.code, e.message, r));\
         s.post('Profiler.start', (e, r) => log(e.code, e.message, r));\
         s.post('Profiler.stop', (e, r) => log(e.code, e.message, r));\
         s.post('Profiler.enable', (e, r) => log(e, r));\
         s.post('Profiler.start', (e, r) => log(e, r));\
         s.post('Profiler.start', (e, r) => log(e, r));\
         s.post('Profiler.stop', (e, r) => {{\
           const p = r.profile;\
           log(e, Object.keys(p).join(), p.nodes[0].callFrame.functionName, p.samples.length, p.timeDeltas.length, p.startTime <= p.endTime);\
         }});\
         s.post('Profiler.stop', (e) => log(e.message));\
         err(() => s.post(1));\
         err(() => s.post('A.b', 5));\
         err(() => s.post('A.b', {{}}, 5));\
         s.disconnect();\
         err(() => s.post('Profiler.enable'));\
         s.disconnect();\
         log(s instanceof require('events'));"
    );
    let expected = [
        "ERR_INSPECTOR_NOT_CONNECTED Session is not connected",
        "ERR_INSPECTOR_ALREADY_CONNECTED The inspector session is already connected",
        "ERR_INSPECTOR_COMMAND Inspector error -32601: 'Foo.bar' wasn't found undefined",
        "ERR_INSPECTOR_COMMAND Inspector error -32000: Profiler is not enabled undefined",
        "ERR_INSPECTOR_COMMAND Inspector error -32000: No recording profiles found undefined",
        "null {}",
        "null {}",
        "null {}",
        "null nodes,startTime,endTime,samples,timeDeltas (root) 0 0 true",
        "Inspector error -32000: No recording profiles found",
        "ERR_INVALID_ARG_TYPE The \"method\" argument must be of type string. Received type number (1)",
        "ERR_INVALID_ARG_TYPE The \"params\" argument must be of type object. Received type number (5)",
        "ERR_INVALID_ARG_TYPE The \"callback\" argument must be of type function. Received type number (5)",
        "ERR_INSPECTOR_NOT_CONNECTED Session is not connected",
        "true",
    ]
    .join("\n");
    assert_eq!(run(&script), expected);
}

#[test]
fn open_and_wait_for_debugger_report_that_no_inspector_exists() {
    assert_eq!(
        run(&format!(
            "{PRELUDE}err(() => i.open()); err(() => i.waitForDebugger()); i.close(); log('closed');"
        )),
        "ERR_INSPECTOR_NOT_AVAILABLE Inspector is not available\nERR_INSPECTOR_NOT_ACTIVE Inspector is not active\nclosed"
    );
}
