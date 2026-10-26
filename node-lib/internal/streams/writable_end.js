// Derived from Node.js lib/internal/streams/writable.js (v26.8.2), split to fit the embedded
// file budget. Copyright Joyent, Inc. and other Node contributors. MIT license.

'use strict';

exports.needFinish = needFinish;
exports.onFinish = onFinish;
exports.prefinish = prefinish;
exports.finishMaybe = finishMaybe;
exports.finish = finish;
exports.callFinishedCallbacks = callFinishedCallbacks;
exports.lazyWebStreams = lazyWebStreams;

const {
  Error,
  ObjectDefineProperties,
  Promise,
  SymbolAsyncDispose,
} = primordials;
const EE = require('events');
const destroyImpl = require('internal/streams/destroy');
const {
  eos,
} = require('internal/streams/end-of-stream');
const {
  AbortError,
  codes: {
    ERR_METHOD_NOT_IMPLEMENTED,
    ERR_MULTIPLE_CALLBACK,
    ERR_STREAM_ALREADY_FINISHED,
    ERR_STREAM_DESTROYED,
  },
} = require('internal/errors');
const {
  kState,
  kObjectMode,
  kErrorEmitted,
  kAutoDestroy,
  kDestroyed,
  kClosed,
  kCloseEmitted,
  kErrored,
  kConstructed,
} = require('internal/streams/utils');
const Writable = require('internal/streams/writable');
const { errorOrDestroy, kOnFinishedValue, kErroredValue, kSync, kFinalCalled, kNeedDrain, kEnding, kFinished, kWriting, kPrefinished, kOnFinished, kHasWritable, kWritable, kCorked, kBuffered, kEnded } = require('internal/streams/writable_state');
const { _write, errorBuffer } = require('internal/streams/writable_write');

Writable.prototype._write = function(chunk, encoding, cb) {
  if (this._writev) {
    this._writev([{ chunk, encoding }], cb);
  } else {
    throw new ERR_METHOD_NOT_IMPLEMENTED('_write()');
  }
};

Writable.prototype._writev = null;

Writable.prototype.end = function(chunk, encoding, cb) {
  const state = this._writableState;

  if (typeof chunk === 'function') {
    cb = chunk;
    chunk = null;
    encoding = null;
  } else if (typeof encoding === 'function') {
    cb = encoding;
    encoding = null;
  }

  let err;

  if (chunk != null) {
    const ret = _write(this, chunk, encoding);
    if (ret instanceof Error) {
      err = ret;
    }
  }

  // .end() fully uncorks.
  if ((state[kState] & kCorked) !== 0) {
    state.corked = 1;
    this.uncork();
  }

  if (err) {
    // Do nothing...
  } else if ((state[kState] & (kEnding | kErrored)) === 0) {
    // This is forgiving in terms of unnecessary calls to end() and can hide
    // logic errors. However, usually such errors are harmless and causing a
    // hard error can be disproportionately destructive. It is not always
    // trivial for the user to determine whether end() needs to be called
    // or not.

    state[kState] |= kEnding;
    finishMaybe(this, state, true);
    state[kState] |= kEnded;
  } else if ((state[kState] & kFinished) !== 0) {
    err = new ERR_STREAM_ALREADY_FINISHED('end');
  } else if ((state[kState] & kDestroyed) !== 0) {
    err = new ERR_STREAM_DESTROYED('end');
  }

  if (typeof cb === 'function') {
    if (err) {
      process.nextTick(cb, err);
    } else if ((state[kState] & kErrored) !== 0) {
      process.nextTick(cb, state[kErroredValue]);
    } else if ((state[kState] & kFinished) !== 0) {
      process.nextTick(cb, null);
    } else {
      state[kState] |= kOnFinished;
      state[kOnFinishedValue] ??= [];
      state[kOnFinishedValue].push(cb);
    }
  }

  return this;
};

function needFinish(state) {
  return (
    // State is ended && constructed but not destroyed, finished, writing, errorEmitted or closedEmitted
    (state[kState] & (
      kEnding |
          kDestroyed |
          kConstructed |
          kFinished |
          kWriting |
          kErrorEmitted |
          kCloseEmitted |
          kErrored |
          kBuffered
    )) === (kEnding | kConstructed) && state.length === 0);
}

function onFinish(stream, state, err) {
  if ((state[kState] & kPrefinished) !== 0) {
    errorOrDestroy(stream, err ?? new ERR_MULTIPLE_CALLBACK());
    return;
  }
  state.pendingcb--;
  if (err) {
    callFinishedCallbacks(state, err);
    errorOrDestroy(stream, err, (state[kState] & kSync) !== 0);
  } else if (needFinish(state)) {
    state[kState] |= kPrefinished;
    stream.emit('prefinish');
    // Backwards compat. Don't check state.sync here.
    // Some streams assume 'finish' will be emitted
    // asynchronously relative to _final callback.
    state.pendingcb++;
    process.nextTick(finish, stream, state);
  }
}

function prefinish(stream, state) {
  if ((state[kState] & (kPrefinished | kFinalCalled)) !== 0) {
    return;
  }

  if (typeof stream._final === 'function' && (state[kState] & kDestroyed) === 0) {
    state[kState] |= kFinalCalled | kSync;
    state.pendingcb++;

    try {
      stream._final((err) => onFinish(stream, state, err));
    } catch (err) {
      onFinish(stream, state, err);
    }

    state[kState] &= ~kSync;
  } else {
    state[kState] |= kFinalCalled | kPrefinished;
    stream.emit('prefinish');
  }
}

