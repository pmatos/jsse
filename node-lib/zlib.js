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

// Node's `zlib` for `--node` (issue #1239): lib/zlib.js (v26.8.2) split across
// internal/zlib/{base,classes,binding}.js, over a native z_stream binding
// (libz-rs-sys) and the `brotli` crate. Zstd and the experimental ZIP archive
// API are not provided. See docs/adr/2026-10-27-0450-node-host-zlib.md.

const {
  ObjectDefineProperties,
  ObjectDefineProperty,
  ObjectEntries,
  ObjectFreeze,
  ObjectKeys,
} = primordials;

const {
  codes: { ERR_INVALID_ARG_TYPE },
} = require('internal/errors');
const { assignFunctionName } = require('internal/util');
const { isArrayBufferView } = require('internal/util/types');
const { validateUint32 } = require('internal/validators');
const { crc32: crc32Native } = require('internal/zlib/binding');
const { zlibBuffer, zlibBufferSync } = require('internal/zlib/base');
const {
  BrotliCompress,
  BrotliDecompress,
  Deflate,
  DeflateRaw,
  Gunzip,
  Gzip,
  Inflate,
  InflateRaw,
  Unzip,
} = require('internal/zlib/classes');
const constants = require('internal/zlib/constants');

// Translation table for return codes.
const codes = {
  Z_OK: constants.Z_OK,
  Z_STREAM_END: constants.Z_STREAM_END,
  Z_NEED_DICT: constants.Z_NEED_DICT,
  Z_ERRNO: constants.Z_ERRNO,
  Z_STREAM_ERROR: constants.Z_STREAM_ERROR,
  Z_DATA_ERROR: constants.Z_DATA_ERROR,
  Z_MEM_ERROR: constants.Z_MEM_ERROR,
  Z_BUF_ERROR: constants.Z_BUF_ERROR,
  Z_VERSION_ERROR: constants.Z_VERSION_ERROR,
};

for (const ckey of ObjectKeys(codes)) {
  codes[codes[ckey]] = ckey;
}

function createConvenienceMethod(ctor, sync) {
  if (sync) {
    return function syncBufferWrapper(buffer, opts) {
      return zlibBufferSync(new ctor(opts), buffer);
    };
  }
  return function asyncBufferWrapper(buffer, opts, callback) {
    if (typeof opts === 'function') {
      callback = opts;
      opts = {};
    }
    return zlibBuffer(new ctor(opts), buffer, callback);
  };
}


function createProperty(ctor) {
  return {
    __proto__: null,
    configurable: true,
    enumerable: true,
    value: assignFunctionName(`create${ctor.name}`, function(options) {
      return new ctor(options);
    }),
  };
}


function crc32(data, value = 0) {
  if (typeof data !== 'string' && !isArrayBufferView(data)) {
    throw new ERR_INVALID_ARG_TYPE('data', ['Buffer', 'TypedArray', 'DataView', 'string'], data);
  }
  validateUint32(value, 'value');
  // Coerce -0 to +0.
  value += 0;
  return crc32Native(data, value);
}

module.exports = {
  crc32,
  Deflate,
  Inflate,
  Gzip,
  Gunzip,
  DeflateRaw,
  InflateRaw,
  Unzip,
  BrotliCompress,
  BrotliDecompress,

  // Convenience methods.
  // compress/decompress a string or buffer in one step.
  deflate: createConvenienceMethod(Deflate, false),
  deflateSync: createConvenienceMethod(Deflate, true),
  gzip: createConvenienceMethod(Gzip, false),
  gzipSync: createConvenienceMethod(Gzip, true),
  deflateRaw: createConvenienceMethod(DeflateRaw, false),
  deflateRawSync: createConvenienceMethod(DeflateRaw, true),
  unzip: createConvenienceMethod(Unzip, false),
  unzipSync: createConvenienceMethod(Unzip, true),
  inflate: createConvenienceMethod(Inflate, false),
  inflateSync: createConvenienceMethod(Inflate, true),
  gunzip: createConvenienceMethod(Gunzip, false),
  gunzipSync: createConvenienceMethod(Gunzip, true),
  inflateRaw: createConvenienceMethod(InflateRaw, false),
  inflateRawSync: createConvenienceMethod(InflateRaw, true),
  brotliCompress: createConvenienceMethod(BrotliCompress, false),
  brotliCompressSync: createConvenienceMethod(BrotliCompress, true),
  brotliDecompress: createConvenienceMethod(BrotliDecompress, false),
  brotliDecompressSync: createConvenienceMethod(BrotliDecompress, true),
};

ObjectDefineProperties(module.exports, {
  createDeflate: createProperty(Deflate),
  createInflate: createProperty(Inflate),
  createDeflateRaw: createProperty(DeflateRaw),
  createInflateRaw: createProperty(InflateRaw),
  createGzip: createProperty(Gzip),
  createGunzip: createProperty(Gunzip),
  createUnzip: createProperty(Unzip),
  createBrotliCompress: createProperty(BrotliCompress),
  createBrotliDecompress: createProperty(BrotliDecompress),
  constants: {
    __proto__: null,
    configurable: false,
    enumerable: true,
    value: constants,
  },
  codes: {
    __proto__: null,
    enumerable: true,
    writable: false,
    value: ObjectFreeze(codes),
  },
});

// These should be considered deprecated
// expose all the zlib constants
for (const { 0: key, 1: value } of ObjectEntries(constants)) {
  if (key.startsWith('BROTLI')) continue;
  ObjectDefineProperty(module.exports, key, {
    __proto__: null,
    enumerable: false,
    value,
    writable: false,
  });
}
