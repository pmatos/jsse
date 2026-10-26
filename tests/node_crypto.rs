//! The embedded `crypto` module under `--node` (#1226), reached through
//! `require`. Known answers from FIPS 180-4/202, RFC 1321/2202/4231 and Node;
//! `scripts/run-node-crypto-oracle.sh` diffs the full corpus against Node.

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

fn each_mode(source: &str, want: &str) {
    for flags in [&[][..], &["--no-bytecode"][..]] {
        assert_eq!(node(source, flags, None), want, "flags {flags:?}");
    }
    assert_eq!(node(source, &[], Some("3")), want, "GC stress");
}

#[test]
fn known_digests() {
    let source = r#"
      const c = require("crypto");
      const h = (a, s, o) => c.createHash(a, o).update(s).digest("hex");
      print([
        h("md5", "abc"),
        h("sha1", "abc"),
        h("sha224", "abc"),
        h("sha256", ""),
        h("sha384", "abc"),
        h("sha512", "abc"),
        h("sha512-256", "abc"),
        h("sha3-256", "abc"),
        h("sha3-512", ""),
        h("shake128", "", { outputLength: 5 }),
      ].join("\n"));
    "#;
    each_mode(
        source,
        "900150983cd24fb0d6963f7d28e17f72\n\
         a9993e364706816aba3e25717850c26c9cd0d89d\n\
         23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7\n\
         e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n\
         cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7\n\
         ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f\n\
         53048e2681941ef99b2e29b76b4c7dabe4c2d0c634fc6d46e0e2f13107e7af23\n\
         3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532\n\
         a69f73cca23a9ac5c8b567dc185a756e97c982164fe25859e0d1dcc1475c80a615b2123af1f5f94c11e3e9402c3ac558f500199d95b6d3e301758586281dcd26\n\
         7f9c2ba4e8",
    );
}

#[test]
fn streaming_copy_hmac_and_encodings() {
    let source = r#"
      const c = require("crypto");
      const big = new Uint8Array(1 << 20).fill(97);
      const a = c.createHash("sha256");
      for (let off = 0; off < big.length; off += 100003) a.update(big.subarray(off, off + 100003));
      const b = c.createHash("sha1").update("ab");
      const copy = b.copy().update("c");
      print([
        a.digest("hex") === c.createHash("sha256").update(big).digest("hex"),
        a.constructor === c.Hash,
        b.digest("hex"),
        copy.digest("base64"),
        c.createHmac("sha256", "Jefe").update("what do ya want for nothing?").digest("hex"),
        c.createHmac("sha256", Buffer.alloc(131, 0xaa)).update("Test Using Larger Than Block-Size Key - Hash Key First").digest("hex"),
        c.createHash("md5").update("616263", "hex").digest("base64url"),
        c.createHash("md5").update("\ud800x").digest("hex") === c.createHash("md5").update("�x").digest("hex"),
        c.hash("sha1", "abc"),
        Buffer.isBuffer(c.createHash("md5").digest()),
      ].join("\n"));
    "#;
    each_mode(
        source,
        "true\ntrue\n\
         da23614e02469a0d7c7bd1bdab5c9c474b1904dc\n\
         qZk+NkcGgWq6PiVxeFDCbJzQ2J0=\n\
         5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843\n\
         60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54\n\
         kAFQmDzST7DWlj99KOF_cg\n\
         true\n\
         a9993e364706816aba3e25717850c26c9cd0d89d\n\
         true",
    );
}

#[test]
fn errors_match_node() {
    let source = r#"
      const c = require("crypto");
      const t = (f) => { try { f(); return "ok"; } catch (e) { return `${e.name}|${e.code}|${e.message}`; } };
      const done = c.createHash("md5"); done.digest();
      print([
        t(() => c.createHash("nope")),
        t(() => c.createHash(5)),
        t(() => c.createHash("md5").update(5)),
        t(() => done.digest()),
        t(() => c.createHmac("nope", "k")),
        t(() => c.createHash("sha256", { outputLength: 5 })),
        t(() => c.timingSafeEqual(Buffer.from("a"), Buffer.from("ab"))),
        t(() => c.randomInt(5, 1)),
        t(() => c.randomBytes(-1)),
      ].join("\n"));
    "#;
    each_mode(
        source,
        "Error|undefined|Digest method not supported\n\
         TypeError|ERR_INVALID_ARG_TYPE|The \"algorithm\" argument must be of type string. Received type number (5)\n\
         TypeError|ERR_INVALID_ARG_TYPE|The \"data\" argument must be of type string or an instance of Buffer, TypedArray, or DataView. Received type number (5)\n\
         Error|ERR_CRYPTO_HASH_FINALIZED|Digest already called\n\
         TypeError|ERR_CRYPTO_INVALID_DIGEST|Invalid digest: nope\n\
         Error|ERR_OSSL_EVP_NOT_XOF_OR_INVALID_LENGTH|error:030000B2:digital envelope routines::not XOF or invalid length\n\
         RangeError|ERR_CRYPTO_TIMING_SAFE_EQUAL_LENGTH|Input buffers must have the same byte length\n\
         RangeError|ERR_OUT_OF_RANGE|The value of \"max\" is out of range. It must be greater than the value of \"min\" (5). Received 1\n\
         RangeError|ERR_OUT_OF_RANGE|The value of \"size\" is out of range. It must be >= 0 && <= 2147483647. Received -1",
    );
}

#[test]
fn random_and_webcrypto() {
    let source = r#"
      const c = require("crypto");
      const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
      const seen = new Set();
      for (let i = 0; i < 100; i++) seen.add(c.randomInt(3));
      c.subtle.digest("SHA-256", Buffer.from("abc")).then((r) => {
        print([
          c.randomBytes(24).length,
          uuid.test(c.randomUUID()),
          [...seen].sort().join(),
          c.webcrypto === globalThis.crypto,
          r instanceof ArrayBuffer,
          Buffer.from(r).toString("hex"),
          c.getRandomValues(new Uint32Array(4)).length,
          globalThis.crypto.getRandomValues(new Uint8Array(4)).length,
          uuid.test(globalThis.crypto.randomUUID()),
          Object.keys(c).includes("webcrypto"),
          c === require("node:crypto"),
        ].join("\n"));
      });
    "#;
    each_mode(
        source,
        "24\ntrue\n0,1,2\ntrue\ntrue\nba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n4\n4\ntrue\ntrue\ntrue",
    );
}

#[test]
fn global_environment_is_unchanged_without_node() {
    let out = run(
        &[
            "-e",
            "print(typeof crypto, typeof require, typeof __host_crypto)",
        ],
        None,
    );
    assert_eq!(stdout(&out), "undefined undefined undefined");
    let hidden = node(
        "print(typeof __host_crypto, typeof globalThis.crypto.subtle)",
        &[],
        None,
    );
    assert_eq!(hidden, "undefined object");
}
