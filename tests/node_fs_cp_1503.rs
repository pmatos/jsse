//! Node host mode asynchronous `fs.cp` (#1503): the copy yields to the event
//! loop between entries, an async `filter` is awaited, validation throws
//! synchronously and descriptors are not leaked. The Node-oracle diff is
//! `scripts/fs-fixtures/cp-1503.fixture.js` through `scripts/run-node-fs-oracle.sh`.
#![cfg(unix)]

use std::process::{Command, Output};

fn jsse(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(args)
        .output()
        .expect("run jsse")
}

fn run(source: &str) -> (String, String, i32) {
    let out = jsse(&["--node", "-e", source]);
    (
        String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("jsse-node-fs-1503-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn big_tree(dir: &std::path::Path) {
    for d in 0..6 {
        let sub = dir.join(format!("d{d}"));
        std::fs::create_dir_all(sub.join("inner")).unwrap();
        for f in 0..400 {
            std::fs::write(sub.join(format!("f{f}")), "x").unwrap();
            std::fs::write(sub.join("inner").join(format!("g{f}")), "y").unwrap();
        }
    }
}

const TICKER: &str = "let ticks = 0, done = false;
     (function tick() { ticks++; if (!done) setImmediate(tick); })();";

#[test]
fn recursive_cp_does_not_block_the_event_loop() {
    let src = scratch("cb-src");
    let dest = scratch("cb-dest").join("out");
    big_tree(&src);
    let script = format!(
        "const fs = require('fs'); {TICKER}
         fs.cp({:?}, {:?}, {{ recursive: true }}, (e) => {{
           done = true;
           const a = fs.readdirSync({:?}, {{ recursive: true }}).sort();
           const b = fs.readdirSync({:?}, {{ recursive: true }}).sort();
           console.log(e, a.length, JSON.stringify(a) === JSON.stringify(b), ticks > 100);
         }});",
        src, dest, src, dest
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("null 4812 true true", 0), "{err}");
}

#[test]
fn promise_cp_does_not_block_the_event_loop() {
    let src = scratch("p-src");
    let dest = scratch("p-dest").join("out");
    big_tree(&src);
    let script = format!(
        "const fs = require('fs'); {TICKER}
         fs.promises.cp({:?}, {:?}, {{ recursive: true }}).then((v) => {{
           done = true;
           console.log(v, fs.readdirSync({:?}, {{ recursive: true }}).length, ticks > 100);
         }});",
        src, dest, dest
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("undefined 4812 true", 0), "{err}");
}

#[test]
fn an_async_filter_is_awaited_for_every_entry_root_first() {
    let src = scratch("f-src");
    big_tree(&src);
    let dest = scratch("f-dest").join("out");
    let script = format!(
        "const fs = require('fs'); let calls = 0, first;
         fs.cp({:?}, {:?}, {{ recursive: true, filter: (s) => new Promise((resolve) => {{
           first = first || s; calls++;
           setImmediate(() => resolve(!s.endsWith('inner') && !s.endsWith('f7')));
         }}) }}, (e) => {{
           const list = fs.readdirSync({:?}, {{ recursive: true }});
           console.log(e, calls, first === {:?}, list.some((n) => n.endsWith('inner')),
             list.some((n) => n.endsWith('/f7')), list.length);
         }});",
        src, dest, dest, src
    );
    let (out, err, rc) = run(&script);
    assert_eq!(
        (out.as_str(), rc),
        ("null 2413 true false false 2400", 0),
        "{err}"
    );
}

#[test]
fn a_rejecting_filter_fails_the_copy_with_its_reason() {
    let dir = scratch("reject");
    std::fs::write(dir.join("a"), "a").unwrap();
    let script = format!(
        "const fs = require('fs');
         fs.cp({:?} + '/a', {:?} + '/b', {{ filter: async () => {{ throw new RangeError('no'); }} }}, (e) => {{
           console.log(e.name, e.message, fs.existsSync({:?} + '/b'));
         }});",
        dir, dir, dir
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("RangeError no false", 0), "{err}");
}

#[test]
fn the_synchronous_form_still_rejects_a_promise_returning_filter() {
    let dir = scratch("sync");
    std::fs::write(dir.join("a"), "a").unwrap();
    let script = format!(
        "const fs = require('fs');
         try {{ fs.cpSync({:?} + '/a', {:?} + '/b', {{ filter: async () => true }}); }}
         catch (e) {{ console.log(e.code); }}",
        dir, dir
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("ERR_INVALID_RETURN_VALUE", 0), "{err}");
}

#[test]
fn argument_validation_throws_synchronously_and_rejects_for_the_promise_form() {
    let (out, err, rc) = run(
        "const fs = require('fs'); const noop = () => {}; let calls = 0; const seen = [];
         for (const f of [() => fs.cp(1, 'b', noop), () => fs.cp('a', 'b', { force: 1 }, noop),
                          () => fs.cp('a', 'b'), () => fs.cp('a', 'b', {}, 5)]) {
           try { f(); seen.push('none'); } catch (e) { seen.push(e.code); }
         }
         fs.promises.cp(1, 'b').catch((e) => { seen.push('rejected ' + e.code); console.log(seen.join(' ')); });",
    );
    assert_eq!(
        (out.as_str(), rc),
        (
            "ERR_INVALID_ARG_TYPE ERR_INVALID_ARG_TYPE ERR_INVALID_ARG_TYPE ERR_INVALID_ARG_TYPE rejected ERR_INVALID_ARG_TYPE",
            0
        ),
        "{err}"
    );
}

#[test]
fn a_failed_copy_leaks_no_descriptors() {
    let dir = scratch("leak");
    std::fs::create_dir_all(dir.join("src/sub")).unwrap();
    std::fs::write(dir.join("src/a"), "a").unwrap();
    std::fs::write(dir.join("src/sub/b"), "b").unwrap();
    let script = format!(
        "const fs = require('fs'); const fds = () => fs.readdirSync('/proc/self/fd').length;
         const before = fds(); let left = 20, failures = 0;
         for (let i = 0; i < 20; i++) {{
           fs.cp({:?} + '/src', {:?} + '/src/sub/in' + i, {{ recursive: true }}, (e) => {{
             if (e && e.code === 'ERR_FS_CP_EINVAL') failures++;
             if (--left === 0) console.log(failures, fds() - before);
           }});
         }}",
        dir, dir
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("20 0", 0), "{err}");
}

#[test]
fn cp_survives_gc_stress() {
    let src = scratch("gc-src");
    std::fs::create_dir_all(src.join("sub")).unwrap();
    std::fs::write(src.join("a"), "a").unwrap();
    std::fs::write(src.join("sub/b"), "b").unwrap();
    let dest = scratch("gc-dest").join("out");
    let script = format!(
        "const fs = require('fs');
         fs.promises.cp({:?}, {:?}, {{ recursive: true, filter: async () => true }}).then(() => {{
           console.log(fs.readdirSync({:?}, {{ recursive: true }}).sort().join());
         }});",
        src, dest, dest
    );
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .env("JSSE_GC_STRESS", "3")
        .args(["--node", "-e", &script])
        .output()
        .expect("run jsse");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "a,sub,sub/b"
    );
}
