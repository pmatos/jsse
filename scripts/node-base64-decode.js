// Node-compatible base64 / base64url string decoder for the Buffer shim (#1328).
//
// Node decodes a UTF-16 string by (1) trying the WHATWG forgiving-base64
// decoder and, when the input is not valid, (2) falling back to the legacy
// decoder in nbytes (`Base64Decode`). Every input the first accepts decodes
// identically under the second, so this is a port of the second alone:
//
//   - each UTF-16 code unit is truncated to its low byte before the alphabet
//     lookup, so U+0161 reads as 'a', U+013D as '=' and U+D83D (a high
//     surrogate) as '=' as well. A non-alphabet byte is skipped, but '=' stops
//     decoding for good;
//   - output is capped at `dstlen` bytes, which is `Buffer.byteLength`'s
//     estimate for Buffer.from and the remaining room for Buffer#write.
//
// The Buffer shim consumes the helper from `globalThis.__node_base64` and
// removes it. See tests/node_buffer_base64.rs.
(function () {
  "use strict";

  var T = new Uint8Array(256).fill(255);
  var ALPHA =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  for (var n = 0; n < 64; n++) T[ALPHA.charCodeAt(n)] = n;
  T[0x2d] = 62; // '-'
  T[0x5f] = 63; // '_'

  // Buffer.byteLength(str, "base64"): a length-based estimate, not a decode.
  function byteLength(str) {
    var len = str.length;
    if (len > 0 && str.charCodeAt(len - 1) === 0x3d) len--;
    if (len > 1 && str.charCodeAt(len - 1) === 0x3d) len--;
    return (len * 3) >>> 2;
  }

  function decodedSize(str) {
    var size = str.length;
    if (size < 2) return 0;
    if (str.charCodeAt(size - 1) === 0x3d) {
      size--;
      if (str.charCodeAt(size - 1) === 0x3d) size--;
    }
    return size > 1 ? (size >>> 2) * 3 + ((size % 4) + 1 >>> 1) : 0;
  }

  function decode(str, dstlen) {
    var srclen = str.length;
    var dsize = decodedSize(str);
    var dst = new Uint8Array(Math.min(dstlen, (srclen >>> 2) * 3 + 3));
    var i = 0;
    var k = 0;

    // One 4-character group, skipping non-alphabet characters. Returns false
    // when decoding must stop (a '=', the end of input or of the output).
    function group() {
      var hi = 0;
      var lo;
      var c;
      for (var step = 0; step < 4; step++) {
        for (;;) {
          c = str.charCodeAt(i) & 0xff;
          lo = T[c];
          i++;
          if (lo < 64) break;
          if (c === 0x3d || i >= srclen) return false;
        }
        if (step === 1) dst[k++] = ((hi & 0x3f) << 2) | ((lo & 0x30) >> 4);
        else if (step === 2) dst[k++] = ((hi & 0x0f) << 4) | ((lo & 0x3c) >> 2);
        else if (step === 3) dst[k++] = ((hi & 0x03) << 6) | (lo & 0x3f);
        if (i >= srclen || k >= dstlen) return false;
        hi = lo;
      }
      return true;
    }

    var maxK = Math.floor(Math.min(dstlen, dsize) / 3) * 3;
    var maxI = (srclen >>> 2) << 2;
    while (i < maxI && k < maxK) {
      var a = T[str.charCodeAt(i) & 0xff];
      var b = T[str.charCodeAt(i + 1) & 0xff];
      var c2 = T[str.charCodeAt(i + 2) & 0xff];
      var d = T[str.charCodeAt(i + 3) & 0xff];
      if ((a | b | c2 | d) & 0x80) {
        if (!group()) return dst.subarray(0, k);
        maxI = i + (((srclen - i) >>> 2) << 2);
      } else {
        dst[k++] = (a << 2) | (b >> 4);
        dst[k++] = (b << 4) | (c2 >> 2);
        dst[k++] = (c2 << 6) | d;
        i += 4;
      }
    }
    if (i < srclen && k < dstlen) group();
    return dst.subarray(0, k);
  }

  Object.defineProperty(globalThis, "__node_base64", {
    value: { decode: decode, byteLength: byteLength },
    writable: true,
    configurable: true,
    enumerable: false,
  });
})();
