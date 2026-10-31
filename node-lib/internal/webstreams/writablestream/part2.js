
// Same as writerClosedPromise(), for the writer's [[readyPromise]]. The
// pending epoch is derivable too: a writable stream with backpressure and
// no queued close has a pending ready promise.
function writerReadyPromise(writer) {
  let ready = writer[kState].ready;
  if (ready === undefined) {
    const stream = writer[kState].stream;
    if (stream === undefined) {
      ready = rejectedHandledRecord(lazyWritableReleasedError());
    } else {
      switch (stream[kState].state) {
        case 'writable':
          ready = !writableStreamCloseQueuedOrInFlight(stream) &&
                  stream[kState].backpressure ?
            PromiseWithResolvers() :
            resolvedRecord();
          break;
        case 'closed':
          ready = resolvedRecord();
          break;
        default:
          ready = rejectedHandledRecord(stream[kState].storedError);
      }
    }
    writer[kState].ready = ready;
  }
  return ready;
}

function writableStreamAbort(stream, reason) {
  const { controller } = stream[kState];

  let state = stream[kState].state;
  if (state === 'closed' || state === 'errored')
    return PromiseResolve();

  controller[kState].abortController.abort(reason);

  state = stream[kState].state;
  if (state === 'closed' || state === 'errored')
    return PromiseResolve();

  if (stream[kState].pendingAbortRequest.abort.promise !== undefined)
    return stream[kState].pendingAbortRequest.abort.promise;

  assert(state === 'writable' || state === 'erroring');

  let wasAlreadyErroring = false;
  if (state === 'erroring') {
    wasAlreadyErroring = true;
    reason = undefined;
  }

  const abort = PromiseWithResolvers();

  stream[kState].pendingAbortRequest = {
    abort,
    reason,
    wasAlreadyErroring,
  };

  if (!wasAlreadyErroring)
    writableStreamStartErroring(stream, reason);

  return abort.promise;
}

function writableStreamClose(stream) {
  const {
    state,
    writer,
    backpressure,
    controller,
  } = stream[kState];
  if (state === 'closed' || state === 'errored') {
    return PromiseReject(
      new ERR_INVALID_STATE.TypeError('WritableStream is closed'));
  }
  assert(state === 'writable' || state === 'erroring');
  assert(!writableStreamCloseQueuedOrInFlight(stream));
  stream[kState].closeRequest = PromiseWithResolvers();
  stream[kState].closeQueuedOrInFlight = true;
  const { promise } = stream[kState].closeRequest;
  if (writer !== undefined && backpressure && state === 'writable')
    writer[kState].ready?.resolve();
  writableStreamDefaultControllerClose(controller);
  return promise;
}

// Callers invoke this only in the 'writable' state with no close queued or
// in flight (the two conditions the spec's assertions check), so the
// backpressure value is derived here from the desired size instead of being
// computed and passed in by each caller.
function writableStreamUpdateBackpressure(controller, streamState) {
  const controllerState = controller[kState];
  const backpressure =
    controllerState.highWaterMark - controllerState.queueTotalSize <= 0;
  const writer = streamState.writer;
  const changed = streamState.backpressure !== backpressure;
  // The state field is published before the ready record is resolved so
  // that a ready resolve hook (pipeTo's pump continuation) observes the
  // new value.
  streamState.backpressure = backpressure;
  if (writer !== undefined && changed) {
    if (backpressure) {
      // The spec replaces [[readyPromise]] with a fresh pending promise;
      // dropping the cache lets the next observation derive it.
      writer[kState].ready = undefined;
    } else {
      writer[kState].ready?.resolve();
    }
  }
}

function writableStreamStartErroring(stream, reason) {
  assert(stream[kState].storedError === undefined);
  assert(stream[kState].state === 'writable');
  const {
    controller,
    writer,
  } = stream[kState];
  assert(controller !== undefined);
  stream[kState].state = 'erroring';
  stream[kState].storedError = reason;
  if (writer !== undefined) {
    writableStreamDefaultWriterEnsureReadyPromiseRejected(writer, reason);
  }
  if (!writableStreamHasOperationMarkedInFlight(stream) &&
      controller[kState].started) {
    writableStreamFinishErroring(stream);
  }
}

function writableStreamRejectCloseAndClosedPromiseIfNeeded(stream) {
  assert(stream[kState].state === 'errored');
  if (stream[kState].closeRequest.promise !== undefined) {
    assert(stream[kState].inFlightCloseRequest.promise === undefined);
    stream[kState].closeRequest.reject(stream[kState].storedError);
    stream[kState].closeRequest = kNilRequest;
    stream[kState].closeQueuedOrInFlight = false;
  }

  const closedPromiseCache = stream[kState].closedPromise;
  if (closedPromiseCache !== undefined && isPromisePending(closedPromiseCache.promise)) {
    setPromiseHandled(closedPromiseCache.promise);
    closedPromiseCache.reject(stream[kState].storedError);
  }

  const {
    writer,
  } = stream[kState];
  if (writer !== undefined) {
    const closeCache = writer[kState].close;
    if (closeCache !== undefined && isPromisePending(closeCache.promise)) {
      setPromiseHandled(closeCache.promise);
      closeCache.reject(stream[kState].storedError);
    }
  }
}

