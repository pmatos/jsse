
function readableStreamDefaultControllerEnqueue(controller, chunk) {
  // Equivalent to readableStreamDefaultControllerCanCloseOrEnqueue()
  // followed by isReadableStreamLocked() and
  // readableStreamGetNumReadRequests(), but with the state loaded once:
  // this runs for every enqueued chunk.
  const controllerState = controller[kState];
  const stream = controllerState.stream;
  if (controllerState.closeRequested || stream[kState].state !== 'readable')
    return;

  const reader = stream[kState].reader;
  if (reader !== undefined &&
      reader[kState] !== undefined &&
      reader[kType] === 'ReadableStreamDefaultReader' &&
      reader[kState].readRequests.length) {
    // Fulfilling a read request can run user code synchronously (the
    // pipeTo read request invokes the sink's write algorithm), so the
    // full ShouldCallPull predicate has to be re-evaluated afterwards.
    readableStreamFulfillReadRequest(stream, chunk, false);
  } else if (controllerState.sizeAlgorithm === defaultSizeAlgorithm) {
    // The internal default size algorithm is never observable by user
    // code, always returns 1, and cannot throw: enqueue with the
    // constant size instead of calling it or validating the result.
    // No user code runs between the guards at the top of this function
    // and this point, so ShouldCallPull reduces to the started flag and
    // the desired size (this branch implies no parked read requests,
    // ruling out the reader arm of the predicate).
    materializeQueue(controllerState).pushPair(chunk, 1);
    controllerState.queueTotalSize++;
    if (controllerState.started &&
        controllerState.highWaterMark - controllerState.queueTotalSize > 0) {
      readableStreamDefaultControllerPull(controller);
    }
    return;
  } else {
    // The user-supplied size algorithm may run arbitrary code, so the
    // full ShouldCallPull predicate has to be re-evaluated afterwards.
    try {
      const chunkSize =
        FunctionPrototypeCall(
          controllerState.sizeAlgorithm,
          undefined,
          chunk);
      enqueueValueWithSize(controller, chunk, chunkSize);
    } catch (error) {
      readableStreamDefaultControllerError(controller, error);
      throw error;
    }
  }
  readableStreamDefaultControllerCallPullIfNeeded(controller);
}

function readableStreamDefaultControllerHasBackpressure(controller) {
  return !readableStreamDefaultControllerShouldCallPull(controller);
}

function readableStreamDefaultControllerCanCloseOrEnqueue(controller) {
  const {
    stream,
  } = controller[kState];
  return !controller[kState].closeRequested &&
         stream[kState].state === 'readable';
}

function readableStreamDefaultControllerGetDesiredSize(controller) {
  const {
    stream,
    highWaterMark,
    queueTotalSize,
  } = controller[kState];
  switch (stream[kState].state) {
    case 'errored': return null;
    case 'closed': return 0;
    default:
      return highWaterMark - queueTotalSize;
  }
}

function readableStreamDefaultControllerShouldCallPull(controller) {
  // Single-pass version of the spec's predicate chain (CanCloseOrEnqueue,
  // IsLocked, HasDefaultReader, GetNumReadRequests, GetDesiredSize): this
  // runs at least once per chunk on every default-stream path. The
  // desired-size computation is inlined because the stream state is
  // already known to be 'readable' here.
  const controllerState = controller[kState];
  const stream = controllerState.stream;
  if (controllerState.closeRequested ||
      stream[kState].state !== 'readable' ||
      !controllerState.started)
    return false;

  const reader = stream[kState].reader;
  if (reader !== undefined &&
      reader[kState] !== undefined &&
      reader[kType] === 'ReadableStreamDefaultReader' &&
      reader[kState].readRequests.length) {
    return true;
  }

  return controllerState.highWaterMark - controllerState.queueTotalSize > 0;
}

