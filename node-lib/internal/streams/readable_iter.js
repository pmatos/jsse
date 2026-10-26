// Derived from Node.js lib/internal/streams/readable.js (v26.8.2), split to fit the embedded
// file budget. Copyright Joyent, Inc. and other Node contributors. MIT license.

'use strict';

exports.streamToAsyncIterator = streamToAsyncIterator;
exports.createAsyncIterator = createAsyncIterator;

const {
  AsyncIteratorPrototype,
  FunctionPrototypeCall,
  ObjectDefineProperties,
  ObjectKeys,
  Promise,
  PromisePrototypeThen,
  PromiseReject,
  PromiseResolve,
  SymbolAsyncIterator,
} = primordials;
const {
  addAbortSignalNoValidate,
} = require('internal/streams/add-abort-signal');
const {
  eos,
} = require('internal/streams/end-of-stream');
const destroyImpl = require('internal/streams/destroy');
const {
  aggregateTwoErrors,
} = require('internal/errors');
const {
  validateAbortSignal,
  validateObject,
} = require('internal/validators');
const FixedQueue = require('internal/fixed_queue');
const Readable = require('internal/streams/readable');
const { nop, errorOrDestroy } = require('internal/streams/readable_state');

// Wrap an old-style stream as the async data source.
// This is *not* part of the readable stream interface.
// It is an ugly unfortunate mess of history.
Readable.prototype.wrap = function(stream) {
  let paused = false;

  // TODO (ronag): Should this.destroy(err) emit
  // 'error' on the wrapped stream? Would require
  // a static factory method, e.g. Readable.wrap(stream).

  stream.on('data', (chunk) => {
    if (!this.push(chunk) && stream.pause) {
      paused = true;
      stream.pause();
    }
  });

  stream.on('end', () => {
    this.push(null);
  });

  stream.on('error', (err) => {
    errorOrDestroy(this, err);
  });

  stream.on('close', () => {
    this.destroy();
  });

  stream.on('destroy', () => {
    this.destroy();
  });

  this._read = () => {
    if (paused && stream.resume) {
      paused = false;
      stream.resume();
    }
  };

  // Proxy all the other methods. Important when wrapping filters and duplexes.
  const streamKeys = ObjectKeys(stream);
  for (let j = 0; j < streamKeys.length; j++) {
    const i = streamKeys[j];
    if (this[i] === undefined && typeof stream[i] === 'function') {
      this[i] = stream[i].bind(stream);
    }
  }

  return this;
};

Readable.prototype[SymbolAsyncIterator] = function() {
  return streamToAsyncIterator(this);
};

Readable.prototype.iterator = function(options) {
  if (options !== undefined) {
    validateObject(options, 'options');
  }
  return streamToAsyncIterator(this, options);
};

function streamToAsyncIterator(stream, options) {
  if (typeof stream.read !== 'function') {
    stream = Readable.wrap(stream, { objectMode: true });
  }

  const iter = createAsyncIterator(stream, options);
  iter.stream = stream;
  return iter;
}

