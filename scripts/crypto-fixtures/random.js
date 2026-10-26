// Random, timingSafeEqual, module-shape and Web Crypto corpus for the crypto
// oracle (issue #1226).
"use strict";

const crypto = require("crypto");
const { emit, attempt, hex, pattern, flush, out } = require("./common.js");

const isXof = (a) => a.startsWith("shake");

// Module shape.
for (const name of ["createHash", "createHmac", "getHashes", "hash", "randomBytes", "randomFill", "randomFillSync", "randomInt", "randomUUID", "timingSafeEqual", "getRandomValues"]) {
  emit(`typeof ${name}`, typeof crypto[name]);
}
const hashes = crypto.getHashes();
emit("getHashes", [Array.isArray(hashes), hashes.includes("sha256"), hashes.includes("md5"), hashes.includes("sha3-512"), hashes.every((h) => typeof h === "string"), hashes.every((h) => { try { crypto.createHash(h, isXof(h) ? { outputLength: 1 } : undefined); return true; } catch { return false; } })]);
emit("require aliases", require("node:crypto") === crypto);
emit("webcrypto", [crypto.webcrypto === globalThis.crypto, typeof crypto.webcrypto.subtle, crypto.subtle === crypto.webcrypto.subtle, typeof crypto.webcrypto.getRandomValues, typeof crypto.webcrypto.randomUUID, typeof globalThis.crypto.subtle.digest]);

// Random values, by shape.
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
emit("randomUUID", [UUID.test(crypto.randomUUID()), UUID.test(crypto.randomUUID({ disableEntropyCache: true })), crypto.randomUUID() !== crypto.randomUUID(), globalThis.crypto.randomUUID().length]);
attempt("randomUUID options", () => crypto.randomUUID(5));
attempt("randomUUID entropy", () => crypto.randomUUID({ disableEntropyCache: 1 }));
for (const n of [0, 1, 16, 1000, 65537, -1, 1.5, "8", NaN, 2 ** 31, Infinity, null, undefined]) {
  attempt(`randomBytes ${String(n)}`, () => { const b = crypto.randomBytes(n); return [Buffer.isBuffer(b), b.length, Object.getPrototypeOf(b) === Buffer.prototype]; });
}
emit("randomBytes distinct", crypto.randomBytes(32).equals(crypto.randomBytes(32)));
attempt("randomBytes bad callback", () => crypto.randomBytes(4, 5));
for (const [lo, hi] of [[undefined, 1], [undefined, 10], [5, 6], [-3, 3], [0, 2 ** 48], [-(2 ** 47), 2 ** 47 - 1], [3, 3], [5, 1], [0, 2 ** 48 + 1], [1.5, 3], [0, "3"], [0, NaN], [Number.MAX_SAFE_INTEGER - 1, Number.MAX_SAFE_INTEGER], [0, Infinity]]) {
  attempt(`randomInt ${lo} ${hi}`, () => {
    const args = lo === undefined ? [hi] : [lo, hi];
    const seen = new Set();
    const bounded = lo === undefined || hi - lo <= 20;
    for (let i = 0; i < (bounded ? 200 : 20); i++) {
      const v = crypto.randomInt(...args);
      const min = lo === undefined ? 0 : lo;
      if (!Number.isSafeInteger(v) || v < min || v >= hi) return "out of range " + v;
      seen.add(v);
    }
    return bounded ? [...seen].sort((a, b) => a - b).join(",") : "ok";
  });
}
attempt("randomInt bad callback", () => crypto.randomInt(5, 5, 5));
const buf = Buffer.alloc(16);
emit("randomFillSync", [crypto.randomFillSync(buf) === buf, buf.some((b) => b !== 0)]);
const u32 = new Uint32Array(8);
emit("randomFillSync u32", [crypto.randomFillSync(u32, 2, 3) === u32, u32[0], u32[1], u32[5], u32[6], u32[7], u32.slice(2, 5).some((v) => v !== 0)]);
const ab = new ArrayBuffer(8);
emit("randomFillSync ab", [crypto.randomFillSync(ab, 4) === ab, new Uint8Array(ab).slice(0, 4).join(""), new Uint8Array(ab).slice(4).some((v) => v !== 0)]);
for (const [label, args] of [["offset", [Buffer.alloc(4), 5]], ["size", [Buffer.alloc(4), 0, 5]], ["neg", [Buffer.alloc(4), -1]], ["type", ["x"]], ["nan", [Buffer.alloc(4), NaN]], ["sizeoff", [Buffer.alloc(4), 2, 3]], ["u32", [new Uint32Array(2), 3]]]) {
  attempt(`randomFillSync ${label}`, () => crypto.randomFillSync(...args));
}
for (const [label, data] of [["u8", new Uint8Array(65536)], ["i16", new Int16Array(10)], ["bigu64", new BigUint64Array(2)], ["f32", new Float32Array(1)], ["f64", new Float64Array(1)], ["big", new Uint8Array(65537)], ["dv", new DataView(new ArrayBuffer(4))], ["ab", new ArrayBuffer(4)], ["num", 5]]) {
  attempt(`getRandomValues ${label}`, () => { const r = crypto.getRandomValues(data); return r === data; });
  if (["u8", "i16", "bigu64"].includes(label)) attempt(`global getRandomValues ${label}`, () => globalThis.crypto.getRandomValues(data) === data);
}

