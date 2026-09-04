// CommonJS `string_decoder` selector for bundled library dependencies.
//
// iconv-lite's internal codec module imports StringDecoder even when qs only
// selects a legacy DBCS codec. The JSSE fallback covers the buffered write/end
// surface; Node retains its native implementation.
//
// The buffered-state algorithm is derived from Node.js's MIT-licensed
// lib/string_decoder.js:
//
// Copyright Joyent, Inc. and other Node contributors.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

if (typeof __host_write !== "undefined") {
  function decoderKind(encoding) {
    switch (String(encoding).toLowerCase()) {
      case "utf8":
      case "utf-8":
        return "utf8";
      case "ucs2":
      case "ucs-2":
      case "utf16le":
      case "utf-16le":
        return "utf16le";
      case "base64":
      case "base64url":
        return "base64";
      default:
        return "stateless";
    }
  }

  function copyBytes(source, target, targetStart, sourceStart, sourceEnd) {
    for (var i = sourceStart; i < sourceEnd; i++) {
      target[targetStart++] = source[i];
    }
  }

  function utf8CheckByte(byte) {
    if (byte <= 0x7f) return 0;
    if (byte >> 5 === 0x06) return 2;
    if (byte >> 4 === 0x0e) return 3;
    if (byte >> 3 === 0x1e) return 4;
    return byte >> 6 === 0x02 ? -1 : -2;
  }

  function utf8CheckIncomplete(decoder, bytes, start) {
    var index = bytes.length - 1;
    if (index < start) return 0;
    var size = utf8CheckByte(bytes[index]);
    if (size >= 0) {
      if (size > 0) decoder._lastNeed = size - 1;
      return size;
    }
    if (--index < start || size === -2) return 0;
    size = utf8CheckByte(bytes[index]);
    if (size >= 0) {
      if (size > 0) decoder._lastNeed = size - 2;
      return size;
    }
    if (--index < start || size === -2) return 0;
    size = utf8CheckByte(bytes[index]);
    if (size >= 0) {
      if (size > 0) {
        if (size === 2) size = 0;
        else decoder._lastNeed = size - 3;
      }
      return size;
    }
    return 0;
  }

  function fillLastBytes(decoder, bytes) {
    var targetStart = decoder._lastTotal - decoder._lastNeed;
    if (decoder._lastNeed <= bytes.length) {
      copyBytes(bytes, decoder._lastChar, targetStart, 0, decoder._lastNeed);
      return decoder._lastChar
        .subarray(0, decoder._lastTotal)
        .toString(decoder.encoding);
    }
    copyBytes(bytes, decoder._lastChar, targetStart, 0, bytes.length);
    decoder._lastNeed -= bytes.length;
  }

  function utf8Text(decoder, bytes, start) {
    var total = utf8CheckIncomplete(decoder, bytes, start);
    if (!decoder._lastNeed) return bytes.toString("utf8", start);
    decoder._lastTotal = total;
    var end = bytes.length - (total - decoder._lastNeed);
    copyBytes(bytes, decoder._lastChar, 0, end, bytes.length);
    return bytes.toString("utf8", start, end);
  }

  function utf16Text(decoder, bytes, start) {
    if ((bytes.length - start) % 2 === 0) {
      var out = bytes.toString(decoder.encoding, start);
      if (out) {
        var codeUnit = out.charCodeAt(out.length - 1);
        if (codeUnit >= 0xd800 && codeUnit <= 0xdbff) {
          decoder._lastNeed = 2;
          decoder._lastTotal = 4;
          decoder._lastChar[0] = bytes[bytes.length - 2];
          decoder._lastChar[1] = bytes[bytes.length - 1];
          return out.slice(0, -1);
        }
      }
      return out;
    }
    decoder._lastNeed = 1;
    decoder._lastTotal = 2;
    decoder._lastChar[0] = bytes[bytes.length - 1];
    return bytes.toString(decoder.encoding, start, bytes.length - 1);
  }

  function base64Text(decoder, bytes, start) {
    var trailing = (bytes.length - start) % 3;
    if (trailing === 0) return bytes.toString(decoder.encoding, start);
    decoder._lastNeed = 3 - trailing;
    decoder._lastTotal = 3;
    if (trailing === 1) {
      decoder._lastChar[0] = bytes[bytes.length - 1];
    } else {
      decoder._lastChar[0] = bytes[bytes.length - 2];
      decoder._lastChar[1] = bytes[bytes.length - 1];
    }
    return bytes.toString(decoder.encoding, start, bytes.length - trailing);
  }

  function statefulText(decoder, bytes, start) {
    if (decoder._kind === "utf8") return utf8Text(decoder, bytes, start);
    if (decoder._kind === "utf16le") return utf16Text(decoder, bytes, start);
    return base64Text(decoder, bytes, start);
  }

  function StringDecoder(encoding) {
    this.encoding = encoding || "utf8";
    this._kind = decoderKind(this.encoding);
    if (this._kind !== "stateless") {
      this._lastNeed = 0;
      this._lastTotal = 0;
      this._lastChar = globalThis.Buffer.alloc(
        this._kind === "base64" ? 3 : 4
      );
    }
  }

  StringDecoder.prototype.write = function (buffer) {
    var bytes = globalThis.Buffer.from(buffer);
    if (this._kind === "stateless") return bytes.toString(this.encoding);
    if (bytes.length === 0) return "";

    if (this._kind === "utf8") {
      if (this._lastNeed) {
        var buffered = this._lastTotal - this._lastNeed;
        var combined = globalThis.Buffer.alloc(buffered + bytes.length);
        copyBytes(this._lastChar, combined, 0, 0, buffered);
        copyBytes(bytes, combined, buffered, 0, bytes.length);
        bytes = combined;
        this._lastNeed = 0;
      }
      return utf8Text(this, bytes, 0);
    }

    var out;
    var start;
    if (this._lastNeed) {
      out = fillLastBytes(this, bytes);
      if (out === undefined) return "";
      start = this._lastNeed;
      this._lastNeed = 0;
    } else {
      start = 0;
    }
    if (start < bytes.length) {
      var rest = statefulText(this, bytes, start);
      return out ? out + rest : rest;
    }
    return out || "";
  };

  StringDecoder.prototype.end = function (buffer) {
    var out = buffer === undefined ? "" : this.write(buffer);
    if (this._kind === "stateless" || !this._lastNeed) return out;
    out += this._lastChar
      .subarray(0, this._lastTotal - this._lastNeed)
      .toString(this.encoding);
    this._lastNeed = 0;
    this._lastTotal = 0;
    return out;
  };
  module.exports = { StringDecoder: StringDecoder };
} else {
  module.exports = require("node:string_decoder");
}