function finishMaybe(stream, state, sync) {
  if (needFinish(state)) {
    prefinish(stream, state);
    if (state.pendingcb === 0) {
      if (sync) {
        state.pendingcb++;
        process.nextTick((stream, state) => {
          if (needFinish(state)) {
            finish(stream, state);
          } else {
            state.pendingcb--;
          }
        }, stream, state);
      } else if (needFinish(state)) {
        state.pendingcb++;
        finish(stream, state);
      }
    }
  }
}

function finish(stream, state) {
  state.pendingcb--;
  state[kState] |= kFinished;

  callFinishedCallbacks(state, null);

  stream.emit('finish');

  if ((state[kState] & kAutoDestroy) !== 0) {
    // In case of duplex streams we need a way to detect
    // if the readable side is ready for autoDestroy as well.
    const rState = stream._readableState;
    const autoDestroy = !rState || (
      rState.autoDestroy &&
      // We don't expect the readable to ever 'end'
      // if readable is explicitly set to false.
      (rState.endEmitted || rState.readable === false)
    );
    if (autoDestroy) {
      stream.destroy();
    }
  }
}

function callFinishedCallbacks(state, err) {
  if ((state[kState] & kOnFinished) === 0) {
    return;
  }

  const onfinishCallbacks = state[kOnFinishedValue];
  state[kOnFinishedValue] = null;
  state[kState] &= ~kOnFinished;
  for (let i = 0; i < onfinishCallbacks.length; i++) {
    onfinishCallbacks[i](err);
  }
}

ObjectDefineProperties(Writable.prototype, {
  closed: {
    __proto__: null,
    get() {
      return this._writableState ? (this._writableState[kState] & kClosed) !== 0 : false;
    },
  },

  destroyed: {
    __proto__: null,
    get() {
      return this._writableState ? (this._writableState[kState] & kDestroyed) !== 0 : false;
    },
    set(value) {
      // Backward compatibility, the user is explicitly managing destroyed.
      if (!this._writableState) return;

      if (value) this._writableState[kState] |= kDestroyed;
      else this._writableState[kState] &= ~kDestroyed;
    },
  },

  writable: {
    __proto__: null,
    get() {
      const w = this._writableState;
      // w.writable === false means that this is part of a Duplex stream
      // where the writable side was disabled upon construction.
      // Compat. The user might manually disable writable side through
      // deprecated setter.
      return !!w && w.writable !== false &&
        (w[kState] & (kEnding | kEnded | kDestroyed | kErrored)) === 0;
    },
    set(val) {
      // Backwards compatible.
      if (this._writableState) {
        this._writableState.writable = !!val;
      }
    },
  },

  writableFinished: {
    __proto__: null,
    get() {
      const state = this._writableState;
      return state ? (state[kState] & kFinished) !== 0 : false;
    },
  },

  writableObjectMode: {
    __proto__: null,
    get() {
      const state = this._writableState;
      return state ? (state[kState] & kObjectMode) !== 0 : false;
    },
  },

  writableBuffer: {
    __proto__: null,
    get() {
      const state = this._writableState;
      return state && state.getBuffer();
    },
  },

  writableEnded: {
    __proto__: null,
    get() {
      const state = this._writableState;
      return state ? (state[kState] & kEnding) !== 0 : false;
    },
  },

  writableNeedDrain: {
    __proto__: null,
    get() {
      const state = this._writableState;
      return state ? (state[kState] & (kDestroyed | kEnding | kNeedDrain)) === kNeedDrain : false;
    },
  },

  writableHighWaterMark: {
    __proto__: null,
    get() {
      const state = this._writableState;
      return state?.highWaterMark;
    },
  },

  writableCorked: {
    __proto__: null,
    get() {
      const state = this._writableState;
      return state ? state.corked : 0;
    },
  },

  writableLength: {
    __proto__: null,
    get() {
      const state = this._writableState;
      return state?.length;
    },
  },

  errored: {
    __proto__: null,
    enumerable: false,
    get() {
      const state = this._writableState;
      return state ? state.errored : null;
    },
  },

  writableAborted: {
    __proto__: null,
    get: function() {
      const state = this._writableState;
      return (
        (state[kState] & (kHasWritable | kWritable)) !== kHasWritable &&
        (state[kState] & (kDestroyed | kErrored)) !== 0 &&
        (state[kState] & kFinished) === 0
      );
    },
  },
});

const destroy = destroyImpl.destroy;
Writable.prototype.destroy = function(err, cb) {
  const state = this._writableState;

  // Invoke pending callbacks.
  if ((state[kState] & (kBuffered | kOnFinished)) !== 0 && (state[kState] & kDestroyed) === 0) {
    process.nextTick(errorBuffer, state);
  }

  destroy.call(this, err, cb);
  return this;
};

Writable.prototype._undestroy = destroyImpl.undestroy;
Writable.prototype._destroy = function(err, cb) {
  cb(err);
};

Writable.prototype[EE.captureRejectionSymbol] = function(err) {
  this.destroy(err);
};

let webStreamsAdapters;

// Lazy to avoid circular references
function lazyWebStreams() {
  if (webStreamsAdapters === undefined)
    webStreamsAdapters = require('internal/webstreams/adapters');
  return webStreamsAdapters;
}

Writable.fromWeb = function(writableStream, options) {
  return lazyWebStreams().newStreamWritableFromWritableStream(
    writableStream,
    options);
};

Writable.toWeb = function(streamWritable) {
  return lazyWebStreams().newWritableStreamFromStreamWritable(streamWritable);
};

Writable.prototype[SymbolAsyncDispose] = async function() {
  let error;
  if (!this.destroyed) {
    error = this.writableFinished ? null : new AbortError();
    this.destroy(error);
  }
  await new Promise((resolve, reject) =>
    eos(this, (err) => (err && err.name !== 'AbortError' ? reject(err) : resolve(null))),
  );
};
