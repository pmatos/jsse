
class ReadableStream {
  [kType] = 'ReadableStream';

  /**
   * @param {UnderlyingSource} [source]
   * @param {QueuingStrategy} [strategy]
   */
  constructor(source = kEmptyObject, strategy = kEmptyObject) {
    markTransferMode(this, false, true);
    validateObject(source, 'source', kValidateObjectAllowObjects);
    validateObject(strategy, 'strategy', kValidateObjectAllowObjectsAndNull);
    this[kState] = createReadableStreamState();

    // The spec requires handling of the strategy first
    // here. Specifically, if getting the size and
    // highWaterMark from the strategy fail, that has
    // to trigger a throw before getting the details
    // from the source. So be sure to keep these in
    // this order.
    const size = strategy?.size;
    const highWaterMark = strategy?.highWaterMark;
    const type = source.type;

    if (`${type}` === 'bytes') {
      if (size !== undefined)
        throw new ERR_INVALID_ARG_VALUE.RangeError('strategy.size', size);
      setupReadableByteStreamControllerFromSource(
        this,
        source,
        extractHighWaterMark(highWaterMark, 0));
    } else {
      if (type !== undefined)
        throw new ERR_INVALID_ARG_VALUE('source.type', type);
      setupReadableStreamDefaultControllerFromSource(
        this,
        source,
        extractHighWaterMark(highWaterMark, 1),
        extractSizeAlgorithm(size));
    }
  }

  get [kIsDisturbed]() {
    return this[kState].disturbed;
  }

  get [kIsErrored]() {
    return this[kState].state === 'errored';
  }

  get [kIsReadable]() {
    return this[kState].state === 'readable';
  }

  [kControllerErrorFunction](error) {
    // Used by the internal stream interop (addAbortSignal). Historically
    // only default controllers were wired here; byte stream controllers
    // keep the previous no-op behavior.
    const controller = this[kState].controller;
    if (isReadableStreamDefaultController(controller))
      controller.error(error);
  }

