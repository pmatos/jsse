'use strict';

const {
  ArrayPrototypeFilter,
  ObjectKeys,
  PromisePrototypeThen,
  PromiseResolve,
  PromiseWithResolvers,
  SafePromiseAllReturnVoid,
  SafePromisePrototypeFinally,
  SafeSet,
  StringPrototypeStartsWith,
  Symbol,
  TypeError,
  TypedArrayPrototypeGetBuffer,
  TypedArrayPrototypeGetByteLength,
  TypedArrayPrototypeGetByteOffset,
  Uint8Array,
} = primordials;

const { TextEncoder } = require('internal/encoding');

const {
  ReadableStream,
  isReadableStream,
} = require('internal/webstreams/readablestream');

const {
  WritableStream,
  isWritableStream,
} = require('internal/webstreams/writablestream');

const {
  CountQueuingStrategy,
  ByteLengthQueuingStrategy,
} = require('internal/webstreams/queuingstrategies');

const {
  Writable,
  Readable,
  Duplex,
  destroy,
} = require('stream');

const {
  isDestroyed,
  isReadable,
  isWritable,
  isWritableEnded,
} = require('internal/streams/utils');

const {
  Buffer,
} = require('buffer');

const {
  isAnyArrayBuffer,
} = require('internal/util/types');

const {
  AbortError,
  codes: {
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_ARG_VALUE,
    ERR_INVALID_STATE,
    ERR_STREAM_PREMATURE_CLOSE,
  },
} = require('internal/errors');

const {
  getDeprecationWarningEmitter,
  kEmptyObject,
  normalizeEncoding,
  setOwnProperty,
} = require('internal/util');

const {
  validateBoolean,
  validateFunction,
  validateObject,
  validateOneOf,
} = require('internal/validators');

const {
  eos,
  kEosNodeSynchronousCallback,
} = require('internal/streams/end-of-stream');

const encoder = new TextEncoder();

const kValidateChunk = Symbol('kValidateChunk');
const kDestroyOnSyncError = Symbol('kDestroyOnSyncError');

// Collect all negative (error) ZLIB codes and Z_NEED_DICT
let zlibFailures;
function isZlibFailure(code) {
  if (zlibFailures === undefined) {
    const zlib = require('zlib').constants;
    zlibFailures = new SafeSet(
      ArrayPrototypeFilter(
        ObjectKeys(zlib),
        (name) => name === 'Z_NEED_DICT' || zlib[name] < 0,
      ),
    );
  }
  return zlibFailures.has(code);
}

/**
 * @param {Error|null} cause
 * @returns {Error|null}
 */
function handleKnownInternalErrors(cause) {
  const causeCode = cause?.code;
  switch (true) {
    case causeCode === 'ERR_STREAM_PREMATURE_CLOSE': {
      return new AbortError(undefined, { cause });
    }
    case isZlibFailure(causeCode):
    // Brotli decoder error codes are formatted as 'ERR_' +
    // BrotliDecoderErrorString(), where the latter returns strings like
    // '_ERROR_FORMAT_...', '_ERROR_ALLOC_...', '_ERROR_UNREACHABLE', etc.
    // The resulting JS error codes all start with 'ERR__ERROR_'.
    // Falls through
    case causeCode != null &&
      StringPrototypeStartsWith(causeCode, 'ERR__ERROR_'): {
      // eslint-disable-next-line no-restricted-syntax
      const error = new TypeError(undefined, { cause });
      setOwnProperty(error, 'code', causeCode);
      return error;
    }
    default:
      return cause;
  }
}

const noop = () => {};

/**
 * @typedef {import('../../stream').Writable} Writable
 * @typedef {import('../../stream').Readable} Readable
 * @typedef {import('./writablestream').WritableStream} WritableStream
 * @typedef {import('./readablestream').ReadableStream} ReadableStream
 */

/**
 * @typedef {import('../abort_controller').AbortSignal} AbortSignal
 */

/**
 * @param {Writable} streamWritable
 * @param {object} [options]
 * @returns {WritableStream}
 */
