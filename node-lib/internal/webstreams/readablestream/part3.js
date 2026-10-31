
class ReadableStreamAsyncIteratorReadRequest {
  constructor(reader, state, promise) {
    this.reader = reader;
    this.state = state;
    this.promise = promise;
  }

  [kChunk](chunk) {
    this.state.current = undefined;
    this.promise.resolve({ done: false, value: chunk });
  }

  [kClose]() {
    this.state.current = undefined;
    this.state.done = true;
    readableStreamReaderGenericRelease(this.reader);
    this.promise.resolve({ done: true, value: undefined });
  }

  [kError](error) {
    this.state.current = undefined;
    this.state.done = true;
    readableStreamReaderGenericRelease(this.reader);
    this.promise.reject(error);
  }
}

class DefaultReadRequest {
  constructor() {
    this[kState] = PromiseWithResolvers();
  }

  [kChunk](value) {
    this[kState].resolve({ done: false, value });
  }

  [kClose]() {
    this[kState].resolve({ done: true, value: undefined });
  }

  [kError](error) {
    this[kState].reject(error);
  }

  get promise() { return this[kState].promise; }
}

class ReadIntoRequest {
  constructor() {
    this[kState] = PromiseWithResolvers();
  }

  [kChunk](value) {
    this[kState].resolve({ done: false, value });
  }

  [kClose](value) {
    this[kState].resolve({ done: true, value });
  }

  [kError](error) {
    this[kState].reject(error);
  }

  get promise() { return this[kState].promise; }
}

class ReadableStreamDefaultReader {
  [kType] = 'ReadableStreamDefaultReader';

  /**
   * @param {ReadableStream} stream
   */
  constructor(stream) {
    if (!isReadableStream(stream))
      throw new ERR_INVALID_ARG_TYPE('stream', 'ReadableStream', stream);
    this[kState] = {
      // All fields are unconditionally assigned during setup.
      readRequests: undefined,
      stream: undefined,
      close: undefined,
    };
    setupReadableStreamDefaultReader(this, stream);
  }

  /**
   * @returns {Promise<{
   *   value : any,
   *   done : boolean
   * }>}
   */
  read() {
    if (!isReadableStreamDefaultReader(this))
      return PromiseReject(new ERR_INVALID_THIS('ReadableStreamDefaultReader'));
    if (this[kState].stream === undefined) {
      return PromiseReject(
        new ERR_INVALID_STATE.TypeError(
          'The reader is not attached to a stream'));
    }

    const stream = this[kState].stream;
    const controller = stream[kState].controller;

    // Fast path: if data is already buffered in the controller's queue,
    // return a resolved promise immediately without creating a read request.
    // This is spec-compliant because read() returns a Promise, and
    // Promise.resolve() callbacks still run in the microtask queue.
    if (stream[kState].state === 'readable') {
      if (isReadableStreamDefaultController(controller)) {
        if (controller[kState].queue.length > 0) {
          stream[kState].disturbed = true;
          const chunk = dequeueValue(controller);

          if (controller[kState].closeRequested && !controller[kState].queue.length) {
            readableStreamDefaultControllerClearAlgorithms(controller);
            readableStreamClose(stream);
          } else if (!controller[kState].closeRequested &&
                     controller[kState].started &&
                     controller[kState].highWaterMark -
                       controller[kState].queueTotalSize > 0) {
            // ShouldCallPull reduced to the conditions not already
            // established on this path: the state is readable and the
            // queue was non-empty, so no read requests can be parked.
            readableStreamDefaultControllerPull(controller);
          }

          return PromiseResolve({ done: false, value: chunk });
        }
      } else if (controller[kState].queueTotalSize > 0) {
        // Byte controller with buffered data: mirror the queue-filled arm
        // of its pull steps (which never consults pendingPullIntos) minus
        // the read request.
        stream[kState].disturbed = true;
        return PromiseResolve({
          done: false,

          value: readableByteStreamControllerDequeueChunk(controller),
        });
      }
    }

    // Slow path: create request and go through normal flow
    const readRequest = new DefaultReadRequest();
    readableStreamDefaultReaderRead(this, readRequest);
    return readRequest.promise;
  }

  releaseLock() {
    if (!isReadableStreamDefaultReader(this))
      throw new ERR_INVALID_THIS('ReadableStreamDefaultReader');
    if (this[kState].stream === undefined)
      return;
    readableStreamDefaultReaderRelease(this);
  }

  /**
   * @readonly
   * @type {Promise<void>}
   */
  get closed() {
    if (!isReadableStreamDefaultReader(this))
      return PromiseReject(new ERR_INVALID_THIS('ReadableStreamDefaultReader'));
    return readerClosedPromise(this).promise;
  }

