'use strict';

// Zlib, its subclasses and the Brotli streams: Node's lib/zlib.js (v26.8.2)
// from `Zlib` through `BrotliDecompress`, over the handles of
// internal/zlib/binding. Zstd is not provided. See
// docs/adr/2026-10-27-0450-node-host-zlib.md.

const {
  MathMax,
  NumberIsNaN,
  ObjectEntries,
  ObjectKeys,
  ObjectSetPrototypeOf,
  Uint32Array,
} = primordials;

require('internal/zlib/errors');
const {
  codes: {
    ERR_BROTLI_INVALID_PARAM,
    ERR_INVALID_ARG_TYPE,
  },
} = require('internal/errors');
const {
  isArrayBufferView,
  isAnyArrayBuffer,
} = require('internal/util/types');
const binding = require('internal/zlib/binding');
const assert = require('internal/assert');
const { Buffer } = require('buffer');
const { ZlibBase, processCallback } = require('internal/zlib/base');
const { checkRangesOrGetDefault, deprecateInstantiation } = require('internal/zlib/util');
const constants = require('internal/zlib/constants');
const {
  Z_NO_FLUSH, Z_SYNC_FLUSH, Z_FULL_FLUSH, Z_FINISH,
  Z_MIN_WINDOWBITS, Z_MAX_WINDOWBITS, Z_MIN_LEVEL, Z_MAX_LEVEL,
  Z_MIN_MEMLEVEL, Z_MAX_MEMLEVEL, Z_DEFAULT_COMPRESSION,
  Z_DEFAULT_STRATEGY, Z_DEFAULT_WINDOWBITS, Z_DEFAULT_MEMLEVEL, Z_FIXED,
  DEFLATE, DEFLATERAW, INFLATE, INFLATERAW, GZIP, GUNZIP, UNZIP,
  BROTLI_DECODE, BROTLI_ENCODE,
  BROTLI_OPERATION_PROCESS, BROTLI_OPERATION_FLUSH,
  BROTLI_OPERATION_FINISH,
} = constants;

const zlibDefaultOpts = {
  flush: Z_NO_FLUSH,
  finishFlush: Z_FINISH,
  fullFlush: Z_FULL_FLUSH,
};
// Base class for all streams actually backed by zlib and using zlib-specific
// parameters.
function Zlib(opts, mode) {
  let windowBits = Z_DEFAULT_WINDOWBITS;
  let level = Z_DEFAULT_COMPRESSION;
  let memLevel = Z_DEFAULT_MEMLEVEL;
  let strategy = Z_DEFAULT_STRATEGY;
  let dictionary;

  if (opts) {
    // windowBits is special. On the compression side, 0 is an invalid value.
    // But on the decompression side, a value of 0 for windowBits tells zlib
    // to use the window size in the zlib header of the compressed stream.
    if ((opts.windowBits == null || opts.windowBits === 0) &&
        (mode === INFLATE ||
         mode === GUNZIP ||
         mode === UNZIP)) {
      windowBits = 0;
    } else {
      // `{ windowBits: 8 }` is valid for deflate but not gzip.
      const min = Z_MIN_WINDOWBITS + (mode === GZIP ? 1 : 0);
      windowBits = checkRangesOrGetDefault(
        opts.windowBits, 'options.windowBits',
        min, Z_MAX_WINDOWBITS, Z_DEFAULT_WINDOWBITS);
    }

    level = checkRangesOrGetDefault(
      opts.level, 'options.level',
      Z_MIN_LEVEL, Z_MAX_LEVEL, Z_DEFAULT_COMPRESSION);

    memLevel = checkRangesOrGetDefault(
      opts.memLevel, 'options.memLevel',
      Z_MIN_MEMLEVEL, Z_MAX_MEMLEVEL, Z_DEFAULT_MEMLEVEL);

    strategy = checkRangesOrGetDefault(
      opts.strategy, 'options.strategy',
      Z_DEFAULT_STRATEGY, Z_FIXED, Z_DEFAULT_STRATEGY);

    dictionary = opts.dictionary;
    if (dictionary !== undefined && !isArrayBufferView(dictionary)) {
      if (isAnyArrayBuffer(dictionary)) {
        dictionary = Buffer.from(dictionary);
      } else {
        throw new ERR_INVALID_ARG_TYPE(
          'options.dictionary',
          ['Buffer', 'TypedArray', 'DataView', 'ArrayBuffer'],
          dictionary,
        );
      }
    }
  }

  const handle = new binding.Zlib(mode);
  // Ideally, we could let ZlibBase() set up _writeState. I haven't been able
  // to come up with a good solution that doesn't break our internal API,
  // and with it all supported npm versions at the time of writing.
  this._writeState = new Uint32Array(2);
  handle.init(windowBits,
              level,
              memLevel,
              strategy,
              this._writeState,
              processCallback,
              dictionary,
              opts?.rejectGarbageAfterEnd === true);

  ZlibBase.call(this, opts, mode, handle, zlibDefaultOpts);

  this._level = level;
  this._strategy = strategy;
  this._mode = mode;
}
ObjectSetPrototypeOf(Zlib.prototype, ZlibBase.prototype);
ObjectSetPrototypeOf(Zlib, ZlibBase);

