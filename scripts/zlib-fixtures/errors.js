// Errors, option validation and unusual inputs (issue #1239): messages, codes
// and errnos as Node reports them.
"use strict";

const zlib = require("zlib");
const { emit, attempt, hex, sha, flush, INPUTS } = require("./common.js");

const small = Buffer.from("hello hello hello hello");
const deflated = zlib.deflateSync(small, { level: 0 });
const gz = zlib.gzipSync(small, { level: 0 });

attempt("inflate garbage", () => zlib.inflateSync(Buffer.from("garbage")));
attempt("inflate empty", () => zlib.inflateSync(Buffer.alloc(0)));
attempt("inflate truncated", () => zlib.inflateSync(deflated.subarray(0, deflated.length - 3)));
attempt("inflate truncated sync flush", () =>
  zlib.inflateSync(deflated.subarray(0, deflated.length - 3), { finishFlush: zlib.constants.Z_SYNC_FLUSH }));
attempt("gunzip truncated", () => zlib.gunzipSync(gz.subarray(0, gz.length - 4)));
attempt("gunzip bad crc", () => {
  const bad = Buffer.from(gz);
  bad[bad.length - 6] ^= 1;
  return zlib.gunzipSync(bad);
});
attempt("gunzip bad length", () => {
  const bad = Buffer.from(gz);
  bad[bad.length - 1] ^= 1;
  return zlib.gunzipSync(bad);
});
attempt("inflate adler", () => {
  const bad = Buffer.from(deflated);
  bad[bad.length - 1] ^= 1;
  return zlib.inflateSync(bad);
});
attempt("inflateRaw garbage", () => zlib.inflateRawSync(Buffer.from("garbage in")));
attempt("inflateRaw bad block type", () => zlib.inflateRawSync(Buffer.from([0x07, 0x00])));
attempt("gunzip as inflate", () => zlib.inflateSync(gz));
attempt("inflate as gunzip", () => zlib.gunzipSync(deflated));
attempt("unzip garbage", () => zlib.unzipSync(Buffer.from("garbage")));
attempt("unzip one byte 1f", () => zlib.unzipSync(Buffer.from([0x1f])));
attempt("unzip 1f 00", () => zlib.unzipSync(Buffer.from([0x1f, 0x00, 0x00])));

const dict = Buffer.from("hello world");
const withDict = zlib.deflateSync(Buffer.from("hello world hello"), { dictionary: dict });
attempt("missing dictionary", () => zlib.inflateSync(withDict));
attempt("wrong dictionary", () => zlib.inflateSync(withDict, { dictionary: Buffer.from("other dict") }));
attempt("right dictionary", () => zlib.inflateSync(withDict, { dictionary: dict }).toString());
attempt("raw dictionary mismatch", () => {
  const raw = zlib.deflateRawSync(Buffer.from("hello world hello"), { dictionary: dict });
  return zlib.inflateRawSync(raw, { dictionary: Buffer.from("zzz") }).length;
});

const member = (s) => zlib.gzipSync(Buffer.from(s));
attempt("gunzip members", () => zlib.gunzipSync(Buffer.concat([member("one "), member("two ")])).toString());
attempt("gunzip trailing zeros", () =>
  zlib.gunzipSync(Buffer.concat([member("zeros"), Buffer.alloc(7)])).toString());
attempt("gunzip trailing garbage", () => zlib.gunzipSync(Buffer.concat([member("junk"), Buffer.from("zz")])).toString());
attempt("gunzip reject garbage", () =>
  zlib.gunzipSync(Buffer.concat([member("junk"), Buffer.from("zz")]), { rejectGarbageAfterEnd: true }));
attempt("inflate trailing", () => zlib.inflateSync(Buffer.concat([deflated, Buffer.from("xyz")])).toString());
attempt("inflate reject trailing", () =>
  zlib.inflateSync(Buffer.concat([deflated, Buffer.from("xyz")]), { rejectGarbageAfterEnd: true }));

