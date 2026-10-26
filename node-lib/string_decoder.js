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

// Node v26.8.2 lib/string_decoder.js (MIT; license above). The native decoder
// is internal/string_decoder.js over the prelude's Buffer.

const {
  ArrayBufferIsView,
  ObjectDefineProperties,
  Symbol,
  TypedArrayPrototypeSubarray,
} = primordials;

const { Buffer } = require('buffer');
const {
  kIncompleteCharactersStart,
  kIncompleteCharactersEnd,
  kMissingBytes,
  kBufferedBytes,
  kEncodingField,
  kSize,
  decode,
  flush,
  encodings,
  normalizeEncoding,
} = require('internal/string_decoder');
const {
  codes: {
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_THIS,
    ERR_UNKNOWN_ENCODING,
  },
} = require('internal/errors');

const kNativeDecoder = Symbol('kNativeDecoder');

const encodingsMap = {};
for (let i = 0; i < encodings.length; ++i)
  encodingsMap[encodings[i]] = i;

function normalizeEncodingOrThrow(enc) {
  const nenc = normalizeEncoding(enc);
  if (nenc === undefined) {
    if (!Buffer.isEncoding(enc))
      throw new ERR_UNKNOWN_ENCODING(enc);
    return enc;
  }
  return nenc;
}

/**
 * StringDecoder provides an interface for efficiently splitting a series of
 * buffers into a series of JS strings without breaking apart multi-byte
 * characters.
 */
function StringDecoder(encoding) {
  this.encoding = normalizeEncodingOrThrow(encoding);
  this[kNativeDecoder] = Buffer.alloc(kSize);
  this[kNativeDecoder][kEncodingField] = encodingsMap[this.encoding];
}

StringDecoder.prototype.write = function write(buf) {
  if (typeof buf === 'string')
    return buf;
  if (!ArrayBufferIsView(buf))
    throw new ERR_INVALID_ARG_TYPE('buf',
                                   ['Buffer', 'TypedArray', 'DataView'],
                                   buf);
  if (!this[kNativeDecoder]) {
    throw new ERR_INVALID_THIS('StringDecoder');
  }
  return decode(this[kNativeDecoder], buf);
};

StringDecoder.prototype.end = function end(buf) {
  const ret = buf === undefined ? '' : this.write(buf);
  if (this[kNativeDecoder][kBufferedBytes] > 0)
    return ret + flush(this[kNativeDecoder]);
  return ret;
};

/* Everything below this line is undocumented legacy stuff. */
StringDecoder.prototype.text = function text(buf, offset) {
  this[kNativeDecoder][kMissingBytes] = 0;
  this[kNativeDecoder][kBufferedBytes] = 0;
  return this.write(buf.slice(offset));
};

ObjectDefineProperties(StringDecoder.prototype, {
  lastChar: {
    __proto__: null,
    configurable: true,
    enumerable: true,
    get() {
      return TypedArrayPrototypeSubarray(this[kNativeDecoder],
                                         kIncompleteCharactersStart,
                                         kIncompleteCharactersEnd);
    },
  },
  lastNeed: {
    __proto__: null,
    configurable: true,
    enumerable: true,
    get() {
      return this[kNativeDecoder][kMissingBytes];
    },
  },
  lastTotal: {
    __proto__: null,
    configurable: true,
    enumerable: true,
    get() {
      return this[kNativeDecoder][kBufferedBytes] +
             this[kNativeDecoder][kMissingBytes];
    },
  },
});

module.exports = {
  StringDecoder,
};
