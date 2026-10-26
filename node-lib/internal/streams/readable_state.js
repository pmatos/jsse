// Derived from Node.js lib/internal/streams/readable.js (v26.8.2), split to fit the embedded
// file budget. Copyright Joyent, Inc. and other Node contributors. MIT license.

'use strict';

exports.makeBitMapDescriptor = makeBitMapDescriptor;
exports.ReadableState = ReadableState;

const {
  ObjectDefineProperties,
  Symbol,
  SymbolSpecies,
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
const {
  codes: {
    ERR_UNKNOWN_ENCODING,
  },
} = require('internal/errors');

const FastBuffer = Buffer[SymbolSpecies];
const nop = () => {};

const { errorOrDestroy } = destroyImpl;

const kErroredValue = Symbol('kErroredValue');
const kDefaultEncodingValue = Symbol('kDefaultEncodingValue');
const kDecoderValue = Symbol('kDecoderValue');
const kEncodingValue = Symbol('kEncodingValue');

const kEnded = 1 << 9;
const kEndEmitted = 1 << 10;
const kReading = 1 << 11;
const kSync = 1 << 12;
const kNeedReadable = 1 << 13;
const kEmittedReadable = 1 << 14;
const kReadableListening = 1 << 15;
const kResumeScheduled = 1 << 16;
const kMultiAwaitDrain = 1 << 17;
const kReadingMore = 1 << 18;
const kDataEmitted = 1 << 19;
const kDefaultUTF8Encoding = 1 << 20;
const kDecoder = 1 << 21;
const kEncoding = 1 << 22;
const kHasFlowing = 1 << 23;
const kFlowing = 1 << 24;
const kHasPaused = 1 << 25;
const kPaused = 1 << 26;
const kDataListening = 1 << 27;
const kEndScheduled = 1 << 28;

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
ObjectDefineProperties(ReadableState.prototype, {
  objectMode: makeBitMapDescriptor(kObjectMode),
  ended: makeBitMapDescriptor(kEnded),
  endEmitted: makeBitMapDescriptor(kEndEmitted),
  reading: makeBitMapDescriptor(kReading),
  // Stream is still being constructed and cannot be
  // destroyed until construction finished or failed.
  // Async construction is opt in, therefore we start as
  // constructed.
  constructed: makeBitMapDescriptor(kConstructed),
  // A flag to be able to tell if the event 'readable'/'data' is emitted
  // immediately, or on a later tick.  We set this to true at first, because
  // any actions that shouldn't happen until "later" should generally also
  // not happen before the first read call.
  sync: makeBitMapDescriptor(kSync),
  // Whenever we return null, then we set a flag to say
  // that we're awaiting a 'readable' event emission.
  needReadable: makeBitMapDescriptor(kNeedReadable),
  emittedReadable: makeBitMapDescriptor(kEmittedReadable),
  readableListening: makeBitMapDescriptor(kReadableListening),
  resumeScheduled: makeBitMapDescriptor(kResumeScheduled),
  // True if the error was already emitted and should not be thrown again.
  errorEmitted: makeBitMapDescriptor(kErrorEmitted),
  emitClose: makeBitMapDescriptor(kEmitClose),
  autoDestroy: makeBitMapDescriptor(kAutoDestroy),
  // Has it been destroyed.
  destroyed: makeBitMapDescriptor(kDestroyed),
  // Indicates whether the stream has finished destroying.
  closed: makeBitMapDescriptor(kClosed),
  // True if close has been emitted or would have been emitted
  // depending on emitClose.
  closeEmitted: makeBitMapDescriptor(kCloseEmitted),
  multiAwaitDrain: makeBitMapDescriptor(kMultiAwaitDrain),
  // If true, a maybeReadMore has been scheduled.
  readingMore: makeBitMapDescriptor(kReadingMore),
  dataEmitted: makeBitMapDescriptor(kDataEmitted),

  // Indicates whether the stream has errored. When true no further
  // _read calls, 'data' or 'readable' events should occur. This is needed
  // since when autoDestroy is disabled we need a way to tell whether the
  // stream has failed.
  errored: {
    __proto__: null,
    enumerable: false,
    get() {
      return (this[kState] & kErrored) !== 0 ? this[kErroredValue] : null;
    },
    set(value) {
      if (value) {
        this[kErroredValue] = value;
        this[kState] |= kErrored;
      } else {
        this[kState] &= ~kErrored;
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

  decoder: {
    __proto__: null,
    enumerable: false,
    get() {
      return (this[kState] & kDecoder) !== 0 ? this[kDecoderValue] : null;
    },
    set(value) {
      if (value) {
        this[kDecoderValue] = value;
        this[kState] |= kDecoder;
      } else {
        this[kState] &= ~kDecoder;
      }
    },
  },

  encoding: {
    __proto__: null,
    enumerable: false,
    get() {
      return (this[kState] & kEncoding) !== 0 ? this[kEncodingValue] : null;
    },
    set(value) {
      if (value) {
        this[kEncodingValue] = value;
        this[kState] |= kEncoding;
      } else {
        this[kState] &= ~kEncoding;
      }
    },
  },

  flowing: {
    __proto__: null,
    enumerable: false,
    get() {
      return (this[kState] & kHasFlowing) !== 0 ? (this[kState] & kFlowing) !== 0 : null;
    },
    set(value) {
      if (value == null) {
        this[kState] &= ~(kHasFlowing | kFlowing);
      } else if (value) {
        this[kState] |= (kHasFlowing | kFlowing);
      } else {
        this[kState] |= kHasFlowing;
        this[kState] &= ~kFlowing;
      }
    },
  },
});


function ReadableState(options, stream, isDuplex) {
  // Bit map field to store ReadableState more efficiently with 1 bit per field
  // instead of a V8 slot per field.
  this[kState] = kEmitClose | kAutoDestroy | kConstructed | kSync;

  // Object stream flag. Used to make read(n) ignore n and to
  // make all the buffer merging and length checks go away.
  if (options?.objectMode)
    this[kState] |= kObjectMode;

  if (isDuplex && options?.readableObjectMode)
    this[kState] |= kObjectMode;

  // The point at which it stops calling _read() to fill the buffer
  // Note: 0 is a valid value, means "don't call _read preemptively ever"
  this.highWaterMark = options ?
    getHighWaterMark(this, options, 'readableHighWaterMark', isDuplex) :
    getDefaultHighWaterMark(false);

  this.buffer = [];
  this.bufferIndex = 0;
  this.length = 0;
  this.pipes = [];

  // Should close be emitted on destroy. Defaults to true.
  if (options && options.emitClose === false) this[kState] &= ~kEmitClose;

  // Should .destroy() be called after 'end' (and potentially 'finish').
  if (options && options.autoDestroy === false) this[kState] &= ~kAutoDestroy;

  // Crypto is kind of old and crusty.  Historically, its default string
  // encoding is 'binary' so we have to make this configurable.
  // Everything else in the universe uses 'utf8', though.
  const defaultEncoding = options?.defaultEncoding;
  if (defaultEncoding == null || defaultEncoding === 'utf8' || defaultEncoding === 'utf-8') {
    this[kState] |= kDefaultUTF8Encoding;
  } else if (Buffer.isEncoding(defaultEncoding)) {
    this.defaultEncoding = defaultEncoding;
  } else {
    throw new ERR_UNKNOWN_ENCODING(defaultEncoding);
  }

  // Ref the piped dest which we need a drain event on it
  // type: null | Writable | Set<Writable>.
  this.awaitDrainWriters = null;

  if (options?.encoding) {
    this.decoder = new (require('string_decoder').StringDecoder)(options.encoding);
    this.encoding = options.encoding;
  }
}

ReadableState.prototype[kOnConstructed] = function onConstructed(stream) {
  if ((this[kState] & kNeedReadable) !== 0) {
    maybeReadMore(stream, this);
  }
};

ObjectDefineProperties(ReadableState.prototype, {
  // Legacy getter for `pipesCount`.
  pipesCount: {
    __proto__: null,
    get() {
      return this.pipes.length;
    },
  },

  // Legacy property for `paused`.
  paused: {
    __proto__: null,
    get() {
      return (this[kState] & kPaused) !== 0;
    },
    set(value) {
      this[kState] |= kHasPaused;
      if (value) {
        this[kState] |= kPaused;
      } else {
        this[kState] &= ~kPaused;
      }
    },
  },
});

// Exposed for testing purposes only.

Object.assign(exports, {
  FastBuffer,
  nop,
  errorOrDestroy,
  kDecoderValue,
  kEnded,
  kEndEmitted,
  kReading,
  kSync,
  kNeedReadable,
  kEmittedReadable,
  kReadableListening,
  kResumeScheduled,
  kMultiAwaitDrain,
  kReadingMore,
  kDataEmitted,
  kDecoder,
  kHasFlowing,
  kFlowing,
  kHasPaused,
  kPaused,
  kDataListening,
  kEndScheduled,
  makeBitMapDescriptor,
  ReadableState,
});

const { maybeReadMore } = require('internal/streams/readable_read');
