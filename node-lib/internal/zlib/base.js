'use strict';

// The stream engine behind zlib: Node's lib/zlib.js (v26.8.2) up to and
// including `processCallback`, split from the Zlib/Brotli classes and the public
// facade (zlib.js) for the file budget. The Zstd branches are removed. See
// docs/adr/2026-10-27-0450-node-host-zlib.md.

const {
  ArrayBuffer,
  ObjectDefineProperty,
  ObjectSetPrototypeOf,
  Symbol,
} = primordials;

require('internal/zlib/errors');
const {
  codes: {
    ERR_BUFFER_TOO_LARGE,
    ERR_INVALID_ARG_TYPE,
    ERR_OUT_OF_RANGE,
    ERR_TRAILING_JUNK_AFTER_STREAM_END,
  },
  genericNodeError,
} = require('internal/errors');
const { Transform, finished } = require('stream');
const {
  isArrayBufferView,
  isAnyArrayBuffer,
  isUint8Array,
} = require('internal/util/types');
const assert = require('internal/assert');
const {
  Buffer,
  kMaxLength,
} = require('buffer');
const { validateBoolean, validateFunction } = require('internal/validators');
const { FastBuffer, checkRangesOrGetDefault, owner_symbol, validateFiniteNumber } =
  require('internal/zlib/util');
const constants = require('internal/zlib/constants');
const {
  Z_NO_FLUSH, Z_BLOCK, Z_PARTIAL_FLUSH, Z_SYNC_FLUSH, Z_FULL_FLUSH, Z_FINISH,
  Z_MIN_CHUNK, Z_DEFAULT_CHUNK,
  DEFLATE, BROTLI_DECODE, BROTLI_ENCODE,
  BROTLI_OPERATION_PROCESS, BROTLI_OPERATION_EMIT_METADATA,
} = constants;

const kFlushFlag = Symbol('kFlushFlag');
const kError = Symbol('kError');

function zlibBuffer(engine, buffer, callback) {
  validateFunction(callback, 'callback');
  // Streams do not support non-Uint8Array ArrayBufferViews yet. Convert it to a
  // Buffer without copying.
  if (isArrayBufferView(buffer) && !isUint8Array(buffer)) {
    buffer = Buffer.from(buffer.buffer, buffer.byteOffset, buffer.byteLength);
  } else if (isAnyArrayBuffer(buffer)) {
    buffer = Buffer.from(buffer);
  }
  engine.buffers = null;
  engine.nread = 0;
  engine.cb = callback;
  engine.on('data', zlibBufferOnData);
  engine.on('error', zlibBufferOnError);
  engine.on('end', zlibBufferOnEnd);
  engine.end(buffer);
}

function zlibBufferOnData(chunk) {
  if (!this.buffers) {
    this.buffers = [chunk];
  } else {
    this.buffers.push(chunk);
  }
  this.nread += chunk.length;
  if (this.nread > this._maxOutputLength) {
    this.close();
    this.removeAllListeners('end');
    this.cb(new ERR_BUFFER_TOO_LARGE(this._maxOutputLength));
  }
}

function zlibBufferOnError(err) {
  this.removeAllListeners('end');
  this.cb(err);
}

function zlibBufferOnEnd() {
  let buf;
  if (this.nread === 0) {
    buf = new FastBuffer();
  } else {
    const bufs = this.buffers;
    buf = (bufs.length === 1 ? bufs[0] : Buffer.concat(bufs, this.nread));
  }
  this.close();
  if (this._info)
    this.cb(null, { buffer: buf, engine: this });
  else
    this.cb(null, buf);
}

function zlibBufferSync(engine, buffer) {
  if (typeof buffer === 'string') {
    buffer = Buffer.from(buffer);
  } else if (!isArrayBufferView(buffer)) {
    if (isAnyArrayBuffer(buffer)) {
      buffer = Buffer.from(buffer);
    } else {
      throw new ERR_INVALID_ARG_TYPE(
        'buffer',
        ['string', 'Buffer', 'TypedArray', 'DataView', 'ArrayBuffer'],
        buffer,
      );
    }
  }
  buffer = processChunkSync(engine, buffer, engine._finishFlushFlag);
  if (engine._info)
    return { buffer, engine };
  return buffer;
}

function zlibOnError(message, errno, code) {
  const self = this[owner_symbol];
  // There is no way to cleanly recover.
  // Continuing only obscures problems.

  const error = genericNodeError(message, { errno, code });
  error.errno = errno;
  error.code = code;
  self.destroy(error);
  self[kError] = error;
}