// Async iterator over a Readable. Requests received while another is
// outstanding are queued and processed in order.
function createAsyncIterator(stream, options) {
  let callback = nop;
  let error;            // undefined: active, null: ended cleanly, else: Error
  let started = false;
  let completed = false;
  let inFlight = false; // An asynchronous request is outstanding
  let queue = null;     // Requests received while inFlight
  let draining = false;
  let cleanup;

  // Used both as the 'readable' listener (where `this === stream`) and
  // as a promise executor storing the resolver that wakes up a pending
  // pump().
  function wakeup(resolve) {
    if (this === stream) {
      callback();
      callback = nop;
    } else {
      callback = resolve;
    }
  }

  function start() {
    started = true;

    stream.on('readable', wakeup);

    cleanup = eos(stream, { writable: false }, (err) => {
      error = err ? aggregateTwoErrors(error, err) : null;
      callback();
      callback = nop;
    });
  }

  // Complete the iterator and either destroy the stream or detach
  // from it.
  function finalize() {
    completed = true;

    const preserveHalfOpenDuplex =
      error === null &&
      stream.allowHalfOpen === true &&
      stream.writable === true &&
      stream.writableEnded !== true;

    if (
      (error || options?.destroyOnReturn !== false) &&
      (error === undefined || stream._readableState.autoDestroy) &&
      !preserveHalfOpenDuplex
    ) {
      destroyImpl.destroyer(stream, null);
    } else {
      stream.off('readable', wakeup);
      cleanup();
    }
  }

  function settleError(err, reject) {
    error = aggregateTwoErrors(error, err);
    finalize();
    reject(error);
  }

  function drain() {
    // Requests settled synchronously call back into drain(); the guard
    // keeps a single loop going instead of recursing once per request.
    if (draining) {
      return;
    }
    draining = true;
    try {
      while (!inFlight && !queue.isEmpty()) {
        const req = queue.shift();
        if (req.type === 'next') {
          processNext(req.resolve, req.reject);
        } else if (req.type === 'return') {
          processReturn(req.value, req.resolve);
        } else {
          processThrow(req.value, req.reject);
        }
      }
    } finally {
      draining = false;
    }
  }

  // Thenable chunks are unwrapped before delivery; a rejection tears
  // down the iterator and the stream.
  function onChunkFulfilled(value) {
    inFlight = false;
    if (queue !== null) drain();
    return { done: false, value };
  }

  function onChunkRejected(err) {
    inFlight = false;
    error = aggregateTwoErrors(error, err);
    finalize();
    if (queue !== null) drain();
    throw error;
  }

  // Runs with inFlight === true; settles the request and hands over to
  // any requests that queued up behind it.
  function pump(resolve, reject) {
    const chunk = stream.destroyed ? null : stream.read();
    if (chunk !== null) {
      // Read `then` only once so that a getter cannot observe (or throw
      // on) a second access.
      const then = chunk.then;
      if (typeof then === 'function') {
        FunctionPrototypeCall(then, chunk, (value) => {
          inFlight = false;
          resolve({ done: false, value });
          if (queue !== null) drain();
        }, (err) => {
          inFlight = false;
          settleError(err, reject);
          if (queue !== null) drain();
        });
        return;
      }
      inFlight = false;
      resolve({ done: false, value: chunk });
      if (queue !== null) drain();
    } else if (error) {
      inFlight = false;
      settleError(error, reject);
      if (queue !== null) drain();
    } else if (error === null) {
      inFlight = false;
      finalize();
      resolve({ done: true, value: undefined });
      if (queue !== null) drain();
    } else {
      // No data buffered yet; wait for 'readable' or end-of-stream and
      // retry.
      PromisePrototypeThen(new Promise(wakeup), () => pump(resolve, reject));
    }
  }

  function processNext(resolve, reject) {
    if (completed) {
      resolve({ done: true, value: undefined });
      return;
    }
    if (!started) start();
    inFlight = true;
    pump(resolve, reject);
  }

  function processReturn(value, resolve) {
    if (!completed) {
      if (started) {
        finalize();
      } else {
        // Never started: complete without touching the stream.
        completed = true;
      }
    }
    resolve({ done: true, value });
  }

  function processThrow(err, reject) {
    if (completed || !started) {
      completed = true;
      reject(err);
      return;
    }
    settleError(err, reject);
  }

  return {
    __proto__: AsyncIteratorPrototype,
    next() {
      if (!inFlight && !completed) {
        if (!started) start();
        // Fast path: a chunk is already buffered.
        const chunk = stream.destroyed ? null : stream.read();
        if (chunk !== null) {
          // Read `then` only once so that a getter cannot observe (or
          // throw on) a second access.
          const then = chunk.then;
          if (typeof then === 'function') {
            inFlight = true;
            return FunctionPrototypeCall(
              then, chunk, onChunkFulfilled, onChunkRejected);
          }
          return PromiseResolve({ done: false, value: chunk });
        }
        if (error) {
          finalize();
          return PromiseReject(error);
        }
        if (error === null) {
          finalize();
          return PromiseResolve({ done: true, value: undefined });
        }
        // No data buffered yet; wait for 'readable' or end-of-stream.
        inFlight = true;
        return new Promise((resolve, reject) => {
          PromisePrototypeThen(new Promise(wakeup), () => pump(resolve, reject));
        });
      }
      return new Promise((resolve, reject) => {
        if (inFlight) {
          queue ??= new FixedQueue();
          queue.push({ __proto__: null, type: 'next', value: undefined, resolve, reject });
        } else {
          resolve({ done: true, value: undefined });
        }
      });
    },
    return(value) {
      return new Promise((resolve, reject) => {
        if (inFlight) {
          queue ??= new FixedQueue();
          queue.push({ __proto__: null, type: 'return', value, resolve, reject });
        } else {
          processReturn(value, resolve);
        }
      });
    },
    throw(err) {
      return new Promise((resolve, reject) => {
        if (inFlight) {
          queue ??= new FixedQueue();
          queue.push({ __proto__: null, type: 'throw', value: err, resolve, reject });
        } else {
          processThrow(err, reject);
        }
      });
    },
  };
}

