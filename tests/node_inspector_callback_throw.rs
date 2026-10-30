//! `inspector.Session#post` under `--node` (#1247): an exception thrown by a command callback does
//! not unwind through `post`. Node's `Session#onMessage` catches it and reports it with
//! `process.emitWarning(error)`, so the program carries on and exits 0. tsc relies on this when a
//! `--generateCpuProfile` path cannot be written (the `writeFileSync` sits in the `Profiler.stop`
//! callback); scripts/run-tsc-projects-oracle.py runs it as `infm-cpuprofile-unwritable`.

use std::process::Command;

fn run(source: &str) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["--node", "-e", source])
        .output()
        .expect("run jsse");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

const SETUP: &str = "const i = require('inspector'); const s = new i.Session(); s.connect();\
    process.on('warning', (w) => console.log('warning', w.name, w.code, w.message));";

#[test]
fn a_throwing_callback_becomes_a_process_warning() {
    let script = format!(
        "{SETUP}\
         s.post('Profiler.enable');\
         s.post('Profiler.start');\
         s.post('Profiler.stop', () => {{\
           console.log('in callback');\
           const e = new Error('boom'); e.code = 'EBOOM'; throw e;\
         }});\
         console.log('after post');\
         process.on('exit', (c) => console.log('exit', c));"
    );
    let (ok, stdout, stderr) = run(&script);
    assert!(ok, "{stderr}");
    assert_eq!(
        stdout, "in callback\nafter post\nwarning Error EBOOM boom\nexit 0\n",
        "{stderr}"
    );
    assert!(stderr.contains("[EBOOM] Error: boom"), "{stderr}");
}

#[test]
fn a_throwing_error_callback_is_reported_the_same_way() {
    let script = format!(
        "{SETUP}\
         s.post('Nope.nope', (e) => {{ console.log(e.code); throw new TypeError('again'); }});\
         console.log('after post');"
    );
    let (ok, stdout, stderr) = run(&script);
    assert!(ok, "{stderr}");
    assert_eq!(
        stdout, "ERR_INSPECTOR_COMMAND\nafter post\nwarning TypeError undefined again\n",
        "{stderr}"
    );
}

#[test]
fn argument_errors_still_throw_from_post() {
    let script = format!(
        "{SETUP}\
         try {{ s.post('A.b', {{}}, 5); }} catch (e) {{ console.log(e.code); }}\
         s.disconnect();\
         try {{ s.post('A.b', () => {{}}); }} catch (e) {{ console.log(e.code); }}"
    );
    let (ok, stdout, stderr) = run(&script);
    assert!(ok, "{stderr}");
    assert_eq!(
        stdout, "ERR_INVALID_ARG_TYPE\nERR_INSPECTOR_NOT_CONNECTED\n",
        "{stderr}"
    );
}
