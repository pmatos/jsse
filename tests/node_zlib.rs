//! The embedded `zlib` module under `--node` (#1239), reached through
//! `require`. Expected bytes are the ones Node and zlib produce for the
//! algorithmically pinned cases (stored blocks, Huffman-only, RLE, empty
//! input); `scripts/run-node-zlib-oracle.sh` diffs the full corpus against Node.

use std::process::{Command, Output};

fn run(args: &[&str], gc_stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    cmd.args(args);
    cmd.output().expect("run jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn node(source: &str, flags: &[&str], gc_stress: Option<&str>) -> String {
    let mut args = vec!["--node"];
    args.extend_from_slice(flags);
    args.extend(["-e", source]);
    let out = run(&args, gc_stress);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout(&out)
}

fn both_engines(source: &str, want: &str) {
    for flags in [&[][..], &["--no-bytecode"][..]] {
        assert_eq!(node(source, flags, None), want, "flags {flags:?}");
    }
}

fn each_mode(source: &str, want: &str) {
    for flags in [&[][..], &["--no-bytecode"][..]] {
        assert_eq!(node(source, flags, None), want, "flags {flags:?}");
    }
    assert_eq!(node(source, &[], Some("3")), want, "GC stress");
}

#[test]
fn pinned_compressed_bytes() {
    let source = r#"
      const z = require("zlib");
      const h = (b) => b.toString("hex");
      print([
        h(z.deflateSync("abc", { level: 0 })),
        h(z.deflateSync("hello hello hello", { strategy: z.constants.Z_HUFFMAN_ONLY })),
        h(z.deflateRawSync("a".repeat(36), { strategy: z.constants.Z_RLE })),
        h(z.deflateSync("")),
        h(z.gzipSync("")),
        h(z.deflateRawSync("")),
        h(z.gzipSync("abc", { level: 0 })),
        h(z.deflateSync("hello world hello", { level: 0, dictionary: Buffer.from("hello world") })),
        h(z.deflateSync("abc", { level: 0, finishFlush: z.constants.Z_SYNC_FLUSH })),
      ].join("\n"));
    "#;
    each_mode(
        source,
        "7801010300fcff616263024d0127\n\
7801cb48cdc9c957c848cdc9c957c848cdc9c907003a2e067d\n\
4b24020000\n\
789c030000000001\n\
1f8b080000000000000303000000000000000000\n\
0300\n\
1f8b0800000000000403010300fcff616263c241243503000000\n\
783f1a0b045d011100eeff68656c6c6f20776f726c642068656c6c6f3b200691\n\
7801000300fcff616263000000ffff",
    );
}

#[test]
fn decodes_streams_made_by_node() {
    let source = r#"
      const z = require("zlib");
      const from = (s) => Buffer.from(s, "hex");
      print([
        z.inflateSync(from("789ccb48cdc9c957c8402701680308b1")),
        z.gunzipSync(from("1f8b08000000000000034b4c4a0600c241243503000000")),
        z.unzipSync(from("1f8b08000000000000034b4c4a0600c241243503000000")),
        z.unzipSync(from("789ccb48cdc9c957c8402701680308b1")),
        z.inflateRawSync(from("4b4c4a0600")),
      ].join("\n"));
    "#;
    each_mode(
        source,
        "hello hello hello hello\nabc\nabc\nhello hello hello hello\nabc",
    );
}

#[test]
fn crc32_matches_node() {
    let source = r#"
      const z = require("zlib");
      print([
        z.crc32("hello"), z.crc32("abc"), z.crc32(""),
        z.crc32(Buffer.from("world"), z.crc32("hello ")),
        z.crc32("a\ud800b"), z.crc32("a", 2 ** 32 - 1),
      ].join(" "));
    "#;
    each_mode(
        source,
        "907060870 891568578 0 222957957 3501822242 3310005809",
    );
}

#[test]
fn round_trips_every_mode_level_and_strategy() {
    let source = r#"
      const z = require("zlib");
      const data = Buffer.from("The quick brown fox jumps over the lazy dog. ".repeat(90));
      let bad = 0, runs = 0;
      for (const [c, d] of [["deflate", "inflate"], ["gzip", "gunzip"], ["deflateRaw", "inflateRaw"], ["gzip", "unzip"]]) {
        for (const level of [-1, 0, 1, 5, 9]) {
          for (let strategy = 0; strategy <= 4; strategy++) {
            for (const memLevel of [1, 9]) {
              runs++;
              const back = z[d + "Sync"](z[c + "Sync"](data, { level, strategy, memLevel }));
              if (!back.equals(data)) bad++;
            }
          }
        }
      }
      print(runs, bad);
    "#;
    both_engines(source, "200 0");
}

#[test]
fn callbacks_and_streams_keep_node_ordering() {
    let source = r#"
      const z = require("zlib");
      const log = [];
      z.deflate("async", { level: 0 }, (e, b) => {
        log.push("deflate:" + e + ":" + b.toString("hex"));
        z.inflate(b, (e2, r) => {
          log.push("inflate:" + e2 + ":" + r);
          const gz = z.createGzip({ level: 0, chunkSize: 64 });
          const chunks = [];
          gz.on("data", (c) => chunks.push(c.length));
          gz.on("end", () => log.push("end:" + chunks.join(",")));
          gz.on("close", () => { log.push("close"); print(log.join("\n")); });
          gz.write(Buffer.alloc(100, 7));
          gz.write(Buffer.alloc(100, 8));
          gz.end();
        });
      });
      log.push("sync-first");
    "#;
    each_mode(
        source,
        "sync-first\n\
deflate:null:7801010500faff6173796e630660021f\n\
inflate:null:async\n\
end:10,54,64,64,31\n\
close",
    );
}

#[test]
fn errors_match_node() {
    let source = r#"
      const z = require("zlib");
      const t = (fn) => { try { return fn(); } catch (e) { return [e.code, e.errno, e.message].join("|"); } };
      const dict = Buffer.from("hello world");
      const withDict = z.deflateSync("hello world hello", { dictionary: dict });
      print([
        t(() => z.inflateSync(Buffer.from("garbage"))),
        t(() => z.inflateSync(Buffer.alloc(0))),
        t(() => z.gunzipSync(z.gzipSync("abc").subarray(0, 12))),
        t(() => z.inflateSync(withDict)),
        t(() => z.inflateSync(withDict, { dictionary: Buffer.from("other") })),
        t(() => z.unzipSync(Buffer.from([0x1f, 0x00, 0x00]))),
        t(() => z.deflateSync("a", { level: 10 })),
        t(() => z.deflateSync("a", { chunkSize: 63 })),
        t(() => z.inflateSync(z.deflateSync(Buffer.alloc(1000)), { maxOutputLength: 10 })),
        t(() => z.gunzipSync(Buffer.concat([z.gzipSync("x"), Buffer.from("zz")]), { rejectGarbageAfterEnd: true })),
        t(() => z.deflateSync(42)),
      ].join("\n"));
    "#;
    each_mode(
        source,
        "Z_DATA_ERROR|-3|incorrect header check\n\
Z_BUF_ERROR|-5|unexpected end of file\n\
Z_BUF_ERROR|-5|unexpected end of file\n\
Z_NEED_DICT|2|Missing dictionary\n\
Z_NEED_DICT|2|Bad dictionary\n\
Z_DATA_ERROR|-3|unknown compression method\n\
ERR_OUT_OF_RANGE||The value of \"options.level\" is out of range. It must be >= -1 and <= 9. Received 10\n\
ERR_OUT_OF_RANGE||The value of \"options.chunkSize\" is out of range. It must be >= 64. Received 63\n\
ERR_BUFFER_TOO_LARGE||Cannot create a Buffer larger than 10 bytes\n\
ERR_TRAILING_JUNK_AFTER_STREAM_END||Trailing junk found after the end of the compressed stream\n\
ERR_INVALID_ARG_TYPE||The \"buffer\" argument must be of type string or an instance of Buffer, TypedArray, DataView, or ArrayBuffer. Received type number (42)",
    );
}

#[test]
fn brotli_reports_unavailable_until_the_codec_lands() {
    let source = r#"
      const z = require("zlib");
      try { z.brotliCompressSync("a"); } catch (e) { print(e.code, e.message); }
      print(typeof z.createBrotliDecompress, z.constants.BROTLI_PARAM_QUALITY);
    "#;
    assert_eq!(
        node(source, &[], None),
        "ERR_FEATURE_UNAVAILABLE_ON_PLATFORM The feature Brotli is unavailable on the current platform, which is being used to run Node.js\nfunction 1"
    );
}

#[test]
fn stream_error_destroys_the_engine() {
    let source = r#"
      const z = require("zlib");
      const s = z.createInflate();
      const events = [];
      s.on("error", (e) => events.push("error:" + e.code));
      s.on("close", () => { events.push("close:" + s.destroyed); print(events.join(" ")); });
      s.write(Buffer.from("garbage"));
    "#;
    each_mode(source, "error:Z_DATA_ERROR close:true");
}

#[test]
fn large_input_runs_in_place_over_many_output_chunks() {
    let source = r#"
      const z = require("zlib");
      const data = Buffer.alloc(4 << 20, 97);
      const packed = z.gzipSync(data, { level: 0 });
      const back = z.gunzipSync(packed);
      print(packed.length > data.length, back.length === data.length, z.crc32(back) === z.crc32(data));
    "#;
    assert_eq!(node(source, &[], None), "true true true");
}

#[test]
fn global_environment_is_unchanged_without_node() {
    let out = run(
        &[
            "-e",
            "print(typeof zlib, typeof require, typeof __host_zlib)",
        ],
        None,
    );
    assert_eq!(stdout(&out), "undefined undefined undefined");
    let hidden = node(
        "print(typeof __host_zlib, typeof globalThis.zlib)",
        &[],
        None,
    );
    assert_eq!(hidden, "undefined undefined");
}
