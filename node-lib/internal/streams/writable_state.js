// Derived from Node.js lib/internal/streams/writable.js (v26.8.2), split to fit the embedded
// file budget. Copyright Joyent, Inc. and other Node contributors. MIT license.

'use strict';

exports.nop = nop;
exports.makeBitMapDescriptor = makeBitMapDescriptor;
exports.WritableState = WritableState;
exports.resetBuffer = resetBuffer;

const {
  ArrayPrototypeSlice,
  ObjectDefineProperties,
  ObjectDefineProperty,
  Symbol,
} = primordials;
const {
  Buffer,
} = require('internal/host/buffer');
const destroyImpl = require('internal/streams/destroy');
const {
  getHighWaterMark,
  getDefaultHighWaterMark,
} = require('internal/streams/state');
const {
  codes: {
    ERR_UNKNOWN_ENCODING,
  },
} = require('internal/errors');
const {
  kState,
  kObjectMode,
  kErrorEmitted,
  kAutoDestroy,
  kEmitClose,
  kDestroyed,
  kClosed,
  kCloseEmitted,
  kErrored,
  kConstructed,
  kOnConstructed,
} = require('internal/streams/utils');

const { errorOrDestroy } = destroyImpl;
function nop() {}

const kOnFinishedValue = Symbol('kOnFinishedValue');
const kErroredValue = Symbol('kErroredValue');
const kDefaultEncodingValue = Symbol('kDefaultEncodingValue');
const kWriteCbValue = Symbol('kWriteCbValue');
const kAfterWriteTickInfoValue = Symbol('kAfterWriteTickInfoValue');
const kBufferedValue = Symbol('kBufferedValue');

const kSync = 1 << 9;
const kFinalCalled = 1 << 10;
const kNeedDrain = 1 << 11;
const kEnding = 1 << 12;
const kFinished = 1 << 13;
const kDecodeStrings = 1 << 14;
const kWriting = 1 << 15;
const kBufferProcessing = 1 << 16;
const kPrefinished = 1 << 17;
const kAllBuffers = 1 << 18;
const kAllNoop = 1 << 19;
const kOnFinished = 1 << 20;
const kHasWritable = 1 << 21;
const kWritable = 1 << 22;
const kCorked = 1 << 23;
const kDefaultUTF8Encoding = 1 << 24;
const kWriteCb = 1 << 25;
const kExpectWriteCb = 1 << 26;
const kAfterWriteTickInfo = 1 << 27;
const kAfterWritePending = 1 << 28;
const kBuffered = 1 << 29;
const kEnded = 1 << 30;