// This callback is used by `.params()` to wait until a full flush happened
// before adjusting the parameters. In particular, the call to the native
// `params()` function should not happen while a write is currently in progress
// on the threadpool.
function paramsAfterFlushCallback(level, strategy, callback) {
  assert(this._handle, 'zlib binding closed');
  this._handle.params(level, strategy);
  if (!this.destroyed) {
    this._level = level;
    this._strategy = strategy;
    if (callback) callback();
  }
}

Zlib.prototype.params = function params(level, strategy, callback) {
  checkRangesOrGetDefault(level, 'level', Z_MIN_LEVEL, Z_MAX_LEVEL);
  checkRangesOrGetDefault(strategy, 'strategy', Z_DEFAULT_STRATEGY, Z_FIXED);

  if (this._level !== level || this._strategy !== strategy) {
    this.flush(
      Z_SYNC_FLUSH,
      paramsAfterFlushCallback.bind(this, level, strategy, callback),
    );
  } else {
    process.nextTick(callback);
  }
};

// generic zlib
// minimal 2-byte header
function Deflate(opts) {
  if (!(this instanceof Deflate)) {
    return deprecateInstantiation(Deflate, 'DEP0184', opts);
  }
  Zlib.call(this, opts, DEFLATE);
}
ObjectSetPrototypeOf(Deflate.prototype, Zlib.prototype);
ObjectSetPrototypeOf(Deflate, Zlib);

function Inflate(opts) {
  if (!(this instanceof Inflate)) {
    return deprecateInstantiation(Inflate, 'DEP0184', opts);
  }
  Zlib.call(this, opts, INFLATE);
}
ObjectSetPrototypeOf(Inflate.prototype, Zlib.prototype);
ObjectSetPrototypeOf(Inflate, Zlib);

function Gzip(opts) {
  if (!(this instanceof Gzip)) {
    return deprecateInstantiation(Gzip, 'DEP0184', opts);
  }
  Zlib.call(this, opts, GZIP);
}
ObjectSetPrototypeOf(Gzip.prototype, Zlib.prototype);
ObjectSetPrototypeOf(Gzip, Zlib);

function Gunzip(opts) {
  if (!(this instanceof Gunzip)) {
    return deprecateInstantiation(Gunzip, 'DEP0184', opts);
  }
  Zlib.call(this, opts, GUNZIP);
}
ObjectSetPrototypeOf(Gunzip.prototype, Zlib.prototype);
ObjectSetPrototypeOf(Gunzip, Zlib);

function DeflateRaw(opts) {
  if (opts && opts.windowBits === 8) opts.windowBits = 9;
  if (!(this instanceof DeflateRaw)) {
    return deprecateInstantiation(DeflateRaw, 'DEP0184', opts);
  }
  Zlib.call(this, opts, DEFLATERAW);
}
ObjectSetPrototypeOf(DeflateRaw.prototype, Zlib.prototype);
ObjectSetPrototypeOf(DeflateRaw, Zlib);

