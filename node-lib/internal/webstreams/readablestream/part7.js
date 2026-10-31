
function readableByteStreamControllerFillHeadPullIntoDescriptor(
  controller,
  size,
  desc) {
  const {
    pendingPullIntos,
    byobRequest,
  } = controller[kState];
  assert(!pendingPullIntos.length || pendingPullIntos.peek() === desc);
  assert(byobRequest === null);
  desc.bytesFilled += size;
}

function readableByteStreamControllerEnqueue(controller, chunk) {
  const {
    closeRequested,
    pendingPullIntos,
    queue,
    stream,
  } = controller[kState];

  const buffer = ArrayBufferViewGetBuffer(chunk);
  const byteOffset = ArrayBufferViewGetByteOffset(chunk);
  const byteLength = ArrayBufferViewGetByteLength(chunk);

  if (closeRequested || stream[kState].state !== 'readable')
    return;

  const transferredBuffer = ArrayBufferPrototypeTransfer(buffer);

  if (pendingPullIntos.length) {
    const firstPendingPullInto = pendingPullIntos.peek();

    if (ArrayBufferPrototypeGetDetached(firstPendingPullInto.buffer)) {
      throw new ERR_INVALID_STATE.TypeError(
        'Destination ArrayBuffer is detached',
      );
    }

    readableByteStreamControllerInvalidateBYOBRequest(controller);

    firstPendingPullInto.buffer = ArrayBufferPrototypeTransfer(
      firstPendingPullInto.buffer,
    );

    if (firstPendingPullInto.type === 'none') {
      readableByteStreamControllerEnqueueDetachedPullIntoToQueue(
        controller,
        firstPendingPullInto,
      );
    }
  }

  // Single consolidated pass over the reader state. The spec routes this
  // through HasDefaultReader / ProcessReadRequestsUsingQueue /
  // GetNumReadRequests / FulfillReadRequest, which would re-run the same
  // reader brand check and re-load the read request list four times on
  // this per-chunk path.
  const { reader } = stream[kState];
  if (reader !== undefined &&
      reader[kState] !== undefined &&
      reader[kType] === 'ReadableStreamDefaultReader') {
    const { readRequests } = reader[kState];
    if (readRequests.length && controller[kState].queueTotalSize > 0) {
      // Only possible when data was enqueued while the stream was not
      // being read; read requests otherwise never coexist with a
      // non-empty queue.
      readableByteStreamControllerProcessReadRequestsUsingQueue(controller);
    }
    if (!readRequests.length) {
      readableByteStreamControllerEnqueueChunkToQueue(
        controller,
        transferredBuffer,
        byteOffset,
        byteLength);
    } else {
      assert(!queue.length);
      if (pendingPullIntos.length) {
        assert(pendingPullIntos.peek().type === 'default');
        readableByteStreamControllerShiftPendingPullInto(controller);
      }
      const transferredView =
        new Uint8Array(transferredBuffer, byteOffset, byteLength);
      const readRequest = readRequests.shift();
      readRequest[kChunk](transferredView);
    }
  } else if (readableStreamHasBYOBReader(stream)) {
    readableByteStreamControllerEnqueueChunkToQueue(
      controller,
      transferredBuffer,
      byteOffset,
      byteLength);
    const filledPullIntos = readableByteStreamControllerProcessPullIntoDescriptorsUsingQueue(
      controller);
    readableByteStreamControllerCommitPullIntoDescriptors(stream, filledPullIntos);
  } else {
    assert(!isReadableStreamLocked(stream));
    readableByteStreamControllerEnqueueChunkToQueue(
      controller,
      transferredBuffer,
      byteOffset,
      byteLength);
  }
  readableByteStreamControllerCallPullIfNeeded(controller);
}

function readableByteStreamControllerEnqueueClonedChunkToQueue(
  controller,
  buffer,
  byteOffset,
  byteLength,
) {
  let cloneResult;
  try {
    cloneResult = ArrayBufferPrototypeSlice(
      buffer,
      byteOffset,
      byteOffset + byteLength,
    );
  } catch (error) {
    readableByteStreamControllerError(controller, error);
    throw error;
  }
  readableByteStreamControllerEnqueueChunkToQueue(
    controller,
    cloneResult,
    0,
    byteLength,
  );
}

function readableByteStreamControllerEnqueueChunkToQueue(
  controller,
  buffer,
  byteOffset,
  byteLength) {
  const state = controller[kState];
  materializeQueue(state).push({
    buffer,
    byteOffset,
    byteLength,
  });
  state.queueTotalSize += byteLength;
}