function readableStreamDefaultControllerCallPullIfNeeded(controller) {
  if (!readableStreamDefaultControllerShouldCallPull(controller))
    return;
  readableStreamDefaultControllerPull(controller);
}

// The ShouldCallPull half of CallPullIfNeeded, split out so that callers
// that have already established the predicate from state in scope (the
// enqueue path above) can skip re-running it.
function readableStreamDefaultControllerPull(controller) {
  if (controller[kState].pulling) {
    controller[kState].pullAgain = true;
    return;
  }
  assert(!controller[kState].pullAgain);
  controller[kState].pulling = true;
  if (controller[kState].pullFulfilled === undefined) {
    // The pull reaction closures only capture the controller, so they are
    // created once on the first pull and reused for every subsequent pull
    // instead of allocating two fresh closures per chunk.
    controller[kState].pullFulfilled = () => {
      controller[kState].pulling = false;
      if (controller[kState].pullAgain) {
        controller[kState].pullAgain = false;
        readableStreamDefaultControllerCallPullIfNeeded(controller);
      }
    };
    controller[kState].pullRejected =
      (error) => readableStreamDefaultControllerError(controller, error);
  }
  // The pull algorithm may be a raw callback (a wrapped user source.pull
  // returns its result uncoerced; a synchronous throw surfaces here) or an
  // internal algorithm that always returns a promise; thenAlgorithmResult
  // handles both.
  let result;
  try {
    result = controller[kState].pullAlgorithm(controller);
  } catch (error) {
    result = PromiseReject(error);
  }
  thenAlgorithmResult(
    result,
    controller[kState].pullFulfilled,
    controller[kState].pullRejected);
}

function readableStreamDefaultControllerClearAlgorithms(controller) {
  controller[kState].pullAlgorithm = undefined;
  controller[kState].cancelAlgorithm = undefined;
  controller[kState].sizeAlgorithm = undefined;
}

function readableStreamDefaultControllerError(controller, error) {
  const {
    stream,
  } = controller[kState];
  if (stream[kState].state === 'readable') {
    resetQueue(controller);
    readableStreamDefaultControllerClearAlgorithms(controller);
    readableStreamError(stream, error);
  }
}

function readableStreamDefaultControllerCancelSteps(controller, reason) {
  resetQueue(controller);
  const result = controller[kState].cancelAlgorithm(reason);
  readableStreamDefaultControllerClearAlgorithms(controller);
  return result;
}

function readableStreamDefaultControllerPullSteps(controller, readRequest) {
  const {
    stream,
    queue,
  } = controller[kState];
  if (queue.length) {
    const chunk = dequeueValue(controller);
    if (controller[kState].closeRequested && !queue.length) {
      readableStreamDefaultControllerClearAlgorithms(controller);
      readableStreamClose(stream);
    } else if (!controller[kState].closeRequested &&
               controller[kState].started &&
               controller[kState].highWaterMark -
                 controller[kState].queueTotalSize > 0) {
      // Reduced ShouldCallPull: the state is known to be readable and the
      // queue was non-empty, so no read requests can be parked.
      readableStreamDefaultControllerPull(controller);
    }
    readRequest[kChunk](chunk);
    return;
  }
  readableStreamAddReadRequest(stream, readRequest);
  // Reduced ShouldCallPull: the state is known to be readable, an empty
  // queue in that state implies close has not been requested (close with
  // an empty queue closes the stream immediately), and the read request
  // parked above already satisfies the reader-with-pending-reads arm.
  if (controller[kState].started)
    readableStreamDefaultControllerPull(controller);
}

