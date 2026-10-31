//! Node host mode `options.signal` on `fs` (#1505): readFile, writeFile and
//! appendFile in the callback API, the promise API and the FileHandle forms
//! run Node's own steps and check the signal where Node does; stat, lstat and
//! fstat take a signal. The Node-oracle diff is
//! `scripts/fs-fixtures/signal-1505.fixture.js` through
//! `scripts/run-node-fs-oracle.sh`; the expected values here are what that
//! oracle printed for Node v26.8.2.
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
    let dir = std::env::temp_dir().join(format!("jsse-node-fs-1505-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const PRELUDE: &str = "const fs = require('fs'); const fsp = fs.promises;
    const reason = new Error('why'); const CHUNK = 512 * 1024;
    const outcome = (e) => e ? e.name + ' ' + e.code + ' ' + (e.cause === reason) : 'ok';
    const size = (f) => { try { return fs.statSync(f).size; } catch (e) { return e.code; } };";

#[test]
fn an_abort_right_after_the_call_leaves_the_file_opened_and_empty() {
    let dir = scratch("tick");
    let script = format!(
        "{PRELUDE} const D = {dir:?};
         fs.writeFileSync(D + '/a', 'alpha');
         const ac1 = new AbortController();
         fs.writeFile(D + '/new', 'data', {{ signal: ac1.signal }}, (e) => {{
           console.log('cb writeFile', outcome(e), size(D + '/new'));
           const ac2 = new AbortController();
           fs.appendFile(D + '/a', '+more', {{ signal: ac2.signal }}, (e) => {{
             console.log('cb appendFile', outcome(e), size(D + '/a'));
             const ac3 = new AbortController();
             const p = fsp.writeFile(D + '/p', 'data', {{ signal: ac3.signal }});
             ac3.abort(reason);
             p.catch((e) => console.log('p writeFile', outcome(e), size(D + '/p')));
           }});
           ac2.abort(reason);
         }});
         ac1.abort(reason);"
    );
    let (out, err, rc) = run(&script);
    assert_eq!(
        (out.as_str(), rc),
        (
            "cb writeFile AbortError ABORT_ERR true 0\n\
             cb appendFile AbortError ABORT_ERR true 5\n\
             p writeFile AbortError ABORT_ERR true 0",
            0
        ),
        "{err}"
    );
}

#[test]
fn an_aborted_signal_fails_the_callback_forms_before_they_start() {
    let dir = scratch("pre");
    let script = format!(
        "{PRELUDE} const D = {dir:?};
         const ac = new AbortController(); ac.abort(reason);
         let sync = true;
         const seen = [];
         const done = (name) => (e) => seen.push(name + ' ' + sync + ' ' + outcome(e));
         fs.readFile(D + '/none', {{ signal: ac.signal }}, done('readFile'));
         fs.writeFile(D + '/w', 'x', {{ signal: ac.signal }}, done('writeFile'));
         fs.appendFile(D + '/w', 'x', {{ signal: ac.signal }}, done('appendFile'));
         fs.stat(D, {{ signal: ac.signal }}, done('stat'));
         sync = false;
         console.log(seen.join('\\n'), size(D + '/w'));"
    );
    let (out, err, rc) = run(&script);
    assert_eq!(
        (out.as_str(), rc),
        (
            "readFile true AbortError ABORT_ERR true\n\
             writeFile true AbortError ABORT_ERR true\n\
             appendFile true AbortError ABORT_ERR true\n\
             stat true AbortError ABORT_ERR true ENOENT",
            0
        ),
        "{err}"
    );
}

#[test]
fn a_write_of_an_iterable_stops_between_chunks() {
    let dir = scratch("iter");
    let script = format!(
        "{PRELUDE} const D = {dir:?};
         (async () => {{
           for (const [n, after] of [[10, 2], [CHUNK + 3, 1], [CHUNK + 3, 2], [10, 4]]) {{
             const chunks = async function* (ac) {{
               for (let i = 0; i < 4; i++) {{
                 yield Buffer.alloc(n, 0x62 + i);
                 if (i + 1 === after) ac.abort(reason);
               }}
             }};
             let r, r2;
             const ac1 = new AbortController();
             try {{ await fsp.writeFile(D + '/it', chunks(ac1), {{ signal: ac1.signal }}); }} catch (e) {{ r = e; }}
             const h = await fsp.open(D + '/it2', 'w');
             const ac2 = new AbortController();
             try {{ await h.writeFile(chunks(ac2), {{ signal: ac2.signal }}); }} catch (e) {{ r2 = e; }}
             await h.close();
             console.log(n, after, outcome(r), size(D + '/it'), outcome(r2), size(D + '/it2'));
           }}
         }})();"
    );
    let (out, err, rc) = run(&script);
    assert_eq!(
        (out.as_str(), rc),
        (
            "10 2 AbortError ABORT_ERR true 20 AbortError ABORT_ERR true 20\n\
             524291 1 AbortError ABORT_ERR true 524291 AbortError ABORT_ERR true 524291\n\
             524291 2 AbortError ABORT_ERR true 1048582 AbortError ABORT_ERR true 1048582\n\
             10 4 ok 40 ok 40",
            0
        ),
        "{err}"
    );
}

#[test]
fn a_filehandle_is_checked_before_each_chunk_and_after_its_fstat() {
    let dir = scratch("handle");
    let script = format!(
        "{PRELUDE} const D = {dir:?};
         (async () => {{
           fs.writeFileSync(D + '/a', 'alpha');
           fs.writeFileSync(D + '/big', Buffer.alloc(CHUNK * 2 + 17, 0x61));
           const via = async (label, open, flag, op) => {{
             const h = await fsp.open(D + '/' + open, flag);
             const ac = new AbortController();
             const p = op(h, ac.signal);
             ac.abort(reason);
             let r;
             try {{ r = await p; }} catch (e) {{ r = e; }}
             await h.close();
             console.log(label, r instanceof Error ? outcome(r) : 'ok', size(D + '/h'));
           }};
           await via('readFile small', 'a', 'r', (h, signal) => h.readFile({{ signal }}));
           await via('readFile via fsp', 'big', 'r', (h, signal) => fsp.readFile(h, {{ signal }}));
           await via('writeFile small', 'h', 'w', (h, signal) => h.writeFile('hd', {{ signal }}));
           await via('writeFile big', 'h', 'w', (h, signal) => h.writeFile(Buffer.alloc(CHUNK + 5), {{ signal }}));
           await via('appendFile small', 'h', 'w', (h, signal) => fsp.appendFile(h, 'app', {{ signal }}));
         }})();"
    );
    let (out, err, rc) = run(&script);
    assert_eq!(
        (out.as_str(), rc),
        (
            "readFile small AbortError ABORT_ERR true ENOENT\n\
             readFile via fsp AbortError ABORT_ERR true ENOENT\n\
             writeFile small ok 2\n\
             writeFile big AbortError ABORT_ERR true 524288\n\
             appendFile small ok 3",
            0
        ),
        "{err}"
    );
}

#[test]
fn a_large_write_aborted_midway_keeps_whole_chunks_and_the_loop_runs() {
    let dir = scratch("mid");
    let script = format!(
        "{PRELUDE} const D = {dir:?}; const total = CHUNK * 64;
         const big = Buffer.alloc(total, 0x65);
         (async () => {{
           let ticks = 0, running = true;
           (function tick() {{ ticks++; if (running) setImmediate(tick); }})();
           for (let attempt = 0; attempt < 5; attempt++) {{
             fs.rmSync(D + '/part', {{ force: true }});
             const ac = new AbortController();
             const watch = () => {{
               if (size(D + '/part') > 0) ac.abort(reason); else setImmediate(watch);
             }};
             setImmediate(watch);
             let r;
             try {{ await fsp.writeFile(D + '/part', big, {{ signal: ac.signal }}); }} catch (e) {{ r = e; }}
             const s = size(D + '/part');
             if (!r) continue;
             running = false;
             console.log(outcome(r), s % CHUNK === 0, s > 0 && s < total, ticks > 2);
             return;
           }}
           console.log('no partial write');
         }})();"
    );
    let (out, err, rc) = run(&script);
    assert_eq!(
        (out.as_str(), rc),
        ("AbortError ABORT_ERR true true true true", 0),
        "{err}"
    );
}

#[test]
fn an_abort_is_delivered_when_it_happens_not_when_a_blocked_read_ends() {
    let dir = scratch("fifo");
    let fifo = dir.join("pipe");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let script = format!(
        "{PRELUDE} const fifo = {fifo:?}; const seen = [];
         const finish = () => {{ if (seen.length === 2) {{ console.log(seen.sort().join()); process.exit(0); }} }};
         const ac1 = new AbortController();
         fs.readFile(fifo, {{ signal: ac1.signal }}, (e) => {{ seen.push('cb ' + e.code); finish(); }});
         const ac2 = new AbortController();
         fsp.readFile(fifo, {{ signal: ac2.signal }}).catch((e) => {{ seen.push('p ' + e.code); finish(); }});
         setTimeout(() => {{ ac1.abort(); ac2.abort(); }}, 50);"
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("cb ABORT_ERR,p ABORT_ERR", 0), "{err}");
}

#[test]
fn aborted_calls_close_the_files_they_opened() {
    let dir = scratch("fds");
    let script = format!(
        "{PRELUDE} const D = {dir:?};
         const fds = () => fs.readdirSync('/proc/self/fd').length;
         (async () => {{
           fs.writeFileSync(D + '/a', 'alpha');
           fs.writeFileSync(D + '/big', Buffer.alloc(CHUNK * 2 + 17, 0x61));
           const before = fds();
           for (let i = 0; i < 20; i++) {{
             for (const f of [
               (signal) => fsp.readFile(D + '/a', {{ signal }}),
               (signal) => fsp.readFile(D + '/big', {{ signal }}),
               (signal) => fsp.writeFile(D + '/w', Buffer.alloc(CHUNK * 2), {{ signal }}),
               (signal) => new Promise((resolve, reject) => fs.writeFile(D + '/w', 'x', {{ signal }}, (e) => e ? reject(e) : resolve())),
               (signal) => new Promise((resolve, reject) => fs.readFile(D + '/big', {{ signal }}, (e) => e ? reject(e) : resolve())),
             ]) {{
               const ac = new AbortController();
               const p = f(ac.signal);
               ac.abort(reason);
               await p.catch(() => {{}});
             }}
           }}
           await new Promise((r) => setTimeout(r, 100));
           console.log(fds() - before);
         }})();"
    );
    if !std::path::Path::new("/proc/self/fd").exists() {
        return;
    }
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("0", 0), "{err}");
}

#[test]
fn a_read_of_unknown_size_stops_reading_after_an_abort() {
    if !std::path::Path::new("/proc/self/fd").exists() {
        return;
    }
    let script = format!(
        "{PRELUDE} const fds = () => fs.readdirSync('/proc/self/fd').length;
         (async () => {{
           const before = fds();
           const ac = new AbortController();
           const p = fsp.readFile('/dev/zero', {{ signal: ac.signal }}).catch((e) => e.code);
           setTimeout(() => ac.abort(), 20);
           console.log(await p);
           await new Promise((r) => setTimeout(r, 300));
           console.log(fds() - before);
           process.exit(0);
         }})();"
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("ABORT_ERR\n0", 0), "{err}");
}

#[test]
fn a_composite_of_aborts_survives_gc_stress() {
    let dir = scratch("gc");
    let script = format!(
        "{PRELUDE} const D = {dir:?};
         (async () => {{
           fs.writeFileSync(D + '/a', 'alpha');
           const ac = new AbortController();
           const p = fsp.writeFile(D + '/w', Buffer.alloc(CHUNK + 1), {{ signal: ac.signal }});
           ac.abort(reason);
           const r = await p.catch((e) => outcome(e));
           const h = await fsp.open(D + '/w', 'w');
           const live = new AbortController();
           await h.writeFile('abc', {{ signal: live.signal }});
           await h.close();
           const s = await fsp.stat(D + '/w', {{ signal: live.signal }});
           console.log(r, size(D + '/w'), s.size, await fsp.readFile(D + '/a', {{ signal: live.signal, encoding: 'utf8' }}));
         }})();"
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
            "AbortError ABORT_ERR true 3 3 alpha",
            "{period}"
        );
    }
}

#[test]
fn the_default_global_environment_is_unchanged() {
    let out = jsse(&[
        "-e",
        "print(typeof require, typeof __host_fs_open, typeof __node_fs_support, typeof AbortController)",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let (a, b, c, _) = {
        let mut it = stdout.trim_end().splitn(4, ' ');
        (it.next(), it.next(), it.next(), it.next())
    };
    assert_eq!(
        (a, b, c),
        (Some("undefined"), Some("undefined"), Some("undefined"))
    );
}
