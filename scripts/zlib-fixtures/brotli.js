// The Brotli matrix (issue #1239): every input through every quality, lgwin,
// mode and size hint, as sync and callback calls, plus parameter validation,
// dictionaries, errors and `maxOutputLength`. A round-trip verdict is always
// printed; the compressed bytes only at the qualities where jsse's `brotli`
// crate is byte-identical to Node's libbrotli: qualities 0, 1 and 6-8 always,
// quality 5 up to 100000 input bytes at the default window and size hint
// (pinned() below). Run with `all` as the first argument to print every digest
// while investigating.
"use strict";

const zlib = require("zlib");
const { emit, attempt, sha, hex, flush, INPUTS } = require("./common.js");
const { constants: c } = zlib;

const ALL = process.argv[2] === "all";
const pinned = (quality, length, lgwin = 22, hint = 0) =>
  ALL || [0, 1, 6, 7, 8].includes(quality) || (quality === 5 && length <= 100000 && lgwin >= 22 && hint < 100000);
const params = (p) => ({ params: p });

for (const [name, make] of Object.entries(INPUTS)) {
  const input = make();
  const digest = sha(input);
  for (let quality = 0; quality <= 11; quality++) {
    if (input.length > 100000 && quality > 9) continue;
    for (const mode of [0, 1, 2]) {
      if (mode > 0 && !["text5k", "random1k", "hello"].includes(name)) continue;
      const packed = zlib.brotliCompressSync(input, params({
        [c.BROTLI_PARAM_QUALITY]: quality, [c.BROTLI_PARAM_MODE]: mode,
      }));
      const back = zlib.brotliDecompressSync(packed);
      const bytes = pinned(quality, input.length) ? " " + sha(packed) : "";
      emit(`brotli ${name} Q${quality} M${mode}`, (sha(back) === digest) + bytes);
    }
  }
  const dflt = zlib.brotliCompressSync(input);
  emit(`brotli ${name} default`, sha(zlib.brotliDecompressSync(dflt)) === digest);
  if (ALL) emit(`brotli ${name} default digest`, sha(dflt));
}

for (const name of ["text5k", "text70k", "random1k"]) {
  const input = INPUTS[name]();
  for (const lgwin of [10, 12, 16, 22, 24]) {
    for (const quality of [1, 5, 9]) {
      for (const lgblock of [0, 16, 20]) {
        const packed = zlib.brotliCompressSync(input, params({
          [c.BROTLI_PARAM_LGWIN]: lgwin, [c.BROTLI_PARAM_QUALITY]: quality, [c.BROTLI_PARAM_LGBLOCK]: lgblock,
        }));
        const bytes = pinned(quality, input.length, lgwin) ? " " + sha(packed) : "";
        emit(`window ${name} W${lgwin} Q${quality} B${lgblock}`, (sha(zlib.brotliDecompressSync(packed)) === sha(input)) + bytes);
      }
    }
  }
  for (const hint of [0, 1, 100, 5000, 1 << 20]) {
    const packed = zlib.brotliCompressSync(input, params({ [c.BROTLI_PARAM_SIZE_HINT]: hint, [c.BROTLI_PARAM_QUALITY]: 5 }));
    emit(`size hint ${name} ${hint}`, (sha(zlib.brotliDecompressSync(packed)) === sha(input)) +
      (pinned(5, input.length, 22, hint) ? " " + sha(packed) : ""));
  }
}

{
  const input = INPUTS.text5k();
  const large = zlib.brotliCompressSync(input, params({
    [c.BROTLI_PARAM_LARGE_WINDOW]: true, [c.BROTLI_PARAM_LGWIN]: 28, [c.BROTLI_PARAM_QUALITY]: 5,
  }));
  emit("large window sha", sha(large));
  attempt("large window without decoder param", () => zlib.brotliDecompressSync(large));
  attempt("large window with decoder param", () =>
    sha(zlib.brotliDecompressSync(large, params({ [c.BROTLI_DECODER_PARAM_LARGE_WINDOW]: 1 }))) === sha(input));
  attempt("disable literal context modeling", () =>
    sha(zlib.brotliCompressSync(input, params({ [c.BROTLI_PARAM_DISABLE_LITERAL_CONTEXT_MODELING]: true, [c.BROTLI_PARAM_QUALITY]: 5 }))));
}

