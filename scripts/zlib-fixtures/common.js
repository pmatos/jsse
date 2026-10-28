// Shared helpers of the zlib differential corpus (issue #1239): each fixture
// prints one line per check, and its stdout must be byte-identical on
// `jsse --node` and Node. Run via scripts/run-node-zlib-oracle.sh.
"use strict";

const crypto = require("crypto");

const out = [];
function emit(label, value) {
  out.push(label + " " + (typeof value === "string" ? value : JSON.stringify(value)));
}
function attempt(label, fn) {
  try {
    const v = fn();
    emit(label, v instanceof Uint8Array ? "bytes:" + Buffer.from(v).toString("hex") : v);
  } catch (e) {
    emit(label, "throws " + e.name + "|" + e.code + "|" + e.errno + "|" + e.message);
  }
}
const hex = (b) => Buffer.from(b).toString("hex");
const sha = (b) => crypto.createHash("sha1").update(b).digest("hex");
function flush() {
  process.stdout.write(out.join("\n") + "\n");
}

function random(length, seed) {
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
  return Buffer.from(buf.buffer);
}

const WORDS = ["function", "return", "const", "the", "quick", "brown", "fox", "jumps",
  "over", "lazy", "dog", "\n", "  ", "{", "}", "zlib", "stream", "buffer"];
function text(length, seed) {
  let s = (seed * 2654435761) >>> 0;
  const buf = Buffer.alloc(length);
  let o = 0;
  while (o < length) {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    let w = WORDS[(s >>> 8) % WORDS.length];
    if (((s >>> 20) % 7) === 0) w += String(s % 1000);
    o += buf.write(w, o);
  }
  return buf;
}

function zeros(length) {
  return Buffer.alloc(length);
}

function ramp(length) {
  const buf = Buffer.alloc(length);
  for (let i = 0; i < length; i++) buf[i] = i & 255;
  return buf;
}

// The inputs every corpus draws from; `kinds` names them for the cross-decode.
const INPUTS = {
  empty: () => Buffer.alloc(0),
  one: () => Buffer.from("a"),
  hello: () => Buffer.from("hello world hello world hello world"),
  text2k: () => text(2000, 1),
  text5k: () => text(5000, 1),
  text70k: () => text(70000, 2),
  text300k: () => text(300000, 3),
  random1k: () => random(1000, 4),
  random70k: () => random(70000, 5),
  zeros100k: () => zeros(100000),
  ramp40k: () => ramp(40000),
};

function realOpts(opts) {
  const { dictionaryHex, ...rest } = opts;
  return dictionaryHex === undefined ? rest : { ...rest, dictionary: Buffer.from(dictionaryHex, "hex") };
}

module.exports = { realOpts, emit, attempt, hex, sha, flush, out, random, text, zeros, ramp, INPUTS };
