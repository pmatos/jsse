// Shared helpers of the crypto differential corpus (issue #1226): each fixture
// prints one line per check, and its stdout must be byte-identical on
// `jsse --node` and Node. Run via scripts/run-node-crypto-oracle.sh.
"use strict";

const out = [];
function emit(label, value) {
  out.push(label + " " + (typeof value === "string" ? value : JSON.stringify(value)));
}
function attempt(label, fn) {
  try {
    const v = fn();
    emit(label, v instanceof Uint8Array ? "bytes:" + Buffer.from(v).toString("hex") : v);
  } catch (e) {
    emit(label, "throws " + e.name + "|" + e.code + "|" + e.message);
  }
}
const hex = (b) => Buffer.from(b).toString("hex");
function flush() {
  process.stdout.write(out.join("\n") + "\n");
}

function pattern(length, seed) {
  const block = new Uint8Array(65536);
  let s = (seed * 2654435761) >>> 0;
  for (let i = 0; i < block.length; i++) {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    block[i] = s >>> 24;
  }
  const buf = new Uint8Array(length);
  for (let off = 0; off < length; off += block.length) {
    buf.set(block.subarray(0, Math.min(block.length, length - off)), off);
  }
  return buf;
}

module.exports = { emit, attempt, hex, pattern, flush, out };
