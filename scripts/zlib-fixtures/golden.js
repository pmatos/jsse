// Streams produced by Node (Chromium's zlib) that jsse must decode to the same
// bytes (issue #1239). Regenerate with `node golden.js --generate`.
"use strict";

const zlib = require("zlib");
const { INPUTS, realOpts } = require("./common.js");

const CASES = [
  ["deflate", "text2k", {}], ["deflate", "text2k", { level: 1 }],
  ["deflate", "text2k", { level: 9 }], ["gzip", "text2k", { level: 6 }],
  ["gzip", "text2k", { level: 3, memLevel: 9 }], ["deflateRaw", "text2k", { level: 4 }],
  ["deflate", "random1k", {}], ["deflate", "zeros100k", { level: 1 }],
  ["deflate", "ramp40k", { level: 7 }], ["deflate", "text2k", { windowBits: 9 }],
  ["deflate", "text2k", { strategy: 1 }], ["deflate", "hello", { dictionaryHex: Buffer.from("hello world").toString("hex") }],
];

if (process.argv[2] === "--generate") {
  const rows = CASES.map(([fn, input, opts]) =>
    JSON.stringify([fn, input, opts, zlib[fn + "Sync"](INPUTS[input](), realOpts(opts)).toString("hex")]));
  console.log("module.exports = [\n  " + rows.join(",\n  ") + ",\n];");
}