  // Used by the internal stream interop (end-of-stream). Materialized
  // lazily since its settlement is derivable from the stream state; the
  // settle sites in readableStreamClose/Error only touch the cache.
  get [kIsClosedPromise]() {
    let cache = this[kState].closedPromise;
    if (cache === undefined) {
      switch (this[kState].state) {
        case 'readable':
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
    if (!isReadableStream(this))
      throw new ERR_INVALID_THIS('ReadableStream');
    return isReadableStreamLocked(this);
  }

  static from(iterable) {
    return readableStreamFromIterable(iterable);
  }

  /**
   * @param {any} [reason]
   * @returns { Promise<void> }
   */
  cancel(reason = undefined) {
    if (!isReadableStream(this))
      return PromiseReject(new ERR_INVALID_THIS('ReadableStream'));
    if (isReadableStreamLocked(this)) {
      return PromiseReject(
        new ERR_INVALID_STATE.TypeError('ReadableStream is locked'));
    }
    return readableStreamCancel(this, reason);
  }

  /**
   * @param {{
   *   mode? : "byob"
   * }} [options]
   * @returns {ReadableStreamReader}
   */
  getReader(options = kEmptyObject) {
    if (!isReadableStream(this))
      throw new ERR_INVALID_THIS('ReadableStream');
    validateObject(options, 'options', kValidateObjectAllowObjectsAndNull);
    const mode = options?.mode;

    if (mode === undefined)
      // eslint-disable-next-line no-use-before-define
      return new ReadableStreamDefaultReader(this);

    if (`${mode}` !== 'byob')
      throw new ERR_INVALID_ARG_VALUE('options.mode', mode);
    // eslint-disable-next-line no-use-before-define
    return new ReadableStreamBYOBReader(this);
  }

  /**
   * @param {ReadableWritablePair} transform
   * @param {StreamPipeOptions} [options]
   * @returns {ReadableStream}
   */
  pipeThrough(transform, options = kEmptyObject) {
    if (!isReadableStream(this))
      throw new ERR_INVALID_THIS('ReadableStream');
    const readable = transform?.readable;
    if (!isReadableStream(readable)) {
      throw new ERR_INVALID_ARG_TYPE(
        'transform.readable',
        'ReadableStream',
        readable);
    }
    const writable = transform?.writable;
    if (!isWritableStream(writable)) {
      throw new ERR_INVALID_ARG_TYPE(
        'transform.writable',
        'WritableStream',
        writable);
    }

    // The web platform tests require that these be handled one at a
    // time and in a specific order. options can be null or undefined.
    validateObject(options, 'options', kValidateObjectAllowObjectsAndNull);
    const preventAbort = options?.preventAbort;
    const preventCancel = options?.preventCancel;
    const preventClose = options?.preventClose;
    const signal = options?.signal;

    if (signal !== undefined) {
      validateAbortSignal(signal, 'options.signal');
    }

    if (isReadableStreamLocked(this))
      throw new ERR_INVALID_STATE.TypeError('The ReadableStream is locked');
    if (isWritableStreamLocked(writable))
      throw new ERR_INVALID_STATE.TypeError('The WritableStream is locked');

    const promise = readableStreamPipeTo(
      this,
      writable,
      !!preventClose,
      !!preventAbort,
      !!preventCancel,
      signal);
    setPromiseHandled(promise);

    return readable;
  }

  /**
   * @param {WritableStream} destination
   * @param {StreamPipeOptions} [options]
   * @returns {Promise<void>}
   */
  pipeTo(destination, options = kEmptyObject) {
    try {
      if (!isReadableStream(this))
        throw new ERR_INVALID_THIS('ReadableStream');
      if (!isWritableStream(destination)) {
        throw new ERR_INVALID_ARG_TYPE(
          'transform.writable',
          'WritableStream',
          destination);
      }

      validateObject(options, 'options', kValidateObjectAllowObjectsAndNull);
      const preventAbort = options?.preventAbort;
      const preventCancel = options?.preventCancel;
      const preventClose = options?.preventClose;
      const signal = options?.signal;

      if (signal !== undefined) {
        validateAbortSignal(signal, 'options.signal');
      }

      if (isReadableStreamLocked(this))
        throw new ERR_INVALID_STATE.TypeError('The ReadableStream is locked');
      if (isWritableStreamLocked(destination))
        throw new ERR_INVALID_STATE.TypeError('The WritableStream is locked');

      return readableStreamPipeTo(
        this,
        destination,
        !!preventClose,
        !!preventAbort,
        !!preventCancel,
        signal);
    } catch (error) {
      return PromiseReject(error);
    }
  }

  /**
   * @returns {ReadableStream[]}
   */
  tee() {
    if (!isReadableStream(this))
      throw new ERR_INVALID_THIS('ReadableStream');
    return readableStreamTee(this, false);
  }

  /**
   * @param {{
   *   preventCancel? : boolean,
   * }} [options]
   * @returns {AsyncIterable}
   */
  values(options = kEmptyObject) {
    if (!isReadableStream(this))
      throw new ERR_INVALID_THIS('ReadableStream');
    validateObject(options, 'options', kValidateObjectAllowObjectsAndNull);
    const preventCancel = !!(options?.preventCancel);

    // eslint-disable-next-line no-use-before-define
    const reader = new ReadableStreamDefaultReader(this);

    // No __proto__ here to avoid the performance hit.
    const state = {
      done: false,
      current: undefined,
    };
    let started = false;
    // A single reusable read request: at most one read is ever in flight
    // (next() chains through state.current), and the request is consumed
    // before the next read starts, so only its promise record changes
    // per read.
    // eslint-disable-next-line no-use-before-define
    const readRequest = new ReadableStreamAsyncIteratorReadRequest(reader, state, undefined);

    // The nextSteps function is not an async function in order
    // to make it more efficient. Because nextSteps explicitly
    // creates a Promise and returns it in the common case,
    // making it an async function just causes two additional
    // unnecessary Promise allocations to occur, which just add
    // cost.
    function nextSteps() {
      if (state.done)
        return PromiseResolve({ done: true, value: undefined });

      if (reader[kState].stream === undefined) {
        return PromiseReject(
          new ERR_INVALID_STATE.TypeError(
            'The reader is not bound to a ReadableStream'));
      }
      const promise = PromiseWithResolvers();

      readRequest.promise = promise;
      readableStreamDefaultReaderRead(reader, readRequest);
      return promise.promise;
    }

    async function returnSteps(value) {
      if (state.done)
        return { done: true, value }; // eslint-disable-line node-core/avoid-prototype-pollution
      state.done = true;

      if (reader[kState].stream === undefined) {
        throw new ERR_INVALID_STATE.TypeError(
          'The reader is not bound to a ReadableStream');
      }
      assert(!reader[kState].readRequests.length);
      if (!preventCancel) {
        const result = readableStreamReaderGenericCancel(reader, value);
        readableStreamReaderGenericRelease(reader);
        await result;
        return { done: true, value }; // eslint-disable-line node-core/avoid-prototype-pollution
      }

      readableStreamReaderGenericRelease(reader);
      return { done: true, value }; // eslint-disable-line node-core/avoid-prototype-pollution
    }

    // TODO(@jasnell): Explore whether an async generator
    // can be used here instead of a custom iterator object.
    return ObjectSetPrototypeOf({
      // Changing either of these functions (next or return)
      // to async functions causes a failure in the streams
      // Web Platform Tests that check for use of a modified
      // Promise.prototype.then. Since the await keyword
      // uses Promise.prototype.then, it is open to prototype
      // pollution, which causes the test to fail. The other
      // await uses here do not trigger that failure because
      // the test that fails does not trigger those code paths.
      next() {
        // If this is the first read, delay by one microtask
        // to ensure that the controller has had an opportunity
        // to properly start and perform the initial pull.
        // TODO(@jasnell): The spec doesn't call this out so
        // need to investigate if it's a bug in our impl or
        // the spec.
        if (!started) {
          state.current = PromiseResolve();
          started = true;
        }
        if (state.current !== undefined) {
          state.current =
            PromisePrototypeThen(state.current, nextSteps, nextSteps);
          return state.current;
        }
        // No read is in flight. Mirror the buffered fast path of
        // ReadableStreamDefaultReader.read(): when data is already queued
        // in the controller, resolve immediately without allocating a
        // read request. The result settles synchronously, so leaving
        // state.current undefined matches the state the slow path reaches
        // once its read request callbacks have settled.
        const stream = reader[kState].stream;
        if (!state.done && stream !== undefined &&
            stream[kState].state === 'readable') {
          const controller = stream[kState].controller;
          if (isReadableStreamDefaultController(controller)) {
            if (controller[kState].queue.length > 0) {
              stream[kState].disturbed = true;
              const chunk = dequeueValue(controller);

              if (controller[kState].closeRequested &&
                  !controller[kState].queue.length) {
                readableStreamDefaultControllerClearAlgorithms(controller);
                readableStreamClose(stream);
              } else if (!controller[kState].closeRequested &&
                         controller[kState].started &&
                         controller[kState].highWaterMark -
                           controller[kState].queueTotalSize > 0) {
                // Reduced ShouldCallPull, as in the read() fast path.
                readableStreamDefaultControllerPull(controller);
              }

              return PromiseResolve({ done: false, value: chunk });
            }
          } else if (controller[kState].queueTotalSize > 0) {
            // Byte controller with buffered data: same shape as above via
            // the queue-filled arm of the byte controller's pull steps.
            stream[kState].disturbed = true;
            return PromiseResolve({
              done: false,

              value: readableByteStreamControllerDequeueChunk(controller),
            });
          }
        }
        state.current = nextSteps();
        return state.current;
      },

      return(error) {
        started = true;
        state.current = state.current !== undefined ?
          PromisePrototypeThen(
            state.current,
            () => returnSteps(error),
            () => returnSteps(error)) :
          returnSteps(error);
        return state.current;
      },

      [SymbolAsyncIterator]() { return this; },
    }, AsyncIterator);
  }

  [kInspect](depth, options) {
    return customInspect(depth, options, this[kType], {
      locked: this.locked,
      state: this[kState].state,
      supportsBYOB:
        // eslint-disable-next-line no-use-before-define
        this[kState].controller instanceof ReadableByteStreamController,
    });
  }

  [kTransfer]() {
    if (!isReadableStream(this))
      throw new ERR_INVALID_THIS('ReadableStream');
    if (this.locked) {
      this[kState].transfer.port1?.close();
      this[kState].transfer.port1 = undefined;
      this[kState].transfer.port2 = undefined;
      throw new DOMException(
        'Cannot transfer a locked ReadableStream',
        'DataCloneError');
    }

    const {
      writable,
      promise,
    } = lazyTransfer().newCrossRealmWritableSink(
      this,
      this[kState].transfer.port1);

    this[kState].transfer.writable = writable;
    this[kState].transfer.promise = promise;

    return {
      data: { port: this[kState].transfer.port2 },
      deserializeInfo:
        'internal/webstreams/readablestream:TransferredReadableStream',
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
    setupReadableStreamDefaultControllerFromSource(
      this,
      // The MessagePort is set to be referenced when reading.
      // After two MessagePorts are closed, there is a problem with
      // lingering promise not being properly resolved.
      // https://github.com/nodejs/node/issues/51486
      new transfer.CrossRealmTransformReadableSource(port, true),
      0, defaultSizeAlgorithm);
  }
}

ObjectDefineProperties(ReadableStream.prototype, {
  [SymbolAsyncIterator]: {
    __proto__: null,
    configurable: true,
    enumerable: false,
    writable: true,
    value: ReadableStream.prototype.values,
  },
  locked: kEnumerableProperty,
  cancel: kEnumerableProperty,
  getReader: kEnumerableProperty,
  pipeThrough: kEnumerableProperty,
  pipeTo: kEnumerableProperty,
  tee: kEnumerableProperty,
  values: kEnumerableProperty,
  [SymbolToStringTag]: getNonWritablePropertyDescriptor(ReadableStream.name),
});
ObjectDefineProperties(ReadableStream, {
  from: kEnumerableProperty,
});

function InternalTransferredReadableStream() {
  ObjectSetPrototypeOf(this, ReadableStream.prototype);
  markTransferMode(this, false, true);
  this[kType] = 'ReadableStream';
  this[kState] = createReadableStreamState();
}

ObjectSetPrototypeOf(InternalTransferredReadableStream.prototype, ReadableStream.prototype);
ObjectSetPrototypeOf(InternalTransferredReadableStream, ReadableStream);

function TransferredReadableStream() {
  const stream = new InternalTransferredReadableStream();

  stream.constructor = ReadableStream;

  return stream;
}

TransferredReadableStream.prototype[kDeserialize] = () => {};

class ReadableStreamBYOBRequest {
  [kType] = 'ReadableStreamBYOBRequest';

  constructor(skipThrowSymbol = undefined) {
    if (skipThrowSymbol !== kSkipThrow) {
      throw new ERR_ILLEGAL_CONSTRUCTOR();
    }
  }

  /**
   * @readonly
   * @type {Uint8Array}
   */
  get view() {
    if (!isReadableStreamBYOBRequest(this))
      throw new ERR_INVALID_THIS('ReadableStreamBYOBRequest');
    return this[kState].view;
  }

  /**
   * @param {number} bytesWritten
   */
  respond(bytesWritten) {
    if (!isReadableStreamBYOBRequest(this))
      throw new ERR_INVALID_THIS('ReadableStreamBYOBRequest');
    const {
      view,
      controller,
    } = this[kState];
    if (controller === undefined) {
      throw new ERR_INVALID_STATE.TypeError(
        'This BYOB request has been invalidated');
    }

    const viewByteLength = ArrayBufferViewGetByteLength(view);
    const viewBuffer = ArrayBufferViewGetBuffer(view);
    const viewBufferByteLength = ArrayBufferPrototypeGetByteLength(viewBuffer);

    if (ArrayBufferPrototypeGetDetached(viewBuffer)) {
      throw new ERR_INVALID_STATE.TypeError('Viewed ArrayBuffer is detached');
    }

    assert(viewByteLength > 0);
    assert(viewBufferByteLength > 0);

    readableByteStreamControllerRespond(controller, bytesWritten);
  }

  /**
   * @param {ArrayBufferView} view
   */
  respondWithNewView(view) {
    if (!isReadableStreamBYOBRequest(this))
      throw new ERR_INVALID_THIS('ReadableStreamBYOBRequest');
    const {
      controller,
    } = this[kState];

    if (controller === undefined) {
      throw new ERR_INVALID_STATE.TypeError(
        'This BYOB request has been invalidated');
    }

    validateBuffer(view, 'view');

    if (ArrayBufferPrototypeGetDetached(view.buffer)) {
      throw new ERR_INVALID_STATE.TypeError('Viewed ArrayBuffer is detached');
    }

    readableByteStreamControllerRespondWithNewView(controller, view);
  }

  [kInspect](depth, options) {
    return customInspect(depth, options, this[kType], {
      view: this.view,
      controller: this[kState].controller,
    });
  }
}

ObjectDefineProperties(ReadableStreamBYOBRequest.prototype, {
  view: kEnumerableProperty,
  respond: kEnumerableProperty,
  respondWithNewView: kEnumerableProperty,
  [SymbolToStringTag]: getNonWritablePropertyDescriptor(ReadableStreamBYOBRequest.name),
});

function createReadableStreamBYOBRequest(controller, view) {
  const stream = new ReadableStreamBYOBRequest(kSkipThrow);

  stream[kState] = {
    controller,
    view,
  };

  return stream;
}