function setupReadableStreamDefaultController(
  stream,
  controller,
  startAlgorithm,
  pullAlgorithm,
  cancelAlgorithm,
  highWaterMark,
  sizeAlgorithm) {
  assert(stream[kState].controller === undefined);
  controller[kState] = {
    cancelAlgorithm,
    closeRequested: false,
    highWaterMark,
    pullAgain: false,
    pullAlgorithm,
    pulling: false,
    pullFulfilled: undefined,
    pullRejected: undefined,
    queue: kEmptyQueue,
    queueTotalSize: 0,
    started: false,
    sizeAlgorithm,
    stream,
  };
  stream[kState].controller = controller;

  const startResult = startAlgorithm();

  if (startResult === null ||
      (typeof startResult !== 'object' && typeof startResult !== 'function')) {
    // Non-thenable start result: fulfillment is guaranteed and no .then
    // lookup on the result is observable, so run the post-start step
    // directly at the exact microtask position the promise reaction
    // would have had, skipping two promise allocations.
    queueMicrotask(() => {
      controller[kState].started = true;
      assert(!controller[kState].pulling);
      assert(!controller[kState].pullAgain);
      readableStreamDefaultControllerCallPullIfNeeded(controller);
    });
    return;
  }

  PromisePrototypeThen(
    new Promise((r) => r(startResult)),
    () => {
      controller[kState].started = true;
      assert(!controller[kState].pulling);
      assert(!controller[kState].pullAgain);
      readableStreamDefaultControllerCallPullIfNeeded(controller);
    },
    (error) => readableStreamDefaultControllerError(controller, error));
}

function setupReadableStreamDefaultControllerFromSource(
  stream,
  source,
  highWaterMark,
  sizeAlgorithm) {
  const controller = new ReadableStreamDefaultController(kSkipThrow);
  const start = source?.start;
  const pull = source?.pull;
  const cancel = source?.cancel;
  const startAlgorithm = start ?
    FunctionPrototypeBind(start, source, controller) :
    nonOpCallback;
  const pullAlgorithm = pull ?
    createRawCallback1Param('source.pull', pull, source) :
    nonOpCallback;
  const cancelAlgorithm = cancel ?
    createPromiseCallback1Param('source.cancel', cancel, source) :
    nonOpCancel;

  setupReadableStreamDefaultController(
    stream,
    controller,
    startAlgorithm,
    pullAlgorithm,
    cancelAlgorithm,
    highWaterMark,
    sizeAlgorithm);
}

function readableByteStreamControllerClose(controller) {
  const {
    closeRequested,
    pendingPullIntos,
    queueTotalSize,
    stream,
  } = controller[kState];

  if (closeRequested || stream[kState].state !== 'readable')
    return;

  if (queueTotalSize) {
    controller[kState].closeRequested = true;
    return;
  }

  if (pendingPullIntos.length) {
    const firstPendingPullInto = pendingPullIntos.peek();
    if (firstPendingPullInto.bytesFilled % firstPendingPullInto.elementSize !== 0) {
      const error = new ERR_INVALID_STATE.TypeError('Partial read');
      readableByteStreamControllerError(controller, error);
      throw error;
    }
  }

  readableByteStreamControllerClearAlgorithms(controller);
  readableStreamClose(stream);
}

function readableByteStreamControllerCommitPullIntoDescriptor(stream, desc) {
  assert(stream[kState].state !== 'errored');
  assert(desc.type !== 'none');

  let done = false;
  if (stream[kState].state === 'closed') {
    assert(desc.bytesFilled % desc.elementSize === 0);
    done = true;
  }

  const filledView =
    readableByteStreamControllerConvertPullIntoDescriptor(desc);

  if (desc.type === 'default') {
    readableStreamFulfillReadRequest(stream, filledView, done);
  } else {
    assert(desc.type === 'byob');
    readableStreamFulfillReadIntoRequest(stream, filledView, done);
  }
}

function readableByteStreamControllerCommitPullIntoDescriptors(stream, descriptors) {
  for (let i = 0; i < descriptors.length; ++i) {
    readableByteStreamControllerCommitPullIntoDescriptor(
      stream,
      descriptors[i],
    );
  }
}

