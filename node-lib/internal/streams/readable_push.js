// Derived from Node.js lib/internal/streams/readable.js (v26.8.2), split to fit the embedded
// file budget. Copyright Joyent, Inc. and other Node contributors. MIT license.

'use strict';

exports.readableAddChunkUnshiftByteMode = readableAddChunkUnshiftByteMode;
exports.readableAddChunkUnshiftObjectMode = readableAddChunkUnshiftObjectMode;
exports.readableAddChunkUnshiftValue = readableAddChunkUnshiftValue;
exports.readableAddChunkPushByteMode = readableAddChunkPushByteMode;
exports.readableAddChunkPushObjectMode = readableAddChunkPushObjectMode;
exports.canPushMore = canPushMore;
exports.addChunk = addChunk;
exports.fromList = fromList;
exports.endReadable = endReadable;
exports.endReadableNT = endReadableNT;
exports.endWritableNT = endWritableNT;
exports.lazyWebStreams = lazyWebStreams;

const {
  Promise,
  SymbolAsyncDispose,
  TypedArrayPrototypeSet,
} = primordials;
const EE = require('events');
const {
  Stream,
} = require('internal/streams/legacy');
const {
  Buffer,
} = require('internal/host/buffer');
const {
  eos,
} = require('internal/streams/end-of-stream');
let debug = require('internal/util/debuglog').debuglog('stream', (fn) => {
  debug = fn;
});
const destroyImpl = require('internal/streams/destroy');
const {
  kState,
  kObjectMode,
  kDestroyed,
  kCloseEmitted,
  kErrored,
} = require('internal/streams/utils');
const {
  AbortError,
  codes: {
    ERR_INVALID_ARG_TYPE,
    ERR_STREAM_PUSH_AFTER_EOF,
    ERR_STREAM_UNSHIFT_AFTER_END_EVENT,
  },
} = require('internal/errors');
const from = require('internal/streams/from');
const Readable = require('internal/streams/readable');
const { FastBuffer, errorOrDestroy, kDecoderValue, kEnded, kEndEmitted, kReading, kSync, kNeedReadable, kMultiAwaitDrain, kDataEmitted, kDecoder, kHasFlowing, kFlowing, kPaused, kDataListening, kEndScheduled } = require('internal/streams/readable_state');
const { onEofChunk, emitReadable, maybeReadMore } = require('internal/streams/readable_read');

Readable.prototype.destroy = destroyImpl.destroy;
Readable.prototype._undestroy = destroyImpl.undestroy;
Readable.prototype._destroy = function(err, cb) {
  cb(err);
};

Readable.prototype[EE.captureRejectionSymbol] = function(err) {
  this.destroy(err);
};

Readable.prototype[SymbolAsyncDispose] = async function() {
  let error;
  if (!this.destroyed) {
    error = this.readableEnded ? null : new AbortError();
    this.destroy(error);
  }
  await new Promise((resolve, reject) => eos(this, (err) => (err && err !== error ? reject(err) : resolve(null))));
};

// Manually shove something into the read() buffer.
// This returns true if the highWaterMark has not been hit yet,
// similar to how Writable.write() returns true if you should
// write() some more.
Readable.prototype.push = function(chunk, encoding) {
  debug('push', chunk);

  const state = this._readableState;
  return (state[kState] & kObjectMode) === 0 ?
    readableAddChunkPushByteMode(this, state, chunk, encoding) :
    readableAddChunkPushObjectMode(this, state, chunk, encoding);
};

// Unshift should *always* be something directly out of read().
Readable.prototype.unshift = function(chunk, encoding) {
  debug('unshift', chunk);
  const state = this._readableState;
  return (state[kState] & kObjectMode) === 0 ?
    readableAddChunkUnshiftByteMode(this, state, chunk, encoding) :
    readableAddChunkUnshiftObjectMode(this, state, chunk);
};


