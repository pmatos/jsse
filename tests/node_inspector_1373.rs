//! #1373: `require("module").builtinModules` lists `inspector`, so `require("inspector")` must
//! load instead of throwing MODULE_NOT_FOUND. These tests pin the call sequence tsc makes in
//! `sys.enableCPUProfiler` (`--generateCpuProfile`): no try/catch around the require, then a
//! Session driving Profiler.enable, start and stop, with the profile written as JSON.

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

#[test]
fn listed_inspector_builtins_are_requirable() {
    let script = "const { builtinModules } = require('module');\
        for (const name of ['inspector', 'inspector/promises']) {\
          const listed = builtinModules.includes(name);\
          const bare = require(name);\
          const prefixed = require('node:' + name);\
          console.log(name, listed, bare === prefixed, typeof bare.Session);\
        }";
    assert_eq!(
        run(script),
        "inspector true true function\ninspector/promises true true function"
    );
}

#[test]
fn tsc_cpu_profiler_sequence_writes_a_profile() {
    let script = "const fs = require('fs'), os = require('os'), path = require('path');\
        const inspector = require('inspector');\
        const file = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'jsse-1373-')), 'p.cpuprofile');\
        const session = new inspector.Session();\
        session.connect();\
        session.post('Profiler.enable', () => {\
          session.post('Profiler.start', () => {\
            session.post('Profiler.stop', (err, { profile }) => {\
              if (!err) fs.writeFileSync(file, JSON.stringify(profile));\
              session.disconnect();\
              const p = JSON.parse(fs.readFileSync(file, 'utf8'));\
              console.log(err, Array.isArray(p.nodes), p.nodes.length,\
                p.samples.length, p.timeDeltas.length, p.startTime <= p.endTime);\
              fs.rmSync(path.dirname(file), { recursive: true });\
            });\
          });\
        });";
    assert_eq!(run(script), "null true 2 0 0 true");
}
