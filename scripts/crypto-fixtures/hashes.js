// Hash, HMAC and hash() corpus for the crypto oracle (issue #1226).
"use strict";

const crypto = require("crypto");
const { emit, attempt, hex, pattern, flush } = require("./common.js");

const ALGOS = [
  "md5", "sha1", "sha224", "sha256", "sha384", "sha512", "sha512-224",
  "sha512-256", "sha3-224", "sha3-256", "sha3-384", "sha3-512", "shake128",
  "shake256",
];
const isXof = (a) => a.startsWith("shake");
const opts = (a) => (isXof(a) ? { outputLength: a === "shake128" ? 16 : 32 } : undefined);
const digestOf = (a, data, enc = "hex") =>
  crypto.createHash(a, opts(a)).update(data).digest(enc);

// Known inputs.
for (const a of ALGOS) {
  emit(`empty ${a}`, digestOf(a, ""));
  emit(`abc ${a}`, digestOf(a, "abc"));
  emit(`fox ${a}`, digestOf(a, "The quick brown fox jumps over the lazy dog"));
}

// Block-boundary lengths, every two-chunk split must equal the one-shot digest.
const LENGTHS = [0, 1, 2, 55, 56, 57, 63, 64, 65, 71, 72, 73, 103, 104, 105, 111, 112,
  113, 119, 120, 121, 127, 128, 129, 135, 136, 137, 143, 144, 145, 167, 168, 169, 255, 256, 257];
for (const a of ALGOS) {
  for (const len of LENGTHS) {
    const data = pattern(len, len + 1);
    const whole = digestOf(a, data);
    let splitsOk = true;
    for (let i = 0; i <= len; i++) {
      const h = crypto.createHash(a, opts(a));
      h.update(data.subarray(0, i));
      h.update(data.subarray(i));
      if (h.digest("hex") !== whole) splitsOk = false;
    }
    const bytewise = crypto.createHash(a, opts(a));
    for (let i = 0; i < len; i++) bytewise.update(data.subarray(i, i + 1));
    emit(`boundary ${a} ${len}`, whole + " " + splitsOk + " " + (bytewise.digest("hex") === whole));
  }
}

// Large inputs, one shot and streamed through odd chunk sizes and view offsets.
for (const size of [1 << 20, 16 << 20, (16 << 20) + 5]) {
  const data = pattern(size, 7);
  for (const a of ALGOS) {
    const whole = digestOf(a, data);
    const h = crypto.createHash(a, opts(a));
    const chunk = 1000003;
    for (let off = 0; off < size; off += chunk) h.update(data.subarray(off, Math.min(size, off + chunk)));
    const padded = new Uint8Array(size + 9);
    padded.set(data, 9);
    const viaOffset = digestOf(a, new DataView(padded.buffer, 9, size));
    emit(`large ${a} ${size}`, whole + " " + (h.digest("hex") === whole) + " " + (viaOffset === whole));
  }
}

// copy() mid-stream, independence of the copy and the original.
for (const a of ALGOS) {
  const h = crypto.createHash(a, opts(a));
  h.update("prefix-");
  const c1 = h.copy();
  const c2 = c1.copy();
  h.update("one");
  c1.update("two");
  c2.update("two");
  emit(`copy ${a}`, [h.digest("hex"), c1.digest("hex"), c2.digest("hex")].join(" "));
}
attempt("copy after digest", () => { const h = crypto.createHash("md5"); h.digest(); return h.copy(); });
attempt("copy xof length", () => crypto.createHash("shake128", { outputLength: 4 }).update("x").copy({ outputLength: 9 }).digest("hex"));
attempt("copy bad length", () => crypto.createHash("sha1").copy({ outputLength: 3 }));

