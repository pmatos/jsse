// Stream behaviour (issue #1239): chunk boundaries, flush, params, reset,
// close and destruction, ordering of events. Compressed bytes are compared
// only where the algorithm pins them (stored, Huffman-only, RLE); inflate runs
// over the Node-produced golden streams.
"use strict";

const zlib = require("zlib");
const { pipeline, Readable, Writable } = require("stream");
const { emit, sha, hex, flush, INPUTS, realOpts } = require("./common.js");
const golden = require("./golden-data.js");
const { Z_SYNC_FLUSH, Z_FULL_FLUSH, Z_HUFFMAN_ONLY, Z_RLE } = zlib.constants;

function collect(stream, label, writes, opts = {}) {
  return new Promise((resolve) => {
    const events = [];
    const chunks = [];
    stream.on("data", (c) => chunks.push(c.length + ":" + sha(c).slice(0, 8)));
    stream.on("end", () => events.push("end"));
    stream.on("finish", () => events.push("finish"));
    stream.on("close", () => {
      events.push("close");
      emit(label, [chunks, events, stream.bytesWritten]);
      resolve();
    });
    stream.on("error", (e) => events.push("error:" + e.code));
    (async () => {
      for (const w of writes) {
        if (typeof w === "function") await new Promise((res) => w(stream, res));
        else await new Promise((res) => stream.write(w, res));
      }
      stream.end();
    })();
  });
}

const splits = (buf, n) => {
  const parts = [];
  for (let i = 0; i < buf.length; i += n) parts.push(buf.subarray(i, i + n));
  return parts;
};

(async () => {
  const text = INPUTS.text5k();
  for (const [name, opts] of [["stored", { level: 0 }], ["huffman", { strategy: Z_HUFFMAN_ONLY }], ["rle", { strategy: Z_RLE }]]) {
    for (const chunkSize of [64, 1000, 16384]) {
      for (const n of [1, 999, 5000]) {
        if (n === 1 && chunkSize > 64) continue;
        await collect(zlib.createDeflate({ ...opts, chunkSize }), `deflate ${name} chunk${chunkSize} write${n}`,
          splits(n === 1 ? text.subarray(0, 300) : text, n));
      }
    }
    await collect(zlib.createGzip(opts), `gzip ${name} flush`,
      [text.subarray(0, 100), (s, res) => s.flush(res), text.subarray(100, 200),
        (s, res) => s.flush(Z_FULL_FLUSH, res), text.subarray(200, 300)]);
    await collect(zlib.createDeflateRaw({ ...opts, flush: Z_SYNC_FLUSH }), `raw ${name} flush-per-write`,
      splits(text.subarray(0, 500), 100));
  }

  for (const [i, [fn, input, opts, packed]] of golden.entries()) {
    if (i % 3 !== 0 && fn !== "gzip") continue;
    const inverse = { deflate: "inflate", gzip: "gunzip", deflateRaw: "inflateRaw" }[fn];
    const data = Buffer.from(packed, "hex");
    for (const [chunkSize, n] of [[64, 1], [64, 13], [1024, 7], [16384, data.length]]) {
      await collect(zlib[`create${inverse[0].toUpperCase()}${inverse.slice(1)}`]({ ...realOpts(opts), chunkSize }),
        `golden${i} ${inverse} chunk${chunkSize} write${n}`, splits(data, n));
    }
  }

  const d = zlib.createDeflate({ level: 0 });
  const order = [];
  d.on("data", (c) => order.push("data" + c.length + ":" + sha(c).slice(0, 8)));
  d.write(Buffer.from("params"), () => order.push("written"));
  d.params(6, Z_HUFFMAN_ONLY, () => {
    order.push("params-done");
    d.write(Buffer.from("more"));
    d.end(() => {
      order.push("ended");
      emit("params order", order);
    });
  });
  await new Promise((res) => d.on("close", res));

  const r = zlib.createDeflate({ level: 0 });
  const parts = [];
  r.on("data", (c) => parts.push(hex(c)));
  r.write("first", () => r.flush(() => {
    r.reset();
    r.write("second");
    r.end();
  }));
  await new Promise((res) => r.on("close", res));
  emit("reset", parts);

  const closed = zlib.createGzip();
  const log = [];
  closed.on("close", () => log.push("close"));
  closed.close(() => log.push("callback"));
  await new Promise((res) => setImmediate(res));
  emit("close callback", [log, closed.destroyed]);

  const afterEnd = zlib.createDeflate();
  const errs = [];
  afterEnd.on("error", (e) => errs.push(e.code));
  afterEnd.end("x");
  afterEnd.write("y");
  await new Promise((res) => setImmediate(res));
  await new Promise((res) => setImmediate(res));
  emit("write after end", errs);

  const sink = [];
  await new Promise((res) => pipeline(
    Readable.from([Buffer.from("pipe "), Buffer.from("line")]),
    zlib.createDeflate({ level: 0 }),
    zlib.createInflate(),
    new Writable({ write(c, e, cb) { sink.push(c.toString()); cb(); } }),
    (err) => { emit("pipeline", [err && err.code, sink]); res(); }));

  const gz = zlib.createGunzip();
  const members = [];
  gz.on("data", (c) => members.push(c.toString()));
  gz.end(Buffer.concat([zlib.gzipSync("one,"), zlib.gzipSync("two,"), Buffer.alloc(5)]));
  await new Promise((res) => gz.on("close", res));
  emit("multi-member stream", members);

  const dictionary = Buffer.from("shared dictionary text");
  const dd = zlib.createDeflate({ dictionary, level: 0 });
  const packed = [];
  dd.on("data", (c) => packed.push(c));
  dd.end("shared dictionary text and more");
  await new Promise((res) => dd.on("close", res));
  emit("dictionary stream", hex(Buffer.concat(packed)));

  flush();
})();