// TODO(benjamingr) it is likely slower to do it this way than with free functions
function makeBitMapDescriptor(bit) {
  return {
    enumerable: false,
    get() { return (this[kState] & bit) !== 0; },
    set(value) {
      if (value) this[kState] |= bit;
      else this[kState] &= ~bit;
    },
  };
}
ObjectDefineProperties(WritableState.prototype, {
  // Object stream flag to indicate whether or not this stream
  // contains buffers or objects.
  objectMode: makeBitMapDescriptor(kObjectMode),

  // if _final has been called.
  finalCalled: makeBitMapDescriptor(kFinalCalled),

  // drain event flag.
  needDrain: makeBitMapDescriptor(kNeedDrain),

  // At the start of calling end()
  ending: makeBitMapDescriptor(kEnding),

  // When end() has been called, and returned.
  ended: makeBitMapDescriptor(kEnded),

  // When 'finish' is emitted.
  finished: makeBitMapDescriptor(kFinished),

  // Has it been destroyed.
  destroyed: makeBitMapDescriptor(kDestroyed),

  // Should we decode strings into buffers before passing to _write?
  // this is here so that some node-core streams can optimize string
  // handling at a lower level.
  decodeStrings: makeBitMapDescriptor(kDecodeStrings),

  // A flag to see when we're in the middle of a write.
  writing: makeBitMapDescriptor(kWriting),

  // A flag to be able to tell if the onwrite cb is called immediately,
  // or on a later tick.  We set this to true at first, because any
  // actions that shouldn't happen until "later" should generally also
  // not happen before the first write call.
  sync: makeBitMapDescriptor(kSync),

  // A flag to know if we're processing previously buffered items, which
  // may call the _write() callback in the same tick, so that we don't
  // end up in an overlapped onwrite situation.
  bufferProcessing: makeBitMapDescriptor(kBufferProcessing),

  // Stream is still being constructed and cannot be
  // destroyed until construction finished or failed.
  // Async construction is opt in, therefore we start as
  // constructed.
  constructed: makeBitMapDescriptor(kConstructed),

  // Emit prefinish if the only thing we're waiting for is _write cbs
  // This is relevant for synchronous Transform streams.
  prefinished: makeBitMapDescriptor(kPrefinished),

  // True if the error was already emitted and should not be thrown again.
  errorEmitted: makeBitMapDescriptor(kErrorEmitted),

  // Should close be emitted on destroy. Defaults to true.
  emitClose: makeBitMapDescriptor(kEmitClose),

  // Should .destroy() be called after 'finish' (and potentially 'end').
  autoDestroy: makeBitMapDescriptor(kAutoDestroy),

  // Indicates whether the stream has finished destroying.
  closed: makeBitMapDescriptor(kClosed),

  // True if close has been emitted or would have been emitted
  // depending on emitClose.
  closeEmitted: makeBitMapDescriptor(kCloseEmitted),

  allBuffers: makeBitMapDescriptor(kAllBuffers),
  allNoop: makeBitMapDescriptor(kAllNoop),

  // Indicates whether the stream has errored. When true all write() calls
  // should return false. This is needed since when autoDestroy
  // is disabled we need a way to tell whether the stream has failed.
  // This is/should be a cold path.
  errored: {
    __proto__: null,
    enumerable: false,
    get() { return (this[kState] & kErrored) !== 0 ? this[kErroredValue] : null; },
    set(value) {
      if (value) {
        this[kErroredValue] = value;
        this[kState] |= kErrored;
      } else {
        this[kState] &= ~kErrored;
      }
    },
  },

  writable: {
    __proto__: null,
    enumerable: false,
    get() { return (this[kState] & kHasWritable) !== 0 ? (this[kState] & kWritable) !== 0 : undefined; },
    set(value) {
      if (value == null) {
        this[kState] &= ~(kHasWritable | kWritable);
      } else if (value) {
        this[kState] |= (kHasWritable | kWritable);
      } else {
        this[kState] |= kHasWritable;
        this[kState] &= ~kWritable;
      }
    },
  },

  defaultEncoding: {
    __proto__: null,
    enumerable: false,
    get() { return (this[kState] & kDefaultUTF8Encoding) !== 0 ? 'utf8' : this[kDefaultEncodingValue]; },
    set(value) {
      if (value === 'utf8' || value === 'utf-8') {
        this[kState] |= kDefaultUTF8Encoding;
      } else {
        this[kState] &= ~kDefaultUTF8Encoding;
        this[kDefaultEncodingValue] = value;
      }
    },
  },

  // The callback that the user supplies to write(chunk, encoding, cb).
  writecb: {
    __proto__: null,
    enumerable: false,
    get() { return (this[kState] & kWriteCb) !== 0 ? this[kWriteCbValue] : nop; },
    set(value) {
      this[kWriteCbValue] = value;
      if (value) {
        this[kState] |= kWriteCb;
      } else {
        this[kState] &= ~kWriteCb;
      }
    },
  },

  // Storage for data passed to the afterWrite() callback in case of
  // synchronous _write() completion.
  afterWriteTickInfo: {
    __proto__: null,
    enumerable: false,
    get() { return (this[kState] & kAfterWriteTickInfo) !== 0 ? this[kAfterWriteTickInfoValue] : null; },
    set(value) {
      this[kAfterWriteTickInfoValue] = value;
      if (value) {
        this[kState] |= kAfterWriteTickInfo;
      } else {
        this[kState] &= ~kAfterWriteTickInfo;
      }
    },
  },

  buffered: {
    __proto__: null,
    enumerable: false,
    get() { return (this[kState] & kBuffered) !== 0 ? this[kBufferedValue] : []; },
    set(value) {
      this[kBufferedValue] = value;
      if (value) {
        this[kState] |= kBuffered;
      } else {
        this[kState] &= ~kBuffered;
      }
    },
  },

});