  /**
   * @param {any} [reason]
   * @returns {Promise<void>}
   */
  cancel(reason = undefined) {
    if (!isReadableStreamDefaultReader(this))
      return PromiseReject(new ERR_INVALID_THIS('ReadableStreamDefaultReader'));
    if (this[kState].stream === undefined) {
      return PromiseReject(new ERR_INVALID_STATE.TypeError(
        'The reader is not attached to a stream'));
    }
    return readableStreamReaderGenericCancel(this, reason);
  }

  [kInspect](depth, options) {
    return customInspect(depth, options, this[kType], {
      stream: this[kState].stream,
      readRequests: this[kState].readRequests.length,
      close: readerClosedPromise(this).promise,
    });
  }
}

ObjectDefineProperties(ReadableStreamDefaultReader.prototype, {
  closed: kEnumerableProperty,
  read: kEnumerableProperty,
  releaseLock: kEnumerableProperty,
  cancel: kEnumerableProperty,
  [SymbolToStringTag]: getNonWritablePropertyDescriptor(ReadableStreamDefaultReader.name),
});

class ReadableStreamBYOBReader {
  [kType] = 'ReadableStreamBYOBReader';

  /**
   * @param {ReadableStream} stream
   */
  constructor(stream) {
    if (!isReadableStream(stream))
      throw new ERR_INVALID_ARG_TYPE('stream', 'ReadableStream', stream);
    this[kState] = {
      // All fields are unconditionally assigned during setup.
      stream: undefined,
      readIntoRequests: undefined,
      close: undefined,
    };
    setupReadableStreamBYOBReader(this, stream);
  }

  /**
   * @param {ArrayBufferView} view
   * @param {{
   *   min? : number
   * }} [options]
   * @returns {Promise<{
   *   value : ArrayBufferView,
   *   done : boolean,
   * }>}
   */
  async read(view, options = kEmptyObject) {
    if (!isReadableStreamBYOBReader(this))
      throw new ERR_INVALID_THIS('ReadableStreamBYOBReader');
    validateBuffer(view, 'view');
    validateObject(options, 'options', kValidateObjectAllowObjectsAndNull);

    const viewByteLength = ArrayBufferViewGetByteLength(view);
    const viewBuffer = ArrayBufferViewGetBuffer(view);

    if (isSharedArrayBuffer(viewBuffer)) {
      throw new ERR_INVALID_ARG_VALUE(
        'view',
        view,
        'must not be backed by a SharedArrayBuffer',
      );
    }

    const viewBufferByteLength = ArrayBufferPrototypeGetByteLength(viewBuffer);

    if (viewByteLength === 0 || viewBufferByteLength === 0) {
      throw new ERR_INVALID_STATE.TypeError(
        'View or Viewed ArrayBuffer is zero-length or detached');
    }

    // Supposed to assert here that the view's buffer is not
    // detached, but there's no API available to use to check that.

    const min = options?.min ?? 1;
    validateNumber(min, 'options.min');
    if (!NumberIsInteger(min))
      throw new ERR_INVALID_ARG_VALUE('options.min', min, 'must be an integer');
    if (min <= 0)
      throw new ERR_INVALID_ARG_VALUE('options.min', min, 'must be greater than 0');
    if (!isDataView(view)) {
      if (min > TypedArrayPrototypeGetLength(view)) {
        throw new ERR_OUT_OF_RANGE('options.min', '<= view.length', min);
      }
    } else if (min > viewByteLength) {
      throw new ERR_OUT_OF_RANGE('options.min', '<= view.byteLength', min);
    }

    if (this[kState].stream === undefined) {
      throw new ERR_INVALID_STATE.TypeError('The reader is not attached to a stream');
    }
    const readIntoRequest = new ReadIntoRequest();
    readableStreamBYOBReaderRead(this, view, min, readIntoRequest);
    return readIntoRequest.promise;
  }

  releaseLock() {
    if (!isReadableStreamBYOBReader(this))
      throw new ERR_INVALID_THIS('ReadableStreamBYOBReader');
    if (this[kState].stream === undefined)
      return;
    readableStreamBYOBReaderRelease(this);
  }

  /**
   * @readonly
   * @type {Promise<void>}
   */
  get closed() {
    if (!isReadableStreamBYOBReader(this))
      return PromiseReject(new ERR_INVALID_THIS('ReadableStreamBYOBReader'));
    return readerClosedPromise(this).promise;
  }

  /**
   * @param {any} [reason]
   * @returns {Promise<void>}
   */
  cancel(reason = undefined) {
    if (!isReadableStreamBYOBReader(this))
      return PromiseReject(new ERR_INVALID_THIS('ReadableStreamBYOBReader'));
    if (this[kState].stream === undefined) {
      return PromiseReject(new ERR_INVALID_STATE.TypeError(
        'The reader is not attached to a stream'));
    }
    return readableStreamReaderGenericCancel(this, reason);
  }

