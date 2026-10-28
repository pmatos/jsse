// Cross-decode (issue #1239): `produce <file>` writes every stream this
// engine compresses; `consume <file>` decodes a file produced by either
// engine, sync and streamed, and prints one verdict per stream. The oracle has
// each engine consume the other's file, so compressed bytes need not match.
"use strict";

const zlib = require("zlib");
const fs = require("fs");
const { sha, INPUTS } = require("./common.js");

const PAIRS = [["deflate", "inflate"], ["gzip", "gunzip"], ["deflateRaw", "inflateRaw"], ["gzip", "unzip"]];
const NAMES = ["empty", "hello", "text5k", "text70k", "random70k", "zeros100k", "ramp40k"];

function* configs() {
  for (const name of NAMES) {
    for (const [comp, decomp] of PAIRS) {
      for (const level of [-1, 0, 1, 4, 6, 9]) {
        for (const strategy of [0, 1, 2, 3, 4]) {
          yield { name, comp, decomp, opts: { level, strategy } };
        }
      }
      for (const windowBits of [9, 12]) {
        for (const memLevel of [1, 9]) {
          yield { name, comp, decomp, opts: { windowBits, memLevel, level: 6 } };
        }
      }
    }
  }
}
const BROTLI = ["brotliCompress", "brotliDecompress"];
function* brotliConfigs() {
  for (const name of NAMES) {
    for (let quality = 0; quality <= 11; quality++) {
      if (quality > 9 && name !== "empty" && name !== "hello" && name !== "text5k") continue;
      yield { name, comp: BROTLI[0], decomp: BROTLI[1], opts: { params: { 1: quality } } };
    }
    for (const lgwin of [10, 16, 24]) {
      yield { name, comp: BROTLI[0], decomp: BROTLI[1], opts: { params: { 1: 5, 2: lgwin, 3: 16 } } };
    }
    for (const mode of [1, 2]) {
      yield { name, comp: BROTLI[0], decomp: BROTLI[1], opts: { params: { 0: mode, 1: 6 } } };
    }
  }
}
const allConfigs = function* () {
  yield* configs();
  yield* brotliConfigs();
};
const keyOf = (c) => `${c.name} ${c.comp}/${c.decomp} ${JSON.stringify(c.opts)}`;

if (process.argv[2] === "produce") {
  const rows = {};
  const inputs = {};
  for (const c of allConfigs()) {
    if (c.name === "text70k" && c.opts.strategy > 1) continue;
    inputs[c.name] ??= INPUTS[c.name]();
    rows[keyOf(c)] = zlib[c.comp + "Sync"](inputs[c.name], c.opts).toString("hex");
  }
  fs.writeFileSync(process.argv[3], JSON.stringify(rows));
} else {
  const rows = JSON.parse(fs.readFileSync(process.argv[3], "utf8"));
  const lines = [];
  const digests = {};
  (async () => {
    for (const c of allConfigs()) {
      const key = keyOf(c);
      if (!(key in rows)) continue;
      const packed = Buffer.from(rows[key], "hex");
      const want = (digests[c.name] ??= sha(INPUTS[c.name]()));
      let ok = sha(zlib[c.decomp + "Sync"](packed)) === want;
      if (packed.length > 1000 && (c.opts.level % 3 === 0 || c.comp === BROTLI[0])) {
        const parts = [];
        const s = zlib["create" + c.decomp[0].toUpperCase() + c.decomp.slice(1)]();
        s.on("data", (d) => parts.push(d));
        for (let i = 0; i < packed.length; i += 4097) s.write(packed.subarray(i, i + 4097));
        s.end();
        await new Promise((res, rej) => { s.on("end", res); s.on("error", rej); });
        ok = ok && sha(Buffer.concat(parts)) === want;
      }
      lines.push(`${key} ${ok ? "ok" : "FAIL"}`);
    }
    process.stdout.write(lines.join("\n") + "\n");
  })();
}