function readableAddChunkUnshiftByteMode(stream, state, chunk, encoding) {
  if (chunk === null) {
    state[kState] &= ~kReading;
    onEofChunk(stream, state);

    return false;
  }

  if (typeof chunk === 'string') {
    encoding ||= state.defaultEncoding;
    if (state.encoding !== encoding) {
      if (state.encoding) {
        // When unshifting, if state.encoding is set, we have to save
        // the string in the BufferList with the state encoding.
        chunk = Buffer.from(chunk, encoding).toString(state.encoding);
      } else {
        chunk = Buffer.from(chunk, encoding);
      }
    }
  } else if (Stream._isArrayBufferView(chunk)) {
    chunk = Stream._uint8ArrayToBuffer(chunk);
  } else if (chunk !== undefined && !(chunk instanceof Buffer)) {
    errorOrDestroy(stream, new ERR_INVALID_ARG_TYPE(
      'chunk', ['string', 'Buffer', 'TypedArray', 'DataView'], chunk));
    return false;
  }


  if (!(chunk && chunk.length > 0)) {
    return canPushMore(state);
  }

  return readableAddChunkUnshiftValue(stream, state, chunk);
}

function readableAddChunkUnshiftObjectMode(stream, state, chunk) {
  if (chunk === null) {
    state[kState] &= ~kReading;
    onEofChunk(stream, state);

    return false;
  }

  return readableAddChunkUnshiftValue(stream, state, chunk);
}

function readableAddChunkUnshiftValue(stream, state, chunk) {
  if ((state[kState] & kEndEmitted) !== 0)
    errorOrDestroy(stream, new ERR_STREAM_UNSHIFT_AFTER_END_EVENT());
  else if ((state[kState] & (kDestroyed | kErrored)) !== 0)
    return false;
  else
    addChunk(stream, state, chunk, true);

  return canPushMore(state);
}

function readableAddChunkPushByteMode(stream, state, chunk, encoding) {
  if (chunk === null) {
    state[kState] &= ~kReading;
    onEofChunk(stream, state);
    return false;
  }

  if (typeof chunk === 'string') {
    encoding ||= state.defaultEncoding;
    if (state.encoding !== encoding) {
      chunk = Buffer.from(chunk, encoding);
      encoding = '';
    }
  } else if (chunk instanceof Buffer) {
    encoding = '';
  } else if (Stream._isArrayBufferView(chunk)) {
    chunk = Stream._uint8ArrayToBuffer(chunk);
    encoding = '';
  } else if (chunk !== undefined) {
    errorOrDestroy(stream, new ERR_INVALID_ARG_TYPE(
      'chunk', ['string', 'Buffer', 'TypedArray', 'DataView'], chunk));
    return false;
  }

  if (!chunk || chunk.length <= 0) {
    state[kState] &= ~kReading;
    maybeReadMore(stream, state);

    return canPushMore(state);
  }

  if ((state[kState] & kEnded) !== 0) {
    errorOrDestroy(stream, new ERR_STREAM_PUSH_AFTER_EOF());
    return false;
  }

  if ((state[kState] & (kDestroyed | kErrored)) !== 0) {
    return false;
  }

  state[kState] &= ~kReading;
  if ((state[kState] & kDecoder) !== 0 && !encoding) {
    chunk = state[kDecoderValue].write(chunk);
    if (chunk.length === 0) {
      maybeReadMore(stream, state);
      return canPushMore(state);
    }
  }

  addChunk(stream, state, chunk, false);
  return canPushMore(state);
}

function readableAddChunkPushObjectMode(stream, state, chunk, encoding) {
  if (chunk === null) {
    state[kState] &= ~kReading;
    onEofChunk(stream, state);
    return false;
  }

  if ((state[kState] & kEnded) !== 0) {
    errorOrDestroy(stream, new ERR_STREAM_PUSH_AFTER_EOF());
    return false;
  }

  if ((state[kState] & (kDestroyed | kErrored)) !== 0) {
    return false;
  }

  state[kState] &= ~kReading;

  if ((state[kState] & kDecoder) !== 0 && !encoding) {
    chunk = state[kDecoderValue].write(chunk);
  }

  addChunk(stream, state, chunk, false);
  return canPushMore(state);
}