function newWritableStreamFromStreamWritable(streamWritable, options = kEmptyObject) {
  // Not using the internal/streams/utils isWritableNodeStream utility
  // here because it will return false if streamWritable is a Duplex
  // whose writable option is false. For a Duplex that is not writable,
  // we want it to pass this check but return a closed WritableStream.
  // We check if the given stream is a stream.Writable or http.OutgoingMessage
  const checkIfWritableOrOutgoingMessage =
    streamWritable &&
    typeof streamWritable?.write === 'function' &&
    typeof streamWritable?.on === 'function';
  if (!checkIfWritableOrOutgoingMessage) {
    throw new ERR_INVALID_ARG_TYPE(
      'streamWritable',
      'stream.Writable',
      streamWritable,
    );
  }

  if (isDestroyed(streamWritable) || !isWritable(streamWritable)) {
    const writable = new WritableStream();
    writable.close();
    return writable;
  }

  const highWaterMark = streamWritable.writableHighWaterMark;
  const strategy =
    streamWritable.writableObjectMode ?
      new CountQueuingStrategy({ highWaterMark }) :
      {
        highWaterMark,
        size(chunk) {
          return chunk?.byteLength ?? chunk?.length ?? 1;
        },
      };

  let controller;
  let backpressurePromise;
  let closed;

  function onDrain() {
    backpressurePromise?.resolve();
  }

  const cleanup = eos(streamWritable, (error) => {
    error = handleKnownInternalErrors(error);

    cleanup();
    // This is a protection against non-standard, legacy streams
    // that happen to emit an error event again after finished is called.
    streamWritable.on('error', () => {});
    if (error != null) {
      backpressurePromise?.reject(error);
      // If closed is not undefined, the error is happening
      // after the WritableStream close has already started.
      // We need to reject it here.
      if (closed !== undefined) {
        closed.reject(error);
        closed = undefined;
      }
      controller?.error(error);
      controller = undefined;
      return;
    }

    if (closed !== undefined) {
      closed.resolve();
      closed = undefined;
      return;
    }
    controller?.error(new AbortError());
    controller = undefined;
  });

  streamWritable.on('drain', onDrain);

  return new WritableStream({
    start(c) { controller = c; },

    write(chunk) {
      try {
        options[kValidateChunk]?.(chunk);
        if (!streamWritable.writableObjectMode && isAnyArrayBuffer(chunk)) {
          chunk = new Uint8Array(chunk);
        }
        if (streamWritable.writableNeedDrain || !streamWritable.write(chunk)) {
          backpressurePromise = PromiseWithResolvers();
          if (!streamWritable.writableNeedDrain) {
            backpressurePromise.resolve();
          }
          return SafePromisePrototypeFinally(
            backpressurePromise.promise, () => {
              backpressurePromise = undefined;
            });
        }
      } catch (error) {
        // When the kDestroyOnSyncError flag is set (e.g. for
        // CompressionStream), a sync throw must also destroy the
        // stream so the readable side is errored too. Without this
        // the readable side hangs forever. This replicates the
        // TransformStream semantics: error both sides on any throw
        // in the transform path.
        if (options[kDestroyOnSyncError]) {
          destroy(streamWritable, error);
        }
        throw error;
      }
    },

    abort(reason) {
      destroy(streamWritable, reason);
    },

    close() {
      if (closed === undefined && !isWritableEnded(streamWritable)) {
        closed = PromiseWithResolvers();
        streamWritable.end();
        return closed.promise;
      }

      controller = undefined;
      return PromiseResolve();
    },
  }, strategy);
}

/**
 * @param {WritableStream} writableStream
 * @param {{
 *   decodeStrings? : boolean,
 *   highWaterMark? : number,
 *   objectMode? : boolean,
 *   signal? : AbortSignal,
 * }} [options]
 * @returns {Writable}
 */
