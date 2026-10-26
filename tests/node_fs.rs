//! Node host mode synchronous `fs` (#1217): the `__host_fs_*` floor and the JS
//! `fs` basics built on it. Child-process checks; the Node-oracle diff lives in
//! `scripts/run-node-fs-oracle.sh`.
#![cfg(unix)]

use std::process::{Command, Output};

fn jsse(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(args)
        .output()
        .expect("run jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn run_node(dir: &std::path::Path, source: &str) -> String {
    let script = format!(
        "const fs = require(\"fs\"); const D = {:?};\n{source}",
        dir.to_str().unwrap()
    );
    let out = jsse(&["--node", "-e", &script]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout(&out)
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("jsse-node-fs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn fs_is_absent_without_node_and_the_floor_is_private_with_it() {
    for flags in [&[][..], &["--prelude", "/dev/null"][..]] {
        let mut args = flags.to_vec();
        args.extend(["-e", "print(typeof __host_fs_open, typeof __node_builtins)"]);
        assert_eq!(stdout(&jsse(&args)), "undefined undefined", "{flags:?}");
    }
    let out = jsse(&[
        "--node",
        "-e",
        "print(typeof __host_fs_open, typeof __host_fs_read_file, typeof __node_fs_support, typeof fs, Object.keys(globalThis).some(function (k) { return /fs|builtins/.test(k); }))",
    ]);
    assert_eq!(
        stdout(&out),
        "undefined undefined undefined undefined false"
    );
}

#[test]
fn errors_carry_nodes_uv_exception_shape() {
    let dir = scratch("uv");
    let out = run_node(
        &dir,
        r#"
        try { fs.readFileSync(D + "/missing"); } catch (e) {
          console.log(e instanceof Error, e.code, e.errno, e.syscall, e.path === D + "/missing", e.message === "ENOENT: no such file or directory, open '" + D + "/missing'");
        }
        try { fs.renameSync(D + "/a", D + "/b"); } catch (e) {
          console.log(e.code, e.syscall, e.path === D + "/a", e.dest === D + "/b");
        }
        "#,
    );
    assert_eq!(
        out,
        "true ENOENT -2 open true true\nENOENT rename true true"
    );
}

#[test]
fn invalid_arguments_throw_type_errors_with_nodes_codes() {
    let dir = scratch("args");
    let out = run_node(
        &dir,
        r#"
        function code(f) { try { f(); return "none"; } catch (e) { return e.name + ":" + e.code; } }
        console.log(code(() => fs.readFileSync(42.5)));
        console.log(code(() => fs.readFileSync({})));
        console.log(code(() => fs.readFileSync("a\0b")));
        console.log(code(() => fs.writeFileSync(D + "/f", 5)));
        console.log(code(() => fs.readFileSync(D, "nope")));
        console.log(code(() => fs.closeSync("3")));
        console.log(code(() => fs.closeSync(-1)));
        console.log(code(() => fs.openSync(D, "zz")));
        "#,
    );
    assert_eq!(
        out,
        "TypeError:ERR_INVALID_ARG_TYPE\nTypeError:ERR_INVALID_ARG_TYPE\nTypeError:ERR_INVALID_ARG_VALUE\nTypeError:ERR_INVALID_ARG_TYPE\nTypeError:ERR_INVALID_ARG_VALUE\nTypeError:ERR_INVALID_ARG_TYPE\nRangeError:ERR_OUT_OF_RANGE\nTypeError:ERR_INVALID_ARG_VALUE"
    );
}

#[test]
fn file_urls_buffers_and_non_utf8_names_are_accepted_as_paths() {
    let dir = scratch("paths");
    let name = dir.join("plain.txt");
    std::fs::write(&name, "plain").unwrap();
    let out = run_node(
        &dir,
        r#"
        const p = D + "/plain.txt";
        const url = { href: "file://" + p, protocol: "file:", hostname: "", pathname: p };
        console.log(fs.readFileSync(url, "utf8"), fs.readFileSync(Buffer.from(p), "utf8"));
        const bad = { href: "http://x/", protocol: "http:", hostname: "", pathname: "/" };
        try { fs.readFileSync(bad); } catch (e) { console.log(e.code); }
        fs.writeFileSync(Buffer.concat([Buffer.from(D + "/"), Buffer.from([0xff, 0x41])]), "raw");
        console.log(fs.readdirSync(D, "buffer").map((b) => b.toString("hex")).join(","));
        "#,
    );
    assert_eq!(
        out,
        "plain plain\nERR_INVALID_URL_SCHEME\n706c61696e2e747874,ff41"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_large_file_is_read_and_written_without_extra_copies() {
    let dir = scratch("large");
    let mib = 128;
    let out = run_node(
        &dir,
        &format!(
            r#"
            const mib = {mib};
            const hwm = () => Number(/VmHWM:\s+(\d+) kB/.exec(fs.readFileSync("/proc/self/status", "utf8"))[1]) / 1024;
            const before = hwm();
            const data = new Uint8Array(mib << 20).fill(0x5a);
            data[0] = 1; data[data.length - 1] = 2;
            fs.writeFileSync(D + "/big.bin", data);
            const back = fs.readFileSync(D + "/big.bin");
            const grew = hwm() - before;
            console.log(back.length === data.length, back[0], back[back.length - 1], back[mib << 19], Math.round(grew));
            "#
        ),
    );
    let fields: Vec<&str> = out.split(' ').collect();
    assert_eq!(&fields[..4], ["true", "1", "2", "90"], "{out}");
    let grew: u64 = fields[4].parse().unwrap();
    // The resident source array plus the one buffer the read returns; a
    // staging copy of either would add another 128 MiB.
    assert!(grew < 2 * mib + mib / 2, "peak RSS grew {grew} MiB: {out}");
}