function writableStreamMarkFirstWriteRequestInFlight(stream) {
  assert(stream[kState].inFlightWriteRequest.promise === undefined);
  assert(stream[kState].writeRequests.length);
  const writeRequest = stream[kState].writeRequests.shift();
  stream[kState].inFlightWriteRequest = writeRequest;
}

function writableStreamMarkCloseRequestInFlight(stream) {
  assert(stream[kState].inFlightWriteRequest.promise === undefined);
  assert(stream[kState].closeRequest.promise !== undefined);
  stream[kState].inFlightCloseRequest = stream[kState].closeRequest;
  stream[kState].closeRequest = kNilRequest;
}

function writableStreamHasOperationMarkedInFlight(stream) {
  const {
    inFlightWriteRequest,
    inFlightCloseRequest,
  } = stream[kState];
  if (inFlightWriteRequest.promise === undefined &&
      inFlightCloseRequest.promise === undefined) {
    return false;
  }
  return true;
}

function writableStreamFinishInFlightWriteWithError(stream, error) {
  assert(stream[kState].inFlightWriteRequest.promise !== undefined);
  stream[kState].inFlightWriteRequest.reject(error);
  stream[kState].inFlightWriteRequest = kNilRequest;
  assert(stream[kState].state === 'writable' ||
         stream[kState].state === 'erroring');
  writableStreamDealWithRejection(stream, error);
}

function writableStreamFinishInFlightWrite(stream) {
  assert(stream[kState].inFlightWriteRequest.promise !== undefined);
  stream[kState].inFlightWriteRequest.resolve();
  stream[kState].inFlightWriteRequest = kNilRequest;
}

function writableStreamFinishInFlightCloseWithError(stream, error) {
  assert(stream[kState].inFlightCloseRequest.promise !== undefined);
  stream[kState].inFlightCloseRequest.reject(error);
  stream[kState].inFlightCloseRequest = kNilRequest;
  stream[kState].closeQueuedOrInFlight = false;
  assert(stream[kState].state === 'writable' ||
         stream[kState].state === 'erroring');
  if (stream[kState].pendingAbortRequest.abort.promise !== undefined) {
    stream[kState].pendingAbortRequest.abort.reject(error);
    stream[kState].pendingAbortRequest = kNilPendingAbortRequest;
  }
  writableStreamDealWithRejection(stream, error);
}

function writableStreamFinishInFlightClose(stream) {
  assert(stream[kState].inFlightCloseRequest.promise !== undefined);
  stream[kState].inFlightCloseRequest.resolve();
  stream[kState].inFlightCloseRequest = kNilRequest;
  stream[kState].closeQueuedOrInFlight = false;
  if (stream[kState].state === 'erroring') {
    stream[kState].storedError = undefined;
    if (stream[kState].pendingAbortRequest.abort.promise !== undefined) {
      stream[kState].pendingAbortRequest.abort.resolve();
      stream[kState].pendingAbortRequest = kNilPendingAbortRequest;
    }
  }
  stream[kState].state = 'closed';
  if (stream[kState].writer !== undefined)
    stream[kState].writer[kState].close?.resolve();
  stream[kState].closedPromise?.resolve();
  assert(stream[kState].pendingAbortRequest.abort.promise === undefined);
  assert(stream[kState].storedError === undefined);
}

function writableStreamFinishErroring(stream) {
  assert(stream[kState].state === 'erroring');
  assert(!writableStreamHasOperationMarkedInFlight(stream));
  stream[kState].state = 'errored';
  stream[kState].controller[kError]();
  const storedError = stream[kState].storedError;
  const writeRequests = stream[kState].writeRequests;
  while (writeRequests.length)
    writeRequests.shift().reject(storedError);

  if (stream[kState].pendingAbortRequest.abort.promise === undefined) {
    writableStreamRejectCloseAndClosedPromiseIfNeeded(stream);
    return;
  }

  const abortRequest = stream[kState].pendingAbortRequest;
  stream[kState].pendingAbortRequest = kNilPendingAbortRequest;
  if (abortRequest.wasAlreadyErroring) {
    abortRequest.abort.reject(storedError);
    writableStreamRejectCloseAndClosedPromiseIfNeeded(stream);
    return;
  }
  PromisePrototypeThen(
    stream[kState].controller[kAbort](abortRequest.reason),
    () => {
      abortRequest.abort.resolve();
      writableStreamRejectCloseAndClosedPromiseIfNeeded(stream);
    },
    (error) => {
      abortRequest.abort.reject(error);
      writableStreamRejectCloseAndClosedPromiseIfNeeded(stream);
    });
}