const FLUSH_BOUND = [
  [ Z_NO_FLUSH, Z_BLOCK ],
  [ BROTLI_OPERATION_PROCESS, BROTLI_OPERATION_EMIT_METADATA ],
];
const FLUSH_BOUND_IDX_NORMAL = 0;
const FLUSH_BOUND_IDX_BROTLI = 1;

/**
 * The base class for all Zlib-style streams.
 * @class
 */
function ZlibBase(opts, mode, handle, { flush, finishFlush, fullFlush }) {
  let chunkSize = Z_DEFAULT_CHUNK;
  let maxOutputLength = kMaxLength;
  // The ZlibBase class is not exported to user land, the mode should only be
  // passed in by us.
  assert(typeof mode === 'number');
  assert(mode >= DEFLATE && mode <= BROTLI_ENCODE);

  let flushBoundIdx;
  if (mode === BROTLI_ENCODE || mode === BROTLI_DECODE) {
    flushBoundIdx = FLUSH_BOUND_IDX_BROTLI;
  } else {
    flushBoundIdx = FLUSH_BOUND_IDX_NORMAL;
  }

  if (opts) {
    chunkSize = opts.chunkSize;
    if (!validateFiniteNumber(chunkSize, 'options.chunkSize')) {
      chunkSize = Z_DEFAULT_CHUNK;
    } else if (chunkSize < Z_MIN_CHUNK) {
      throw new ERR_OUT_OF_RANGE('options.chunkSize',
                                 `>= ${Z_MIN_CHUNK}`, chunkSize);
    }

    flush = checkRangesOrGetDefault(
      opts.flush, 'options.flush',
      FLUSH_BOUND[flushBoundIdx][0], FLUSH_BOUND[flushBoundIdx][1], flush);

    finishFlush = checkRangesOrGetDefault(
      opts.finishFlush, 'options.finishFlush',
      FLUSH_BOUND[flushBoundIdx][0], FLUSH_BOUND[flushBoundIdx][1],
      finishFlush);

    maxOutputLength = checkRangesOrGetDefault(
      opts.maxOutputLength, 'options.maxOutputLength',
      1, kMaxLength, kMaxLength);

    if (opts.rejectGarbageAfterEnd !== undefined) {
      validateBoolean(
        opts.rejectGarbageAfterEnd,
        'options.rejectGarbageAfterEnd',
      );
    }

    if (opts.encoding || opts.objectMode || opts.writableObjectMode) {
      opts = { ...opts };
      opts.encoding = null;
      opts.objectMode = false;
      opts.writableObjectMode = false;
    }
  }

  Transform.call(this, { autoDestroy: true, ...opts });
  this[kError] = null;
  this.bytesWritten = 0;
  this._handle = handle;
  handle[owner_symbol] = this;
  // Used by processCallback() and zlibOnError()
  handle.onerror = zlibOnError;
  this._outBuffer = Buffer.allocUnsafe(chunkSize);
  this._outOffset = 0;

  this._chunkSize = chunkSize;
  this._defaultFlushFlag = flush;
  this._finishFlushFlag = finishFlush;
  this._defaultFullFlushFlag = fullFlush;
  this._flushBoundIdx = flushBoundIdx;
  this._info = opts?.info;
  this._maxOutputLength = maxOutputLength;

  this._rejectGarbageAfterEnd = opts?.rejectGarbageAfterEnd === true;
}
ObjectSetPrototypeOf(ZlibBase.prototype, Transform.prototype);
ObjectSetPrototypeOf(ZlibBase, Transform);

ObjectDefineProperty(ZlibBase.prototype, '_closed', {
  __proto__: null,
  configurable: true,
  enumerable: true,
  get() {
    return !this._handle;
  },
});

/**
 * @this {ZlibBase}
 * @returns {void}
 */
ZlibBase.prototype.reset = function() {
  assert(this._handle, 'zlib binding closed');
  return this._handle.reset();
};

/**
 * This is the _flush function called by the transform class,
 * internally, when the last chunk has been written.
 * @returns {void}
 * @this {ZlibBase}
 */
ZlibBase.prototype._flush = function(callback) {
  this._transform(new FastBuffer(), '', callback);
};

/**
 * Force Transform compat behavior.
 * @returns {void}
 * @this {ZlibBase}
 */
ZlibBase.prototype._final = function(callback) {
  callback();
};