function readableByteStreamControllerInvalidateBYOBRequest(controller) {
  if (controller[kState].byobRequest === null)
    return;
  controller[kState].byobRequest[kState].controller = undefined;
  controller[kState].byobRequest[kState].view = null;
  controller[kState].byobRequest = null;
}

function readableByteStreamControllerClearAlgorithms(controller) {
  controller[kState].pullAlgorithm = undefined;
  controller[kState].cancelAlgorithm = undefined;
}

function readableByteStreamControllerClearPendingPullIntos(controller) {
  readableByteStreamControllerInvalidateBYOBRequest(controller);
  controller[kState].pendingPullIntos = kEmptyQueue;
}

function readableByteStreamControllerGetDesiredSize(controller) {
  const {
    stream,
    highWaterMark,
    queueTotalSize,
  } = controller[kState];
  switch (stream[kState].state) {
    case 'errored': return null;
    case 'closed': return 0;
    default: return highWaterMark - queueTotalSize;
  }
}

function readableByteStreamControllerShouldCallPull(controller) {
  // Single-pass version of the spec's predicate chain (HasDefaultReader,
  // GetNumReadRequests, HasBYOBReader, GetNumReadIntoRequests,
  // GetDesiredSize): this runs at least once per chunk on every byte
  // stream path. The desired-size computation is inlined because the
  // stream state is already known to be 'readable' here.
  const controllerState = controller[kState];
  const stream = controllerState.stream;
  if (stream[kState].state !== 'readable' ||
      controllerState.closeRequested ||
      !controllerState.started) {
    return false;
  }
  const reader = stream[kState].reader;
  if (reader !== undefined && reader[kState] !== undefined) {
    const type = reader[kType];
    if (type === 'ReadableStreamDefaultReader') {
      if (reader[kState].readRequests.length) return true;
    } else if (type === 'ReadableStreamBYOBReader') {
      if (reader[kState].readIntoRequests.length) return true;
    }
  }

  return controllerState.highWaterMark - controllerState.queueTotalSize > 0;
}

function readableByteStreamControllerHandleQueueDrain(controller) {
  const {
    closeRequested,
    queueTotalSize,
    stream,
  } = controller[kState];
  assert(stream[kState].state === 'readable');
  if (!queueTotalSize && closeRequested) {
    readableByteStreamControllerClearAlgorithms(controller);
    readableStreamClose(stream);
    return;
  }
  readableByteStreamControllerCallPullIfNeeded(controller);
}

function readableByteStreamControllerPullInto(
  controller,
  view,
  min,
  readIntoRequest) {
  const {
    closeRequested,
    stream,
    pendingPullIntos,
  } = controller[kState];
  let elementSize = 1;
  let ctor = DataView;
  if (isArrayBufferView(view) && !isDataView(view)) {
    elementSize = view.constructor.BYTES_PER_ELEMENT;
    ctor = view.constructor;
  }

  const minimumFill = min * elementSize;
  assert(minimumFill >= elementSize && minimumFill <= view.byteLength);
  assert(minimumFill % elementSize === 0);

  const buffer = ArrayBufferViewGetBuffer(view);
  const byteOffset = ArrayBufferViewGetByteOffset(view);
  const byteLength = ArrayBufferViewGetByteLength(view);
  const bufferByteLength = ArrayBufferPrototypeGetByteLength(buffer);

  let transferredBuffer;
  try {
    transferredBuffer = ArrayBufferPrototypeTransfer(buffer);
  } catch (error) {
    readIntoRequest[kError](error);
    return;
  }
  const desc = {
    buffer: transferredBuffer,
    bufferByteLength,
    byteOffset,
    byteLength,
    bytesFilled: 0,
    minimumFill,
    elementSize,
    ctor,
    type: 'byob',
  };
  if (pendingPullIntos.length) {
    pendingPullIntos.push(desc);
    readableStreamAddReadIntoRequest(stream, readIntoRequest);
    return;
  }
  if (stream[kState].state === 'closed') {
    const emptyView = new ctor(desc.buffer, byteOffset, 0);
    readIntoRequest[kClose](emptyView);
    return;
  }
  if (controller[kState].queueTotalSize) {
    if (readableByteStreamControllerFillPullIntoDescriptorFromQueue(
      controller,
      desc)) {
      const filledView =
        readableByteStreamControllerConvertPullIntoDescriptor(desc);
      readableByteStreamControllerHandleQueueDrain(controller);
      readIntoRequest[kChunk](filledView);
      return;
    }
    if (closeRequested) {
      const error = new ERR_INVALID_STATE.TypeError('ReadableStream closed');
      readableByteStreamControllerError(controller, error);
      readIntoRequest[kError](error);
      return;
    }
  }
  materializePendingPullIntos(controller[kState]).push(desc);
  readableStreamAddReadIntoRequest(stream, readIntoRequest);
  readableByteStreamControllerCallPullIfNeeded(controller);
}