function newStreamWritableFromWritableStream(writableStream, options = kEmptyObject) {
  if (!isWritableStream(writableStream)) {
    throw new ERR_INVALID_ARG_TYPE(
      'writableStream',
      'WritableStream',
      writableStream);
  }

  validateObject(options, 'options');
  const {
    highWaterMark,
    decodeStrings = true,
    objectMode = false,
    signal,
  } = options;

  validateBoolean(objectMode, 'options.objectMode');
  validateBoolean(decodeStrings, 'options.decodeStrings');

  const writer = writableStream.getWriter();
  let closed = false;

  const writable = new Writable({
    highWaterMark,
    objectMode,
    decodeStrings,
    signal,

    writev(chunks, callback) {
      function done(error) {
        try {
          callback(error);
        } catch (error) {
          // In a next tick because this is happening within
          // a promise context, and if there are any errors
          // thrown we don't want those to cause an unhandled
          // rejection. Let's just escape the promise and
          // handle it separately.
          process.nextTick(() => destroy(writable, error));
        }
      }

      PromisePrototypeThen(
        PromisePrototypeThen(
          writer.ready,
          () => SafePromiseAllReturnVoid(
            chunks,
            (data) => writer.write(data.chunk))),
        done,
        done);
    },

    write(chunk, encoding, callback) {
      if (typeof chunk === 'string' && decodeStrings && !objectMode) {
        const enc = normalizeEncoding(encoding);

        if (enc === 'utf8') {
          chunk = encoder.encode(chunk);
        } else {
          chunk = Buffer.from(chunk, encoding);
          chunk = new Uint8Array(
            TypedArrayPrototypeGetBuffer(chunk),
            TypedArrayPrototypeGetByteOffset(chunk),
            TypedArrayPrototypeGetByteLength(chunk),
          );
        }
      }

      function done(error) {
        try {
          callback(error);
        } catch (error) {
          destroy(writable, error);
        }
      }

      PromisePrototypeThen(
        PromisePrototypeThen(writer.ready, () => writer.write(chunk)),
        done,
        done);
    },

    destroy(error, callback) {
      function done() {
        try {
          callback(error);
        } catch (error) {
          // In a next tick because this is happening within
          // a promise context, and if there are any errors
          // thrown we don't want those to cause an unhandled
          // rejection. Let's just escape the promise and
          // handle it separately.
          process.nextTick(() => { throw error; });
        }
      }

      if (!closed) {
        if (error != null) {
          PromisePrototypeThen(
            writer.abort(error),
            done,
            done);
        } else {
          PromisePrototypeThen(
            writer.close(),
            done,
            done);
        }
        return;
      }

      done();
    },

    final(callback) {
      function done(error) {
        try {
          callback(error);
        } catch (error) {
          // In a next tick because this is happening within
          // a promise context, and if there are any errors
          // thrown we don't want those to cause an unhandled
          // rejection. Let's just escape the promise and
          // handle it separately.
          process.nextTick(() => destroy(writable, error));
        }
      }

      if (!closed) {
        PromisePrototypeThen(
          writer.close(),
          done,
          done);
      }
    },
  });

  PromisePrototypeThen(
    writer.closed,
    () => {
      // If the WritableStream closes before the stream.Writable has been
      // ended, we signal an error on the stream.Writable.
      closed = true;
      if (!isWritableEnded(writable))
        destroy(writable, new ERR_STREAM_PREMATURE_CLOSE());
    },
    (error) => {
      // If the WritableStream errors before the stream.Writable has been
      // destroyed, signal an error on the stream.Writable.
      closed = true;
      destroy(writable, error);
    });

  return writable;
}

const kErrorSentinelAttached = Symbol('kErrorSentinelAttached');

/**
 * @typedef {import('./queuingstrategies').QueuingStrategy} QueuingStrategy
 * @param {Readable} streamReadable
 * @param {{
 *  strategy? : QueuingStrategy
 *  type? : 'bytes',
 * }} [options]
 * @returns {ReadableStream}
 */