attempt("maxOutputLength", () => zlib.inflateSync(zlib.deflateSync(Buffer.alloc(1000)), { maxOutputLength: 10 }));
attempt("maxOutputLength deflate", () => zlib.deflateSync(INPUTS.random1k(), { maxOutputLength: 5 }));
attempt("chunkSize small", () => zlib.inflateSync(zlib.deflateSync(INPUTS.text5k()), { chunkSize: 64 }).length);
attempt("chunkSize low", () => zlib.deflateSync("a", { chunkSize: 63 }));
attempt("chunkSize inf", () => zlib.deflateSync("a", { chunkSize: Infinity }));
attempt("chunkSize string", () => zlib.deflateSync("a", { chunkSize: "x" }));
attempt("level range", () => zlib.deflateSync("a", { level: 10 }));
attempt("level type", () => zlib.deflateSync("a", { level: "9" }));
attempt("windowBits low", () => zlib.deflateSync("a", { windowBits: 7 }));
attempt("windowBits gzip 8", () => zlib.gzipSync("a", { windowBits: 8 }).length);
attempt("windowBits raw 8", () => hex(zlib.deflateRawSync("a", { windowBits: 8 })));
attempt("windowBits inflate 0", () => zlib.inflateSync(zlib.deflateSync("a"), { windowBits: 0 }).toString());
attempt("memLevel range", () => zlib.deflateSync("a", { memLevel: 10 }));
attempt("strategy range", () => zlib.deflateSync("a", { strategy: 5 }));
attempt("flush range", () => zlib.deflateSync("a", { flush: 9 }));
attempt("finishFlush range", () => zlib.deflateSync("a", { finishFlush: -1 }));
attempt("dictionary type", () => zlib.deflateSync("a", { dictionary: "text" }));
attempt("dictionary arraybuffer", () => zlib.inflateSync(zlib.deflateSync("a", { dictionary: new Uint8Array(dict).buffer }), { dictionary: dict }).toString());
attempt("rejectGarbage type", () => zlib.gunzipSync(gz, { rejectGarbageAfterEnd: 1 }));
attempt("input type", () => zlib.deflateSync(42));
attempt("input null", () => zlib.deflateSync(null));
attempt("input arraybuffer", () => hex(zlib.deflateSync(new Uint8Array([1, 2, 3]).buffer, { level: 0 })));
attempt("input typedarray", () => hex(zlib.deflateSync(new Uint16Array([1, 2, 3]), { level: 0 })));
attempt("input dataview", () => hex(zlib.deflateSync(new DataView(new Uint8Array([9, 8, 7]).buffer), { level: 0 })));
attempt("input string", () => hex(zlib.deflateSync("héllo", { level: 0 })));
attempt("callback missing", () => zlib.deflate("a"));
attempt("callback type", () => zlib.deflate("a", {}, 5));
attempt("crc32 type", () => zlib.crc32(5));
attempt("crc32 value", () => zlib.crc32("a", -1));
attempt("crc32 value big", () => zlib.crc32("a", 2 ** 32));
attempt("crc32 neg zero", () => zlib.crc32("a", -0));
attempt("crc32 string", () => zlib.crc32("hello"));
attempt("crc32 empty", () => zlib.crc32(""));
attempt("crc32 chain", () => zlib.crc32(Buffer.from("world"), zlib.crc32("hello ")));
attempt("crc32 unicode", () => zlib.crc32("héllo wörld \u{1F600}"));
attempt("crc32 lone surrogate", () => zlib.crc32("a\ud800b"));
attempt("crc32 view", () => zlib.crc32(new Uint16Array([1, 2, 3, 258])));
attempt("crc32 offset view", () => zlib.crc32(Buffer.from("xxhelloxx").subarray(2, 7)));
attempt("new without new", () => typeof zlib.Deflate().write);
attempt("params bad", () => zlib.createDeflate().params(99, 0));

(async () => {
  const wait = (fn) => new Promise((res) => fn((e, v) => res(e ? `${e.code}|${e.errno}|${e.message}` : v.toString())));
  emit("async garbage", await wait((cb) => zlib.inflate(Buffer.from("garbage"), cb)));
  emit("async truncated", await wait((cb) => zlib.inflate(deflated.subarray(0, 8), cb)));
  emit("async maxOutputLength", await wait((cb) => zlib.inflate(zlib.deflateSync(Buffer.alloc(100)), { maxOutputLength: 5 }, cb)));
  emit("async ok", await wait((cb) => zlib.gunzip(gz, cb)));

  const s = zlib.createInflate();
  const events = [];
  s.on("error", (e) => events.push("error:" + e.code));
  s.on("close", () => events.push("close"));
  s.write(Buffer.from("garbage"));
  await new Promise((res) => setImmediate(res));
  await new Promise((res) => setImmediate(res));
  emit("stream error events", events);
  emit("stream destroyed", [s.destroyed, s._handle === null]);
  flush();
})();