function WritableState(options, stream, isDuplex) {
  // Bit map field to store WritableState more efficiently with 1 bit per field
  // instead of a V8 slot per field.
  this[kState] = kSync | kConstructed | kEmitClose | kAutoDestroy;

  if (options?.objectMode)
    this[kState] |= kObjectMode;

  if (isDuplex && options?.writableObjectMode)
    this[kState] |= kObjectMode;

  // The point at which write() starts returning false
  // Note: 0 is a valid value, means that we always return false if
  // the entire buffer is not flushed immediately on write().
  this.highWaterMark = options ?
    getHighWaterMark(this, options, 'writableHighWaterMark', isDuplex) :
    getDefaultHighWaterMark(false);

  if (!options || options.decodeStrings !== false) this[kState] |= kDecodeStrings;

  // Should close be emitted on destroy. Defaults to true.
  if (options && options.emitClose === false) this[kState] &= ~kEmitClose;

  // Should .destroy() be called after 'end' (and potentially 'finish').
  if (options && options.autoDestroy === false) this[kState] &= ~kAutoDestroy;

  // Crypto is kind of old and crusty.  Historically, its default string
  // encoding is 'binary' so we have to make this configurable.
  // Everything else in the universe uses 'utf8', though.
  const defaultEncoding = options ? options.defaultEncoding : null;
  if (defaultEncoding == null || defaultEncoding === 'utf8' || defaultEncoding === 'utf-8') {
    this[kState] |= kDefaultUTF8Encoding;
  } else if (Buffer.isEncoding(defaultEncoding)) {
    this[kState] &= ~kDefaultUTF8Encoding;
    this[kDefaultEncodingValue] = defaultEncoding;
  } else {
    throw new ERR_UNKNOWN_ENCODING(defaultEncoding);
  }

  // Not an actual buffer we keep track of, but a measurement
  // of how much we're waiting to get pushed to some underlying
  // socket or file.
  this.length = 0;

  // When true all writes will be buffered until .uncork() call.
  this.corked = 0;

  // The callback that's passed to _write(chunk, cb).
  this.onwrite = (er) => onwrite(stream, er);

  // The amount that is being written when _write is called.
  this.writelen = 0;

  resetBuffer(this);

  // Number of pending user-supplied write callbacks
  // this must be 0 before 'finish' can be emitted.
  this.pendingcb = 0;
}

function resetBuffer(state) {
  state[kBufferedValue] = null;
  state.bufferedIndex = 0;
  state[kState] |= kAllBuffers | kAllNoop;
  state[kState] &= ~kBuffered;
}

WritableState.prototype.getBuffer = function getBuffer() {
  return (this[kState] & kBuffered) === 0 ? [] : ArrayPrototypeSlice(this.buffered, this.bufferedIndex);
};

ObjectDefineProperty(WritableState.prototype, 'bufferedRequestCount', {
  __proto__: null,
  get() {
    return (this[kState] & kBuffered) === 0 ? 0 : this[kBufferedValue].length - this.bufferedIndex;
  },
});

WritableState.prototype[kOnConstructed] = function onConstructed(stream) {
  if ((this[kState] & kWriting) === 0) {
    clearBuffer(stream, this);
  }

  if ((this[kState] & kEnding) !== 0) {
    finishMaybe(stream, this);
  }
};

Object.assign(exports, {
  errorOrDestroy,
  kOnFinishedValue,
  kErroredValue,
  kWriteCbValue,
  kAfterWriteTickInfoValue,
  kBufferedValue,
  kSync,
  kFinalCalled,
  kNeedDrain,
  kEnding,
  kFinished,
  kDecodeStrings,
  kWriting,
  kBufferProcessing,
  kPrefinished,
  kAllBuffers,
  kAllNoop,
  kOnFinished,
  kHasWritable,
  kWritable,
  kCorked,
  kDefaultUTF8Encoding,
  kWriteCb,
  kExpectWriteCb,
  kAfterWriteTickInfo,
  kAfterWritePending,
  kBuffered,
  kEnded,
  nop,
  makeBitMapDescriptor,
  WritableState,
  resetBuffer,
});

const { onwrite, clearBuffer } = require('internal/streams/writable_write');
const { finishMaybe } = require('internal/streams/writable_end');
