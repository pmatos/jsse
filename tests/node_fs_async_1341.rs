//! Node host mode asynchronous `fs` follow-ups (#1341): validation that throws
//! synchronously, tree walks and metadata calls that do not block the event
//! loop, multi-step callbacks that clean up after a failure, readv/writev and
//! GC stress. The Node-oracle diff is `scripts/fs-fixtures/async-1341.fixture.js`
//! through `scripts/run-node-fs-oracle.sh`.
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
    let dir = std::env::temp_dir().join(format!("jsse-node-fs-1341-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn big_tree(dir: &std::path::Path) {
    for d in 0..8 {
        let sub = dir.join(format!("d{d}"));
        std::fs::create_dir_all(sub.join("inner")).unwrap();
        for f in 0..500 {
            std::fs::write(sub.join(format!("f{f}")), "x").unwrap();
            std::fs::write(sub.join("inner").join(format!("g{f}")), "y").unwrap();
        }
    }
}

const TICKER: &str = "let ticks = 0, done = false;
     (function tick() { ticks++; if (!done) setImmediate(tick); })();";

#[test]
fn validation_errors_of_every_callback_function_are_thrown_synchronously() {
    let (out, err, rc) = run(
        "const fs = require('fs'); const noop = () => {}; const seen = [];
         const calls = {
           rm: () => fs.rm(1, noop),
           rmOptions: () => fs.rm('x', { recursive: 'y' }, noop),
           cp: () => fs.cp(1, 2, noop),
           cpOptions: () => fs.cp('a', 'b', { force: 1 }, noop),
           readdir: () => fs.readdir('x', { recursive: 'y' }, noop),
           truncate: () => fs.truncate('x', 'y', noop),
           realpathOptions: () => fs.realpath('x', 5, noop),
           readv: () => fs.readv(0, 5, noop),
           writev: () => fs.writev(0, [Buffer.alloc(1)], 0, 5),
           signal: () => fs.readFile('x', { signal: {} }, noop),
         };
         for (const [name, call] of Object.entries(calls)) {
           try { call(); seen.push(name + ' none'); } catch (e) { seen.push(name + ' ' + e.code); }
         }
         console.log(seen.join('\\n'));",
    );
    assert_eq!(rc, 0, "{err}");
    for line in out.lines() {
        assert!(line.ends_with("ERR_INVALID_ARG_TYPE"), "{line}");
    }
    assert_eq!(out.lines().count(), 10, "{out}");
}

#[test]
fn a_callback_that_fails_validation_never_runs() {
    let (out, err, rc) = run("const fs = require('fs'); let calls = 0;
         try { fs.rm(1, () => calls++); } catch {}
         try { fs.readdir('x', { recursive: 'y' }, () => calls++); } catch {}
         try { fs.truncate('x', 'y', () => calls++); } catch {}
         setTimeout(() => console.log('calls', calls), 50);");
    assert_eq!((out.as_str(), rc), ("calls 0", 0), "{err}");
}

#[test]
fn recursive_rm_does_not_block_the_event_loop() {
    let dir = scratch("rm");
    big_tree(&dir);
    let script = format!(
        "const fs = require('fs'); {TICKER}
         fs.rm({:?}, {{ recursive: true }}, (e) => {{
           done = true;
           console.log(e, fs.existsSync({:?}), ticks > 2);
         }});",
        dir, dir
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("null false true", 0), "{err}");
}

#[test]
fn recursive_readdir_does_not_block_and_matches_the_synchronous_listing() {
    let dir = scratch("readdir");
    big_tree(&dir);
    let script = format!(
        "const fs = require('fs'); {TICKER}
         const expected = fs.readdirSync({:?}, {{ recursive: true }});
         fs.readdir({:?}, {{ recursive: true }}, (e, list) => {{
           done = true;
           console.log(e, list.length, JSON.stringify(list) === JSON.stringify(expected), ticks > 2);
         }});",
        dir, dir
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("null 8016 true true", 0), "{err}");
}

#[test]
fn metadata_calls_run_on_the_pool_while_the_loop_thread_is_busy() {
    let dir = scratch("pool");
    std::fs::write(dir.join("f"), "x").unwrap();
    let script = format!(
        "const fs = require('fs'); const f = {:?} + '/f';
         let called = 0;
         fs.chmod(f, 0o600, () => called++);
         fs.symlink('f', {:?} + '/ln', () => called++);
         fs.link(f, {:?} + '/hard', () => called++);
         fs.mkdtemp({:?} + '/t-', () => called++);
         fs.utimes(f, 1000, 2000, () => called++);
         const until = Date.now() + 400; while (Date.now() < until) {{}}
         console.log(called, (fs.statSync(f).mode & 0o777).toString(8),
           fs.existsSync({:?} + '/ln'), fs.existsSync({:?} + '/hard'),
           fs.readdirSync({:?}).filter((n) => n.startsWith('t-')).length, fs.statSync(f).mtimeMs);",
        dir, dir, dir, dir, dir, dir, dir
    );
    let (out, err, rc) = run(&script);
    assert_eq!(
        (out.as_str(), rc),
        ("0 600 true true 1 2000000", 0),
        "{err}"
    );
}

#[test]
fn a_failed_multi_step_truncate_still_closes_the_descriptor() {
    let (out, err, rc) = run("const fs = require('fs');
         const probe = () => { const fd = fs.openSync('/', 'r'); fs.closeSync(fd); return fd; };
         const before = probe();
         fs.truncate('/dev/null', 1, (e) => {
           console.log(e && e.code, e && e.syscall, probe() === before);
         });");
    assert_eq!((out.as_str(), rc), ("EINVAL ftruncate true", 0), "{err}");
}

#[test]
fn rm_reports_the_first_failure_with_the_failing_entry() {
    let dir = scratch("rmfail");
    std::fs::create_dir_all(dir.join("keep/inner")).unwrap();
    let script = format!(
        "const fs = require('fs');
         fs.rm({:?} + '/missing/x', {{ recursive: true }}, (e) => {{
           console.log(e.code, e.syscall);
           fs.rm({:?} + '/keep', (e2) => console.log(e2.code));
         }});",
        dir, dir
    );
    let (out, err, rc) = run(&script);
    assert_eq!(
        (out.as_str(), rc),
        ("ENOENT lstat\nERR_FS_EISDIR", 0),
        "{err}"
    );
}

#[test]
fn readv_and_writev_scatter_and_gather_in_order() {
    let dir = scratch("vector");
    let script = format!(
        "const fs = require('fs'); const f = {:?} + '/v';
         fs.writeFileSync(f, 'abcdefghij');
         const fd = fs.openSync(f, 'r+');
         const a = Buffer.alloc(3), b = Buffer.alloc(4);
         console.log(fs.readvSync(fd, [a, b], 1), a.toString(), b.toString());
         console.log(fs.writevSync(fd, [Buffer.from('XY'), Buffer.from('Z')], 2));
         fs.readv(fd, [Buffer.alloc(2), Buffer.alloc(8)], 0, (e, n, list) => {{
           console.log(e, n, list.map(String).join('|'));
           fs.writev(fd, [], (e2, m, l2) => {{
             console.log(e2, m, l2.length);
             fs.closeSync(fd);
           }});
         }});",
        dir
    );
    let (out, err, rc) = run(&script);
    assert_eq!(
        (out.as_str(), rc),
        ("7 bcd efgh\n3\nnull 10 ab|XYZfghij\nnull 0 0", 0),
        "{err}"
    );
}

#[test]
fn the_new_calls_survive_gc_stress() {
    let dir = scratch("stress");
    for d in 0..3 {
        std::fs::create_dir_all(dir.join(format!("d{d}/in"))).unwrap();
        std::fs::write(dir.join(format!("d{d}/in/f")), "z").unwrap();
    }
    let script = format!(
        "const fs = require('fs'); const fsp = fs.promises; const root = {:?};
         (async () => {{
           fs.mkdirSync(root + '/d1/in', {{ recursive: true }});
           fs.writeFileSync(root + '/d1/in/f', 'z');
           fs.rmSync(root + '/vec', {{ force: true }});
           const list = await fsp.readdir(root, {{ recursive: true, withFileTypes: true }});
           const f = root + '/vec';
           fs.writeFileSync(f, 'hello world');
           const h = await fsp.open(f, 'r+');
           const r = await h.readv([Buffer.alloc(5), Buffer.alloc(6)], 0);
           const w = await h.writev([Buffer.from('J'), Buffer.from('K')], 0);
           await h.close();
           await fsp.chmod(f, 0o600);
           await fsp.truncate(f, 4);
           const real = await fsp.realpath(root + '/d0/in/../in/f');
           await fsp.rm(root + '/d1', {{ recursive: true }});
           console.log(list.length, r.bytesRead, r.buffers.map(String).join('|'), w.bytesWritten,
             fs.readFileSync(f, 'utf8'), real.endsWith('d0/in/f'), fs.existsSync(root + '/d1'));
         }})();",
        dir
    );
    for period in ["1", "5"] {
        let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
            .env("JSSE_GC_STRESS", period)
            .args(["--node", "-e", &script])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{period}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim_end(),
            "9 11 hello| world 2 JKll true false",
            "{period}"
        );
    }
}

#[test]
fn an_abort_is_delivered_when_it_happens_not_when_the_read_ends() {
    let dir = scratch("abort");
    let fifo = dir.join("pipe");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let script = format!(
        "const fs = require('fs'); const ac = new AbortController();
         fs.readFile({:?}, {{ signal: ac.signal }}, (e) => {{
           console.log(e.name, e.code);
           process.exit(0);
         }});
         setTimeout(() => ac.abort(), 50);",
        fifo
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("AbortError ABORT_ERR", 0), "{err}");
}

#[test]
fn a_relative_realpath_uses_the_directory_of_the_call() {
    let dir = scratch("cwd");
    std::fs::create_dir_all(dir.join("a/sub")).unwrap();
    std::fs::create_dir_all(dir.join("b")).unwrap();
    let script = format!(
        "const fs = require('fs'); const root = fs.realpathSync({:?});
         process.chdir(root + '/a');
         fs.realpath('sub', (e, p) => console.log(e, p.slice(root.length)));
         process.chdir(root + '/b');",
        dir
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("null /a/sub", 0), "{err}");
}

#[test]
fn options_changed_after_the_call_are_not_seen_by_later_steps() {
    let dir = scratch("opts");
    std::fs::create_dir_all(dir.join("t/inner")).unwrap();
    let script = format!(
        "const fs = require('fs'); const options = {{ recursive: true }};
         fs.rm({:?} + '/t', options, (e) => console.log(e, fs.existsSync({:?} + '/t')));
         options.recursive = false;",
        dir, dir
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("null false", 0), "{err}");
}

#[test]
fn a_getter_that_calls_fs_while_a_call_is_validated_does_not_disturb_it() {
    let dir = scratch("reentrant");
    std::fs::write(dir.join("f"), "abc").unwrap();
    let script = format!(
        "const fs = require('fs'); const f = {:?} + '/f';
         const options = {{ get encoding() {{ fs.stat(f, () => {{}}); return 'utf8'; }} }};
         fs.readFile(f, options, (e, d) => console.log(e, d));",
        dir
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("null abc", 0), "{err}");
}
