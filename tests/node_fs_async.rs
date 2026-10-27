//! Node host mode asynchronous `fs` (#1234): the callback API, `fs.promises`
//! and the file streams on the worker pool. Child-process checks; the
//! Node-oracle diff is `scripts/fs-fixtures/async*.fixture.js` through
//! `scripts/run-node-fs-oracle.sh`.
#![cfg(unix)]

use std::io::Read;
use std::process::{Command, Output, Stdio};

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
    let dir =
        std::env::temp_dir().join(format!("jsse-node-fs-async-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_async_floor_is_absent_without_node_and_private_with_it() {
    let out = jsse(&["-e", "print(typeof __host_fs_async)"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), "undefined");
    let (out, _, rc) = run("print(typeof __host_fs_async, typeof require('fs').readFile)");
    assert_eq!((out.as_str(), rc), ("undefined function", 0));
}

#[test]
fn lazy_accessors_bind_to_the_receiver_so_a_descriptor_clone_can_be_patched() {
    let (out, err, rc) = run(
        "const fs = require('fs'); const clone = Object.create(Object.getPrototypeOf(fs));
         for (const k of Object.getOwnPropertyNames(fs)) Object.defineProperty(clone, k, Object.getOwnPropertyDescriptor(fs, k));
         clone.readFile = function patched() {};
         fs.close(fs.openSync('/', 'r'));
         console.log(fs.readFile.name, clone.readFile.name, fs.promises === require('fs/promises'));",
    );
    assert_eq!((out.as_str(), rc), ("readFile patched true", 0), "{err}");
}

#[test]
fn a_pending_callback_alone_keeps_the_process_alive() {
    let dir = scratch("alive");
    std::fs::write(dir.join("f"), "payload").unwrap();
    let script = format!(
        "require('fs').readFile({:?}, 'utf8', (e, d) => console.log(e, d))",
        dir.join("f")
    );
    let (out, err, rc) = run(&script);
    assert_eq!((out.as_str(), rc), ("null payload", 0), "{err}");
}

#[test]
fn callbacks_run_in_the_poll_phase_after_ticks_and_promise_jobs() {
    let (out, err, rc) = run(
        "const fs = require('fs'); const log = [];
         fs.stat('/', () => { log.push('stat'); setImmediate(() => log.push('immediate')); process.nextTick(() => log.push('tick-in-cb')); });
         process.nextTick(() => log.push('tick'));
         Promise.resolve().then(() => log.push('promise'));
         setTimeout(() => console.log(log.join()), 300);",
    );
    assert_eq!(
        (out.as_str(), rc),
        ("tick,promise,stat,tick-in-cb,immediate", 0),
        "{err}"
    );
}

#[test]
fn every_callback_gets_its_own_tick_checkpoint() {
    let dir = scratch("checkpoint");
    for name in ["a", "b", "c"] {
        std::fs::write(dir.join(name), name).unwrap();
    }
    let script = format!(
        "const fs = require('fs'); const log = [];
         let left = 3;
         for (const n of ['a', 'b', 'c']) fs.readFile({:?} + '/' + n, 'utf8', (e, d) => {{
           process.nextTick(() => log.push('t' + d)); log.push('c' + d);
           if (--left === 0) setTimeout(() => console.log(log.join(' ')), 10);
         }});",
        dir
    );
    let (out, err, rc) = run(&script);
    let order: Vec<&str> = out.split(' ').collect();
    assert_eq!(rc, 0, "{err}");
    for n in ["a", "b", "c"] {
        let call = order.iter().position(|e| *e == format!("c{n}")).unwrap();
        assert_eq!(order[call + 1], format!("t{n}"), "{out}");
    }
}

#[test]
fn errors_have_nodes_uv_exception_shape() {
    let (out, err, rc) = run(
        "require('fs').stat('/no/such/file', (e) => console.log(e.code, e.errno, e.syscall, e.path, e.message))",
    );
    assert_eq!(
        (out.as_str(), rc),
        (
            "ENOENT -2 stat /no/such/file ENOENT: no such file or directory, stat '/no/such/file'",
            0
        ),
        "{err}"
    );
}

#[test]
fn validation_failures_throw_synchronously_and_never_call_back() {
    let (out, err, rc) = run(
        "const fs = require('fs'); const seen = [];
         for (const f of [() => fs.readFile({}, () => seen.push('called')), () => fs.readFile('/'), () => fs.stat('a\\0b', () => seen.push('called'))]) {
           try { f(); } catch (e) { seen.push(e.code); }
         }
         setTimeout(() => console.log(seen.join()), 50);",
    );
    assert_eq!(
        (out.as_str(), rc),
        (
            "ERR_INVALID_ARG_TYPE,ERR_INVALID_ARG_TYPE,ERR_INVALID_ARG_VALUE",
            0
        ),
        "{err}"
    );
}

#[test]
fn a_throwing_callback_is_an_uncaught_exception_and_exit_in_a_callback_exits() {
    let (_, err, rc) = run("require('fs').stat('/', () => { throw new Error('boom'); })");
    assert_eq!(rc, 1);
    assert!(err.contains("Error: boom"), "{err}");
    let (out, _, rc) = run("process.on('uncaughtException', (e) => {
           console.log('caught', e.message);
           require('fs').stat('/', () => console.log('second'));
         });
         require('fs').stat('/', () => { throw new Error('boom'); });");
    assert_eq!((out.as_str(), rc), ("caught boom\nsecond", 0));
    let (out, _, rc) = run(
        "require('fs').stat('/', () => process.exit(3)); setTimeout(() => console.log('late'), 400);",
    );
    assert_eq!((out.as_str(), rc), ("", 3));
}