function canPushMore(state) {
  // We can push more data if we are below the highWaterMark.
  // Also, if we have no data yet, we can stand some more bytes.
  // This is to work around cases where hwm=0, such as the repl.
  return (state[kState] & kEnded) === 0 &&
    (state.length < state.highWaterMark || state.length === 0);
}

function addChunk(stream, state, chunk, addToFront) {
  if ((state[kState] & (kFlowing | kSync | kDataListening)) === (kFlowing | kDataListening) && state.length === 0) {
    // Use the guard to avoid creating `Set()` repeatedly
    // when we have multiple pipes.
    if ((state[kState] & kMultiAwaitDrain) !== 0) {
      state.awaitDrainWriters.clear();
    } else {
      state.awaitDrainWriters = null;
    }

    state[kState] |= kDataEmitted;
    stream.emit('data', chunk);
  } else {
    // Update the buffer info.
    state.length += (state[kState] & kObjectMode) !== 0 ? 1 : chunk.length;
    if (addToFront) {
      if (state.bufferIndex > 0) {
        state.buffer[--state.bufferIndex] = chunk;
      } else {
        state.buffer.unshift(chunk); // Slow path
      }
    } else {
      state.buffer.push(chunk);
    }

    if ((state[kState] & kNeedReadable) !== 0)
      emitReadable(stream);
  }
  maybeReadMore(stream, state);
}

Readable.prototype.isPaused = function() {
  const state = this._readableState;
  return (state[kState] & kPaused) !== 0 || (state[kState] & (kHasFlowing | kFlowing)) === kHasFlowing;
};

// Backwards compatibility.
Readable.prototype.setEncoding = function(enc) {
  const state = this._readableState;

  const decoder = new (require('string_decoder').StringDecoder)(enc);
  state.decoder = decoder;
  // If setEncoding(null), decoder.encoding equals utf8.
  state.encoding = state.decoder.encoding;

  // Iterate over current buffer to convert already stored Buffers:
  let content = '';
  for (const data of state.buffer.slice(state.bufferIndex)) {
    content += decoder.write(data);
  }
  if ((state[kState] & kEnded) !== 0)
    content += decoder.end();
  state.buffer.length = 0;
  state.bufferIndex = 0;

  if (content !== '')
    state.buffer.push(content);
  state.length = content.length;
  return this;
};

Readable._fromList = fromList;

// Pluck off n bytes from an array of buffers.
// Length is the combined lengths of all the buffers in the list.
// This function is designed to be inlinable, so please take care when making
// changes to the function body.
function fromList(n, state) {
  // `state.length` cannot change while this function runs (only the
  // caller updates it, after this returns) and the chunk lengths feeding
  // the copy loops cannot change across the copy calls, so every
  // repeated property load below is hoisted into a local.
  const stateLength = state.length;

  // nothing buffered.
  if (stateLength === 0)
    return null;

  let idx = state.bufferIndex;
  let ret;

  const buf = state.buffer;
  const len = buf.length;

  if ((state[kState] & kObjectMode) !== 0) {
    ret = buf[idx];
    buf[idx++] = null;
  } else if (!n || n >= stateLength) {
    // Read it all, truncate the list.
    if ((state[kState] & kDecoder) !== 0) {
      ret = '';
      while (idx < len) {
        ret += buf[idx];
        buf[idx++] = null;
      }
    } else if (len - idx === 0) {
      ret = new FastBuffer();
    } else if (len - idx === 1) {
      ret = buf[idx];
      buf[idx++] = null;
    } else {
      ret = Buffer.allocUnsafe(stateLength);

      let i = 0;
      while (idx < len) {
        const data = buf[idx];
        TypedArrayPrototypeSet(ret, data, i);
        i += data.length;
        buf[idx++] = null;
      }
    }
  } else {
    const first = buf[idx];
    const firstLength = first.length;
    if (n < firstLength) {
      // `slice` is the same for buffers and strings.
      ret = first.slice(0, n);
      buf[idx] = first.slice(n);
    } else if (n === firstLength) {
      // First chunk is a perfect match.
      ret = first;
      buf[idx++] = null;
    } else if ((state[kState] & kDecoder) !== 0) {
      ret = '';
      while (idx < len) {
        const str = buf[idx];
        const strLength = str.length;
        if (n > strLength) {
          ret += str;
          n -= strLength;
          buf[idx++] = null;
        } else {
          if (n === strLength) {
            ret += str;
            buf[idx++] = null;
          } else {
            ret += str.slice(0, n);
            buf[idx] = str.slice(n);
          }
          break;
        }
      }
    } else {
      ret = Buffer.allocUnsafe(n);

      const retLen = n;
      while (idx < len) {
        const data = buf[idx];
        const dataLength = data.length;
        if (n > dataLength) {
          TypedArrayPrototypeSet(ret, data, retLen - n);
          n -= dataLength;
          buf[idx++] = null;
        } else {
          if (n === dataLength) {
            TypedArrayPrototypeSet(ret, data, retLen - n);
            buf[idx++] = null;
          } else {
            TypedArrayPrototypeSet(ret, new FastBuffer(data.buffer, data.byteOffset, n), retLen - n);
            buf[idx] = new FastBuffer(data.buffer, data.byteOffset + n, dataLength - n);
          }
          break;
        }
      }
    }
  }

  if (idx === len) {
    state.buffer.length = 0;
    state.bufferIndex = 0;
  } else if (idx > 1024) {
    state.buffer.splice(0, idx);
    state.bufferIndex = 0;
  } else {
    state.bufferIndex = idx;
  }

  return ret;
}

