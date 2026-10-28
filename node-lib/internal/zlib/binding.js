'use strict';

// The handle classes lib/zlib.js drives, in the shape of Node's
// `internalBinding('zlib')` (src/node_zlib.cc, v26.8.2): `init`, `write`,
// `writeSync`, `params`, `reset` and `close` over the native stream table of
// `internal/host/zlib`. Every JS value (the write state, the process callback,
// the in-flight buffers) stays on the handle object. `write` runs the work from
// a `setImmediate` job where Node uses a threadpool, so the completion
// callback is never synchronous. Brotli handles throw until the native codec
// lands.

const native = require('internal/host/zlib');
const { codes: { ERR_FEATURE_UNAVAILABLE_ON_PLATFORM } } = require('internal/errors');

const kId = Symbol('kId');
const kWriteState = Symbol('kWriteState');
const kCallback = Symbol('kCallback');
const kBusy = Symbol('kBusy');
const kPendingClose = Symbol('kPendingClose');
const kClosed = Symbol('kClosed');

class Zlib {
  constructor(mode) {
    this[kId] = native.create(mode);
    this[kWriteState] = null;
    this[kCallback] = null;
    this[kBusy] = false;
    this[kPendingClose] = false;
    this[kClosed] = false;
  }

  init(windowBits, level, memLevel, strategy, writeState, processCallback,
       dictionary, rejectGarbageAfterEnd) {
    this[kWriteState] = writeState;
    this[kCallback] = processCallback;
    native.init(this[kId], windowBits, level, memLevel, strategy, dictionary,
                rejectGarbageAfterEnd === true);
  }

  write(flush, input, inOff, inLen, out, outOff, outLen) {
    this[kBusy] = true;
    setImmediate(() => {
      this[kBusy] = false;
      if (this[kClosed]) return;
      const err = this.#run(flush, input, inOff, inLen, out, outOff, outLen);
      if (err !== undefined) {
        this.#emitError(err);
        return;
      }
      this[kCallback].call(this);
      if (this[kPendingClose]) this.close();
    });
  }

  writeSync(flush, input, inOff, inLen, out, outOff, outLen) {
    const err = this.#run(flush, input, inOff, inLen, out, outOff, outLen);
    if (err !== undefined) this.#emitError(err);
  }

  #run(flush, input, inOff, inLen, out, outOff, outLen) {
    return native.write(this[kId], flush, input, inOff, inLen, out, outOff,
                        outLen, this[kWriteState]);
  }

  #emitError(err) {
    this.onerror(err[0], err[1], err[2]);
    this[kBusy] = false;
    if (this[kPendingClose]) this.close();
  }

  params(level, strategy) {
    const err = native.params(this[kId], level, strategy);
    if (err !== undefined) this.#emitError(err);
  }

  reset() {
    if (this[kBusy]) {
      throw new Error('Cannot reset zlib stream while a write is in progress');
    }
    const err = native.reset(this[kId]);
    if (err !== undefined) this.#emitError(err);
  }

  close() {
    if (this[kBusy]) {
      this[kPendingClose] = true;
      return;
    }
    this[kPendingClose] = false;
    if (this[kClosed]) return;
    this[kClosed] = true;
    native.close(this[kId]);
  }
}

class BrotliEncoder {
  constructor() {
    throw new ERR_FEATURE_UNAVAILABLE_ON_PLATFORM('Brotli');
  }
}

class BrotliDecoder {
  constructor() {
    throw new ERR_FEATURE_UNAVAILABLE_ON_PLATFORM('Brotli');
  }
}

module.exports = {
  BrotliDecoder,
  BrotliEncoder,
  Zlib,
  crc32: native.crc32,
};