async function main() {
  // Web Crypto digest.
  const data = Buffer.from("abc");
  for (const alg of ["SHA-1", "SHA-256", "SHA-384", "SHA-512", "sha-256", { name: "SHA-256" }, { name: "sha-512" }, "SHA3-256", "SHA3-384", "SHA3-512"]) {
    try {
      const r = await crypto.subtle.digest(alg, data);
      emit(`subtle ${JSON.stringify(alg)}`, [r instanceof ArrayBuffer, r.byteLength, hex(r)]);
    } catch (e) {
      emit(`subtle ${JSON.stringify(alg)}`, "rejects " + e.name + "|" + e.code + "|" + e.message);
    }
  }
  for (const [label, alg, d] of [
    ["md5", "MD5", data], ["shake", "SHAKE128", data], ["object", {}, data], ["number", 5, data], ["null", null, data],
    ["string data", "SHA-256", "abc"], ["number data", "SHA-256", 5], ["sab", "SHA-256", new SharedArrayBuffer(2)],
    ["arraybuffer", "SHA-256", new Uint8Array([97, 98, 99]).buffer], ["dataview", "SHA-256", new DataView(new Uint8Array([97, 98, 99]).buffer)],
    ["u16", "SHA-1", new Uint16Array([0x6261, 0x63])], ["empty", "SHA-256", new Uint8Array(0)],
    ["subarray", "SHA-256", Buffer.from("xxabcxx").subarray(2, 5)],
  ]) {
    try {
      const r = await crypto.subtle.digest(alg, d);
      emit(`subtle ${label}`, [r instanceof ArrayBuffer, hex(r)]);
    } catch (e) {
      emit(`subtle ${label}`, "rejects " + e.name + "|" + e.code + "|" + e.message);
    }
  }
  for (const args of [[], ["SHA-1"]]) {
    try { await crypto.subtle.digest(...args); } catch (e) { emit(`subtle args ${args.length}`, e.name + "|" + e.code + "|" + e.message); }
  }
  try { await crypto.subtle.digest.call({}, "SHA-1", data); } catch (e) { emit("subtle this", e.name + "|" + e.code + "|" + e.message); }
  emit("subtle returns promise", crypto.subtle.digest("SHA-1", data) instanceof Promise);
  emit("subtle rejects not throws", await crypto.subtle.digest().then(() => "resolved", (e) => "rejected " + e.code));
  const mutated = new Uint8Array([1, 2, 3]);
  const pending = crypto.subtle.digest("SHA-1", mutated);
  mutated[0] = 9;
  emit("subtle snapshot", hex(await pending) === crypto.createHash("sha1").update(new Uint8Array([1, 2, 3])).digest("hex"));
  const big = pattern(5 << 20, 11);
  emit("subtle large", hex(await crypto.subtle.digest("SHA-256", big)) === crypto.createHash("sha256").update(big).digest("hex"));

  // Async random callbacks.
  const order = [];
  await new Promise((resolve) => {
    crypto.randomBytes(8, function (err, b) { order.push(["randomBytes", err, Buffer.isBuffer(b), b.length]); resolve(); });
    order.push("sync-after-randomBytes");
  });
  await new Promise((resolve) => {
    const target = new Uint8Array(6);
    crypto.randomFill(target, 1, 4, function (err, t) { order.push(["randomFill", err, t === target, target[0], target.slice(1, 5).some((v) => v !== 0)]); resolve(); });
    order.push(["randomFill deferred", target.every((v) => v === 0)]);
  });
  await new Promise((resolve) => {
    crypto.randomFill(new Uint16Array(4), function (err, t) { order.push(["randomFill2", err, t.length]); resolve(); });
  });
  await new Promise((resolve) => {
    crypto.randomInt(5, 10, function (err, n) { order.push(["randomInt", err, n >= 5 && n < 10]); resolve(); });
    order.push("sync-after-randomInt");
  });
  await new Promise((resolve) => {
    crypto.randomInt(3, function (err, n) { order.push(["randomInt1", err, n >= 0 && n < 3]); resolve(); });
  });
  emit("async random", order);
  attempt("randomFill bad cb", () => crypto.randomFill(Buffer.alloc(2), 0, 1, 5));
  attempt("randomFill bad buf", () => crypto.randomFill("x", () => {}));

  flush();
}

main().catch((e) => {
  out.push("FATAL " + (e && e.stack));
  flush();
  process.exitCode = 1;
});