#[test]
fn async_results_match_the_synchronous_ones_on_a_tree() {
    let dir = scratch("tree");
    std::fs::create_dir_all(dir.join("d/sub")).unwrap();
    std::fs::write(dir.join("d/a.txt"), "alpha").unwrap();
    std::fs::write(dir.join("d/sub/b.txt"), "bravo").unwrap();
    std::os::unix::fs::symlink("a.txt", dir.join("d/link")).unwrap();
    let script = format!(
        "const fs = require('fs'); const fsp = require('fs/promises'); const D = {:?} + '/d';
         const key = (s) => JSON.stringify(s, (k, v) => typeof v === 'bigint' ? String(v) : v);
         const cb = (fn, ...a) => new Promise((res, rej) => fn(...a, (e, v) => e ? rej(e) : res(v)));
         const names = (l) => l.map((d) => d.name + (d.isDirectory() ? '/' : d.isSymbolicLink() ? '@' : ''));
         (async () => {{
           const same = async (label, sync, a, p) => {{
             const s = key(sync()), c = key(await a()), q = key(await p());
             console.log(label, s === c && s === q ? 'same' : 'DIFF ' + [s, c, q].join(' | '));
           }};
           await same('readdir', () => fs.readdirSync(D), () => cb(fs.readdir, D), () => fsp.readdir(D));
           await same('types', () => names(fs.readdirSync(D, {{ withFileTypes: true }})), async () => names(await cb(fs.readdir, D, {{ withFileTypes: true }})), async () => names(await fsp.readdir(D, {{ withFileTypes: true }})));
           await same('recursive', () => fs.readdirSync(D, {{ recursive: true }}), () => cb(fs.readdir, D, {{ recursive: true }}), () => fsp.readdir(D, {{ recursive: true }}));
           await same('file', () => fs.readFileSync(D + '/a.txt', 'utf8'), () => cb(fs.readFile, D + '/a.txt', 'utf8'), () => fsp.readFile(D + '/a.txt', 'utf8'));
           await same('buffer', () => fs.readFileSync(D + '/link'), () => cb(fs.readFile, D + '/link'), () => fsp.readFile(D + '/link'));
           for (const f of ['stat', 'lstat']) {{
             const pick = (s) => [s.isFile(), s.isSymbolicLink(), s.size, s.mode, s.ino, s.mtimeMs, s.nlink];
             await same(f, () => pick(fs[f + 'Sync'](D + '/link')), async () => pick(await cb(fs[f], D + '/link')), async () => pick(await fsp[f](D + '/link')));
             await same(f + ' big', () => pick(fs[f + 'Sync'](D + '/link', {{ bigint: true }})), async () => pick(await cb(fs[f], D + '/link', {{ bigint: true }})), async () => pick(await fsp[f](D + '/link', {{ bigint: true }})));
           }}
           await same('readlink', () => fs.readlinkSync(D + '/link'), () => cb(fs.readlink, D + '/link'), () => fsp.readlink(D + '/link'));
           await same('realpath', () => fs.realpathSync(D + '/link'), () => cb(fs.realpath, D + '/link'), () => fsp.realpath(D + '/link'));
         }})();",
        dir
    );
    let (out, err, rc) = run(&script);
    assert_eq!(rc, 0, "{err}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 11, "{out}");
    assert!(lines.iter().all(|l| l.ends_with(" same")), "{out}");
}

#[test]
fn many_concurrent_requests_survive_gc_stress_with_only_the_request_table_rooting_them() {
    let dir = scratch("gc");
    for i in 0..20 {
        std::fs::write(dir.join(format!("f{i}")), format!("content-{i}")).unwrap();
    }
    let script = format!(
        "const fs = require('fs'); let done = 0; const out = [];
         for (let i = 0; i < 20; i++) {{
           const payload = {{ i, pad: new Array(50).fill(i) }};
           fs.readFile({:?} + '/f' + i, 'utf8', (e, d) => {{
             out[payload.i] = d + payload.pad.length;
             if (++done === 20) console.log(out.join());
           }});
         }}
         fs.promises.readFile({:?} + '/f3', 'utf8').then((d) => console.log('p', d));",
        dir, dir
    );
    let expected = (0..20)
        .map(|i| format!("content-{i}50"))
        .collect::<Vec<_>>()
        .join(",");
    for period in ["1", "7"] {
        let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
            .env("JSSE_GC_STRESS", period)
            .args(["--node", "-e", &script])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(stdout.contains(&expected), "{period}: {stdout}");
        assert!(stdout.contains("p content-3"), "{period}: {stdout}");
    }
}

#[test]
fn streams_and_file_handles_round_trip_under_gc_stress() {
    let dir = scratch("streams");
    let script = format!(
        "const fs = require('fs'); const fsp = fs.promises; const {{ pipeline }} = require('stream/promises');
         (async () => {{
           const src = {:?} + '/src', dst = {:?} + '/dst';
           await fsp.writeFile(src, Buffer.alloc(300000, 'xyz'));
           await pipeline(fs.createReadStream(src, {{ highWaterMark: 4096 }}), fs.createWriteStream(dst));
           const fh = await fsp.open(dst, 'r');
           const {{ bytesRead, buffer }} = await fh.read(Buffer.alloc(6), 0, 6, 299994);
           await fh.close();
           console.log((await fsp.stat(dst)).size, bytesRead, buffer.toString());
         }})();",
        dir, dir
    );
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .env("JSSE_GC_STRESS", "5")
        .args(["--node", "-e", &script])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "300000 6 xyzxyz"
    );
}

