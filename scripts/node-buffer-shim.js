// Buffer + TextEncoder/TextDecoder shim for jsse library-test bundles.
//
// The keystone of the Node host-compat layer: many npm libraries reference
// `Buffer` / `TextEncoder` at import time and fail to even load without them.
// This is a pure-JS shim — Buffer is a subclass of Uint8Array riding jsse's
// existing TypedArray / ArrayBuffer / DataView, so it needs zero new engine
// object kinds. Nothing here is baked into jsse's default global object; the
// shim is only prepended to library bundles, so test262 never sees it.
//
// Every global is guarded so that on Node (where Buffer/TextEncoder already
// exist) the shim is an inert no-op. That lets `run-library-tests.sh --node`
// run the exact same bundle against Node as a reference oracle.
//
// Wrapped in an IIFE so the helper functions below do not leak into the global
// scope shared with the bundled library.
(function () {
  "use strict";

  // ---- encoding helpers ----------------------------------------------------

  function normalizeEncoding(enc) {
    if (enc === undefined || enc === null) return "utf8";
    switch (String(enc).toLowerCase()) {
      case "utf8":
      case "utf-8":
        return "utf8";
      case "ucs2":
      case "ucs-2":
      case "utf16le":
      case "utf-16le":
        return "ucs2";
      case "latin1":
      case "binary":
        return "latin1";
      case "ascii":
        return "ascii";
      case "hex":
        return "hex";
      case "base64":
        return "base64";
      case "base64url":
        return "base64url";
      default:
        throw new TypeError("Unknown encoding: " + enc);
    }
  }

  // TextDecoder uses Encoding Standard labels for the decoder families below;
  // Buffer has a different vocabulary. Keep normalization separate so
  // accepting utf-16be does not make Buffer.isEncoding("utf-16be") return true.
  function normalizeTextDecoderEncoding(label) {
    switch (String(label).trim().toLowerCase()) {
      case "unicode-1-1-utf-8":
      case "unicode11utf8":
      case "unicode20utf8":
      case "utf-8":
      case "utf8":
      case "x-unicode20utf8":
        return "utf8";
      case "ansi_x3.4-1968":
      case "ascii":
      case "cp1252":
      case "cp819":
      case "csisolatin1":
      case "ibm819":
      case "iso-8859-1":
      case "iso-ir-100":
      case "iso8859-1":
      case "iso88591":
      case "iso_8859-1":
      case "iso_8859-1:1987":
      case "l1":
      case "latin1":
      case "us-ascii":
      case "windows-1252":
      case "x-cp1252":
        return "windows1252";
      case "csunicode":
      case "iso-10646-ucs-2":
      case "ucs-2":
      case "unicode":
      case "unicodefeff":
      case "utf-16":
      case "utf-16le":
        return "utf16le";
      case "unicodefffe":
      case "utf-16be":
        return "utf16be";
      default:
        throw new RangeError("Unsupported encoding: " + label);
    }
  }

  // UTF-8 encode a JS (UTF-16) string to a Uint8Array. Surrogate pairs combine
  // into a 4-byte sequence; a lone surrogate becomes U+FFFD (matching the Rust
  // host floor's __host_write).
  // Write the UTF-8 bytes of a single code point into `target` (array or typed
  // array) at `pos`; returns the position past the last byte written.
  function encodeCodePointInto(code, target, pos) {
    if (code < 0x80) {
      target[pos++] = code;
    } else if (code < 0x800) {
      target[pos++] = 0xc0 | (code >> 6);
      target[pos++] = 0x80 | (code & 0x3f);
    } else if (code < 0x10000) {
      target[pos++] = 0xe0 | (code >> 12);
      target[pos++] = 0x80 | ((code >> 6) & 0x3f);
      target[pos++] = 0x80 | (code & 0x3f);
    } else {
      target[pos++] = 0xf0 | (code >> 18);
      target[pos++] = 0x80 | ((code >> 12) & 0x3f);
      target[pos++] = 0x80 | ((code >> 6) & 0x3f);
      target[pos++] = 0x80 | (code & 0x3f);
    }
    return pos;
  }

  function utf8Encode(str) {
    var out = [];
    var n = 0;
    for (var i = 0; i < str.length; i++) {
      var code = str.charCodeAt(i);
      if (code >= 0xd800 && code <= 0xdbff) {
        var next = str.charCodeAt(i + 1);
        if (next >= 0xdc00 && next <= 0xdfff) {
          code = 0x10000 + ((code - 0xd800) << 10) + (next - 0xdc00);
          i++;
        } else {
          code = 0xfffd;
        }
      } else if (code >= 0xdc00 && code <= 0xdfff) {
        code = 0xfffd;
      }
      n = encodeCodePointInto(code, out, n);
    }
    return Uint8Array.from(out);
  }

  // UTF-8 decode a byte array. Invalid sequences emit U+FFFD via the "maximal
  // subpart" rule (the byte that broke a multibyte sequence is reprocessed, not
  // consumed) unless `fatal`, which throws a TypeError. A leading BOM is
  // stripped unless `ignoreBOM`. Returns { out: string, pending: N }: when
  // `streaming`, a trailing incomplete-but-valid multibyte prefix is NOT
  // finalized — its N bytes are reported so the caller can buffer them for the
  // next chunk (Node's TextDecoder `{ stream: true }`).
  function utf8Decode(bytes, fatal, ignoreBOM, streaming) {
    var out = [];
    var i = 0;
    var pending = 0;
    var len = bytes.length;
    if (
      !ignoreBOM &&
      len >= 3 &&
      bytes[0] === 0xef &&
      bytes[1] === 0xbb &&
      bytes[2] === 0xbf
    ) {
      i = 3;
    }
    function fail() {
      if (fatal) {
        throw new TypeError(
          "The encoded data was not valid for encoding utf-8"
        );
      }
      out.push(0xfffd);
    }
    while (i < len) {
      var b0 = bytes[i];
      // size, cp, and the legal range for the *first* continuation byte, which
      // depends on the lead (WHATWG UTF-8 decoder). Getting these bounds right
      // is what rejects overlong encodings and surrogate/out-of-range code
      // points per byte, so malformed input yields the same count of U+FFFD as
      // Node/TextDecoder (e.g. `[e0,80,41]` → "��A", not "�A").
      var size, cp, lower, upper;
      if (b0 < 0x80) {
        out.push(b0);
        i++;
        continue;
      } else if (b0 >= 0xc2 && b0 <= 0xdf) {
        size = 2;
        cp = b0 & 0x1f;
        lower = 0x80;
        upper = 0xbf;
      } else if (b0 >= 0xe0 && b0 <= 0xef) {
        size = 3;
        cp = b0 & 0x0f;
        lower = b0 === 0xe0 ? 0xa0 : 0x80; // reject 3-byte overlong
        upper = b0 === 0xed ? 0x9f : 0xbf; // reject surrogates (ED A0..)
      } else if (b0 >= 0xf0 && b0 <= 0xf4) {
        size = 4;
        cp = b0 & 0x07;
        lower = b0 === 0xf0 ? 0x90 : 0x80; // reject 4-byte overlong
        upper = b0 === 0xf4 ? 0x8f : 0xbf; // reject > U+10FFFF
      } else {
        // Invalid lead byte (0x80-0xC1, 0xF5-0xFF): consume one byte.
        fail();
        i++;
        continue;
      }
      // Validate each continuation byte against its legal range *before*
      // accumulating; consume only the bytes that were valid, leaving the
      // offending byte to be reprocessed (maximal-subpart substitution).
      var consumed = 1;
      var ok = true;
      var ranOut = false;
      for (var j = 1; j < size; j++) {
        var bx = bytes[i + j];
        if (bx === undefined) {
          // Ran off the end mid-sequence: the bytes so far are a valid prefix.
          ok = false;
          ranOut = true;
          break;
        }
        var lo = j === 1 ? lower : 0x80;
        var hi = j === 1 ? upper : 0xbf;
        if (bx < lo || bx > hi) {
          ok = false;
          break;
        }
        cp = (cp << 6) | (bx & 0x3f);
        consumed++;
      }
      if (!ok) {
        if (ranOut && streaming) {
          // Buffer the incomplete trailing sequence for the next chunk.
          pending = len - i;
          break;
        }
        fail();
        i += consumed;
        continue;
      }
      out.push(cp);
      i += size;
    }
    return { out: codePointsToString(out), pending: pending };
  }

  var WINDOWS_1252 = [
    0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021,
    0x2c6, 0x2030, 0x160, 0x2039, 0x152, 0x8d, 0x17d, 0x8f,
    0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014,
    0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e, 0x178,
  ];

  function windows1252Decode(bytes) {
    var out = [];
    for (var i = 0; i < bytes.length; i++) {
      var byte = bytes[i];
      out.push(byte >= 0x80 && byte <= 0x9f ? WINDOWS_1252[byte - 0x80] : byte);
    }
    return codePointsToString(out);
  }

  function utf16Decode(bytes, bigEndian, fatal, ignoreBOM, streaming) {
    var out = [];
    var i = 0;
    var pending = 0;
    var readUnit = function (offset) {
      return bigEndian
        ? (bytes[offset] << 8) | bytes[offset + 1]
        : bytes[offset] | (bytes[offset + 1] << 8);
    };

    if (!ignoreBOM && bytes.length >= 2 && readUnit(0) === 0xfeff) i = 2;

    function fail() {
      if (fatal) {
        throw new TypeError(
          "The encoded data was not valid for encoding utf-16"
        );
      }
      out.push(0xfffd);
    }

    while (i < bytes.length) {
      if (i + 1 >= bytes.length) {
        if (streaming) pending = bytes.length - i;
        else fail();
        break;
      }
      var first = readUnit(i);
      if (first >= 0xd800 && first <= 0xdbff) {
        if (i + 3 >= bytes.length) {
          if (streaming) pending = bytes.length - i;
          else fail();
          break;
        }
        var second = readUnit(i + 2);
        if (second >= 0xdc00 && second <= 0xdfff) {
          out.push(
            0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00)
          );
          i += 4;
          continue;
        }
        fail();
        i += 2;
        continue;
      }
      if (first >= 0xdc00 && first <= 0xdfff) {
        fail();
        i += 2;
        continue;
      }
      out.push(first);
      i += 2;
    }
    return { out: codePointsToString(out), pending: pending };
  }

  // Build a string from code points in bounded chunks (fromCharCode/apply on a
  // huge array can blow the call stack).
  function codePointsToString(cps) {
    var s = "";
    var chunk = [];
    for (var i = 0; i < cps.length; i++) {
      chunk.push(cps[i]);
      if (chunk.length >= 0x1000) {
        s += String.fromCodePoint.apply(null, chunk);
        chunk.length = 0;
      }
    }
    if (chunk.length) s += String.fromCodePoint.apply(null, chunk);
    return s;
  }

  var HEX = [];
  for (var h = 0; h < 256; h++) {
    HEX.push((h < 16 ? "0" : "") + h.toString(16));
  }

  function hexVal(c) {
    if (c >= 0x30 && c <= 0x39) return c - 0x30; // 0-9
    if (c >= 0x61 && c <= 0x66) return c - 0x61 + 10; // a-f
    if (c >= 0x41 && c <= 0x46) return c - 0x41 + 10; // A-F
    return -1;
  }

  function hexDecode(str) {
    var out = [];
    for (var i = 0; i + 1 < str.length + (str.length % 2); i += 2) {
      if (i + 1 >= str.length) break;
      var hi = hexVal(str.charCodeAt(i));
      var lo = hexVal(str.charCodeAt(i + 1));
      if (hi < 0 || lo < 0) break; // stop at first invalid nibble (Node)
      out.push((hi << 4) | lo);
    }
    return Uint8Array.from(out);
  }

  var B64 =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  var B64URL =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
  // Reverse lookup accepts both the standard and URL-safe alphabets, matching
  // Node (its "base64" decoder also accepts '-' and '_').
  var B64REV = {};
  for (var k = 0; k < B64.length; k++) B64REV[B64.charCodeAt(k)] = k;
  B64REV["-".charCodeAt(0)] = 62;
  B64REV["_".charCodeAt(0)] = 63;

  function base64Encode(bytes, urlSafe) {
    var chars = urlSafe ? B64URL : B64;
    var out = "";
    var len = bytes.length;
    var i;
    for (i = 0; i < len - 2; i += 3) {
      var n = (bytes[i] << 16) | (bytes[i + 1] << 8) | bytes[i + 2];
      out +=
        chars[(n >> 18) & 63] +
        chars[(n >> 12) & 63] +
        chars[(n >> 6) & 63] +
        chars[n & 63];
    }
    var rem = len - i;
    if (rem === 1) {
      var n1 = bytes[i] << 16;
      out += chars[(n1 >> 18) & 63] + chars[(n1 >> 12) & 63];
      if (!urlSafe) out += "==";
    } else if (rem === 2) {
      var n2 = (bytes[i] << 16) | (bytes[i + 1] << 8);
      out +=
        chars[(n2 >> 18) & 63] +
        chars[(n2 >> 12) & 63] +
        chars[(n2 >> 6) & 63];
      if (!urlSafe) out += "=";
    }
    return out;
  }

  // Forgiving base64 decode: whitespace and other non-alphabet characters are
  // skipped, '=' terminates, and missing padding is tolerated (Node behavior).
  function base64Decode(str) {
    var out = [];
    var bits = 0;
    var nbits = 0;
    for (var i = 0; i < str.length; i++) {
      var c = str.charCodeAt(i);
      var v = B64REV[c];
      if (v === undefined) {
        if (c === 0x3d) break; // '='
        continue;
      }
      bits = (bits << 6) | v;
      nbits += 6;
      if (nbits >= 8) {
        nbits -= 8;
        out.push((bits >> nbits) & 0xff);
      }
    }
    return Uint8Array.from(out);
  }

  // Encode a JS string to a plain Uint8Array of bytes for `encoding`.
  function strToBytes(str, encoding) {
    str = String(str);
    encoding = normalizeEncoding(encoding);
    var i, out;
    switch (encoding) {
      case "utf8":
        return utf8Encode(str);
      case "ascii":
      case "latin1":
        // Both mask each code unit to 8 bits on encode (Node); the ascii/
        // latin1 split only matters on decode.
        out = new Uint8Array(str.length);
        for (i = 0; i < str.length; i++) out[i] = str.charCodeAt(i) & 0xff;
        return out;
      case "hex":
        return hexDecode(str);
      case "base64":
      case "base64url":
        return base64Decode(str);
      case "ucs2": {
        out = new Uint8Array(str.length * 2);
        var dv = new DataView(out.buffer);
        for (i = 0; i < str.length; i++) {
          dv.setUint16(i * 2, str.charCodeAt(i), true);
        }
        return out;
      }
    }
    throw new TypeError("Unknown encoding: " + encoding);
  }

  // ---- TextEncoder / TextDecoder ------------------------------------------

  if (typeof globalThis.TextEncoder === "undefined") {
    var TextEncoderShim = function TextEncoder() {};
    Object.defineProperty(TextEncoderShim.prototype, "encoding", {
      get: function () {
        return "utf-8";
      },
      configurable: true,
    });
    TextEncoderShim.prototype.encode = function (input) {
      return utf8Encode(input === undefined ? "" : String(input));
    };
    TextEncoderShim.prototype.encodeInto = function (source, dest) {
      // Encode code point by code point, stopping before any character whose
      // UTF-8 bytes would not fit — a partial multibyte sequence is never
      // written (Web/Node semantics). `read` counts source UTF-16 code units
      // consumed, `written` counts bytes written.
      source = String(source);
      var written = 0;
      var read = 0;
      var cap = dest.length;
      for (var i = 0; i < source.length; ) {
        var code = source.charCodeAt(i);
        var consumed = 1;
        if (code >= 0xd800 && code <= 0xdbff) {
          var next = source.charCodeAt(i + 1);
          if (next >= 0xdc00 && next <= 0xdfff) {
            code = 0x10000 + ((code - 0xd800) << 10) + (next - 0xdc00);
            consumed = 2;
          } else {
            code = 0xfffd;
          }
        } else if (code >= 0xdc00 && code <= 0xdfff) {
          code = 0xfffd;
        }
        var need = code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
        if (written + need > cap) break;
        written = encodeCodePointInto(code, dest, written);
        read += consumed;
        i += consumed;
      }
      return { read: read, written: written };
    };
    globalThis.TextEncoder = TextEncoderShim;
  }

  if (typeof globalThis.TextDecoder === "undefined") {
    var TextDecoderShim = function TextDecoder(label, options) {
      var enc = normalizeTextDecoderEncoding(
        label === undefined ? "utf-8" : String(label)
      );
      options = options || {};
      this._encoding = enc;
      this._fatal = !!options.fatal;
      this._ignoreBOM = !!options.ignoreBOM;
      this._pending = null; // trailing bytes buffered across stream chunks
      this._bomHandled = false; // only the leading BOM of the stream is stripped
    };
    Object.defineProperty(TextDecoderShim.prototype, "encoding", {
      get: function () {
        if (this._encoding === "windows1252") return "windows-1252";
        if (this._encoding === "utf16le") return "utf-16le";
        if (this._encoding === "utf16be") return "utf-16be";
        return "utf-8";
      },
      configurable: true,
    });
    Object.defineProperty(TextDecoderShim.prototype, "fatal", {
      get: function () {
        return this._fatal;
      },
      configurable: true,
    });
    Object.defineProperty(TextDecoderShim.prototype, "ignoreBOM", {
      get: function () {
        return this._ignoreBOM;
      },
      configurable: true,
    });
    TextDecoderShim.prototype.decode = function (input, options) {
      var bytes;
      if (input === undefined) {
        bytes = new Uint8Array(0);
      } else if (input instanceof Uint8Array) {
        bytes = input;
      } else if (input instanceof ArrayBuffer) {
        bytes = new Uint8Array(input);
      } else if (ArrayBuffer.isView(input)) {
        bytes = new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
      } else {
        throw new TypeError("decode() expects a BufferSource");
      }
      var stream = !!(options && options.stream);
      // Prepend any bytes buffered from a previous `{ stream: true }` chunk.
      if (this._pending && this._pending.length) {
        var combined = new Uint8Array(this._pending.length + bytes.length);
        combined.set(this._pending, 0);
        combined.set(bytes, this._pending.length);
        bytes = combined;
      }
      // Only the BOM at the very start of the stream is stripped; once bytes
      // have actually been consumed an interior BOM at a chunk boundary is
      // literal. A non-streaming (final) decode flushes and RESETS the decoder,
      // so a reused decoder strips a leading BOM again (Node).
      var ignoreBOM = this._ignoreBOM || this._bomHandled;
      var res;
      if (this._encoding === "windows1252") {
        res = { out: windows1252Decode(bytes), pending: 0 };
      } else if (this._encoding === "utf16le") {
        res = utf16Decode(bytes, false, this._fatal, ignoreBOM, stream);
      } else if (this._encoding === "utf16be") {
        res = utf16Decode(bytes, true, this._fatal, ignoreBOM, stream);
      } else {
        res = utf8Decode(bytes, this._fatal, ignoreBOM, stream);
      }
      if (stream) {
        // Mark BOM-handled only once real bytes were consumed (not merely
        // buffered), so a BOM split across chunks is still stripped.
        if (bytes.length - res.pending > 0) this._bomHandled = true;
        this._pending =
          res.pending > 0 ? bytes.subarray(bytes.length - res.pending) : null;
      } else {
        this._pending = null;
        this._bomHandled = false;
      }
      return res.out;
    };
    globalThis.TextDecoder = TextDecoderShim;
  }

  // ---- Buffer --------------------------------------------------------------

  if (typeof globalThis.Buffer === "undefined") {
    var Buffer = class Buffer extends Uint8Array {
      constructor(arg, encodingOrOffset, length) {
        if (typeof arg === "string") {
          // Legacy `new Buffer(string[, encoding])` — deprecated but still used
          // by older npm packages; route it through Buffer.from(string)'s path
          // so it yields the same bytes instead of an empty buffer.
          var strBytes = strToBytes(arg, encodingOrOffset);
          super(strBytes.length);
          this.set(strBytes);
        } else if (typeof arg === "number") {
          // `new Buffer(size)` (legacy) and every internal size-based
          // allocation; validate so a negative/NaN size throws a RangeError
          // instead of coercing into a multi-GiB allocation.
          super(toAllocSize(arg));
        } else {
          // ArrayBuffer[, offset, length] | TypedArray | array-like, and the
          // @@species path (new Buffer(buffer, byteOffset, length)) that
          // subarray/slice use — forward verbatim to Uint8Array.
          super(arg, encodingOrOffset, length);
        }
      }

      // ----- static factories -----
      static from(value, encodingOrOffset, length) {
        if (typeof value === "string") {
          var bytes = strToBytes(value, encodingOrOffset);
          var b = new Buffer(bytes.length);
          b.set(bytes);
          return b;
        }
        if (
          value instanceof ArrayBuffer ||
          (typeof SharedArrayBuffer !== "undefined" &&
            value instanceof SharedArrayBuffer)
        ) {
          // Share the backing memory (Node semantics). Apply ToInteger to the
          // offset/length (truncate toward zero, NaN → 0) BEFORE deriving the
          // default length — not `| 0` (wraps 2**32 to 0) nor raw Number (a
          // fractional offset would skew byteLength - offset). An out-of-range
          // offset/length is a RangeError; a negative length is an empty view.
          var offset =
            encodingOrOffset === undefined
              ? 0
              : Math.trunc(Number(encodingOrOffset));
          if (offset !== offset) offset = 0; // NaN
          if (offset < 0 || offset > value.byteLength) {
            throw new RangeError('"offset" is outside of buffer bounds');
          }
          var len;
          if (length === undefined) {
            len = value.byteLength - offset;
          } else {
            len = Math.trunc(Number(length));
            if (len !== len || len < 0) len = 0;
            if (offset + len > value.byteLength) {
              throw new RangeError('"length" is outside of buffer bounds');
            }
          }
          return new Buffer(value, offset, len);
        }
        if (value instanceof Uint8Array) {
          // Byte view (incl. Buffer) → copy the bytes 1:1.
          var copyU8 = new Buffer(value.length);
          copyU8.set(value);
          return copyU8;
        }
        if (ArrayBuffer.isView(value)) {
          // Other TypedArray → copy the *elements*, each truncated to a byte
          // (Node: `Buffer.from(new Uint16Array([0x1234])) === <34>`, not the
          // raw little-endian backing bytes). set() does the per-element ToUint8
          // conversion. A DataView has no element `length`, so Node yields an
          // empty Buffer — the `typeof length` guard routes it there.
          if (typeof value.length === "number") {
            var copyEl = new Buffer(value.length);
            copyEl.set(value);
            return copyEl;
          }
          return new Buffer(0);
        }
        if (value != null && typeof value === "object") {
          // { type: 'Buffer', data: [...] } — the Buffer#toJSON round-trip.
          if (value.type === "Buffer" && Array.isArray(value.data)) {
            return Buffer.from(value.data);
          }
          // Node honors valueOf() / Symbol.toPrimitive returning a usable value
          // BEFORE treating an object as array-like, so Buffer.from(new
          // String("abc")) uses the string "abc" (→ <61 62 63>), not the indexed
          // characters as octets. A plain array/array-like whose valueOf returns
          // itself falls through to the array-like path below.
          var prim;
          if (typeof value.valueOf === "function") {
            var vo = value.valueOf();
            if (vo !== value) prim = vo;
          }
          if (
            prim === undefined &&
            typeof value[Symbol.toPrimitive] === "function"
          ) {
            prim = value[Symbol.toPrimitive]("default");
          }
          if (prim != null && prim !== value) {
            return Buffer.from(prim, encodingOrOffset, length);
          }
        }
        if (value != null && typeof value.length === "number") {
          // Array-like of octets: each element is coerced via ToUint8. The
          // length is clamped ToLength-style (negative/NaN → 0, fractional
          // floored) so a sentinel like { length: -1 } yields an empty buffer
          // (Node) rather than a multi-GiB allocation.
          var alen = value.length;
          alen =
            alen !== alen || alen < 0
              ? 0
              : Math.min(Math.floor(alen), 0xffffffff);
          var out = new Buffer(alen);
          for (var i = 0; i < out.length; i++) out[i] = value[i];
          return out;
        }
        throw new TypeError(
          "The first argument must be of type string or an instance of " +
            "Buffer, ArrayBuffer, Array, or Array-like Object."
        );
      }

      static alloc(size, fill, encoding) {
        // alloc requires a numeric size (Node: a string/null/object size is a
        // TypeError — unlike `new Buffer(size)`, where a string is legacy
        // content). The constructor then range-validates the number.
        if (typeof size !== "number") {
          throw new TypeError('The "size" argument must be of type number');
        }
        var b = new Buffer(size);
        if (fill !== undefined && fill !== 0) b.fill(fill, 0, b.length, encoding);
        return b;
      }

      static allocUnsafe(size) {
        // jsse zero-fills; that is always a safe superset of "unsafe".
        if (typeof size !== "number") {
          throw new TypeError('The "size" argument must be of type number');
        }
        return new Buffer(size);
      }

      static allocUnsafeSlow(size) {
        if (typeof size !== "number") {
          throw new TypeError('The "size" argument must be of type number');
        }
        return new Buffer(size);
      }

      static isBuffer(obj) {
        return obj instanceof Buffer;
      }

      static isEncoding(enc) {
        // Node returns false for any non-string (incl. undefined/null); only a
        // string naming a known encoding is valid. normalizeEncoding treats
        // undefined/null as the utf8 default, so guard the type here.
        if (typeof enc !== "string") return false;
        try {
          normalizeEncoding(enc);
          return true;
        } catch (e) {
          return false;
        }
      }

      static byteLength(value, encoding) {
        if (typeof value !== "string") {
          if (
            value instanceof ArrayBuffer ||
            (typeof SharedArrayBuffer !== "undefined" &&
              value instanceof SharedArrayBuffer) ||
            ArrayBuffer.isView(value)
          ) {
            return value.byteLength;
          }
          // Node throws for anything else rather than measuring String(value)
          // (which would make byteLength(123) return 3).
          throw new TypeError(
            'The "string" argument must be of type string or an instance of ' +
              "Buffer or ArrayBuffer."
          );
        }
        // Unlike toString/from, byteLength does not throw on an unrecognized
        // encoding — it falls back to UTF-8 (Node).
        try {
          encoding = normalizeEncoding(encoding);
        } catch (e) {
          encoding = "utf8";
        }
        switch (encoding) {
          case "ascii":
          case "latin1":
            return value.length;
          case "hex":
            return value.length >>> 1;
          case "ucs2":
            return value.length * 2;
          case "base64":
          case "base64url":
            return base64Decode(value).length;
          case "utf8":
          default:
            return utf8Encode(value).length;
        }
      }

      static concat(list, totalLength) {
        if (!Array.isArray(list)) {
          throw new TypeError('"list" argument must be an Array of Buffers');
        }
        // Every entry must be a Buffer/Uint8Array (Node rejects a plain array or
        // a wider typed array like Uint16Array with a TypeError).
        for (var i = 0; i < list.length; i++) {
          if (!(list[i] instanceof Uint8Array)) {
            throw new TypeError(
              '"list[' + i + ']" must be an instance of Buffer or Uint8Array'
            );
          }
        }
        if (totalLength === undefined) {
          totalLength = 0;
          for (var s = 0; s < list.length; s++) totalLength += list[s].length;
        } else if (typeof totalLength !== "number") {
          throw new TypeError('"totalLength" argument must be of type number');
        } else if (!Number.isInteger(totalLength)) {
          // Unlike alloc (which floors a fractional size), concat rejects a
          // non-integer totalLength (Node).
          throw new RangeError('"totalLength" is out of range');
        }
        var out = new Buffer(totalLength);
        var pos = 0;
        for (var j = 0; j < list.length && pos < out.length; j++) {
          var item = list[j];
          var n = Math.min(item.length, out.length - pos);
          for (var m = 0; m < n; m++) out[pos + m] = item[m];
          pos += n;
        }
        return out;
      }

      static compare(a, b) {
        if (!(a instanceof Uint8Array) || !(b instanceof Uint8Array)) {
          throw new TypeError(
            "The arguments must be one of type Buffer or Uint8Array"
          );
        }
        return bufCompare(a, 0, a.length, b, 0, b.length);
      }

      // ----- instance methods -----
      toString(encoding, start, end) {
        var len = this.length;
        // Clamp numerically (not `| 0`, which wraps Infinity / >32-bit to 0):
        // start = Infinity → empty slice, end = Infinity → whole buffer (Node).
        start = start === undefined ? 0 : Math.floor(Number(start));
        if (start !== start || start < 0) start = 0;
        if (start > len) start = len;
        end = end === undefined ? len : Math.floor(Number(end));
        if (end !== end || end < 0) end = 0;
        if (end > len) end = len;
        if (end <= start) return "";
        encoding = normalizeEncoding(encoding);
        var i, s;
        switch (encoding) {
          case "utf8":
            return utf8Decode(this.subarray(start, end), false, true).out;
          case "ascii":
            s = "";
            for (i = start; i < end; i++) {
              s += String.fromCharCode(this[i] & 0x7f);
            }
            return s;
          case "latin1":
            s = "";
            for (i = start; i < end; i++) s += String.fromCharCode(this[i]);
            return s;
          case "hex":
            s = "";
            for (i = start; i < end; i++) s += HEX[this[i]];
            return s;
          case "base64":
            return base64Encode(this.subarray(start, end), false);
          case "base64url":
            return base64Encode(this.subarray(start, end), true);
          case "ucs2":
            s = "";
            for (i = start; i + 1 < end; i += 2) {
              s += String.fromCharCode(this[i] | (this[i + 1] << 8));
            }
            return s;
        }
        throw new TypeError("Unknown encoding: " + encoding);
      }

      toJSON() {
        var data = new Array(this.length);
        for (var i = 0; i < this.length; i++) data[i] = this[i];
        return { type: "Buffer", data: data };
      }

      write(string, offset, length, encoding) {
        if (typeof string !== "string") {
          throw new TypeError('The "string" argument must be of type string');
        }
        if (offset === undefined) {
          encoding = "utf8";
          offset = 0;
          length = this.length;
        } else if (typeof offset === "string") {
          encoding = offset;
          offset = 0;
          length = this.length;
        } else if (typeof length === "string") {
          encoding = length;
          length = this.length - offset;
        } else {
          if (length === undefined) length = this.length - offset;
          if (encoding === undefined) encoding = "utf8";
        }
        if (!Number.isInteger(offset) || offset < 0 || offset > this.length) {
          throw new RangeError('"offset" is outside of buffer bounds');
        }
        var remaining = this.length - offset;
        if (!Number.isInteger(length) || length < 0 || length > remaining) {
          throw new RangeError('"length" is outside of buffer bounds');
        }
        var bytes = strToBytes(string, encoding);
        var n = Math.min(bytes.length, length);
        for (var i = 0; i < n; i++) this[offset + i] = bytes[i];
        return n;
      }

      // Node's Buffer#slice shares memory (unlike Uint8Array#slice, which
      // copies); it is a deprecated alias for subarray.
      slice(start, end) {
        return this.subarray(start, end);
      }

      equals(other) {
        if (!(other instanceof Uint8Array)) {
          throw new TypeError('The "otherBuffer" argument must be a Buffer');
        }
        if (this.length !== other.length) return false;
        for (var i = 0; i < this.length; i++) {
          if (this[i] !== other[i]) return false;
        }
        return true;
      }

      compare(target, targetStart, targetEnd, sourceStart, sourceEnd) {
        if (!(target instanceof Uint8Array)) {
          throw new TypeError('The "target" argument must be a Buffer or Uint8Array');
        }
        targetStart = targetStart === undefined ? 0 : targetStart;
        targetEnd = targetEnd === undefined ? target.length : targetEnd;
        sourceStart = sourceStart === undefined ? 0 : sourceStart;
        sourceEnd = sourceEnd === undefined ? this.length : sourceEnd;
        // Each slice bound must be an integer within its buffer (Node throws
        // RangeError otherwise, rather than comparing wrapped indices).
        if (
          !Number.isInteger(targetStart) ||
          !Number.isInteger(targetEnd) ||
          !Number.isInteger(sourceStart) ||
          !Number.isInteger(sourceEnd) ||
          targetStart < 0 ||
          targetEnd > target.length ||
          sourceStart < 0 ||
          sourceEnd > this.length
        ) {
          throw new RangeError("out of range index");
        }
        return bufCompare(
          this,
          sourceStart,
          sourceEnd,
          target,
          targetStart,
          targetEnd
        );
      }

      copy(target, targetStart, sourceStart, sourceEnd) {
        if (!(target instanceof Uint8Array)) {
          throw new TypeError('The "target" argument must be a Buffer or Uint8Array');
        }
        // targetStart is lenient in Node (ToInteger, not `| 0` — a target start
        // at/above 2**32 must stay out of range and copy 0, not wrap to 0 and
        // overwrite from the start), but sourceStart is strictly validated.
        targetStart =
          targetStart === undefined ? 0 : Math.trunc(Number(targetStart));
        if (targetStart !== targetStart) targetStart = 0; // NaN
        // sourceStart is ToInteger-coerced (floored), like Node — not a strict
        // integer — but must land within the source buffer.
        sourceStart =
          sourceStart === undefined ? 0 : Math.trunc(Number(sourceStart));
        if (sourceStart !== sourceStart) sourceStart = 0; // NaN → 0
        if (sourceStart < 0 || sourceStart > this.length) {
          throw new RangeError('"sourceStart" is out of range');
        }
        // sourceEnd: ToInteger (not `| 0`, which wraps 2**32 to 0); a negative
        // end is a RangeError, a too-large end clamps to the source length.
        sourceEnd =
          sourceEnd === undefined ? this.length : Math.trunc(Number(sourceEnd));
        if (sourceEnd !== sourceEnd || sourceEnd < 0) {
          throw new RangeError('"sourceEnd" is out of range');
        }
        if (sourceEnd > this.length) sourceEnd = this.length;
        var n = Math.min(sourceEnd - sourceStart, target.length - targetStart);
        if (n <= 0) return 0;
        // memmove semantics: snapshot the source range first so an overlapping
        // in-place copy (target aliases this) can't read bytes it has already
        // overwritten (Node: copy [0,4) of "abcde" to offset 1 → "aabcd").
        var src = new Uint8Array(this.subarray(sourceStart, sourceStart + n));
        target.set(src, targetStart);
        return n;
      }

      fill(value, offset, end, encoding) {
        // fill(value, encoding) — a string second/third arg is the encoding ONLY
        // when the fill value is itself a string; for a numeric/Uint8Array value
        // a string in the offset/end position is an invalid offset (Node throws
        // TypeError), not an encoding.
        if (typeof value === "string") {
          if (typeof offset === "string") {
            encoding = offset;
            offset = 0;
            end = this.length;
          } else if (typeof end === "string") {
            encoding = end;
            end = this.length;
          }
        }
        offset = offset === undefined ? 0 : offset;
        end = end === undefined ? this.length : end;
        if (typeof offset !== "number" || typeof end !== "number") {
          throw new TypeError('"offset"/"end" must be numbers');
        }
        if (!Number.isInteger(offset) || offset < 0 || offset > this.length) {
          throw new RangeError('"offset" is out of range');
        }
        if (!Number.isInteger(end) || end < 0 || end > this.length) {
          throw new RangeError('"end" is out of range');
        }
        if (typeof value === "string") {
          // An empty string zero-fills the range (Node), but a non-empty string
          // that encodes to no bytes (e.g. invalid hex "zz") is an invalid fill.
          if (value === "") {
            for (var z = offset; z < end; z++) this[z] = 0;
            return this;
          }
          var bytes = strToBytes(value, encoding);
          if (bytes.length === 0) {
            throw new TypeError(
              'The value "' + value + '" is invalid for argument "value"'
            );
          }
          for (var j = offset, p = 0; j < end; j++, p++) {
            this[j] = bytes[p % bytes.length];
          }
          return this;
        }
        if (value instanceof Uint8Array) {
          if (value.length === 0) return this;
          for (var k = offset, q = 0; k < end; k++, q++) {
            this[k] = value[q % value.length];
          }
          return this;
        }
        // Any other value (number, boolean, null, object) is coerced to a single
        // byte (Node): true → 01, false/null/NaN → 00, 2.9 → 02.
        var n = Number(value);
        if (n !== n) n = 0;
        n = n & 0xff;
        for (var i = offset; i < end; i++) this[i] = n;
        return this;
      }

      indexOf(value, byteOffset, encoding) {
        return bufIndexOf(this, value, byteOffset, encoding, true);
      }

      lastIndexOf(value, byteOffset, encoding) {
        return bufIndexOf(this, value, byteOffset, encoding, false);
      }

      includes(value, byteOffset, encoding) {
        return this.indexOf(value, byteOffset, encoding) !== -1;
      }

      // subarray is inherited from Uint8Array and already returns a Buffer
      // (subclass) that shares memory, thanks to @@species.
    };

    // ----- shared comparison / search helpers -----
    function bufCompare(a, aStart, aEnd, b, bStart, bEnd) {
      var aLen = aEnd - aStart;
      var bLen = bEnd - bStart;
      var len = Math.min(aLen, bLen);
      for (var i = 0; i < len; i++) {
        var x = a[aStart + i];
        var y = b[bStart + i];
        if (x < y) return -1;
        if (x > y) return 1;
      }
      if (aLen < bLen) return -1;
      if (aLen > bLen) return 1;
      return 0;
    }

    function toSearchBytes(value, encoding) {
      if (typeof value === "number") return Uint8Array.of(value & 0xff);
      if (typeof value === "string") return strToBytes(value, encoding);
      if (value instanceof Uint8Array) return value;
      throw new TypeError(
        "The value argument must be one of type number, string, or Buffer"
      );
    }

    function bufIndexOf(buf, value, byteOffset, encoding, forward) {
      if (typeof byteOffset === "string") {
        encoding = byteOffset;
        byteOffset = undefined;
      }
      var needle = toSearchBytes(value, encoding);
      var len = buf.length;
      var start;
      // ToInteger the byteOffset (not `| 0`, which wraps Infinity/2**32 to 0 and
      // would search from the start instead of finding no match).
      if (forward) {
        start =
          byteOffset === undefined ? 0 : Math.trunc(Number(byteOffset));
        if (start !== start) start = 0; // NaN
        if (start < 0) start = Math.max(len + start, 0);
        if (needle.length === 0) return start <= len ? start : len;
        for (var i = start; i + needle.length <= len; i++) {
          if (matchAt(buf, needle, i)) return i;
        }
        return -1;
      }
      start =
        byteOffset === undefined
          ? len - needle.length
          : Math.trunc(Number(byteOffset));
      if (start !== start) start = len - needle.length; // NaN
      if (start < 0) start = len + start;
      if (start > len - needle.length) start = len - needle.length;
      if (needle.length === 0) return start < 0 ? 0 : start;
      for (var j = start; j >= 0; j--) {
        if (matchAt(buf, needle, j)) return j;
      }
      return -1;
    }

    function matchAt(buf, needle, pos) {
      for (var i = 0; i < needle.length; i++) {
        if (buf[pos + i] !== needle[i]) return false;
      }
      return true;
    }

    // Validate an allocation size the way Node does: it must be a non-negative
    // number within range, else RangeError (never a silent multi-GiB coercion).
    // A fractional size is floored, matching Node.
    function toAllocSize(size) {
      if (
        typeof size !== "number" ||
        size !== size || // NaN
        size < 0 ||
        size > 0xffffffff
      ) {
        throw new RangeError(
          'The value "' + size + '" is invalid for option "size"'
        );
      }
      return Math.floor(size);
    }

    // ----- fixed-width numeric read/write via DataView -----
    function dvOf(buf) {
      return new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
    }

    // Validate a read/write offset the way Node does, rather than `| 0` (which
    // would turn 1.9 / Infinity / 2**32 into an in-bounds index and read the
    // wrong bytes): a non-number is a TypeError; a non-integer / negative /
    // out-of-range offset is a RangeError. The caller defaults undefined to 0.
    function assertOffset(offset, size, buf) {
      if (typeof offset !== "number") {
        throw new TypeError('"offset" must be a number');
      }
      if (
        !Number.isInteger(offset) ||
        offset < 0 ||
        offset + size > buf.length
      ) {
        throw new RangeError('"offset" is out of range');
      }
    }

    // [readName, writeName, DataView getter, DataView setter, size, littleEndian]
    // littleEndian === null → single-byte accessor (no endianness argument).
    var FIXED = [
      ["readUInt8", "writeUInt8", "getUint8", "setUint8", 1, null],
      ["readInt8", "writeInt8", "getInt8", "setInt8", 1, null],
      ["readUInt16LE", "writeUInt16LE", "getUint16", "setUint16", 2, true],
      ["readUInt16BE", "writeUInt16BE", "getUint16", "setUint16", 2, false],
      ["readInt16LE", "writeInt16LE", "getInt16", "setInt16", 2, true],
      ["readInt16BE", "writeInt16BE", "getInt16", "setInt16", 2, false],
      ["readUInt32LE", "writeUInt32LE", "getUint32", "setUint32", 4, true],
      ["readUInt32BE", "writeUInt32BE", "getUint32", "setUint32", 4, false],
      ["readInt32LE", "writeInt32LE", "getInt32", "setInt32", 4, true],
      ["readInt32BE", "writeInt32BE", "getInt32", "setInt32", 4, false],
      ["readFloatLE", "writeFloatLE", "getFloat32", "setFloat32", 4, true],
      ["readFloatBE", "writeFloatBE", "getFloat32", "setFloat32", 4, false],
      ["readDoubleLE", "writeDoubleLE", "getFloat64", "setFloat64", 8, true],
      ["readDoubleBE", "writeDoubleBE", "getFloat64", "setFloat64", 8, false],
      [
        "readBigUInt64LE",
        "writeBigUInt64LE",
        "getBigUint64",
        "setBigUint64",
        8,
        true,
      ],
      [
        "readBigUInt64BE",
        "writeBigUInt64BE",
        "getBigUint64",
        "setBigUint64",
        8,
        false,
      ],
      [
        "readBigInt64LE",
        "writeBigInt64LE",
        "getBigInt64",
        "setBigInt64",
        8,
        true,
      ],
      [
        "readBigInt64BE",
        "writeBigInt64BE",
        "getBigInt64",
        "setBigInt64",
        8,
        false,
      ],
    ];
    // The accepted [min, max] for a fixed-width write, derived from the setter.
    // Floats have no range; integers/BigInts must fit their width or Node throws
    // RangeError (DataView setters would silently wrap/truncate instead).
    function rangeFor(set, size) {
      if (set.indexOf("Float") !== -1) return null;
      var bits = size * 8;
      if (set.indexOf("Big") !== -1) {
        return set.indexOf("Uint") !== -1
          ? [0n, (1n << 64n) - 1n]
          : [-(1n << 63n), (1n << 63n) - 1n];
      }
      return set.indexOf("Uint") !== -1
        ? [0, Math.pow(2, bits) - 1]
        : [-Math.pow(2, bits - 1), Math.pow(2, bits - 1) - 1];
    }
    FIXED.forEach(function (spec) {
      var rd = spec[0],
        wr = spec[1],
        get = spec[2],
        set = spec[3],
        size = spec[4],
        le = spec[5];
      var range = rangeFor(set, size);
      Buffer.prototype[rd] = function (offset) {
        offset = offset === undefined ? 0 : offset;
        assertOffset(offset, size, this);
        var dv = dvOf(this);
        return le === null ? dv[get](offset) : dv[get](offset, le);
      };
      Buffer.prototype[wr] = function (value, offset) {
        offset = offset === undefined ? 0 : offset;
        assertOffset(offset, size, this);
        if (range !== null && (value < range[0] || value > range[1])) {
          throw new RangeError('The value of "value" is out of range');
        }
        var dv = dvOf(this);
        if (le === null) dv[set](offset, value);
        else dv[set](offset, value, le);
        return offset + size;
      };
    });

    // ----- variable-width (1..6 byte) integer read/write -----
    // Node bounds-checks these (the fixed-width family gets it free from
    // DataView): byteLength must be 1..6 and [offset, offset+byteLength) must
    // lie inside the buffer, else RangeError — so a truncated read yields an
    // exception, not NaN, and a write can't run past the end.
    function checkVarWidth(buf, offset, byteLength) {
      if (
        typeof byteLength !== "number" ||
        !Number.isInteger(byteLength) ||
        byteLength < 1 ||
        byteLength > 6
      ) {
        throw new RangeError('"byteLength" must be an integer >= 1 and <= 6');
      }
      assertOffset(offset, byteLength, buf);
    }
    Buffer.prototype.readUIntLE = function (offset, byteLength) {
      offset = offset === undefined ? 0 : offset;
      checkVarWidth(this, offset, byteLength);
      var val = 0;
      var mul = 1;
      for (var i = 0; i < byteLength; i++) {
        val += this[offset + i] * mul;
        mul *= 0x100;
      }
      return val;
    };
    Buffer.prototype.readUIntBE = function (offset, byteLength) {
      offset = offset === undefined ? 0 : offset;
      checkVarWidth(this, offset, byteLength);
      var val = 0;
      for (var i = 0; i < byteLength; i++) val = val * 0x100 + this[offset + i];
      return val;
    };
    Buffer.prototype.readIntLE = function (offset, byteLength) {
      var val = this.readUIntLE(offset, byteLength);
      var max = Math.pow(2, 8 * byteLength);
      if (val >= max / 2) val -= max;
      return val;
    };
    Buffer.prototype.readIntBE = function (offset, byteLength) {
      var val = this.readUIntBE(offset, byteLength);
      var max = Math.pow(2, 8 * byteLength);
      if (val >= max / 2) val -= max;
      return val;
    };
    Buffer.prototype.writeUIntLE = function (value, offset, byteLength) {
      offset = offset === undefined ? 0 : offset;
      checkVarWidth(this, offset, byteLength);
      // The max is inclusive (2^(8·byteLength) − 1), so a fractional value just
      // above the max (e.g. 255.9 for 1 byte) is out of range, not truncated.
      if (value < 0 || value > Math.pow(2, 8 * byteLength) - 1) {
        throw new RangeError('"value" is out of range');
      }
      var v = value;
      for (var i = 0; i < byteLength; i++) {
        this[offset + i] = v % 0x100;
        v = Math.floor(v / 0x100);
      }
      return offset + byteLength;
    };
    Buffer.prototype.writeUIntBE = function (value, offset, byteLength) {
      offset = offset === undefined ? 0 : offset;
      checkVarWidth(this, offset, byteLength);
      if (value < 0 || value > Math.pow(2, 8 * byteLength) - 1) {
        throw new RangeError('"value" is out of range');
      }
      var v = value;
      for (var i = byteLength - 1; i >= 0; i--) {
        this[offset + i] = v % 0x100;
        v = Math.floor(v / 0x100);
      }
      return offset + byteLength;
    };
    Buffer.prototype.writeIntLE = function (value, offset, byteLength) {
      var limit = Math.pow(2, 8 * byteLength - 1);
      if (value < -limit || value > limit - 1) {
        throw new RangeError('"value" is out of range');
      }
      if (value < 0) value += Math.pow(2, 8 * byteLength);
      return this.writeUIntLE(value, offset, byteLength);
    };
    Buffer.prototype.writeIntBE = function (value, offset, byteLength) {
      var limit = Math.pow(2, 8 * byteLength - 1);
      if (value < -limit || value > limit - 1) {
        throw new RangeError('"value" is out of range');
      }
      if (value < 0) value += Math.pow(2, 8 * byteLength);
      return this.writeUIntBE(value, offset, byteLength);
    };

    // Node's Buffer#inspect: `<Buffer 01 02 …>` (lowercase hex, up to
    // INSPECT_MAX_BYTES=50 bytes, then a "… N more byte(s)" tail).
    Buffer.prototype.inspect = function inspect() {
      var max = 50;
      var n = this.length < max ? this.length : max;
      var parts = [];
      for (var i = 0; i < n; i++) {
        var h = this[i].toString(16);
        parts.push(h.length < 2 ? "0" + h : h);
      }
      var body = parts.join(" ");
      if (this.length > max) {
        var extra = this.length - max;
        body += " ... " + extra + " more byte" + (extra > 1 ? "s" : "");
      }
      return "<Buffer " + body + ">";
    };

    // Native Buffer's public static methods are enumerable own properties.
    // Class syntax defines statics as non-enumerable, so normalize the methods
    // implemented above. Packages such as safer-buffer intentionally discover
    // and copy this surface with `for (key in Buffer)`.
    [
      "from",
      "alloc",
      "allocUnsafe",
      "allocUnsafeSlow",
      "isBuffer",
      "isEncoding",
      "byteLength",
      "concat",
      "compare",
    ].forEach(function (name) {
      var descriptor = Object.getOwnPropertyDescriptor(Buffer, name);
      if (descriptor && !descriptor.enumerable) {
        descriptor.enumerable = true;
        Object.defineProperty(Buffer, name, descriptor);
      }
    });

    // Note: the legacy `SlowBuffer` global is intentionally NOT defined — it was
    // removed from Node (undefined in the `buffer` module of current releases),
    // so adding it would diverge from the reference engine. Buffer.allocUnsafeSlow
    // (still a documented Node API) remains available as the static method.
    globalThis.Buffer = Buffer;
  }
})();
