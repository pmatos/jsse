'use strict';

const {
  FunctionPrototypeBind,
  FunctionPrototypeCall,
  ObjectDefineProperties,
  ObjectSetPrototypeOf,
  Promise,
  PromisePrototypeThen,
  PromiseReject,
  PromiseResolve,
  PromiseWithResolvers,
  Symbol,
  SymbolToStringTag,
} = primordials;

const {
  codes: {
    ERR_ILLEGAL_CONSTRUCTOR,
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_ARG_VALUE,
    ERR_INVALID_STATE,
    ERR_INVALID_THIS,
  },
} = require('internal/errors');

const {
  DOMException,
} = require('internal/per_context/domexception');

const {
  customInspectSymbol: kInspect,
  kEmptyObject,
  kEnumerableProperty,
  SideEffectFreeRegExpPrototypeSymbolReplace,
} = require('internal/util');

const {
  validateObject,
  kValidateObjectAllowObjects,
  kValidateObjectAllowObjectsAndNull,
} = require('internal/validators');

const {
  MessageChannel,
} = require('internal/worker/io');

const {
  kDeserialize,
  kTransfer,
  kTransferList,
  markTransferMode,
} = require('internal/worker/js_transferable');

const {
  Queue,
  createPromiseCallbackNoParams,
  createPromiseCallback1Param,
  createRawCallback2Params,
  customInspect,
  defaultSizeAlgorithm,
  dequeueValue,
  enqueueValueWithSize,
  extractHighWaterMark,
  extractSizeAlgorithm,
  getNonWritablePropertyDescriptor,
  isBrandCheck,
  isPromisePending,
  kEmptyQueue,
  kState,
  kType,
  lazyTransfer,
  nonOpCallback,
  nonOpCancel,
  peekQueueValue,
  rejectedHandledRecord,
  resetQueue,
  resolvedRecord,
  setPromiseHandled,
  thenAlgorithmResult,
} = require('internal/webstreams/util');

const {
  kIsClosedPromise,
  kIsErrored,
  kIsWritable,
  kControllerErrorFunction,
} = require('internal/streams/utils');

const {
  AbortController,
} = require('internal/abort_controller');

const {
  queueMicrotask,
} = require('internal/process/task_queues');

const assert = require('internal/assert');

const kAbort = Symbol('kAbort');
const kCloseSentinel = Symbol('kCloseSentinel');
const kError = Symbol('kError');
const kSkipThrow = Symbol('kSkipThrow');

// Shared sentinels for the "no pending request" state records. These
// records are only ever replaced wholesale and never mutated in place,
// so single shared instances are safe and avoid an allocation on every
// state reset (one per write on the hot path).
const kNilRequest = {
  __proto__: null,
  promise: undefined,
  resolve: undefined,
  reject: undefined,
};
const kNilPendingAbortRequest = {
  __proto__: null,
  abort: kNilRequest,
  reason: undefined,
  wasAlreadyErroring: false,
};

let releasedError;