function readableByteStreamControllerEnqueueDetachedPullIntoToQueue(
  controller,
  desc,
) {
  const {
    buffer,
    byteOffset,
    bytesFilled,
    type,
  } = desc;
  assert(type === 'none');

  if (bytesFilled > 0) {
    readableByteStreamControllerEnqueueClonedChunkToQueue(
      controller,
      buffer,
      byteOffset,
      bytesFilled,
    );
  }
  readableByteStreamControllerShiftPendingPullInto(controller);
}

function readableByteStreamControllerFillPullIntoDescriptorFromQueue(
  controller,
  desc) {
  const {
    buffer,
    byteLength,
    byteOffset,
    bytesFilled,
    minimumFill,
    elementSize,
  } = desc;
  const maxBytesToCopy = MathMin(
    controller[kState].queueTotalSize,
    byteLength - bytesFilled);
  const maxBytesFilled = bytesFilled + maxBytesToCopy;
  const maxAlignedBytes = maxBytesFilled - (maxBytesFilled % elementSize);
  let totalBytesToCopyRemaining = maxBytesToCopy;
  let ready = false;
  assert(!ArrayBufferPrototypeGetDetached(buffer));
  assert(bytesFilled < minimumFill);
  if (maxAlignedBytes >= minimumFill) {
    totalBytesToCopyRemaining = maxAlignedBytes - bytesFilled;
    ready = true;
  }
  const {
    queue,
  } = controller[kState];

  while (totalBytesToCopyRemaining) {
    const headOfQueue = queue.peek();
    const bytesToCopy = MathMin(
      totalBytesToCopyRemaining,
      headOfQueue.byteLength);
    const destStart = byteOffset + desc.bytesFilled;
    assert(canCopyArrayBuffer(
      buffer,
      destStart,
      headOfQueue.buffer,
      headOfQueue.byteOffset,
      bytesToCopy));
    copyArrayBuffer(
      buffer,
      destStart,
      headOfQueue.buffer,
      headOfQueue.byteOffset,
      bytesToCopy);
    if (headOfQueue.byteLength === bytesToCopy) {
      queue.shift();
    } else {
      headOfQueue.byteOffset += bytesToCopy;
      headOfQueue.byteLength -= bytesToCopy;
    }
    controller[kState].queueTotalSize -= bytesToCopy;
    readableByteStreamControllerFillHeadPullIntoDescriptor(
      controller,
      bytesToCopy,
      desc);
    totalBytesToCopyRemaining -= bytesToCopy;
  }

  if (!ready) {
    assert(!controller[kState].queueTotalSize);
    assert(desc.bytesFilled > 0);
    assert(desc.bytesFilled < minimumFill);
  }
  return ready;
}

function readableByteStreamControllerProcessPullIntoDescriptorsUsingQueue(
  controller) {
  const {
    closeRequested,
    pendingPullIntos,
  } = controller[kState];
  assert(!closeRequested);
  const filledPullIntos = [];
  while (pendingPullIntos.length) {
    if (!controller[kState].queueTotalSize)
      break;
    const desc = pendingPullIntos.peek();
    if (readableByteStreamControllerFillPullIntoDescriptorFromQueue(
      controller,
      desc)) {
      readableByteStreamControllerShiftPendingPullInto(controller);
      ArrayPrototypePush(filledPullIntos, desc);
    }
  }
  return filledPullIntos;
}

function readableByteStreamControllerRespondInReadableState(
  controller,
  bytesWritten,
  desc) {
  const {
    stream,
  } = controller[kState];
  const {
    buffer,
    bytesFilled,
    byteLength,
    type,
  } = desc;

  if (bytesFilled + bytesWritten > byteLength)
    throw new ERR_INVALID_STATE.RangeError('The buffer size is invalid');

  readableByteStreamControllerFillHeadPullIntoDescriptor(
    controller,
    bytesWritten,
    desc);

  if (type === 'none') {
    readableByteStreamControllerEnqueueDetachedPullIntoToQueue(
      controller,
      desc,
    );
    const filledPullIntos = readableByteStreamControllerProcessPullIntoDescriptorsUsingQueue(
      controller,
    );
    readableByteStreamControllerCommitPullIntoDescriptors(stream, filledPullIntos);
    return;
  }

  if (desc.bytesFilled < desc.minimumFill)
    return;

  readableByteStreamControllerShiftPendingPullInto(controller);

  const remainderSize = desc.bytesFilled % desc.elementSize;

  if (remainderSize) {
    const end = desc.byteOffset + desc.bytesFilled;
    const start = end - remainderSize;
    const remainder =
      ArrayBufferPrototypeSlice(
        buffer,
        start,
        end);
    readableByteStreamControllerEnqueueChunkToQueue(
      controller,
      remainder,
      0,
      ArrayBufferPrototypeGetByteLength(remainder));
  }
  desc.bytesFilled -= remainderSize;
  const filledPullIntos = readableByteStreamControllerProcessPullIntoDescriptorsUsingQueue(controller);

  readableByteStreamControllerCommitPullIntoDescriptor(stream, desc);
  readableByteStreamControllerCommitPullIntoDescriptors(stream, filledPullIntos);
}