function newReadableStreamFromStreamReadable(streamReadable, options = kEmptyObject) {
  // Not using the internal/streams/utils isReadableNodeStream utility
  // here because it will return false if streamReadable is a Duplex
  // whose readable option is false. For a Duplex that is not readable,
  // we want it to pass this check but return a closed ReadableStream.
  if (typeof streamReadable?._readableState !== 'object') {
    throw new ERR_INVALID_ARG_TYPE(
      'streamReadable',
      'stream.Readable',
      streamReadable);
  }
  validateObject(options, 'options');
  if (options.type !== undefined) {
    validateOneOf(options.type, 'options.type', ['bytes', undefined]);
  }

  const isBYOB = options.type === 'bytes';
  let controller;
  let wasCanceled = false;
  let strategy;

  /** @type {UnderlyingSource} */
  const underlyingSource = {
    __proto__: null,
    type: isBYOB ? 'bytes' : undefined,
    start(c) { controller = c; },
    cancel(reason) {
      wasCanceled = true;
      destroy(streamReadable, reason);
    },
  };

  const readable = isReadable(streamReadable);
  const objectMode = streamReadable.readableObjectMode;
  if (readable) {
    underlyingSource.pull = function pull() {
      streamReadable.resume();
    };

    const highWaterMark = streamReadable.readableHighWaterMark;
    strategy = isBYOB ? { highWaterMark } :
      options.strategy ?? new (objectMode ? CountQueuingStrategy : ByteLengthQueuingStrategy)({ highWaterMark });
  }
  const readableStream = new ReadableStream(underlyingSource, strategy);

  // When adapting a Duplex as a ReadableStream, readable completion should not
  // wait for a half-open writable side to finish as well.
  let cleanup = noop;
  cleanup = eos(streamReadable, {
    __proto__: null,
    writable: false,
    [kEosNodeSynchronousCallback]: true,
  }, (error) => {
    error = handleKnownInternalErrors(error);

    // If eos calls the callback synchronously, cleanup is still a no-op here.
    cleanup();

    if (!(kErrorSentinelAttached in streamReadable)) {
      // This is a protection against non-standard, legacy streams
      // that happen to emit an error event again after finished is called.
      streamReadable.on('error', noop);
      streamReadable[kErrorSentinelAttached] = true;
    }
    if (wasCanceled) {
      return;
    }
    wasCanceled = true;
    if (error)
      return controller.error(error);
    controller.close();
    if (isBYOB)
      controller.byobRequest?.respond(0);
  });

  if (wasCanceled) {
    // `eos` called the callback synchronously
    cleanup();
  } else if (readable) {
    streamReadable.pause();

    streamReadable.on('data', function onData(chunk) {
      // Copy the Buffer to detach it from the pool.
      if (Buffer.isBuffer(chunk) && !objectMode)
        chunk = new Uint8Array(chunk);
      controller.enqueue(chunk);
      if (controller.desiredSize <= 0)
        streamReadable.pause();
    });
  }

  return readableStream;
}

/**
 * @param {ReadableStream} readableStream
 * @param {{
 *   highWaterMark? : number,
 *   encoding? : string,
 *   objectMode? : boolean,
 *   signal? : AbortSignal,
 * }} [options]
 * @returns {Readable}
 */
function newStreamReadableFromReadableStream(readableStream, options = kEmptyObject) {
  if (!isReadableStream(readableStream)) {
    throw new ERR_INVALID_ARG_TYPE(
      'readableStream',
      'ReadableStream',
      readableStream);
  }

  validateObject(options, 'options');
  const {
    highWaterMark,
    encoding,
    objectMode = false,
    signal,
  } = options;

  if (encoding !== undefined && !Buffer.isEncoding(encoding))
    throw new ERR_INVALID_ARG_VALUE('options.encoding', encoding);
  validateBoolean(objectMode, 'options.objectMode');

  const reader = readableStream.getReader();
  let closed = false;

  const readable = new Readable({
    objectMode,
    highWaterMark,
    encoding,
    signal,

    read() {
      PromisePrototypeThen(
        reader.read(),
        (chunk) => {
          if (chunk.done) {
            // Value should always be undefined here.
            readable.push(null);
          } else {
            readable.push(chunk.value);
          }
        },
        (error) => destroy(readable, error));
    },

    destroy(error, callback) {
      function done() {
        try {
          callback(error);
        } catch (error) {
          // In a next tick because this is happening within
          // a promise context, and if there are any errors
          // thrown we don't want those to cause an unhandled
          // rejection. Let's just escape the promise and
          // handle it separately.
          process.nextTick(() => { throw error; });
        }
      }

      if (!closed) {
        PromisePrototypeThen(
          reader.cancel(error),
          done,
          done);
        return;
      }
      done();
    },
  });

  PromisePrototypeThen(
    reader.closed,
    () => {
      closed = true;
    },
    (error) => {
      closed = true;
      destroy(readable, error);
    });

  return readable;
}

const emitDEP0201 = getDeprecationWarningEmitter(
  'DEP0201',
  "Passing 'options.type' to Duplex.toWeb() is deprecated. " +
      "To specify the ReadableStream type, use 'options.readableType'.",
  newReadableWritablePairFromDuplex);

/**
 * @typedef {import('./readablestream').ReadableWritablePair
 * } ReadableWritablePair
 * @typedef {import('../../stream').Duplex} Duplex
 */