let composeImpl;

Readable.prototype.compose = function compose(stream, options) {
  if (options != null) {
    validateObject(options, 'options');
  }
  if (options?.signal != null) {
    validateAbortSignal(options.signal, 'options.signal');
  }

  composeImpl ??= require('internal/streams/compose');
  const composedStream = composeImpl(this, stream);

  if (options?.signal) {
    // Not validating as we already validated before
    addAbortSignalNoValidate(
      options.signal,
      composedStream,
    );
  }

  return composedStream;
};

// Making it explicit these properties are not enumerable
// because otherwise some prototype manipulation in
// userland will fail.
ObjectDefineProperties(Readable.prototype, {
  readable: {
    __proto__: null,
    get() {
      const r = this._readableState;
      // r.readable === false means that this is part of a Duplex stream
      // where the readable side was disabled upon construction.
      // Compat. The user might manually disable readable side through
      // deprecated setter.
      return !!r && r.readable !== false && !r.destroyed && !r.errorEmitted &&
        !r.endEmitted;
    },
    set(val) {
      // Backwards compat.
      if (this._readableState) {
        this._readableState.readable = !!val;
      }
    },
  },

  readableDidRead: {
    __proto__: null,
    enumerable: false,
    get: function() {
      return this._readableState.dataEmitted;
    },
  },

  readableAborted: {
    __proto__: null,
    enumerable: false,
    get: function() {
      return !!(
        this._readableState.readable !== false &&
        (this._readableState.destroyed || this._readableState.errored) &&
        !this._readableState.endEmitted
      );
    },
  },

  readableHighWaterMark: {
    __proto__: null,
    enumerable: false,
    get: function() {
      return this._readableState.highWaterMark;
    },
  },

  readableBuffer: {
    __proto__: null,
    enumerable: false,
    get: function() {
      return this._readableState?.buffer;
    },
  },

  readableFlowing: {
    __proto__: null,
    enumerable: false,
    get: function() {
      return this._readableState.flowing;
    },
    set: function(state) {
      if (this._readableState) {
        this._readableState.flowing = state;
      }
    },
  },

  readableLength: {
    __proto__: null,
    enumerable: false,
    get() {
      return this._readableState.length;
    },
  },

  readableObjectMode: {
    __proto__: null,
    enumerable: false,
    get() {
      return this._readableState ? this._readableState.objectMode : false;
    },
  },

  readableEncoding: {
    __proto__: null,
    enumerable: false,
    get() {
      return this._readableState ? this._readableState.encoding : null;
    },
  },

  errored: {
    __proto__: null,
    enumerable: false,
    get() {
      return this._readableState ? this._readableState.errored : null;
    },
  },

  closed: {
    __proto__: null,
    get() {
      return this._readableState ? this._readableState.closed : false;
    },
  },

  destroyed: {
    __proto__: null,
    enumerable: false,
    get() {
      return this._readableState ? this._readableState.destroyed : false;
    },
    set(value) {
      // We ignore the value if the stream
      // has not been initialized yet.
      if (!this._readableState) {
        return;
      }

      // Backward compatibility, the user is explicitly
      // managing destroyed.
      this._readableState.destroyed = value;
    },
  },

  readableEnded: {
    __proto__: null,
    enumerable: false,
    get() {
      return this._readableState ? this._readableState.endEmitted : false;
    },
  },

});
