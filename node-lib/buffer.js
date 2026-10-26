// Copyright Joyent, Inc. and other Node contributors.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit
// persons to whom the Software is furnished to do so, subject to the
// following conditions:
//
// The above copyright notice and this permission notice shall be included
// in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN
// NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
// OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
// USE OR OTHER DEALINGS IN THE SOFTWARE.


'use strict';

// The `buffer` module around the prelude's global Buffer (scripts/node-buffer-shim.js),
// shaped after Node v26.8.2 lib/buffer.js. `Buffer`, `atob` and `btoa` are the
// global values; `transcode`, `isUtf8` and `isAscii` are written over JavaScript
// strings and bytes instead of ICU and simdutf. `Blob` and `File` are in
// internal/blob.js. `SlowBuffer` is gone in Node v26.

const {
  ObjectDefineProperties,
  ObjectDefineProperty,
  ReflectApply,
  StringFromCharCode,
  Uint16Array,
  Uint8Array,
} = primordials;

const { Buffer, atob, btoa } = require('internal/host/buffer');
const {
  codes: {
    ERR_INVALID_ARG_TYPE,
  },
  genericNodeError,
} = require('internal/errors');
const { validateNumber } = require('internal/validators');
const { isAnyArrayBuffer, isTypedArray, isUint8Array } = require('internal/util/types');
const { Blob, File, resolveObjectURL } = require('internal/blob');
const { normalizeEncoding } = require('internal/string_decoder');
const inspectState = require('internal/buffer_inspect').state;

const kMaxLength = 2 ** 53 - 1;
const kStringMaxLength = 536870888;
const constants = ObjectDefineProperties({}, {
  MAX_LENGTH: { __proto__: null, value: kMaxLength, enumerable: true },
  MAX_STRING_LENGTH: { __proto__: null, value: kStringMaxLength, enumerable: true },
});

function bytesOf(input) {
  return isTypedArray(input) ?
    new Uint8Array(input.buffer, input.byteOffset, input.byteLength) :
    new Uint8Array(input);
}

function checkInput(input) {
  if (isTypedArray(input) || isAnyArrayBuffer(input)) return bytesOf(input);
  throw new ERR_INVALID_ARG_TYPE('input', ['ArrayBuffer', 'Buffer', 'TypedArray'], input);
}

function isAscii(input) {
  const bytes = checkInput(input);
  for (let i = 0; i < bytes.length; i++) {
    if (bytes[i] > 0x7f) return false;
  }
  return true;
}

function isUtf8(input) {
  const bytes = checkInput(input);
  const length = bytes.length;
  for (let i = 0; i < length;) {
    const b = bytes[i];
    if (b < 0x80) {
      i++;
      continue;
    }
    let need;
    let min;
    if (b >= 0xc2 && b <= 0xdf) {
      need = 1;
      min = 0x80;
    } else if (b >= 0xe0 && b <= 0xef) {
      need = 2;
      min = 0x800;
    } else if (b >= 0xf0 && b <= 0xf4) {
      need = 3;
      min = 0x10000;
    } else {
      return false;
    }
    if (i + need >= length) return false;
    let code = b & (0x3f >> need);
    for (let k = 1; k <= need; k++) {
      const c = bytes[i + k];
      if ((c & 0xc0) !== 0x80) return false;
      code = (code << 6) | (c & 0x3f);
    }
    if (code < min || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) {
      return false;
    }
    i += need + 1;
  }
  return true;
}

const transcodable = { __proto__: null, utf16le: true, utf8: true, latin1: true, ascii: true };

function transcodeError(code, errno) {
  return genericNodeError(`Unable to transcode Buffer [${code}]`, { code, errno });
}