// Input and output encodings.
const TEXTS = ["", "abc", "héllo wörld €", "😀 emoji", "\ud800x", "x\udc00", "00ff10ab", "AQIDBA==", "AQIDBA", "Āā"];
const IN_ENC = [undefined, "utf8", "UTF-8", "latin1", "binary", "ascii", "hex", "base64", "base64url", "ucs2", "utf16le", "UCS-2", "foo", "buffer", ""];
for (const t of TEXTS) {
  for (const enc of IN_ENC) {
    attempt(`update ${JSON.stringify(t)} ${enc}`, () => crypto.createHash("sha256").update(t, enc).digest("hex"));
  }
}
const OUT_ENC = ["hex", "HEX", "base64", "base64url", "latin1", "binary", "utf8", "ascii", "ucs2", "utf16le", "buffer", "foo", undefined, "", null];
for (const enc of OUT_ENC) {
  attempt(`digest enc ${enc}`, () => {
    const r = crypto.createHash("sha1").update("abc").digest(enc);
    return typeof r === "string" ? r : "Buffer:" + Buffer.isBuffer(r) + ":" + hex(r);
  });
}
for (const [label, data] of [
  ["Buffer", Buffer.from("abc")],
  ["Uint8Array", new Uint8Array([97, 98, 99])],
  ["Uint16Array", new Uint16Array([0x6261, 0x63])],
  ["Float64Array", new Float64Array([1.5, 2.5])],
  ["DataView", new DataView(new Uint8Array([97, 98, 99]).buffer)],
  ["BigInt64Array", new BigInt64Array([1n, 2n])],
  ["subarray", Buffer.from("xxabcxx").subarray(2, 5)],
  ["ArrayBuffer", new ArrayBuffer(3)],
  ["number", 5], ["null", null], ["undefined", undefined], ["object", {}], ["array", [97]],
]) {
  attempt(`update type ${label}`, () => crypto.createHash("md5").update(data).digest("hex"));
}

// Constructor shape, algorithm names, errors.
attempt("Hash instanceof", () => [crypto.createHash("md5") instanceof crypto.Hash, crypto.createHmac("md5", "k") instanceof crypto.Hmac, typeof crypto.Hash]);
attempt("Hash without new", () => crypto.Hash("md5").update("a").digest("hex"));
attempt("Hmac without new", () => crypto.Hmac("md5", "k").update("a").digest("hex"));
attempt("update chain", () => { const h = crypto.createHash("md5"); return h.update("a") === h; });
for (const n of ["SHA256", "sha-256", "SHA2-256", "Sha1", "SHA-1", "RSA-SHA256", "sha256WithRSAEncryption", "SHA-512/256", "sha2-512/224", "RSA-SHA512/256", "sha512/256", "SHA512-224", "sha512-256WithRSAEncryption", "SHA3-256", "rsa-sha3-512", "id-rsassa-pkcs1-v1_5-with-sha3-384", "sha3-256withrsaencryption", "SHAKE128", "shake-256", "SSL3-MD5", "ssl3-sha1", "MD5", "RSA-MD5", "RSA-SHA1-2", "sha-3-256", "rsa-sha512-256", "", "nope", "sha", "sha-", "sha2", "sha4", "sha-2-256", "sha_256"]) {
  attempt(`name ${JSON.stringify(n)}`, () => crypto.createHash(n, /^shake/i.test(n) ? { outputLength: 8 } : undefined).update("x").digest("hex"));
}
for (const v of [5, null, undefined, {}, [], true, Symbol.iterator]) {
  attempt(`createHash(${String(typeof v)}:${v === null ? "null" : typeof v === "symbol" ? "sym" : JSON.stringify(v)})`, () => crypto.createHash(v));
}
for (const len of [0, 1, 5, 16, 20, 31, 32, 33, 1000, -1, 1.5, "8", NaN, 2 ** 32, null]) {
  for (const a of ["sha256", "shake128", "shake256", "sha3-256"]) {
    attempt(`outputLength ${a} ${String(len)}`, () => crypto.createHash(a, { outputLength: len }).update("abc").digest("hex"));
  }
}
attempt("options null", () => crypto.createHash("sha1", null).update("a").digest("hex"));
attempt("options string", () => crypto.createHash("sha1", "x").update("a").digest("hex"));
attempt("shake default 128", () => digestOf("shake128", "abc") + crypto.createHash("shake128").update("abc").digest("hex"));
attempt("shake default 256", () => crypto.createHash("shake256").update("abc").digest("hex"));
attempt("digest twice", () => { const h = crypto.createHash("md5"); h.digest(); return h.digest(); });
attempt("update after digest", () => { const h = crypto.createHash("md5"); h.digest(); return h.update("a"); });
attempt("hmac digest twice", () => { const h = crypto.createHmac("md5", "k"); h.update("a").digest(); const r = h.digest("hex"); const r2 = h.digest(); return [r, hex(r2), Buffer.isBuffer(r2)]; });
attempt("hmac update after digest", () => { const h = crypto.createHmac("md5", "k"); h.digest(); return h.update("a"); });
for (const enc of ["", null, 5, Symbol("e"), {}, "buffer", "utf8", "UTF-8"]) {
  attempt(`hmac encoding ${typeof enc === "symbol" ? "symbol" : JSON.stringify(enc)}`, () => crypto.createHmac("md5", "6b", { encoding: enc }).update("a").digest("hex"));
  attempt(`update encoding ${typeof enc === "symbol" ? "symbol" : JSON.stringify(enc)}`, () => crypto.createHash("md5").update("a", enc).digest("hex"));
}
attempt("update spoofed byteLength", () => {
  const u = new Uint8Array([97, 98, 99]);
  Object.defineProperty(u, "byteLength", { value: 0 });
  Object.defineProperty(u, "buffer", { value: new ArrayBuffer(0) });
  return [crypto.createHash("md5").update(u).digest("hex"), crypto.hash("md5", u), crypto.createHmac("md5", u).update("a").digest("hex"), crypto.timingSafeEqual(u, new Uint8Array(3))];
});
attempt("update detached", () => {
  const u = new Uint8Array([97, 98, 99]);
  structuredClone(u.buffer, { transfer: [u.buffer] });
  return [crypto.createHash("md5").update(u).digest("hex"), crypto.hash("md5", u)];
});
attempt("update hex odd", () => crypto.createHash("md5").update("abc", "hex"));
attempt("update hex invalid", () => crypto.createHash("md5").update("zz", "hex").digest("hex"));