function writableStreamDealWithRejection(stream, error) {
  const {
    state,
  } = stream[kState];
  if (state === 'writable') {
    writableStreamStartErroring(stream, error);
    return;
  }

  assert(state === 'erroring');
  writableStreamFinishErroring(stream);
}

function writableStreamCloseQueuedOrInFlight(stream) {
  return stream[kState].closeQueuedOrInFlight;
}

function writableStreamAddWriteRequest(stream) {
  assert(isWritableStreamLocked(stream));
  assert(stream[kState].state === 'writable');
  // PromiseWithResolvers() already returns a { promise, resolve, reject }
  // record, so push it as-is instead of rebuilding an identical object.
  const writeRequest = PromiseWithResolvers();
  const streamState = stream[kState];
  let writeRequests = streamState.writeRequests;
  if (writeRequests === kEmptyQueue)
    writeRequests = streamState.writeRequests = new Queue();
  writeRequests.push(writeRequest);
  return writeRequest.promise;
}

function writableStreamDefaultWriterWrite(writer, chunk) {
  const writerState = writer[kState];
  const stream = writerState.stream;
  assert(stream !== undefined);
  const streamState = stream[kState];
  const {
    controller,
  } = streamState;
  const chunkSize = writableStreamDefaultControllerGetChunkSize(
    controller,
    chunk);
  if (stream !== writerState.stream) {
    return PromiseReject(
      new ERR_INVALID_STATE.TypeError('Mismatched WritableStreams'));
  }
  const {
    state,
  } = streamState;

  if (state === 'errored')
    return PromiseReject(streamState.storedError);

  if (streamState.closeQueuedOrInFlight || state === 'closed') {
    return PromiseReject(
      new ERR_INVALID_STATE.TypeError('WritableStream is closed'));
  }

  if (state === 'erroring')
    return PromiseReject(streamState.storedError);

  assert(state === 'writable');

  const promise = writableStreamAddWriteRequest(stream);
  writableStreamDefaultControllerWrite(controller, chunk, chunkSize);
  return promise;
}

// Variant of writableStreamDefaultWriterWrite for pipeTo: the caller
// provides a shared request object instead of a per-write promise record.
// `pending` is incremented before the controller write, which can settle
// requests synchronously when the stream starts erroring; precondition
// failures are latched on `failed`/`failure`.
function writableStreamDefaultWriterWriteWithRequest(writer, chunk, request) {
  const writerState = writer[kState];
  const stream = writerState.stream;
  assert(stream !== undefined);
  const streamState = stream[kState];
  const {
    controller,
  } = streamState;
  const chunkSize = writableStreamDefaultControllerGetChunkSize(
    controller,
    chunk);
  if (stream !== writerState.stream) {
    request.failed = true;
    request.failure =
      new ERR_INVALID_STATE.TypeError('Mismatched WritableStreams');
    return;
  }
  const {
    state,
  } = streamState;

  if (state === 'errored') {
    request.failed = true;
    request.failure = streamState.storedError;
    return;
  }

  if (streamState.closeQueuedOrInFlight || state === 'closed') {
    request.failed = true;
    request.failure =
      new ERR_INVALID_STATE.TypeError('WritableStream is closed');
    return;
  }

  if (state === 'erroring') {
    request.failed = true;
    request.failure = streamState.storedError;
    return;
  }

  assert(state === 'writable');

  let writeRequests = streamState.writeRequests;
  if (writeRequests === kEmptyQueue)
    writeRequests = streamState.writeRequests = new Queue();
  writeRequests.push(request);
  request.pending++;
  writableStreamDefaultControllerWrite(controller, chunk, chunkSize);
}

function writableStreamDefaultWriterRelease(writer) {
  const {
    stream,
  } = writer[kState];
  assert(stream !== undefined);
  assert(stream[kState].writer === writer);
  const releasedStateError = lazyWritableReleasedError();
  writableStreamDefaultWriterEnsureReadyPromiseRejected(writer, releasedStateError);
  writableStreamDefaultWriterEnsureClosedPromiseRejected(writer, releasedStateError);
  stream[kState].writer = undefined;
  writer[kState].stream = undefined;
}

function writableStreamDefaultWriterGetDesiredSize(writer) {
  const {
    stream,
  } = writer[kState];
  switch (stream[kState].state) {
    case 'errored':
      // Fall through
    case 'erroring':
      return null;
    case 'closed':
      return 0;
  }
  return writableStreamDefaultControllerGetDesiredSize(
    stream[kState].controller);
}