function decodeFrom(source, encoding) {
  if (encoding === 'ascii') {
    const codes = new Uint16Array(source.length);
    for (let i = 0; i < source.length; i++) {
      codes[i] = source[i] > 0x7f ? 0xfffd : source[i];
    }
    let text = '';
    for (let i = 0; i < codes.length; i += 8192) {
      text += ReflectApply(StringFromCharCode, null, codes.subarray(i, i + 8192));
    }
    return text;
  }
  return Buffer.from(source.buffer, source.byteOffset, source.byteLength)
    .toString(encoding);
}

function wellFormedUtf16(text) {
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const d = text.charCodeAt(i + 1);
      if (!(d >= 0xdc00 && d <= 0xdfff)) return false;
      i++;
    } else if (c >= 0xdc00 && c <= 0xdfff) {
      return false;
    }
  }
  return true;
}

function encodeTo(text, encoding) {
  if (encoding === 'utf8' || encoding === 'utf16le') {
    return Buffer.from(text, encoding);
  }
  const limit = encoding === 'ascii' ? 0x7f : 0xff;
  const out = new Uint8Array(text.length);
  let n = 0;
  for (let i = 0; i < text.length; i++) {
    const code = text.codePointAt(i);
    if (code > 0xffff) i++;
    // ICU drops U+FFF8 on this path.
    if (code === 0xfff8) continue;
    out[n++] = code <= limit ? code : 0x3f;
  }
  return Buffer.from(out.buffer, 0, n);
}

// Node routes some pairs around ICU, and those paths fail on malformed input
// (U_INVALID_CHAR_FOUND) where the ICU converters substitute.
function transcode(source, fromEncoding, toEncoding) {
  if (!isUint8Array(source)) {
    throw new ERR_INVALID_ARG_TYPE('source', ['Buffer', 'Uint8Array'], source);
  }
  if (source.length === 0) return Buffer.alloc(0);

  fromEncoding = normalizeEncoding(fromEncoding) || fromEncoding;
  toEncoding = normalizeEncoding(toEncoding) || toEncoding;
  if (!(fromEncoding in transcodable) || !(toEncoding in transcodable)) {
    throw transcodeError('U_ILLEGAL_ARGUMENT_ERROR', 1);
  }
  const latin = fromEncoding === 'latin1' || fromEncoding === 'ascii';
  if (latin && toEncoding === 'utf16le') {
    const out = Buffer.alloc(source.length * 2);
    for (let i = 0; i < source.length; i++) out[i * 2] = source[i];
    return out;
  }
  let text = decodeFrom(source, fromEncoding);
  if (fromEncoding === 'utf8' && toEncoding === 'utf16le') {
    if (!isUtf8(source)) throw transcodeError('U_INVALID_CHAR_FOUND', 10);
  } else if (fromEncoding === 'utf16le') {
    if (toEncoding === 'utf8' &&
        (source.length < 2 || !wellFormedUtf16(text))) {
      throw transcodeError('U_INVALID_CHAR_FOUND', 10);
    }
    if (toEncoding === 'utf16le') {
      const last = text.charCodeAt(text.length - 1);
      const tail = source.length % 2 === 1 && !(last >= 0xd800 && last <= 0xdbff);
      text = text.toWellFormed() + (tail ? '\ufffd' : '');
    }
  }
  return encodeTo(text, toEncoding);
}

module.exports = {
  Buffer,
  transcode,
  isUtf8,
  isAscii,
  kMaxLength,
  kStringMaxLength,
  btoa,
  atob,
  constants: undefined,
  INSPECT_MAX_BYTES: undefined,
  Blob,
  resolveObjectURL,
  File,
};

ObjectDefineProperty(module.exports, 'constants', {
  __proto__: null,
  value: constants,
  writable: false,
  enumerable: true,
  configurable: false,
});

ObjectDefineProperty(module.exports, 'INSPECT_MAX_BYTES', {
  __proto__: null,
  configurable: true,
  enumerable: true,
  get() {
    return inspectState.max;
  },
  set(val) {
    validateNumber(val, 'INSPECT_MAX_BYTES', 0);
    inspectState.max = val;
  },
});
