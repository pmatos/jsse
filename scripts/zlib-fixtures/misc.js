// Surface, constants, crc32 and Node-produced golden streams (issue #1239).
"use strict";

const zlib = require("zlib");
const { emit, sha, hex, flush, INPUTS, realOpts } = require("./common.js");
const golden = require("./golden-data.js");

const optional = /zstd|zip/i;
emit("exports", Object.keys(zlib).filter((k) => !optional.test(k)).sort());
emit("deprecated constants", Object.getOwnPropertyNames(zlib).filter((k) => /^Z_/.test(k)).sort());
emit("codes", zlib.codes);
emit("codes frozen", Object.isFrozen(zlib.codes));
emit("constants proto", Object.getPrototypeOf(zlib.constants));
const constants = Object.entries(zlib.constants).filter(([k]) => !optional.test(k));
emit("constants", constants.map(([k, v]) => k + "=" + v));
emit("constants descriptor", Object.getOwnPropertyDescriptor(zlib, "constants").writable);
emit("factory names", [zlib.createGzip.name, zlib.createInflateRaw.name, zlib.createBrotliCompress.name]);
emit("classes", [zlib.Deflate.name, zlib.Gunzip.name, zlib.Unzip.name,
  Object.getPrototypeOf(zlib.Gzip.prototype).constructor.name,
  new zlib.Gzip() instanceof require("stream").Transform]);
emit("engine props", (() => {
  const d = zlib.createDeflate({ level: 3, strategy: 2 });
  const r = [d._level, d._strategy, d._chunkSize, d._mode, d.bytesWritten, d._closed];
  d.close();
  return r.concat([d._closed]);
})());

const pairs = [["", 0], ["a", 0], ["hello", 0], ["hello", 907060870], ["The quick brown fox", 4294967295]];
for (const [s, init] of pairs) emit(`crc32 ${JSON.stringify(s)} ${init}`, zlib.crc32(s, init));
const big = INPUTS.random70k();
emit("crc32 70k", zlib.crc32(big));
let running = 0;
for (let i = 0; i < big.length; i += 9999) running = zlib.crc32(big.subarray(i, i + 9999), running);
emit("crc32 70k chunked", running);

golden.forEach(([fn, input, opts, packed], i) => {
  const inverse = { deflate: "inflate", gzip: "gunzip", deflateRaw: "inflateRaw" }[fn];
  const back = zlib[inverse + "Sync"](Buffer.from(packed, "hex"), realOpts(opts));
  emit(`golden ${i} ${fn} ${input}`, sha(back) === sha(INPUTS[input]()));
  emit(`golden ${i} unzip`, fn === "deflateRaw" ? "n/a" : sha(zlib.unzipSync(Buffer.from(packed, "hex"), realOpts(opts))) === sha(INPUTS[input]()));
});
flush();