function readableByteStreamControllerRespondWithNewView(controller, view) {
  const {
    stream,
    pendingPullIntos,
  } = controller[kState];
  assert(pendingPullIntos.length);

  const desc = pendingPullIntos.peek();
  assert(stream[kState].state !== 'errored');

  const viewByteLength = ArrayBufferViewGetByteLength(view);
  const viewByteOffset = ArrayBufferViewGetByteOffset(view);
  const viewBuffer = ArrayBufferViewGetBuffer(view);
  const viewBufferByteLength = ArrayBufferPrototypeGetByteLength(viewBuffer);

  if (stream[kState].state === 'closed') {
    if (viewByteLength !== 0)
      throw new ERR_INVALID_STATE.TypeError('View is not zero-length');
  } else {
    assert(stream[kState].state === 'readable');
    if (viewByteLength === 0)
      throw new ERR_INVALID_STATE.TypeError('View is zero-length');
  }

  const {
    byteOffset,
    byteLength,
    bytesFilled,
    bufferByteLength,
  } = desc;

  if (byteOffset + bytesFilled !== viewByteOffset)
    throw new ERR_INVALID_ARG_VALUE.RangeError('view', view);

  if (bytesFilled + viewByteLength > byteLength)
    throw new ERR_INVALID_ARG_VALUE.RangeError('view', view);

  if (bufferByteLength !== viewBufferByteLength)
    throw new ERR_INVALID_ARG_VALUE.RangeError('view', view);

  desc.buffer = ArrayBufferPrototypeTransfer(viewBuffer);

  readableByteStreamControllerRespondInternal(controller, viewByteLength);
}

function readableByteStreamControllerShiftPendingPullInto(controller) {
  assert(controller[kState].byobRequest === null);
  return controller[kState].pendingPullIntos.shift();
}