  [kInspect](depth, options) {
    return customInspect(depth, options, this[kType], {
      stream: this[kState].stream,
      readIntoRequests: this[kState].readIntoRequests.length,
      close: readerClosedPromise(this).promise,
    });
  }
}

ObjectDefineProperties(ReadableStreamBYOBReader.prototype, {
  closed: kEnumerableProperty,
  read: kEnumerableProperty,
  releaseLock: kEnumerableProperty,
  cancel: kEnumerableProperty,
  [SymbolToStringTag]: getNonWritablePropertyDescriptor(ReadableStreamBYOBReader.name),
});

class ReadableStreamDefaultController {
  [kType] = 'ReadableStreamDefaultController';
  [kState] = {};

  constructor(skipThrowSymbol = undefined) {
    if (skipThrowSymbol !== kSkipThrow) {
      throw new ERR_ILLEGAL_CONSTRUCTOR();
    }
  }

  /**
   * @readonly
   * @type {number}
   */
  get desiredSize() {
    return readableStreamDefaultControllerGetDesiredSize(this);
  }

  close() {
    if (!readableStreamDefaultControllerCanCloseOrEnqueue(this))
      throw new ERR_INVALID_STATE.TypeError('Controller is already closed');
    readableStreamDefaultControllerClose(this);
  }

  /**
   * @param {any} [chunk]
   */
  enqueue(chunk = undefined) {
    if (!readableStreamDefaultControllerCanCloseOrEnqueue(this))
      throw new ERR_INVALID_STATE.TypeError('Controller is already closed');
    readableStreamDefaultControllerEnqueue(this, chunk);
  }

  /**
   * @param {any} [error]
   */
  error(error = undefined) {
    readableStreamDefaultControllerError(this, error);
  }

  [kCancel](reason) {
    return readableStreamDefaultControllerCancelSteps(this, reason);
  }

  [kPull](readRequest) {
    readableStreamDefaultControllerPullSteps(this, readRequest);
  }

  [kRelease]() {}

  [kInspect](depth, options) {
    return customInspect(depth, options, this[kType], { });
  }
}

ObjectDefineProperties(ReadableStreamDefaultController.prototype, {
  desiredSize: kEnumerableProperty,
  close: kEnumerableProperty,
  enqueue: kEnumerableProperty,
  error: kEnumerableProperty,
  [SymbolToStringTag]: getNonWritablePropertyDescriptor(ReadableStreamDefaultController.name),
});

class ReadableByteStreamController {
  [kType] = 'ReadableByteStreamController';
  [kState] = {};

  constructor(skipThrowSymbol = undefined) {
    if (skipThrowSymbol !== kSkipThrow) {
      throw new ERR_ILLEGAL_CONSTRUCTOR();
    }
  }

  /**
   * @readonly
   * @type {ReadableStreamBYOBRequest}
   */
  get byobRequest() {
    if (!isReadableByteStreamController(this))
      throw new ERR_INVALID_THIS('ReadableByteStreamController');
    if (this[kState].byobRequest === null &&
        this[kState].pendingPullIntos.length) {
      const {
        buffer,
        byteOffset,
        bytesFilled,
        byteLength,
      } = this[kState].pendingPullIntos.peek();
      const view =
        new Uint8Array(
          buffer,
          byteOffset + bytesFilled,
          byteLength - bytesFilled);
      this[kState].byobRequest = createReadableStreamBYOBRequest(this, view);
    }
    return this[kState].byobRequest;
  }

  /**
   * @readonly
   * @type {number}
   */
  get desiredSize() {
    if (!isReadableByteStreamController(this))
      throw new ERR_INVALID_THIS('ReadableByteStreamController');
    return readableByteStreamControllerGetDesiredSize(this);
  }

  close() {
    if (!isReadableByteStreamController(this))
      throw new ERR_INVALID_THIS('ReadableByteStreamController');
    if (this[kState].closeRequested)
      throw new ERR_INVALID_STATE.TypeError('Controller is already closed');
    if (this[kState].stream[kState].state !== 'readable')
      throw new ERR_INVALID_STATE.TypeError('ReadableStream is already closed');
    readableByteStreamControllerClose(this);
  }

