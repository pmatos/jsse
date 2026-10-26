//! The embedded `buffer` and `string_decoder` modules under `--node` (#1231),
//! reached through `require`. `scripts/run-node-buffer-oracle.sh` diffs the two
//! probes in scripts/buffer-fixtures against Node; this pins their digests (taken
//! from Node) and checks the module wiring without Node.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/buffer-fixtures")
}

fn run(args: &[&str], gc_stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    cmd.args(args).output().expect("run jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim_end().to_string()
}

fn eval(source: &str) -> String {
    let out = run(&["--node", "-e", source], None);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

fn pinned(probe: &str) -> String {
    let digests = std::fs::read_to_string(fixtures().join("digests.txt")).unwrap();
    digests
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{probe} ")))
        .unwrap_or_else(|| panic!("no digest for {probe}"))
        .to_string()
}

fn probe_digest(probe: &str, flags: &[&str], gc_stress: Option<&str>) -> String {
    let script = fixtures().join(format!("{probe}.js"));
    let mut args = vec!["--node"];
    args.extend_from_slice(flags);
    args.push(script.to_str().unwrap());
    let out = run(&args, gc_stress);
    assert!(out.status.success(), "{probe}: {}", stderr(&out));
    stdout(&out).lines().last().unwrap().to_string()
}

#[test]
fn string_decoder_probe_matches_node() {
    let want = pinned("sd-probe");
    assert_eq!(probe_digest("sd-probe", &[], None), want);
    assert_eq!(probe_digest("sd-probe", &["--no-bytecode"], None), want);
}

#[test]
fn buffer_probe_matches_node() {
    let want = pinned("buffer-probe");
    assert_eq!(probe_digest("buffer-probe", &[], None), want);
    assert_eq!(probe_digest("buffer-probe", &["--no-bytecode"], None), want);
}

#[test]
fn probes_pass_under_gc_stress() {
    for probe in ["sd-probe", "buffer-probe"] {
        assert_eq!(
            probe_digest(probe, &[], Some("50")),
            pinned(probe),
            "{probe}"
        );
    }
}

#[test]
fn modules_share_the_global_buffer() {
    let source = r#"
      const buffer = require("buffer");
      const { StringDecoder } = require("node:string_decoder");
      print([
        buffer === require("node:buffer"),
        buffer.Buffer === Buffer,
        buffer.atob === atob && buffer.btoa === btoa,
        require("string_decoder") === require("node:string_decoder"),
        new StringDecoder().lastChar.constructor === Buffer,
        Buffer.isBuffer(buffer.transcode(Buffer.from("a"), "utf8", "ucs2")),
        typeof buffer.SlowBuffer,
        Object.keys(buffer.constants).join(),
      ].join("|"));
    "#;
    assert_eq!(
        eval(source),
        "true|true|true|true|true|true|undefined|MAX_LENGTH,MAX_STRING_LENGTH"
    );
}

#[test]
fn partial_multibyte_sequences_wait_for_the_rest() {
    let source = r#"
      const { StringDecoder } = require("string_decoder");
      const d = new StringDecoder("utf8");
      const euro = Buffer.from("€");
      const out = [d.write(euro.subarray(0, 1)), d.lastNeed, d.write(euro.subarray(1, 2)),
                   d.lastNeed, d.write(euro.subarray(2)), d.lastNeed];
      const u16 = new StringDecoder("utf16le");
      const pair = Buffer.from("\u{1f600}", "utf16le");
      out.push(u16.write(pair.subarray(0, 2)), u16.write(pair.subarray(2)));
      const b64 = new StringDecoder("base64");
      out.push(b64.write(Buffer.from("abcd").subarray(0, 2)), b64.end());
      print(JSON.stringify(out));
    "#;
    assert_eq!(eval(source), r#"["",2,"",1,"€",0,"","😀","","YWI="]"#);
}

#[test]
fn the_module_is_not_in_the_default_environment() {
    let out = run(
        &[
            "-e",
            "print(typeof require, typeof Buffer, typeof StringDecoder, typeof Blob, typeof File)",
        ],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "undefined undefined undefined undefined undefined"
    );
}

#[test]
fn inspect_max_bytes_reaches_buffer_inspection() {
    let source = r#"
      const buffer = require("buffer");
      const util = require("util");
      const out = [buffer.INSPECT_MAX_BYTES, util.inspect(Buffer.from("abcd"))];
      buffer.INSPECT_MAX_BYTES = 2;
      out.push(util.inspect(Buffer.from("abcd")));
      try { buffer.INSPECT_MAX_BYTES = -1; } catch (e) { out.push(e.code); }
      print(out.join("|"));
    "#;
    assert_eq!(
        eval(source),
        "50|<Buffer 61 62 63 64>|<Buffer 61 62 ... 2 more bytes>|ERR_OUT_OF_RANGE"
    );
}
