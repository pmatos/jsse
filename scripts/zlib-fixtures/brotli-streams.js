// Brotli stream behaviour (issue #1239): chunk boundaries, flush operations,
// reset, close, errors mid-stream and event order. Compressed bytes are
// compared only at the pinned qualities (see brotli.js); everything else is
// compared as round trips and event sequences.
"use strict";

const zlib = require("zlib");
const { pipeline, Readable, Writable } = require("stream");
const { emit, sha, hex, flush, INPUTS } = require("./common.js");
const { constants: c } = zlib;
const quality = (q, extra = {}) => ({ params: { [c.BROTLI_PARAM_QUALITY]: q }, ...extra });

function collect(stream, label, writes) {
  return new Promise((resolve) => {
    const events = [];
    const chunks = [];
    stream.on("data", (d) => chunks.push(d.length + ":" + sha(d).slice(0, 8)));
    stream.on("end", () => events.push("end"));
    stream.on("finish", () => events.push("finish"));
    stream.on("close", () => {
      events.push("close");
      emit(label, [chunks, events, stream.bytesWritten]);
      resolve();
    });
    stream.on("error", (e) => events.push("error:" + e.code + ":" + e.errno));
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
  for (const q of [0, 1, 6]) {
    for (const chunkSize of [64, 1000, 16384]) {
      for (const n of [1, 999, 5000]) {
        if (n === 1 && chunkSize > 64) continue;
        await collect(zlib.createBrotliCompress(quality(q, { chunkSize })), `compress Q${q} chunk${chunkSize} write${n}`,
          splits(n === 1 ? text.subarray(0, 300) : text, n));
      }
    }
    await collect(zlib.createBrotliCompress(quality(q)), `compress Q${q} flush`,
      [text.subarray(0, 100), (s, res) => s.flush(res), text.subarray(100, 200),
        (s, res) => s.flush(c.BROTLI_OPERATION_FLUSH, res), text.subarray(200, 300)]);
    await collect(zlib.createBrotliCompress(quality(q, { flush: c.BROTLI_OPERATION_FLUSH })),
      `compress Q${q} flush-per-write`, splits(text.subarray(0, 500), 100));
    await collect(zlib.createBrotliCompress(quality(q)), `compress Q${q} metadata flush`,
      [text.subarray(0, 50), (s, res) => s.flush(c.BROTLI_OPERATION_EMIT_METADATA, res), text.subarray(50, 90)]);
  }

  for (const q of [1, 5, 6]) {
    const packed = zlib.brotliCompressSync(INPUTS.text70k(), quality(q));
    for (const [chunkSize, n] of [[64, 1], [64, 13], [1024, 7], [16384, 5000], [16384, packed.length]]) {
      const input = n === 1 ? packed.subarray(0, 40) : packed;
      await collect(zlib.createBrotliDecompress({ chunkSize }), `decompress Q${q} chunk${chunkSize} write${n}`,
        splits(input, n));
    }
  }
  for (const q of [2, 9, 11]) {
    const packed = zlib.brotliCompressSync(INPUTS.text70k(), quality(q));
    const parts = [];
    const stream = zlib.createBrotliDecompress({ chunkSize: 1024 });
    stream.on("data", (d) => parts.push(d));
    for (const part of splits(packed, 13)) stream.write(part);
    stream.end();
    await new Promise((res) => stream.on("close", res));
    emit(`decompress Q${q} write13`, sha(Buffer.concat(parts)) === sha(INPUTS.text70k()));
  }

  const back = [];
  const e = zlib.createBrotliCompress(quality(5));
  const d = zlib.createBrotliDecompress();
  e.pipe(d).on("data", (b) => back.push(b)).on("end", () => emit("pipe round trip", sha(Buffer.concat(back)) === sha(INPUTS.text70k())));
  for (const part of splits(INPUTS.text70k(), 7000)) e.write(part);
  e.end();
  await new Promise((res) => d.on("close", res));

  const r = zlib.createBrotliCompress(quality(0));
  const parts = [];
  r.on("data", (b) => parts.push(hex(b)));
  r.write("first", () => r.flush(() => {
    r.reset();
    r.write("second");
    r.end();
  }));
  await new Promise((res) => r.on("close", res));
  emit("reset", [parts[0], parts[1].slice(0, 4)]);
  const decoded = zlib.createBrotliDecompress();
  const seen = [];
  decoded.on("data", (b) => seen.push(b.toString()));
  decoded.write(zlib.brotliCompressSync("one "), () => decoded.flush(() => {
    decoded.reset();
    decoded.write(zlib.brotliCompressSync("two "));
    decoded.end();
  }));
  await new Promise((res) => decoded.on("close", res));
  emit("decoder reset", seen);

  const ended = zlib.createBrotliDecompress();
  const tail = [];
  ended.on("data", (b) => tail.push(b.toString()));
  ended.on("error", (err) => tail.push("error:" + err.code));
  ended.write(Buffer.concat([zlib.brotliCompressSync("done"), Buffer.from("xx")]));
  ended.write(zlib.brotliCompressSync("ignored"));
  ended.end();
  await new Promise((res) => ended.on("close", res));
  emit("data after the stream end", tail);

  const bad = zlib.createBrotliDecompress();
  const errors = [];
  bad.on("error", (err) => errors.push(err.code + ":" + err.errno + ":" + err.message));
  bad.on("close", () => errors.push("close"));
  bad.write(Buffer.from("this is not brotli data at all"));
  bad.write(Buffer.from("more"));
  await new Promise((res) => bad.on("close", res));
  emit("garbage stream", errors);

  const cut = zlib.createBrotliDecompress();
  const cutEvents = [];
  cut.on("error", (err) => cutEvents.push(err.code + ":" + err.message));
  cut.on("end", () => cutEvents.push("end"));
  cut.end(zlib.brotliCompressSync(INPUTS.text5k()).subarray(0, 100));
  await new Promise((res) => cut.on("close", res));
  emit("truncated stream", cutEvents);

  const closed = zlib.createBrotliCompress();
  const log = [];
  closed.on("close", () => log.push("close"));
  closed.close(() => log.push("callback"));
  await new Promise((res) => setImmediate(res));
  emit("close callback", [log, closed.destroyed]);

  const afterEnd = zlib.createBrotliCompress();
  const errs = [];
  afterEnd.on("error", (err) => errs.push(err.code));
  afterEnd.end("x");
  afterEnd.write("y");
  await new Promise((res) => setImmediate(res));
  await new Promise((res) => setImmediate(res));
  emit("write after end", errs);

  const sink = [];
  await new Promise((res) => pipeline(
    Readable.from([Buffer.from("pipe "), Buffer.from("line")]),
    zlib.createBrotliCompress(quality(1)),
    zlib.createBrotliDecompress(),
    new Writable({ write(chunk, enc, cb) { sink.push(chunk.toString()); cb(); } }),
    (err) => { emit("pipeline", [err && err.code, sink]); res(); }));

  const noop = zlib.createBrotliCompress();
  await new Promise((res) => noop.params(1, 0, res));
  emit("params is inert", typeof noop._handle.params);
  noop.destroy();

  const dictionary = Buffer.from("shared dictionary text");
  const withDict = zlib.createBrotliCompress({ dictionary, ...quality(6) });
  const packed = [];
  withDict.on("data", (b) => packed.push(b));
  withDict.end("shared dictionary text and more");
  await new Promise((res) => withDict.on("close", res));
  emit("dictionary stream", hex(Buffer.concat(packed)));
  const reopened = zlib.createBrotliDecompress({ dictionary });
  const plain = [];
  reopened.on("data", (b) => plain.push(b));
  reopened.end(Buffer.concat(packed));
  await new Promise((res) => reopened.on("close", res));
  emit("dictionary stream back", Buffer.concat(plain).toString());
  const resetting = zlib.createBrotliCompress({ dictionary, ...quality(6) });
  const afterReset = [];
  resetting.on("data", (b) => afterReset.push(b));
  resetting.write("shared dictionary text", () => resetting.flush(() => {
    resetting.reset();
    resetting.end("shared dictionary text and more");
  }));
  await new Promise((res) => resetting.on("close", res));
  emit("dictionary after reset", afterReset.map(hex));

  flush();
})();