// If a flush is scheduled while another flush is still pending, a way to figure
// out which one is the "stronger" flush is needed.
// This is currently only used to figure out which flush flag to use for the
// last chunk.
// Roughly, the following holds:
// Z_NO_FLUSH < Z_BLOCK < Z_PARTIAL_FLUSH <
//     Z_SYNC_FLUSH < Z_FULL_FLUSH < Z_FINISH
const flushiness = [];
const kFlushFlagList = [Z_NO_FLUSH, Z_BLOCK, Z_PARTIAL_FLUSH,
                        Z_SYNC_FLUSH, Z_FULL_FLUSH, Z_FINISH];
for (let i = 0; i < kFlushFlagList.length; i++) {
  flushiness[kFlushFlagList[i]] = i;
}

function maxFlush(a, b) {
  return flushiness[a] > flushiness[b] ? a : b;
}

// Set up a list of 'special' buffers that can be written using .write()
// from the .flush() code as a way of introducing flushing operations into the
// write sequence.
const kFlushBuffers = [];
{
  const dummyArrayBuffer = new ArrayBuffer();
  for (const flushFlag of kFlushFlagList) {
    kFlushBuffers[flushFlag] = Buffer.from(dummyArrayBuffer);
    kFlushBuffers[flushFlag][kFlushFlag] = flushFlag;
  }
}

ZlibBase.prototype.flush = function(kind, callback) {
  if (typeof kind === 'function' || (kind === undefined && !callback)) {
    callback = kind;
    kind = this._defaultFullFlushFlag;
  }

  kind = checkRangesOrGetDefault(
    kind, 'kind',
    FLUSH_BOUND[this._flushBoundIdx][0], FLUSH_BOUND[this._flushBoundIdx][1],
    this._defaultFullFlushFlag);

  if (this.writableFinished) {
    if (callback)
      process.nextTick(callback);
  } else if (this.writableEnded) {
    if (callback)
      this.once('end', callback);
  } else {
    this.write(kFlushBuffers[kind], '', callback);
  }
};

/**
 * @this {import('stream').Transform}
 * @param {(err?: Error) => any} [callback]
 */
ZlibBase.prototype.close = function(callback) {
  if (callback) finished(this, callback);
  this.destroy();
};

ZlibBase.prototype._destroy = function(err, callback) {
  _close(this);
  callback(err);
};

ZlibBase.prototype._transform = function(chunk, encoding, cb) {
  let flushFlag = this._defaultFlushFlag;
  // We use a 'fake' zero-length chunk to carry information about flushes from
  // the public API to the actual stream implementation.
  if (typeof chunk[kFlushFlag] === 'number') {
    flushFlag = chunk[kFlushFlag];
  }

  // For the last chunk, also apply `_finishFlushFlag`.
  if (this.writableEnded && this.writableLength === chunk.byteLength) {
    flushFlag = maxFlush(flushFlag, this._finishFlushFlag);
  }
  processChunk(this, chunk, flushFlag, cb);
};

ZlibBase.prototype._processChunk = function(chunk, flushFlag, cb) {
  // _processChunk() is left for backwards compatibility
  if (typeof cb === 'function')
    processChunk(this, chunk, flushFlag, cb);
  else
    return processChunkSync(this, chunk, flushFlag);
};

function processChunkSync(self, chunk, flushFlag) {
  let availInBefore = chunk.byteLength;
  let availOutBefore = self._chunkSize - self._outOffset;
  let inOff = 0;
  let availOutAfter;
  let availInAfter;

  const buffers = [];
  let nread = 0;
  let inputRead = 0;
  const state = self._writeState;
  const handle = self._handle;
  let buffer = self._outBuffer;
  let offset = self._outOffset;
  const chunkSize = self._chunkSize;

  let error;
  self.on('error', function onError(er) {
    error = er;
  });

  while (true) {
    handle.writeSync(flushFlag,
                     chunk, // in
                     inOff, // in_off
                     availInBefore, // in_len
                     buffer, // out
                     offset, // out_off
                     availOutBefore); // out_len
    if (error)
      throw error;
    else if (self[kError])
      throw self[kError];

    availOutAfter = state[0];
    availInAfter = state[1];

    const inDelta = (availInBefore - availInAfter);
    inputRead += inDelta;

    const have = availOutBefore - availOutAfter;
    if (have > 0) {
      const out = buffer.slice(offset, offset + have);
      offset += have;
      buffers.push(out);
      nread += out.byteLength;

      if (nread > self._maxOutputLength) {
        _close(self);
        throw new ERR_BUFFER_TOO_LARGE(self._maxOutputLength);
      }

    } else {
      assert(have === 0, 'have should not go down');
    }

    // Exhausted the output buffer, or used all the input create a new one.
    if (availOutAfter === 0 || offset >= chunkSize) {
      availOutBefore = chunkSize;
      offset = 0;
      buffer = Buffer.allocUnsafe(chunkSize);
    }

    if (availOutAfter === 0) {
      // Not actually done. Need to reprocess.
      // Also, update the availInBefore to the availInAfter value,
      // so that if we have to hit it a third (fourth, etc.) time,
      // it'll have the correct byte counts.
      inOff += inDelta;
      availInBefore = availInAfter;
    } else {
      break;
    }
  }

  if (availInAfter > 0 && self._rejectGarbageAfterEnd) {
    _close(self);
    throw new ERR_TRAILING_JUNK_AFTER_STREAM_END();
  }

  self.bytesWritten = inputRead;
  _close(self);

  if (nread === 0)
    return new FastBuffer();

  return (buffers.length === 1 ? buffers[0] : Buffer.concat(buffers, nread));
}