const hello = Buffer.from("hello world hello world");
for (let key = 0; key <= 9; key++) {
  for (const value of [0, 1, 2, 3, 11, 24, 100, 4294967295]) {
    attempt(`encoder param ${key}=${value}`, () => {
      const packed = zlib.brotliCompressSync(hello, params({ [key]: value }));
      return key === 7 ? sha(zlib.brotliDecompressSync(packed)) : sha(packed);
    });
  }
}
for (let key = 0; key <= 3; key++) {
  for (const value of [0, 1, 7]) {
    attempt(`decoder param ${key}=${value}`, () =>
      zlib.brotliDecompressSync(zlib.brotliCompressSync(hello), params({ [key]: value })).toString());
  }
}
attempt("param true", () => sha(zlib.brotliCompressSync(hello, params({ [c.BROTLI_PARAM_LARGE_WINDOW]: true }))));
attempt("param negative", () => sha(zlib.brotliCompressSync(hello, params({ [c.BROTLI_PARAM_QUALITY]: -2 }))));
attempt("param fractional key", () => zlib.brotliCompressSync(hello, params({ "1.5": 3 })));
attempt("param string value", () => zlib.brotliCompressSync(hello, params({ 1: "3" })));
attempt("param duplicate key", () => zlib.brotliCompressSync(hello, params({ 1: 3, "01": 4 })));
attempt("param huge key", () => zlib.brotliCompressSync(hello, params({ 1000: 1 })));
attempt("param named", () => zlib.brotliCompressSync(hello, params({ foo: 1 })));
attempt("stream rejects param", () => zlib.createBrotliCompress(params({ [c.BROTLI_PARAM_DISABLE_LITERAL_CONTEXT_MODELING]: 5 })));
attempt("decoder stream rejects param", () => zlib.createBrotliDecompress(params({ 3: 1 })));

const dictionary = Buffer.from("hello world, the quick brown fox jumps");
const sentence = Buffer.from("the quick brown fox jumps over hello world");
for (const quality of [1, 5, 6, 9, 11]) {
  attempt(`dictionary Q${quality}`, () => {
    const packed = zlib.brotliCompressSync(sentence, { dictionary, ...params({ [c.BROTLI_PARAM_QUALITY]: quality }) });
    const back = zlib.brotliDecompressSync(packed, { dictionary }).toString();
    return [back, packed.length < zlib.brotliCompressSync(sentence, params({ [c.BROTLI_PARAM_QUALITY]: quality })).length,
      pinned(quality, 0) && quality > 1 ? sha(packed) : ""].join("|");
  });
}
attempt("dictionary typed array", () => zlib.brotliDecompressSync(
  zlib.brotliCompressSync(sentence, { dictionary: new Uint8Array(dictionary) }), { dictionary: dictionary.buffer.slice(dictionary.byteOffset, dictionary.byteOffset + dictionary.length) }).toString());
attempt("dictionary invalid type", () => zlib.brotliCompressSync(sentence, { dictionary: "text" }));
attempt("dictionary empty", () => sha(zlib.brotliCompressSync(sentence, { dictionary: Buffer.alloc(0) })));
attempt("dictionary mismatch length", () => zlib.brotliDecompressSync(
  zlib.brotliCompressSync(sentence, { dictionary }), { dictionary: Buffer.from("x") }).length);

const packedHello = zlib.brotliCompressSync(hello, params({ [c.BROTLI_PARAM_QUALITY]: 6 }));
const packedLong = zlib.brotliCompressSync(INPUTS.text5k(), params({ [c.BROTLI_PARAM_QUALITY]: 6 }));
attempt("empty input", () => hex(zlib.brotliCompressSync(Buffer.alloc(0))));
attempt("empty string", () => hex(zlib.brotliCompressSync("")));
attempt("decompress empty", () => zlib.brotliDecompressSync(Buffer.alloc(0)));
attempt("decompress garbage", () => zlib.brotliDecompressSync(Buffer.from("hello world, not brotli")));
attempt("decompress zeros", () => zlib.brotliDecompressSync(Buffer.alloc(64)));
attempt("decompress 0xff", () => zlib.brotliDecompressSync(Buffer.alloc(64, 255)));
attempt("decompress truncated", () => zlib.brotliDecompressSync(packedLong.subarray(0, packedLong.length - 5)));
attempt("decompress truncated 1", () => zlib.brotliDecompressSync(packedLong.subarray(0, 1)));
attempt("decompress truncated sync flush", () =>
  zlib.brotliDecompressSync(packedLong.subarray(0, 40), { finishFlush: c.BROTLI_OPERATION_FLUSH }).length);