function readableByteStreamControllerCallPullIfNeeded(controller) {
  if (!readableByteStreamControllerShouldCallPull(controller))
    return;
  if (controller[kState].pulling) {
    controller[kState].pullAgain = true;
    return;
  }
  assert(!controller[kState].pullAgain);
  controller[kState].pulling = true;
  if (controller[kState].pullFulfilled === undefined) {
    // See readableStreamDefaultControllerCallPullIfNeeded: created once,
    // reused for every pull.
    controller[kState].pullFulfilled = () => {
      controller[kState].pulling = false;
      if (controller[kState].pullAgain) {
        controller[kState].pullAgain = false;
        readableByteStreamControllerCallPullIfNeeded(controller);
      }
    };
    controller[kState].pullRejected =
      (error) => readableByteStreamControllerError(controller, error);
  }
  // See readableStreamDefaultControllerPull for the raw-callback contract.
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

function readableByteStreamControllerError(controller, error) {
  const {
    stream,
  } = controller[kState];
  if (stream[kState].state !== 'readable')
    return;
  readableByteStreamControllerClearPendingPullIntos(controller);
  resetQueue(controller);
  readableByteStreamControllerClearAlgorithms(controller);
  readableStreamError(stream, error);
}

function readableByteStreamControllerCancelSteps(controller, reason) {
  readableByteStreamControllerClearPendingPullIntos(controller);
  resetQueue(controller);
  const result = controller[kState].cancelAlgorithm(reason);
  readableByteStreamControllerClearAlgorithms(controller);
  return result;
}

// Dequeues the first chunk of the byte queue as a Uint8Array view,
// handling queue drain (close-on-empty or pull) before the view is
// created. This is the [[queueTotalSize]] > 0 arm of the byte
// controller's pull steps; it is also called directly from the
// buffered fast paths in ReadableStreamDefaultReader.read() and the
// async iterator, which resolve with the view without allocating a
// read request.
function readableByteStreamControllerDequeueChunk(controller) {
  assert(controller[kState].queueTotalSize > 0);
  const {
    buffer,
    byteOffset,
    byteLength,
  } = controller[kState].queue.shift();

  controller[kState].queueTotalSize -= byteLength;
  readableByteStreamControllerHandleQueueDrain(controller);
  return new Uint8Array(buffer, byteOffset, byteLength);
}

function readableByteStreamControllerFillReadRequestFromQueue(controller, readRequest) {
  readRequest[kChunk](readableByteStreamControllerDequeueChunk(controller));
}

function readableByteStreamControllerProcessReadRequestsUsingQueue(controller) {
  const {
    stream,
    queueTotalSize,
  } = controller[kState];
  const { reader } = stream[kState];
  assert(isReadableStreamDefaultReader(reader));

  while (reader[kState].readRequests.length > 0) {
    if (queueTotalSize === 0) {
      return;
    }
    readableByteStreamControllerFillReadRequestFromQueue(
      controller,
      reader[kState].readRequests.shift(),
    );
  }
}

function readableByteStreamControllerPullSteps(controller, readRequest) {
  const {
    queueTotalSize,
    stream,
  } = controller[kState];
  assert(readableStreamHasDefaultReader(stream));
  if (queueTotalSize) {
    assert(!readableStreamGetNumReadRequests(stream));
    readableByteStreamControllerFillReadRequestFromQueue(
      controller,
      readRequest,
    );
    return;
  }
  const {
    autoAllocateChunkSize,
  } = controller[kState];
  if (autoAllocateChunkSize !== undefined) {
    try {
      const buffer = new ArrayBuffer(autoAllocateChunkSize);
      materializePendingPullIntos(controller[kState]).push(
        {
          buffer,
          bufferByteLength: autoAllocateChunkSize,
          byteOffset: 0,
          byteLength: autoAllocateChunkSize,
          bytesFilled: 0,
          minimumFill: 1,
          elementSize: 1,
          ctor: Uint8Array,
          type: 'default',
        });
    } catch (error) {
      readRequest[kError](error);
      return;
    }
  }

  readableStreamAddReadRequest(stream, readRequest);
  readableByteStreamControllerCallPullIfNeeded(controller);
}

function setupReadableByteStreamController(
  stream,
  controller,
  startAlgorithm,
  pullAlgorithm,
  cancelAlgorithm,
  highWaterMark,
  autoAllocateChunkSize) {
  assert(stream[kState].controller === undefined);
  if (autoAllocateChunkSize !== undefined) {
    assert(NumberIsInteger(autoAllocateChunkSize));
    assert(autoAllocateChunkSize > 0);
  }
  controller[kState] = {
    byobRequest: null,
    closeRequested: false,
    pullAgain: false,
    pulling: false,
    pullFulfilled: undefined,
    pullRejected: undefined,
    started: false,
    stream,
    queue: kEmptyQueue,
    queueTotalSize: 0,
    highWaterMark,
    pullAlgorithm,
    cancelAlgorithm,
    autoAllocateChunkSize,
    pendingPullIntos: kEmptyQueue,
  };
  stream[kState].controller = controller;

  const startResult = startAlgorithm();

  if (startResult === null ||
      (typeof startResult !== 'object' && typeof startResult !== 'function')) {
    // See setupReadableStreamDefaultController.
    queueMicrotask(() => {
      controller[kState].started = true;
      assert(!controller[kState].pulling);
      assert(!controller[kState].pullAgain);
      readableByteStreamControllerCallPullIfNeeded(controller);
    });
    return;
  }

  PromisePrototypeThen(
    new Promise((r) => r(startResult)),
    () => {
      controller[kState].started = true;
      assert(!controller[kState].pulling);
      assert(!controller[kState].pullAgain);
      readableByteStreamControllerCallPullIfNeeded(controller);
    },
    (error) => readableByteStreamControllerError(controller, error));
}

function setupReadableByteStreamControllerFromSource(
  stream,
  source,
  highWaterMark) {
  const controller = new ReadableByteStreamController(kSkipThrow);
  const start = source?.start;
  const pull = source?.pull;
  const cancel = source?.cancel;
  const autoAllocateChunkSize = source?.autoAllocateChunkSize;
  const startAlgorithm = start ?
    FunctionPrototypeBind(start, source, controller) :
    nonOpCallback;
  const pullAlgorithm = pull ?
    createRawCallback1Param('source.pull', pull, source) :
    nonOpCallback;
  const cancelAlgorithm = cancel ?
    createPromiseCallback1Param('source.cancel', cancel, source) :
    nonOpCancel;

  if (autoAllocateChunkSize === 0) {
    throw new ERR_INVALID_ARG_VALUE(
      'source.autoAllocateChunkSize',
      autoAllocateChunkSize);
  }
  setupReadableByteStreamController(
    stream,
    controller,
    startAlgorithm,
    pullAlgorithm,
    cancelAlgorithm,
    highWaterMark,
    autoAllocateChunkSize);
}