// HMAC: key sizes around the block size, key types, encodings.
const HMAC_ALGOS = ALGOS.filter((a) => !isXof(a));
const BLOCK = { md5: 64, sha1: 64, sha224: 64, sha256: 64, sha384: 128, sha512: 128, "sha512-224": 128, "sha512-256": 128, "sha3-224": 144, "sha3-256": 136, "sha3-384": 104, "sha3-512": 72 };
const MSGS = ["", "abc", "x".repeat(1000)];
for (const a of HMAC_ALGOS) {
  const b = BLOCK[a];
  for (const klen of [0, 1, b - 1, b, b + 1, 2 * b + 7, 300]) {
    const key = pattern(klen, klen + 3);
    for (const msg of MSGS) {
      const viaBuffer = crypto.createHmac(a, Buffer.from(key)).update(msg).digest("hex");
      const viaArrayBuffer = crypto.createHmac(a, key.buffer.slice(0, klen)).update(msg).digest("hex");
      const viaDataView = crypto.createHmac(a, new DataView(key.buffer, 0, klen)).update(msg).digest("hex");
      const viaU16 = klen % 2 === 0 ? crypto.createHmac(a, new Uint16Array(key.buffer.slice(0, klen))).update(msg).digest("hex") : viaBuffer;
      const keyStr = Buffer.from(key).toString("latin1");
      const viaStr = crypto.createHmac(a, keyStr, { encoding: "latin1" }).update(msg).digest("hex");
      const viaHex = crypto.createHmac(a, Buffer.from(key).toString("hex"), { encoding: "hex" }).update(msg).digest("hex");
      const split = crypto.createHmac(a, key).update(msg.slice(0, 1)).update(msg.slice(1)).digest("hex");
      emit(`hmac ${a} key${klen} msg${msg.length}`, [viaBuffer, viaArrayBuffer === viaBuffer, viaDataView === viaBuffer, viaU16 === viaBuffer, viaStr === viaBuffer, viaHex === viaBuffer, split === viaBuffer].join(" "));
    }
  }
  emit(`hmac str ${a}`, crypto.createHmac(a, "Jefe").update("what do ya want for nothing?").digest("base64"));
}
for (const v of [undefined, null, 5, {}, [], true]) {
  attempt(`hmac key ${v === undefined ? "undefined" : JSON.stringify(v)}`, () => crypto.createHmac("sha256", v));
}
for (const v of [undefined, null, 5, "nope", "", "shake128", "shake256"]) {
  attempt(`hmac alg ${v === undefined ? "undefined" : JSON.stringify(v)}`, () => crypto.createHmac(v, "k"));
}
attempt("hmac encoding option", () => crypto.createHmac("md5", "6b", { encoding: "hex" }).update("a").digest("hex") === crypto.createHmac("md5", "k").update("a").digest("hex"));
attempt("hmac encoding bogus", () => crypto.createHmac("md5", "k", { encoding: "bogus" }).update("a").digest("hex"));
attempt("hmac digest enc", () => ["base64", "latin1", "buffer", undefined].map((e) => { const r = crypto.createHmac("sha1", "k").update("a").digest(e); return typeof r === "string" ? r : hex(r); }));

