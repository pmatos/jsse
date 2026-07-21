// Web Crypto `crypto.getRandomValues` / `crypto.randomUUID`, for library
// bundles that need cryptographically-secure randomness (issue #302).
//
// Backed by the flag-gated Rust "syscall floor" (issue #229): __host_random_bytes
// reads straight from the OS entropy source. Like node-shim.js and
// node-buffer-shim.js, this is a pure-JS prelude that is never baked into
// jsse's default global object, so test262 is unaffected. It is opt-in per
// library via LIB_SHIM/LIB_SHIMS, not prepended universally.
//
// Guarded inert on real Node, which has had a native Web Crypto `crypto`
// global (including getRandomValues and randomUUID) since Node 19.

(function () {
  "use strict";

  if (typeof crypto !== "undefined") return;

  var hostRandomBytes =
    typeof __host_random_bytes !== "undefined" ? __host_random_bytes : null;

  function randomBytes(n) {
    if (!hostRandomBytes) {
      throw new Error(
        "crypto shim: __host_random_bytes unavailable (run jsse with --node)",
      );
    }
    return hostRandomBytes(n);
  }

  function isIntegerTypedArray(value) {
    return (
      value instanceof Int8Array ||
      value instanceof Uint8Array ||
      value instanceof Uint8ClampedArray ||
      value instanceof Int16Array ||
      value instanceof Uint16Array ||
      value instanceof Int32Array ||
      value instanceof Uint32Array ||
      value instanceof BigInt64Array ||
      value instanceof BigUint64Array
    );
  }

  function getRandomValues(typedArray) {
    if (!isIntegerTypedArray(typedArray)) {
      throw new TypeError(
        "Failed to execute 'getRandomValues': parameter 1 is not of an accepted type",
      );
    }
    if (typedArray.byteLength > 65536) {
      throw new Error(
        "Failed to execute 'getRandomValues': the ArrayBufferView's byte length exceeds the number of bytes of entropy available via this API",
      );
    }
    var bytes = randomBytes(typedArray.byteLength);
    var view = new Uint8Array(
      typedArray.buffer,
      typedArray.byteOffset,
      typedArray.byteLength,
    );
    view.set(bytes);
    return typedArray;
  }

  // RFC 9562 version-4 UUID: 16 random bytes with the version/variant bits
  // overwritten, formatted as 8-4-4-4-12 lowercase hex.
  function randomUUID() {
    var b = randomBytes(16);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    var hex = [];
    for (var i = 0; i < 16; i++) {
      hex.push((b[i] + 0x100).toString(16).slice(1));
    }
    return (
      hex.slice(0, 4).join("") +
      "-" +
      hex.slice(4, 6).join("") +
      "-" +
      hex.slice(6, 8).join("") +
      "-" +
      hex.slice(8, 10).join("") +
      "-" +
      hex.slice(10, 16).join("")
    );
  }

  globalThis.crypto = {
    getRandomValues: getRandomValues,
    randomUUID: randomUUID,
  };
})();