function writableStreamDefaultWriterEnsureReadyPromiseRejected(writer, error) {
  const ready = writer[kState].ready;
  if (ready !== undefined && isPromisePending(ready.promise)) {
    ready.reject(error);
    setPromiseHandled(ready.promise);
  } else {
    // The spec replaces [[readyPromise]] with a promise rejected with the
    // same error the post-release/post-erroring state derives; dropping
    // the cache lets the next observation materialize it.
    writer[kState].ready = undefined;
  }
}

function writableStreamDefaultWriterEnsureClosedPromiseRejected(writer, error) {
  const close = writer[kState].close;
  if (close !== undefined && isPromisePending(close.promise)) {
    close.reject(error);
    setPromiseHandled(close.promise);
  } else {
    // See writableStreamDefaultWriterEnsureReadyPromiseRejected.
    writer[kState].close = undefined;
  }
}

function writableStreamDefaultWriterCloseWithErrorPropagation(writer) {
  const {
    stream,
  } = writer[kState];
  assert(stream !== undefined);
  const {
    state,
  } = stream[kState];
  if (writableStreamCloseQueuedOrInFlight(stream) || state === 'closed')
    return PromiseResolve();

  if (state === 'errored')
    return PromiseReject(stream[kState].storedError);

  assert(state === 'writable' || state === 'erroring');

  return writableStreamDefaultWriterClose(writer);
}

function writableStreamDefaultWriterClose(writer) {
  const {
    stream,
  } = writer[kState];
  assert(stream !== undefined);
  return writableStreamClose(stream);
}

function writableStreamDefaultWriterAbort(writer, reason) {
  const {
    stream,
  } = writer[kState];
  assert(stream !== undefined);
  return writableStreamAbort(stream, reason);
}

function writableStreamDefaultControllerWrite(controller, chunk, chunkSize) {
  try {
    enqueueValueWithSize(controller, chunk, chunkSize);
  } catch (error) {
    writableStreamDefaultControllerErrorIfNeeded(controller, error);
    return;
  }
  const {
    stream,
  } = controller[kState];
  const streamState = stream[kState];
  if (!streamState.closeQueuedOrInFlight &&
      streamState.state === 'writable') {
    writableStreamUpdateBackpressure(controller, streamState);
  }
  writableStreamDefaultControllerAdvanceQueueIfNeeded(controller);
}

function writableStreamDefaultControllerProcessWrite(controller, chunk) {
  const {
    stream,
    writeAlgorithm,
  } = controller[kState];
  writableStreamMarkFirstWriteRequestInFlight(stream);

  if (controller[kState].writeFulfilled === undefined) {
    // The write reaction closures only capture the stream and controller,
    // so they are created once on the first write and reused for every
    // subsequent write instead of allocating two fresh closures per chunk.
    controller[kState].writeFulfilled = () => {
      writableStreamFinishInFlightWrite(stream);
      const streamState = stream[kState];
      const {
        state,
      } = streamState;
      assert(state === 'writable' || state === 'erroring');
      dequeueValue(controller);
      if (!streamState.closeQueuedOrInFlight &&
          state === 'writable') {
        writableStreamUpdateBackpressure(controller, streamState);
      }
      writableStreamDefaultControllerAdvanceQueueIfNeeded(controller);
    };
    controller[kState].writeRejected = (error) => {
      if (stream[kState].state === 'writable')
        writableStreamDefaultControllerClearAlgorithms(controller);
      writableStreamFinishInFlightWriteWithError(stream, error);
    };
  }

  // The write algorithm may be a raw callback (a wrapped user sink.write
  // returns its result uncoerced; a synchronous throw surfaces here) or an
  // internal algorithm that always returns a promise; thenAlgorithmResult
  // handles both.
  let result;
  try {
    result = writeAlgorithm(chunk, controller);
  } catch (error) {
    result = PromiseReject(error);
  }
  thenAlgorithmResult(
    result,
    controller[kState].writeFulfilled,
    controller[kState].writeRejected);
}

function writableStreamDefaultControllerProcessClose(controller) {
  const {
    closeAlgorithm,
    queue,
    stream,
  } = controller[kState];
  writableStreamMarkCloseRequestInFlight(stream);
  dequeueValue(controller);
  assert(!queue.length);
  const sinkClosePromise = closeAlgorithm();
  writableStreamDefaultControllerClearAlgorithms(controller);
  PromisePrototypeThen(
    sinkClosePromise,
    () => writableStreamFinishInFlightClose(stream),
    (error) => writableStreamFinishInFlightCloseWithError(stream, error));
}

function writableStreamDefaultControllerGetDesiredSize(controller) {
  const {
    highWaterMark,
    queueTotalSize,
  } = controller[kState];
  return highWaterMark - queueTotalSize;
}