attempt("decompress trailing", () => zlib.brotliDecompressSync(Buffer.concat([packedHello, Buffer.from("zz")])).toString());
attempt("decompress two streams", () => zlib.brotliDecompressSync(Buffer.concat([packedHello, packedHello])).toString());
for (const at of [0, 1, 2, 3, 5, 9]) {
  attempt(`decompress corrupt byte ${at}`, () => {
    const bad = Buffer.from(packedLong);
    bad[at] ^= 0x5a;
    return sha(zlib.brotliDecompressSync(bad));
  });
}
{
  let seed = 12345;
  const rnd = () => (seed = (Math.imul(seed, 1103515245) + 12345) >>> 0) >>> 16;
  const verdict = (buf) => {
    try {
      return "ok:" + sha(zlib.brotliDecompressSync(buf));
    } catch (e) {
      return e.code + ":" + e.errno;
    }
  };
  const tally = {};
  for (let i = 0; i < 400; i++) {
    const noise = Buffer.alloc(1 + (rnd() % 40));
    for (let j = 0; j < noise.length; j++) noise[j] = rnd() & 255;
    const key = "noise " + verdict(noise);
    tally[key] = (tally[key] || 0) + 1;
    const bad = Buffer.from(packedLong);
    bad[rnd() % bad.length] ^= 1 << (rnd() % 8);
    const mutated = "mutated " + verdict(bad);
    tally[mutated] = (tally[mutated] || 0) + 1;
  }
  for (const key of Object.keys(tally).sort()) emit("fuzz " + key, tally[key]);
}
attempt("max output ok", () => zlib.brotliDecompressSync(packedHello, { maxOutputLength: 23 }).toString());
attempt("max output exceeded", () => zlib.brotliDecompressSync(packedHello, { maxOutputLength: 22 }));
attempt("max output compress", () => zlib.brotliCompressSync(INPUTS.random1k(), { maxOutputLength: 10 }));
attempt("bad chunk size", () => zlib.brotliCompressSync(hello, { chunkSize: 5 }));
attempt("bad flush", () => zlib.brotliCompressSync(hello, { flush: 99 }));
attempt("bad finishFlush", () => zlib.brotliCompressSync(hello, { finishFlush: 99 }));
attempt("zlib flush value", () => sha(zlib.brotliCompressSync(hello, { flush: c.Z_FINISH })));
attempt("info", () => {
  const r = zlib.brotliCompressSync(hello, { info: true });
  return [Buffer.isBuffer(r.buffer), r.engine.constructor.name, r.engine.bytesWritten];
});
attempt("string input", () => zlib.brotliDecompressSync(zlib.brotliCompressSync("héllo ☃")).toString());
attempt("typed array input", () => sha(zlib.brotliDecompressSync(zlib.brotliCompressSync(new Uint16Array([1, 2, 3, 4])))));
attempt("array buffer input", () => sha(zlib.brotliDecompressSync(zlib.brotliCompressSync(new Uint8Array([9, 8, 7]).buffer))));
attempt("invalid input type", () => zlib.brotliCompressSync(12));
attempt("classes", () => [zlib.BrotliCompress.name, zlib.BrotliDecompress.name, typeof zlib.createBrotliCompress,
  new zlib.BrotliCompress() instanceof zlib.BrotliCompress, Object.getPrototypeOf(zlib.BrotliCompress.prototype).constructor.name]);
attempt("brotli constants", () => Object.keys(c).filter((k) => k.startsWith("BROTLI_")).map((k) => k + "=" + c[k]).join(","));

(async () => {
  for (const name of ["hello", "text5k", "random1k", "empty"]) {
    const input = INPUTS[name]();
    const packed = await new Promise((res, rej) =>
      zlib.brotliCompress(input, params({ [c.BROTLI_PARAM_QUALITY]: 5 }), (e, b) => (e ? rej(e) : res(b))));
    const back = await new Promise((res, rej) => zlib.brotliDecompress(packed, (e, b) => (e ? rej(e) : res(b))));
    emit(`callback ${name}`, [sha(back) === sha(input), sha(packed)]);
  }
  await new Promise((res) => zlib.brotliDecompress(Buffer.from("not brotli"), (e) => {
    emit("callback error", [e.code, e.errno, e.message]);
    res();
  }));
  await new Promise((res) => zlib.brotliDecompress(packedHello, { maxOutputLength: 5 }, (e) => {
    emit("callback max output", [e.code, e.message]);
    res();
  }));
  flush();
})();