function endReadable(stream) {
  const state = stream._readableState;

  debug('endReadable');
  if ((state[kState] & (kEndEmitted | kEndScheduled)) === 0) {
    state[kState] |= kEnded | kEndScheduled;
    process.nextTick(endReadableNT, state, stream);
  }
}

function endReadableNT(state, stream) {
  debug('endReadableNT');

  // The scheduled tick is running; allow endReadable() to schedule again.
  // This matters both when the 'end' emission is skipped below (e.g. after
  // an unshift()) and when the stream is later reset for reuse
  // (see undestroy()), which clears kEndEmitted but not this flag.
  state[kState] &= ~kEndScheduled;

  // Check that we didn't get one last unshift.
  if ((state[kState] & (kErrored | kCloseEmitted | kEndEmitted)) === 0 && state.length === 0) {
    state[kState] |= kEndEmitted;
    stream.emit('end');

    if (stream.writable && stream.allowHalfOpen === false) {
      process.nextTick(endWritableNT, stream);
    } else if (state.autoDestroy) {
      // In case of duplex streams we need a way to detect
      // if the writable side is ready for autoDestroy as well.
      const wState = stream._writableState;
      const autoDestroy = !wState || (
        wState.autoDestroy &&
        // We don't expect the writable to ever 'finish'
        // if writable is explicitly set to false.
        (wState.finished || wState.writable === false)
      );

      if (autoDestroy) {
        stream.destroy();
      }
    }
  }
}

function endWritableNT(stream) {
  const writable = stream.writable && !stream.writableEnded &&
    !stream.destroyed;
  if (writable) {
    stream.end();
  }
}

Readable.from = function(iterable, opts) {
  return from(Readable, iterable, opts);
};

let webStreamsAdapters;

// Lazy to avoid circular references
function lazyWebStreams() {
  if (webStreamsAdapters === undefined)
    webStreamsAdapters = require('internal/webstreams/adapters');
  return webStreamsAdapters;
}

Readable.fromWeb = function(readableStream, options) {
  return lazyWebStreams().newStreamReadableFromReadableStream(
    readableStream,
    options);
};

Readable.toWeb = function(streamReadable, options) {
  return lazyWebStreams().newReadableStreamFromStreamReadable(
    streamReadable,
    options);
};

Readable.wrap = function(src, options) {
  return new Readable({
    objectMode: src.readableObjectMode ?? src.objectMode ?? true,
    ...options,
    destroy(err, callback) {
      destroyImpl.destroyer(src, err);
      callback(err);
    },
  }).wrap(src);
};