// Pending pull-into descriptor queues start out as (and are reset to)
// the shared immutable empty queue so that constructing a byte stream
// never allocates descriptor storage; the two push sites that can
// observe an empty queue materialize a real Queue on first use.
function materializePendingPullIntos(state) {
  const pendingPullIntos = state.pendingPullIntos;
  if (pendingPullIntos === kEmptyQueue)
    return state.pendingPullIntos = new Queue();
  return pendingPullIntos;
}

function readableByteStreamControllerRespondInternal(controller, bytesWritten) {
  const {
    stream,
    pendingPullIntos,
  } = controller[kState];
  const desc = pendingPullIntos.peek();
  readableByteStreamControllerInvalidateBYOBRequest(controller);
  if (stream[kState].state === 'closed') {
    if (bytesWritten)
      throw new ERR_INVALID_STATE.TypeError(
        'Controller is closed but view is not zero-length');
    readableByteStreamControllerRespondInClosedState(controller, desc);
  } else {
    assert(stream[kState].state === 'readable');
    if (!bytesWritten)
      throw new ERR_INVALID_STATE.TypeError('View cannot be zero-length');
    readableByteStreamControllerRespondInReadableState(
      controller,
      bytesWritten,
      desc);
  }
  readableByteStreamControllerCallPullIfNeeded(controller);
}

function readableByteStreamControllerRespond(controller, bytesWritten) {
  const {
    pendingPullIntos,
    stream,
  } = controller[kState];
  assert(pendingPullIntos.length);
  const desc = pendingPullIntos.peek();

  if (stream[kState].state === 'closed') {
    if (bytesWritten !== 0)
      throw new ERR_INVALID_ARG_VALUE('bytesWritten', bytesWritten);
  } else {
    assert(stream[kState].state === 'readable');

    if (!bytesWritten)
      throw new ERR_INVALID_ARG_VALUE('bytesWritten', bytesWritten);

    if ((desc.bytesFilled + bytesWritten) > desc.byteLength)
      throw new ERR_INVALID_ARG_VALUE.RangeError('bytesWritten', bytesWritten);
  }

  desc.buffer = ArrayBufferPrototypeTransfer(desc.buffer);

  readableByteStreamControllerRespondInternal(controller, bytesWritten);
}

function readableByteStreamControllerRespondInClosedState(controller, desc) {
  assert(desc.bytesFilled % desc.elementSize === 0);
  if (desc.type === 'none') {
    readableByteStreamControllerShiftPendingPullInto(controller);
  }
  const {
    stream,
  } = controller[kState];
  if (readableStreamHasBYOBReader(stream)) {
    const filledPullIntos = [];
    for (let i = 0; i < readableStreamGetNumReadIntoRequests(stream); ++i) {
      ArrayPrototypePush(filledPullIntos, readableByteStreamControllerShiftPendingPullInto(controller));
    }
    readableByteStreamControllerCommitPullIntoDescriptors(stream, filledPullIntos);
  }
}