function InflateRaw(opts) {
  if (!(this instanceof InflateRaw)) {
    return deprecateInstantiation(InflateRaw, 'DEP0184', opts);
  }
  Zlib.call(this, opts, INFLATERAW);
}
ObjectSetPrototypeOf(InflateRaw.prototype, Zlib.prototype);
ObjectSetPrototypeOf(InflateRaw, Zlib);

function Unzip(opts) {
  if (!(this instanceof Unzip)) {
    return deprecateInstantiation(Unzip, 'DEP0184', opts);
  }
  Zlib.call(this, opts, UNZIP);
}
ObjectSetPrototypeOf(Unzip.prototype, Zlib.prototype);
ObjectSetPrototypeOf(Unzip, Zlib);


const kMaxBrotliParam = MathMax(
  ...ObjectEntries(constants)
    .map(({ 0: key, 1: value }) => (key.startsWith('BROTLI_PARAM_') ? value : 0)),
);
const brotliInitParamsArray = new Uint32Array(kMaxBrotliParam + 1);

const brotliDefaultOpts = {
  flush: BROTLI_OPERATION_PROCESS,
  finishFlush: BROTLI_OPERATION_FINISH,
  fullFlush: BROTLI_OPERATION_FLUSH,
};
function Brotli(opts, mode) {
  assert(mode === BROTLI_DECODE || mode === BROTLI_ENCODE);

  brotliInitParamsArray.fill(-1);
  if (opts?.params) {
    ObjectKeys(opts.params).forEach((origKey) => {
      const key = +origKey;
      if (NumberIsNaN(key) || key < 0 || key > kMaxBrotliParam ||
          (brotliInitParamsArray[key] | 0) !== -1) {
        throw new ERR_BROTLI_INVALID_PARAM(origKey);
      }

      const value = opts.params[origKey];
      if (typeof value !== 'number' && typeof value !== 'boolean') {
        throw new ERR_INVALID_ARG_TYPE('options.params[key]',
                                       'number', opts.params[origKey]);
      }
      brotliInitParamsArray[key] = value;
    });
  }

  let dictionary = opts?.dictionary;
  if (dictionary !== undefined && !isArrayBufferView(dictionary)) {
    if (isAnyArrayBuffer(dictionary)) {
      dictionary = Buffer.from(dictionary);
    } else {
      throw new ERR_INVALID_ARG_TYPE(
        'options.dictionary',
        ['Buffer', 'TypedArray', 'DataView', 'ArrayBuffer'],
        dictionary,
      );
    }
  }

  const handle = mode === BROTLI_DECODE ?
    new binding.BrotliDecoder(mode) : new binding.BrotliEncoder(mode);

  this._writeState = new Uint32Array(2);
  handle.init(
    brotliInitParamsArray,
    this._writeState,
    processCallback,
    dictionary,
  );

  ZlibBase.call(this, opts, mode, handle, brotliDefaultOpts);
}
ObjectSetPrototypeOf(Brotli.prototype, Zlib.prototype);
ObjectSetPrototypeOf(Brotli, Zlib);

function BrotliCompress(opts) {
  if (!(this instanceof BrotliCompress)) {
    return deprecateInstantiation(BrotliCompress, 'DEP0184', opts);
  }
  Brotli.call(this, opts, BROTLI_ENCODE);
}
ObjectSetPrototypeOf(BrotliCompress.prototype, Brotli.prototype);
ObjectSetPrototypeOf(BrotliCompress, Brotli);

function BrotliDecompress(opts) {
  if (!(this instanceof BrotliDecompress)) {
    return deprecateInstantiation(BrotliDecompress, 'DEP0184', opts);
  }
  Brotli.call(this, opts, BROTLI_DECODE);
}
ObjectSetPrototypeOf(BrotliDecompress.prototype, Brotli.prototype);
ObjectSetPrototypeOf(BrotliDecompress, Brotli);


module.exports = {
  BrotliCompress,
  BrotliDecompress,
  Deflate,
  DeflateRaw,
  Gunzip,
  Gzip,
  Inflate,
  InflateRaw,
  Unzip,
};