function processChunk(self, chunk, flushFlag, cb) {
  const handle = self._handle;
  if (!handle) return process.nextTick(cb);

  handle.buffer = chunk;
  handle.cb = cb;
  handle.availOutBefore = self._chunkSize - self._outOffset;
  handle.availInBefore = chunk.byteLength;
  handle.inOff = 0;
  handle.flushFlag = flushFlag;

  handle.write(flushFlag,
               chunk, // in
               0, // in_off
               handle.availInBefore, // in_len
               self._outBuffer, // out
               self._outOffset, // out_off
               handle.availOutBefore); // out_len
}

function processCallback() {
  // This callback's context (`this`) is the `_handle` (ZCtx) object. It is
  // important to null out the values once they are no longer needed since
  // `_handle` can stay in memory long after the buffer is needed.
  const handle = this;
  const self = this[owner_symbol];
  const state = self._writeState;

  if (self.destroyed) {
    this.buffer = null;
    this.cb();
    return;
  }

  const availOutAfter = state[0];
  const availInAfter = state[1];

  const inDelta = handle.availInBefore - availInAfter;
  self.bytesWritten += inDelta;

  const have = handle.availOutBefore - availOutAfter;
  let streamBufferIsFull = false;
  if (have > 0) {
    const out = self._outBuffer.slice(self._outOffset, self._outOffset + have);
    self._outOffset += have;
    streamBufferIsFull = !self.push(out);
  } else {
    assert(have === 0, 'have should not go down');
  }

  if (self.destroyed) {
    this.cb();
    return;
  }

  // Exhausted the output buffer, or used all the input create a new one.
  if (availOutAfter === 0 || self._outOffset >= self._chunkSize) {
    handle.availOutBefore = self._chunkSize;
    self._outOffset = 0;
    self._outBuffer = Buffer.allocUnsafe(self._chunkSize);
  }

  if (availOutAfter === 0) {
    // Not actually done. Need to reprocess.
    // Also, update the availInBefore to the availInAfter value,
    // so that if we have to hit it a third (fourth, etc.) time,
    // it'll have the correct byte counts.
    handle.inOff += inDelta;
    handle.availInBefore = availInAfter;


    if (!streamBufferIsFull) {
      this.write(handle.flushFlag,
                 this.buffer, // in
                 handle.inOff, // in_off
                 handle.availInBefore, // in_len
                 self._outBuffer, // out
                 self._outOffset, // out_off
                 self._chunkSize); // out_len
    } else {
      const oldRead = self._read;
      self._read = (n) => {
        self._read = oldRead;
        this.write(handle.flushFlag,
                   this.buffer, // in
                   handle.inOff, // in_off
                   handle.availInBefore, // in_len
                   self._outBuffer, // out
                   self._outOffset, // out_off
                   self._chunkSize); // out_len
        self._read(n);
      };
    }
    return;
  }

  if (availInAfter > 0) {
    // If we have more input that should be written, but we also have output
    // space available, that means that the compression library was not
    // interested in receiving more data, and in particular that the input
    // stream has ended early.
    // This applies to streams where we don't check data past the end of
    // what was consumed; that is, everything except Gunzip/Unzip.

    if (self._rejectGarbageAfterEnd) {
      const err = new ERR_TRAILING_JUNK_AFTER_STREAM_END();
      self.destroy(err);
      this.cb(err);
      return;
    }

    self.push(null);
  }

  // Finished with the chunk.
  this.buffer = null;
  this.cb();
}

/**
 * @param {ZlibBase} engine
 * @private
 */
function _close(engine) {
  // Caller may invoke .close after a zlib error (which will null _handle)
  engine._handle?.close();
  engine._handle = null;
}

module.exports = {
  ZlibBase,
  processCallback,
  zlibBuffer,
  zlibBufferSync,
};