  /**
   * @param {ArrayBufferView} chunk
   */
  enqueue(chunk) {
    if (!isReadableByteStreamController(this))
      throw new ERR_INVALID_THIS('ReadableByteStreamController');
    validateBuffer(chunk);
    const chunkByteLength = ArrayBufferViewGetByteLength(chunk);
    const chunkBuffer = ArrayBufferViewGetBuffer(chunk);

    if (isSharedArrayBuffer(chunkBuffer)) {
      throw new ERR_INVALID_ARG_VALUE(
        'chunk',
        chunk,
        'must not be backed by a SharedArrayBuffer',
      );
    }

    const chunkBufferByteLength = ArrayBufferPrototypeGetByteLength(chunkBuffer);
    if (chunkByteLength === 0 || chunkBufferByteLength === 0) {
      throw new ERR_INVALID_STATE.TypeError(
        'chunk ArrayBuffer is zero-length or detached');
    }
    if (this[kState].closeRequested)
      throw new ERR_INVALID_STATE.TypeError('Controller is already closed');
    if (this[kState].stream[kState].state !== 'readable')
      throw new ERR_INVALID_STATE.TypeError('ReadableStream is already closed');
    readableByteStreamControllerEnqueue(this, chunk);
  }

  /**
   * @param {any} [error]
   */
  error(error = undefined) {
    if (!isReadableByteStreamController(this))
      throw new ERR_INVALID_THIS('ReadableByteStreamController');
    readableByteStreamControllerError(this, error);
  }

  [kCancel](reason) {
    return readableByteStreamControllerCancelSteps(this, reason);
  }

  [kPull](readRequest) {
    readableByteStreamControllerPullSteps(this, readRequest);
  }

  [kRelease]() {
    const {
      pendingPullIntos,
    } = this[kState];
    if (pendingPullIntos.length > 0) {
      const firstPendingPullInto = pendingPullIntos.peek();
      firstPendingPullInto.type = 'none';
      const queue = new Queue();
      queue.push(firstPendingPullInto);
      this[kState].pendingPullIntos = queue;
    }
  }

  [kInspect](depth, options) {
    return customInspect(depth, options, this[kType], { });
  }
}

ObjectDefineProperties(ReadableByteStreamController.prototype, {
  byobRequest: kEnumerableProperty,
  desiredSize: kEnumerableProperty,
  close: kEnumerableProperty,
  enqueue: kEnumerableProperty,
  error: kEnumerableProperty,
  [SymbolToStringTag]: getNonWritablePropertyDescriptor(ReadableByteStreamController.name),
});

function InternalReadableStream(start, pull, cancel, highWaterMark, size) {
  ObjectSetPrototypeOf(this, ReadableStream.prototype);
  markTransferMode(this, false, true);
  this[kType] = 'ReadableStream';
  this[kState] = createReadableStreamState();
  const controller = new ReadableStreamDefaultController(kSkipThrow);
  setupReadableStreamDefaultController(
    this,
    controller,
    start,
    pull,
    cancel,
    highWaterMark,
    size);
}

ObjectSetPrototypeOf(InternalReadableStream.prototype, ReadableStream.prototype);
ObjectSetPrototypeOf(InternalReadableStream, ReadableStream);

function createReadableStream(start, pull, cancel, highWaterMark = 1, size = defaultSizeAlgorithm) {
  const stream = new InternalReadableStream(start, pull, cancel, highWaterMark, size);

  // For spec compliance the InternalReadableStream must be a ReadableStream
  stream.constructor = ReadableStream;
  return stream;
}

function InternalReadableByteStream(start, pull, cancel) {
  ObjectSetPrototypeOf(this, ReadableStream.prototype);
  markTransferMode(this, false, true);
  this[kType] = 'ReadableStream';
  this[kState] = createReadableStreamState();
  const controller = new ReadableByteStreamController(kSkipThrow);
  setupReadableByteStreamController(
    this,
    controller,
    start,
    pull,
    cancel,
    0,
    undefined);
}

ObjectSetPrototypeOf(InternalReadableByteStream.prototype, ReadableStream.prototype);
ObjectSetPrototypeOf(InternalReadableByteStream, ReadableStream);

function createReadableByteStream(start, pull, cancel) {
  const stream = new InternalReadableByteStream(start, pull, cancel);

  // For spec compliance the InternalReadableByteStream must be a ReadableStream
  stream.constructor = ReadableStream;
  return stream;
}

const isReadableStream =
  isBrandCheck('ReadableStream');
const isReadableByteStreamController =
  isBrandCheck('ReadableByteStreamController');
const isReadableStreamDefaultController =
  isBrandCheck('ReadableStreamDefaultController');
const isReadableStreamBYOBRequest =
  isBrandCheck('ReadableStreamBYOBRequest');
const isReadableStreamDefaultReader =
  isBrandCheck('ReadableStreamDefaultReader');
const isReadableStreamBYOBReader =
  isBrandCheck('ReadableStreamBYOBReader');

// ---- ReadableStream Implementation

function createReadableStreamState() {
  return {
    __proto__: null,
    closedPromise: undefined,
    disturbed: false,
    reader: undefined,
    state: 'readable',
    storedError: undefined,
    transfer: {
      __proto__: null,
      writable: undefined,
      port1: undefined,
      port2: undefined,
      promise: undefined,
    },
  };
}