#[cfg(target_os = "linux")]
fn max_rss_kib(mut command: Command) -> (String, i64) {
    use std::os::unix::process::ExitStatusExt;
    let mut child = command.stdout(Stdio::piped()).spawn().unwrap();
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    let mut status = 0;
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let pid = child.id() as libc::pid_t;
    assert_eq!(unsafe { libc::wait4(pid, &mut status, 0, &mut usage) }, pid);
    let _ = child.wait();
    assert!(std::process::ExitStatus::from_raw(status).success());
    (stdout.trim_end().to_string(), usage.ru_maxrss)
}

#[cfg(target_os = "linux")]
#[test]
fn streaming_a_500_mib_sparse_file_completes_in_bounded_memory() {
    const SIZE: u64 = 500 << 20;
    let dir = scratch("sparse");
    let big = dir.join("big.bin");
    std::fs::File::create(&big).unwrap().set_len(SIZE).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
    command.args([
        "--node",
        "-e",
        &format!(
            "let n = 0; const s = require('fs').createReadStream({big:?});
             s.on('data', (c) => {{ n += c.length; }}).on('end', () => console.log(n)).on('error', (e) => {{ console.log(e); process.exit(1); }});"
        ),
    ]);
    let (out, rss) = max_rss_kib(command);
    assert_eq!(out, SIZE.to_string());
    assert!(
        rss < 250 * 1024,
        "max RSS {} MiB for a {} MiB file",
        rss / 1024,
        SIZE >> 20
    );
}

#[cfg(target_os = "linux")]
#[test]
fn piping_a_large_sparse_file_to_a_write_stream_is_bounded_too() {
    const SIZE: u64 = 128 << 20;
    let dir = scratch("sparse-copy");
    let big = dir.join("big.bin");
    std::fs::File::create(&big).unwrap().set_len(SIZE).unwrap();
    let copy = dir.join("copy.bin");
    let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
    command.args([
        "--node",
        "-e",
        &format!(
            "const fs = require('fs');
             require('stream/promises').pipeline(fs.createReadStream({big:?}), fs.createWriteStream({copy:?}))
               .then(() => console.log(fs.statSync({copy:?}).size), (e) => {{ console.log(e); process.exit(1); }});"
        ),
    ]);
    let (out, rss) = max_rss_kib(command);
    assert_eq!(out, SIZE.to_string());
    assert!(rss < 250 * 1024, "max RSS {} MiB", rss / 1024);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn closing_a_file_handle_waits_for_the_stream_that_holds_it() {
    let dir = scratch("handle-close");
    let file = dir.join("f");
    std::fs::write(&file, "payload").unwrap();
    let (out, err, rc) = run(&format!(
        "const fsp = require('fs/promises');
         (async () => {{
           const h = await fsp.open({file:?});
           const s = h.createReadStream();
           const closed = new Promise((r) => s.on('close', r));
           s.on('error', (e) => console.log(e.code));
           s.resume();
           await h.close();
           await closed;
           console.log('closed', h.fd);
         }})().catch((e) => console.log('FAIL', e.code));"
    ));
    assert_eq!((out.as_str(), rc), ("closed -1", 0), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn promises_write_file_rejects_invalid_data_before_truncating() {
    let dir = scratch("write-invalid");
    let file = dir.join("f");
    std::fs::write(&file, "keep").unwrap();
    let (out, err, rc) = run(&format!(
        "require('fs/promises').writeFile({file:?}, {{}}).then(() => console.log('ok'), (e) => console.log(e.code))"
    ));
    assert_eq!((out.as_str(), rc), ("ERR_INVALID_ARG_TYPE", 0), "{err}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep");
    let _ = std::fs::remove_dir_all(&dir);
}
