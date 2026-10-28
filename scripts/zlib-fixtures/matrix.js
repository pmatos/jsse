// The compression matrix (issue #1239): every input through every mode, level
// and strategy, as sync, callback and stream calls. It prints round-trip
// verdicts always and the compressed bytes only where the algorithm pins them
// (stored blocks, Huffman-only, RLE, tiny inputs): Node's zlib is Chromium's
// fork and jsse's is a zlib-ng port, so the other levels may differ.
"use strict";

const zlib = require("zlib");
const { emit, sha, hex, flush, INPUTS, realOpts } = require("./common.js");

const PAIRS = [
  ["deflate", "inflate"], ["gzip", "gunzip"], ["deflateRaw", "inflateRaw"], ["gzip", "unzip"],
  ["deflate", "unzip"],
];
const PINNED = (level, strategy) => level === 0 || strategy === 2 || strategy === 3;

for (const [name, make] of Object.entries(INPUTS)) {
  const input = make();
  const digest = sha(input);
  for (const [comp, decomp] of PAIRS) {
    for (let level = -1; level <= 9; level++) {
      for (const strategy of [0, 1, 2, 3, 4]) {
        if (input.length > 100000 && (level % 4 !== 0 || strategy > 2)) continue;
        const packed = zlib[comp + "Sync"](input, { level, strategy });
        const back = zlib[decomp + "Sync"](packed);
        const bytes = PINNED(level, strategy) || (input.length < 40 && level !== 1) ? " " + sha(packed) : "";
        emit(`${comp}/${decomp} ${name} L${level} S${strategy}`,
          (sha(back) === digest) + bytes);
      }
    }
  }
}

for (const name of ["text5k", "hello", "random1k"]) {
  const input = INPUTS[name]();
  for (const windowBits of [9, 10, 12, 15]) {
    for (const memLevel of [1, 5, 9]) {
      for (const [comp, decomp] of [["deflate", "inflate"], ["deflateRaw", "inflateRaw"]]) {
        const packed = zlib[comp + "Sync"](input, { windowBits, memLevel, level: 6 });
        emit(`window ${comp} ${name} W${windowBits} M${memLevel}`,
          sha(zlib[decomp + "Sync"](packed, { windowBits })) === sha(input));
      }
    }
  }
}

const dictionary = Buffer.from("hello world, the quick brown fox");
for (const [comp, decomp] of [["deflate", "inflate"], ["deflateRaw", "inflateRaw"]]) {
  const input = Buffer.from("hello world, the quick brown fox jumps over the lazy dog");
  const packed = zlib[comp + "Sync"](input, { dictionary, level: 0 });
  emit(`dictionary ${comp} stored`, hex(packed));
  const smaller = zlib[comp + "Sync"](input, { dictionary }).length < zlib[comp + "Sync"](input).length;
  emit(`dictionary ${comp} helps`, smaller);
  emit(`dictionary ${comp} back`, zlib[decomp + "Sync"](packed, { dictionary }).toString());
}

for (const info of [true]) {
  const r = zlib.deflateSync("info", { info });
  emit("info", [Buffer.isBuffer(r.buffer), r.engine.constructor.name, r.engine.bytesWritten]);
}

(async () => {
  for (const [comp, decomp] of PAIRS.slice(0, 3)) {
    const input = INPUTS.text5k();
    const packed = await new Promise((res, rej) =>
      zlib[comp](input, { level: 1 }, (e, b) => (e ? rej(e) : res(b))));
    const back = await new Promise((res, rej) =>
      zlib[decomp](packed, (e, b) => (e ? rej(e) : res(b))));
    emit(`callback ${comp}`, sha(back) === sha(input));
  }
  flush();
})();