// One-shot hash().
for (const a of ["md5", "sha1", "sha256", "sha512", "sha3-256"]) {
  for (const enc of [undefined, "hex", "base64", "base64url", "latin1", "buffer", "BUFFER"]) {
    attempt(`hash() ${a} ${enc}`, () => { const r = crypto.hash(a, "abc", enc); return typeof r === "string" ? r : "buf:" + hex(r); });
  }
}
attempt("hash() options", () => crypto.hash("sha256", "abc", { outputEncoding: "base64" }));
attempt("hash() shake length", () => crypto.hash("shake128", "abc", { outputLength: 5 }));
attempt("hash() bad length", () => crypto.hash("sha256", "abc", { outputLength: 5 }));
attempt("hash() bad enc", () => crypto.hash("sha256", "abc", "bogus"));
attempt("hash() bad input", () => crypto.hash("sha256", 5));
attempt("hash() bad alg", () => crypto.hash(5, "x"));
attempt("hash() unknown alg", () => crypto.hash("nope", "x"));
attempt("hash() bytes", () => crypto.hash("sha1", Buffer.from("abc")));
attempt("hash() arraybuffer", () => crypto.hash("sha1", new ArrayBuffer(1)));

// timingSafeEqual.
for (const [label, a, b] of [
  ["equal", Buffer.from("abc"), Buffer.from("abc")],
  ["different", Buffer.from("abc"), Buffer.from("abx")],
  ["empty", Buffer.alloc(0), Buffer.alloc(0)],
  ["length", Buffer.from("a"), Buffer.from("ab")],
  ["arraybuffer", new Uint8Array([1, 2, 3]).buffer, new Uint8Array([1, 2, 3]).buffer],
  ["views", new Uint16Array([1, 2]), new Uint8Array([1, 0, 2, 0])],
  ["dataview", new DataView(new ArrayBuffer(2)), new Uint8Array(2)],
  ["string", "a", "a"],
  ["undefined", undefined, Buffer.alloc(1)],
  ["second", Buffer.alloc(1), 5],
]) {
  attempt(`timingSafeEqual ${label}`, () => crypto.timingSafeEqual(a, b));
}

// Views over resizable buffers read the live length.
{
  const rab = new ArrayBuffer(4, { maxByteLength: 16 });
  new Uint8Array(rab).set([1, 2, 3, 4]);
  const views = { tracking: new Uint8Array(rab), fixed: new Uint8Array(rab, 0, 4), dv: new DataView(rab), dvFixed: new DataView(rab, 0, 4) };
  for (const size of [8, 2, 0, 4]) {
    rab.resize(size);
    for (const [label, v] of Object.entries(views)) {
      attempt(`resizable ${size} ${label}`, () => hex(crypto.createHash("md5").update(v).digest()));
      attempt(`resizable hmac ${size} ${label}`, () => hex(crypto.createHmac("md5", v).update("a").digest()));
    }
  }
}

flush();
