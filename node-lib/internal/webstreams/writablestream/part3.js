
function writableStreamDefaultControllerGetChunkSize(controller, chunk) {
  const {
    stream,
    sizeAlgorithm,
  } = controller[kState];
  if (sizeAlgorithm === undefined) {
    assert(stream[kState].state === 'closed' ||
           stream[kState].state === 'errored' ||
           stream[kState].state === 'erroring');
    return 1;
  }

  // The internal default size algorithm is never observable by user
  // code, always returns 1, and cannot throw: skip the call.
  if (sizeAlgorithm === defaultSizeAlgorithm)
    return 1;

  try {
    return FunctionPrototypeCall(
      sizeAlgorithm,
      undefined,
      chunk);
  } catch (error) {
    writableStreamDefaultControllerErrorIfNeeded(controller, error);
    return 1;
  }
}

function writableStreamDefaultControllerErrorIfNeeded(controller, error) {
  const {
    stream,
  } = controller[kState];
  if (stream[kState].state === 'writable')
    writableStreamDefaultControllerError(controller, error);
}

function writableStreamDefaultControllerError(controller, error) {
  const {
    stream,
  } = controller[kState];
  assert(stream[kState].state === 'writable');
  writableStreamDefaultControllerClearAlgorithms(controller);
  writableStreamStartErroring(stream, error);
}

function writableStreamDefaultControllerClose(controller) {
  enqueueValueWithSize(controller, kCloseSentinel, 0);
  writableStreamDefaultControllerAdvanceQueueIfNeeded(controller);
}

function writableStreamDefaultControllerClearAlgorithms(controller) {
  controller[kState].writeAlgorithm = undefined;
  controller[kState].closeAlgorithm = undefined;
  controller[kState].abortAlgorithm = undefined;
  controller[kState].sizeAlgorithm = undefined;
}

function writableStreamDefaultControllerAdvanceQueueIfNeeded(controller) {
  const {
    queue,
    started,
    stream,
  } = controller[kState];
  if (!started || stream[kState].inFlightWriteRequest.promise !== undefined)
    return;

  if (stream[kState].state === 'erroring') {
    writableStreamFinishErroring(stream);
    return;
  }

  if (!queue.length)
    return;

  const value = peekQueueValue(controller);
  if (value === kCloseSentinel)
    writableStreamDefaultControllerProcessClose(controller);
  else
    writableStreamDefaultControllerProcessWrite(controller, value);
}

function setupWritableStreamDefaultControllerFromSink(
  stream,
  sink,
  highWaterMark,
  sizeAlgorithm) {
  const controller = new WritableStreamDefaultController(kSkipThrow);
  const start = sink?.start;
  const write = sink?.write;
  const close = sink?.close;
  const abort = sink?.abort;
  const startAlgorithm = start ?
    FunctionPrototypeBind(start, sink, controller) :
    nonOpCallback;
  const writeAlgorithm = write ?
    createRawCallback2Params('sink.write', write, sink) :
    nonOpCallback;
  const closeAlgorithm = close ?
    createPromiseCallbackNoParams('sink.close', close, sink) :
    nonOpCancel;
  const abortAlgorithm = abort ?
    createPromiseCallback1Param('sink.abort', abort, sink) :
    nonOpCancel;
  setupWritableStreamDefaultController(
    stream,
    controller,
    startAlgorithm,
    writeAlgorithm,
    closeAlgorithm,
    abortAlgorithm,
    highWaterMark,
    sizeAlgorithm);
}

function setupWritableStreamDefaultController(
  stream,
  controller,
  startAlgorithm,
  writeAlgorithm,
  closeAlgorithm,
  abortAlgorithm,
  highWaterMark,
  sizeAlgorithm) {
  assert(isWritableStream(stream));
  assert(stream[kState].controller === undefined);
  controller[kState] = {
    abortAlgorithm,
    closeAlgorithm,
    highWaterMark,
    queue: kEmptyQueue,
    queueTotalSize: 0,
    abortController: new AbortController(),
    sizeAlgorithm,
    started: false,
    stream,
    writeAlgorithm,
    writeFulfilled: undefined,
    writeRejected: undefined,
  };
  stream[kState].controller = controller;

  writableStreamUpdateBackpressure(controller, stream[kState]);

  const startResult = startAlgorithm();

  if (startResult === null ||
      (typeof startResult !== 'object' && typeof startResult !== 'function')) {
    // Non-thenable start result: fulfillment is guaranteed and no .then
    // lookup on the result is observable, so run the post-start step
    // directly at the exact microtask position the promise reaction
    // would have had, skipping two promise allocations.
    queueMicrotask(() => {
      assert(stream[kState].state === 'writable' ||
             stream[kState].state === 'erroring');
      controller[kState].started = true;
      writableStreamDefaultControllerAdvanceQueueIfNeeded(controller);
    });
    return;
  }

  PromisePrototypeThen(
    new Promise((r) => r(startResult)),
    () => {
      assert(stream[kState].state === 'writable' ||
             stream[kState].state === 'erroring');
      controller[kState].started = true;
      writableStreamDefaultControllerAdvanceQueueIfNeeded(controller);
    },
    (error) => {
      assert(stream[kState].state === 'writable' ||
             stream[kState].state === 'erroring');
      controller[kState].started = true;
      writableStreamDealWithRejection(stream, error);
    });
}

module.exports = {
  WritableStream,
  WritableStreamDefaultWriter,
  WritableStreamDefaultController,
  TransferredWritableStream,

  // Exported Brand Checks
  isWritableStream,
  isWritableStreamDefaultController,
  isWritableStreamDefaultWriter,

  isWritableStreamLocked,
  setupWritableStreamDefaultWriter,
  writableStreamAbort,
  writableStreamClose,
  writableStreamUpdateBackpressure,
  writableStreamStartErroring,
  writableStreamRejectCloseAndClosedPromiseIfNeeded,
  writableStreamMarkFirstWriteRequestInFlight,
  writableStreamMarkCloseRequestInFlight,
  writableStreamHasOperationMarkedInFlight,
  writableStreamFinishInFlightWriteWithError,
  writableStreamFinishInFlightWrite,
  writableStreamFinishInFlightCloseWithError,
  writableStreamFinishInFlightClose,
  writableStreamFinishErroring,
  writableStreamDealWithRejection,
  writableStreamCloseQueuedOrInFlight,
  writableStreamAddWriteRequest,
  writableStreamDefaultWriterWrite,
  writableStreamDefaultWriterWriteWithRequest,
  writableStreamDefaultWriterRelease,
  writableStreamDefaultWriterGetDesiredSize,
  writableStreamDefaultWriterEnsureReadyPromiseRejected,
  writableStreamDefaultWriterEnsureClosedPromiseRejected,
  writerClosedPromise,
  writerReadyPromise,
  writableStreamDefaultWriterCloseWithErrorPropagation,
  writableStreamDefaultWriterClose,
  writableStreamDefaultWriterAbort,
  writableStreamDefaultControllerWrite,
  writableStreamDefaultControllerProcessWrite,
  writableStreamDefaultControllerProcessClose,
  writableStreamDefaultControllerGetDesiredSize,
  writableStreamDefaultControllerGetChunkSize,
  writableStreamDefaultControllerErrorIfNeeded,
  writableStreamDefaultControllerError,
  writableStreamDefaultControllerClose,
  writableStreamDefaultControllerClearAlgorithms,
  writableStreamDefaultControllerAdvanceQueueIfNeeded,
  setupWritableStreamDefaultControllerFromSink,
  setupWritableStreamDefaultController,
  createWritableStream,
};