function lazyWritableReleasedError() {
  if (releasedError) {
    return releasedError;
  }
  const userModuleRegExp = /^ {4}at (?:[^/\\(]+ \()(?!node:(.+):\d+:\d+\)$).*/gm;

  releasedError = new ERR_INVALID_STATE.TypeError('Writer has been released');
  // Avoid V8 leak and remove userland stackstrace
  releasedError.stack = SideEffectFreeRegExpPrototypeSymbolReplace(userModuleRegExp, releasedError.stack, '');
  return releasedError;
}

/**
 * @typedef {import('../abort_controller').AbortSignal} AbortSignal
 * @typedef {import('./queuingstrategies').QueuingStrategy
 * } QueuingStrategy
 * @typedef {import('./queuingstrategies').QueuingStrategySize
 * } QueuingStrategySize
 */

/**
 * @callback UnderlyingSinkStartCallback
 * @param {WritableStreamDefaultController} controller
 */

/**
 * @callback UnderlyingSinkWriteCallback
 * @param {any} chunk
 * @param {WritableStreamDefaultController} controller
 * @returns {Promise<void>}
 */

/**
 * @callback UnderlyingSinkCloseCallback
 * @returns {Promise<void>}
 */

/**
 * @callback UnderlyingSinkAbortCallback
 * @param {any} reason
 * @returns {Promise<void>}
 */

/**
 * @typedef {{
 *   start? : UnderlyingSinkStartCallback,
 *   write? : UnderlyingSinkWriteCallback,
 *   close? : UnderlyingSinkCloseCallback,
 *   abort? : UnderlyingSinkAbortCallback,
 *   type? : any,
 * }} UnderlyingSink
 */

class WritableStream {
  [kType] = 'WritableStream';

  /**
   * @param {UnderlyingSink} [sink]
   * @param {QueuingStrategy} [strategy]
   */
  constructor(sink = kEmptyObject, strategy = kEmptyObject) {
    markTransferMode(this, false, true);
    validateObject(sink, 'sink', kValidateObjectAllowObjects);
    validateObject(strategy, 'strategy', kValidateObjectAllowObjectsAndNull);
    const type = sink?.type;
    if (type !== undefined)
      throw new ERR_INVALID_ARG_VALUE.RangeError('type', type);

    this[kState] = createWritableStreamState();

    const size = extractSizeAlgorithm(strategy?.size);
    const highWaterMark = extractHighWaterMark(strategy?.highWaterMark, 1);

    setupWritableStreamDefaultControllerFromSink(
      this,
      sink,
      highWaterMark,
      size);
  }

  get [kIsErrored]() {
    return this[kState].state === 'errored';
  }

  get [kIsWritable]() {
    return this[kState].state === 'writable';
  }

  [kControllerErrorFunction](error) {
    // Used by the internal stream interop (addAbortSignal).
    this[kState].controller.error(error);
  }

  // Used by the internal stream interop (end-of-stream). Materialized
  // lazily since its settlement is derivable from the stream state.
  get [kIsClosedPromise]() {
    let cache = this[kState].closedPromise;
    if (cache === undefined) {
      switch (this[kState].state) {
        case 'writable':
        case 'erroring':
          cache = PromiseWithResolvers();
          break;
        case 'closed':
          cache = resolvedRecord();
          break;
        default:
          cache = rejectedHandledRecord(this[kState].storedError);
      }
      this[kState].closedPromise = cache;
    }
    return cache;
  }

  /**
   * @readonly
   * @type {boolean}
   */
  get locked() {
    if (!isWritableStream(this))
      throw new ERR_INVALID_THIS('WritableStream');
    return isWritableStreamLocked(this);
  }

  /**
   * @param {any} [reason]
   * @returns {Promise<void>}
   */
  abort(reason = undefined) {
    if (!isWritableStream(this))
      return PromiseReject(new ERR_INVALID_THIS('WritableStream'));
    if (isWritableStreamLocked(this)) {
      return PromiseReject(
        new ERR_INVALID_STATE.TypeError('WritableStream is locked'));
    }
    return writableStreamAbort(this, reason);
  }

  /**
   * @returns {Promise<void>}
   */
  close() {
    if (!isWritableStream(this))
      return PromiseReject(new ERR_INVALID_THIS('WritableStream'));
    if (isWritableStreamLocked(this)) {
      return PromiseReject(
        new ERR_INVALID_STATE.TypeError('WritableStream is locked'));
    }
    if (writableStreamCloseQueuedOrInFlight(this)) {
      return PromiseReject(
        new ERR_INVALID_STATE.TypeError('Failure closing WritableStream'));
    }
    return writableStreamClose(this);
  }

  /**
   * @returns {WritableStreamDefaultWriter}
   */
  getWriter() {
    if (!isWritableStream(this))
      throw new ERR_INVALID_THIS('WritableStream');
    // eslint-disable-next-line no-use-before-define
    return new WritableStreamDefaultWriter(this);
  }

  [kInspect](depth, options) {
    return customInspect(depth, options, this[kType], {
      locked: this.locked,
      state: this[kState].state,
    });
  }

  [kTransfer]() {
    if (!isWritableStream(this))
      throw new ERR_INVALID_THIS('WritableStream');
    if (this.locked) {
      this[kState].transfer.port1?.close();
      this[kState].transfer.port1 = undefined;
      this[kState].transfer.port2 = undefined;
      throw new DOMException(
        'Cannot transfer a locked WritableStream',
        'DataCloneError');
    }

    const {
      readable,
      promise,
    } = lazyTransfer().newCrossRealmReadableStream(
      this,
      this[kState].transfer.port1);

    this[kState].transfer.readable = readable;
    this[kState].transfer.promise = promise;

    return {
      data: { port: this[kState].transfer.port2 },
      deserializeInfo:
        'internal/webstreams/writablestream:TransferredWritableStream',
    };
  }

  [kTransferList]() {
    const { port1, port2 } = new MessageChannel();
    this[kState].transfer.port1 = port1;
    this[kState].transfer.port2 = port2;
    return [ port2 ];
  }

  [kDeserialize]({ port }) {
    const transfer = lazyTransfer();
    setupWritableStreamDefaultControllerFromSink(
      this,
      // The MessagePort is set to be referenced when reading.
      // After two MessagePorts are closed, there is a problem with
      // lingering promise not being properly resolved.
      // https://github.com/nodejs/node/issues/51486
      new transfer.CrossRealmTransformWritableSink(port, true),
      1,
      defaultSizeAlgorithm);
  }
}

ObjectDefineProperties(WritableStream.prototype, {
  locked: kEnumerableProperty,
  abort: kEnumerableProperty,
  close: kEnumerableProperty,
  getWriter: kEnumerableProperty,
  [SymbolToStringTag]: getNonWritablePropertyDescriptor(WritableStream.name),
});

function InternalTransferredWritableStream() {
  ObjectSetPrototypeOf(this, WritableStream.prototype);
  markTransferMode(this, false, true);
  this[kType] = 'WritableStream';
  this[kState] = createWritableStreamState();
}

ObjectSetPrototypeOf(InternalTransferredWritableStream.prototype, WritableStream.prototype);
ObjectSetPrototypeOf(InternalTransferredWritableStream, WritableStream);

function TransferredWritableStream() {
  const stream = new InternalTransferredWritableStream();

  stream.constructor = WritableStream;

  return stream;
}

TransferredWritableStream.prototype[kDeserialize] = () => {};

class WritableStreamDefaultWriter {
  [kType] = 'WritableStreamDefaultWriter';

  /**
   * @param {WritableStream} stream
   */
  constructor(stream) {
    if (!isWritableStream(stream))
      throw new ERR_INVALID_ARG_TYPE('stream', 'WritableStream', stream);
    this[kState] = {
      stream: undefined,
      // Lazily materialized by writerClosedPromise()/writerReadyPromise().
      close: undefined,
      ready: undefined,
    };
    setupWritableStreamDefaultWriter(this, stream);
  }

  /**
   * @readonly
   * @type {Promise<void>}
   */
  get closed() {
    if (!isWritableStreamDefaultWriter(this))
      return PromiseReject(new ERR_INVALID_THIS('WritableStreamDefaultWriter'));
    return writerClosedPromise(this).promise;
  }

  /**
   * @readonly
   * @type {number}
   */
  get desiredSize() {
    if (!isWritableStreamDefaultWriter(this))
      throw new ERR_INVALID_THIS('WritableStreamDefaultWriter');
    if (this[kState].stream === undefined) {
      throw new ERR_INVALID_STATE.TypeError(
        'Writer is not bound to a WritableStream');
    }
    return writableStreamDefaultWriterGetDesiredSize(this);
  }

  /**
   * @readonly
   * @type {Promise<void>}
   */
  get ready() {
    if (!isWritableStreamDefaultWriter(this))
      return PromiseReject(new ERR_INVALID_THIS('WritableStreamDefaultWriter'));
    return writerReadyPromise(this).promise;
  }

  /**
   * @param {any} [reason]
   * @returns {Promise<void>}
   */
  abort(reason = undefined) {
    if (!isWritableStreamDefaultWriter(this))
      return PromiseReject(new ERR_INVALID_THIS('WritableStreamDefaultWriter'));
    if (this[kState].stream === undefined) {
      return PromiseReject(
        new ERR_INVALID_STATE.TypeError(
          'Writer is not bound to a WritableStream'));
    }
    return writableStreamDefaultWriterAbort(this, reason);
  }

  /**
   * @returns {Promise<void>}
   */
  close() {
    if (!isWritableStreamDefaultWriter(this))
      return PromiseReject(new ERR_INVALID_THIS('WritableStreamDefaultWriter'));
    const {
      stream,
    } = this[kState];
    if (stream === undefined) {
      return PromiseReject(
        new ERR_INVALID_STATE.TypeError(
          'Writer is not bound to a WritableStream'));
    }
    if (writableStreamCloseQueuedOrInFlight(stream)) {
      return PromiseReject(
        new ERR_INVALID_STATE.TypeError('Failure to close WritableStream'));
    }
    return writableStreamDefaultWriterClose(this);
  }

  releaseLock() {
    if (!isWritableStreamDefaultWriter(this))
      throw new ERR_INVALID_THIS('WritableStreamDefaultWriter');
    const {
      stream,
    } = this[kState];
    if (stream === undefined)
      return;
    assert(stream[kState].writer !== undefined);
    writableStreamDefaultWriterRelease(this);
  }

  /**
   * @param {any} [chunk]
   * @returns {Promise<void>}
   */
  write(chunk = undefined) {
    if (!isWritableStreamDefaultWriter(this))
      return PromiseReject(new ERR_INVALID_THIS('WritableStreamDefaultWriter'));
    if (this[kState].stream === undefined) {
      return PromiseReject(
        new ERR_INVALID_STATE.TypeError(
          'Writer is not bound to a WritableStream'));
    }
    return writableStreamDefaultWriterWrite(this, chunk);
  }

  [kInspect](depth, options) {
    return customInspect(depth, options, this[kType], {
      stream: this[kState].stream,
      close: writerClosedPromise(this).promise,
      ready: writerReadyPromise(this).promise,
      desiredSize: this.desiredSize,
    });
  }
}

ObjectDefineProperties(WritableStreamDefaultWriter.prototype, {
  closed: kEnumerableProperty,
  ready: kEnumerableProperty,
  desiredSize: kEnumerableProperty,
  abort: kEnumerableProperty,
  close: kEnumerableProperty,
  releaseLock: kEnumerableProperty,
  write: kEnumerableProperty,
  [SymbolToStringTag]: getNonWritablePropertyDescriptor(WritableStreamDefaultWriter.name),
});

class WritableStreamDefaultController {
  [kType] = 'WritableStreamDefaultController';

  constructor(skipThrowSymbol = undefined) {
    if (skipThrowSymbol !== kSkipThrow) {
      throw new ERR_ILLEGAL_CONSTRUCTOR();
    }
  }

  [kAbort](reason) {
    const result = this[kState].abortAlgorithm(reason);
    writableStreamDefaultControllerClearAlgorithms(this);
    return result;
  }

  [kError]() {
    resetQueue(this);
  }

  /**
   * @type {AbortSignal}
   */
  get signal() {
    if (!isWritableStreamDefaultController(this))
      throw new ERR_INVALID_THIS('WritableStreamDefaultController');
    return this[kState].abortController.signal;
  }

  /**
   * @param {any} [error]
   */
  error(error = undefined) {
    if (!isWritableStreamDefaultController(this))
      throw new ERR_INVALID_THIS('WritableStreamDefaultController');
    if (this[kState].stream[kState].state !== 'writable')
      return;
    writableStreamDefaultControllerError(this, error);
  }

  [kInspect](depth, options) {
    return customInspect(depth, options, this[kType], {
      stream: this[kState].stream,
    });
  }
}

ObjectDefineProperties(WritableStreamDefaultController.prototype, {
  signal: kEnumerableProperty,
  error: kEnumerableProperty,
  [SymbolToStringTag]: getNonWritablePropertyDescriptor(WritableStreamDefaultController.name),
});

function InternalWritableStream(start, write, close, abort, highWaterMark, size) {
  ObjectSetPrototypeOf(this, WritableStream.prototype);
  markTransferMode(this, false, true);
  this[kType] = 'WritableStream';
  this[kState] = createWritableStreamState();

  const controller = new WritableStreamDefaultController(kSkipThrow);
  setupWritableStreamDefaultController(
    this,
    controller,
    start,
    write,
    close,
    abort,
    highWaterMark,
    size,
  );
}

ObjectSetPrototypeOf(InternalWritableStream.prototype, WritableStream.prototype);
ObjectSetPrototypeOf(InternalWritableStream, WritableStream);

function createWritableStream(start, write, close, abort, highWaterMark = 1, size = defaultSizeAlgorithm) {
  const stream = new InternalWritableStream(start, write, close, abort, highWaterMark, size);

  // For spec compliance the InternalWritableStream must be a WritableStream
  stream.constructor = WritableStream;
  return stream;
}

const isWritableStream =
  isBrandCheck('WritableStream');
const isWritableStreamDefaultWriter =
  isBrandCheck('WritableStreamDefaultWriter');
const isWritableStreamDefaultController =
  isBrandCheck('WritableStreamDefaultController');

function createWritableStreamState() {
  return {
    __proto__: null,
    closedPromise: undefined,
    closeRequest: kNilRequest,
    // Mirrors "closeRequest or inFlightCloseRequest is pending"; kept as a
    // flag because the predicate runs several times per chunk on the write
    // hot path.
    closeQueuedOrInFlight: false,
    inFlightWriteRequest: kNilRequest,
    inFlightCloseRequest: kNilRequest,
    pendingAbortRequest: kNilPendingAbortRequest,
    backpressure: false,
    controller: undefined,
    state: 'writable',
    storedError: undefined,
    // Ring-buffer request queue, materialized lazily on the first pending
    // write (see writableStreamAddWriteRequest) so construction allocates
    // no request storage.
    writeRequests: kEmptyQueue,
    writer: undefined,
    transfer: {
      __proto__: null,
      readable: undefined,
      port1: undefined,
      port2: undefined,
      promise: undefined,
    },
  };
}

function isWritableStreamLocked(stream) {
  return stream[kState].writer !== undefined;
}

function setupWritableStreamDefaultWriter(writer, stream) {
  if (isWritableStreamLocked(stream))
    throw new ERR_INVALID_STATE.TypeError('WritableStream is locked');
  writer[kState].stream = stream;
  stream[kState].writer = writer;
  // The writer's [[readyPromise]] and [[closedPromise]] are materialized
  // lazily by writerReadyPromise()/writerClosedPromise(); their settlement
  // is fully derivable from the writer/stream state, so nothing is
  // allocated here.
}

// Materializes the writer's [[closedPromise]] record on first
// observation. While unobserved, its settlement is derivable: a detached
// writer was released (rejected with the shared released error);
// otherwise it follows the stream state. The settle sites only touch the
// record when it has been materialized.
function writerClosedPromise(writer) {
  let close = writer[kState].close;
  if (close === undefined) {
    const stream = writer[kState].stream;
    if (stream === undefined) {
      close = rejectedHandledRecord(lazyWritableReleasedError());
    } else {
      switch (stream[kState].state) {
        case 'writable':
        case 'erroring':
          close = PromiseWithResolvers();
          break;
        case 'closed':
          close = resolvedRecord();
          break;
        default:
          close = rejectedHandledRecord(stream[kState].storedError);
      }
    }
    writer[kState].close = close;
  }
  return close;
}
