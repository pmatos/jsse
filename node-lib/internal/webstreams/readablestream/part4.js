
function readableStreamFromIterable(iterable) {
  let stream;
  const iteratorGetter = iterable[SymbolAsyncIterator] ?? iterable[SymbolIterator];
  if (iteratorGetter == null || typeof iteratorGetter !== 'function') {
    throw new ERR_ARG_NOT_ITERABLE(iterable);
  }
  const iterator = FunctionPrototypeCall(iteratorGetter, iterable);
  if (iterator === null || (typeof iterator !== 'object' && typeof iterator !== 'function')) {
    throw new ERR_INVALID_STATE.TypeError('The iterator method must return an object');
  }
  // Per GetIteratorDirect, the next method is looked up once.
  const nextMethod = iterator.next;
  const startAlgorithm = nonOpCallback;

  // Callback-style pull: the reaction steps are reused across chunks and
  // completion is delivered to the controller's cached pull reactions
  // (the kParkedAlgorithmResult contract). One pull runs at a time, so a
  // single slot carries a non-thenable next() result between steps.
  let pendingIterResult;

  function rejectPull(error) {
    readableStreamDefaultControllerError(stream[kState].controller, error);
  }

  function processIterResult(iterResult) {
    const controller = stream[kState].controller;
    if (typeof iterResult !== 'object' || iterResult === null) {
      rejectPull(new ERR_INVALID_STATE.TypeError(
        'The promise returned by the iterator.next() method must fulfill with an object'));
      return;
    }
    try {
      if (iterResult.done) {
        readableStreamDefaultControllerClose(controller);
      } else {
        const value = iterResult.value;
        if (value !== null &&
            (typeof value === 'object' || typeof value === 'function')) {
          // Adopted like `await iterResult.value`, keeping the observable
          // .then lookup on plain objects.
          PromisePrototypeThen(PromiseResolve(value), enqueueValue, rejectPull);
          return;
        }
        readableStreamDefaultControllerEnqueue(controller, value);
      }
    } catch (error) {
      rejectPull(error);
      return;
    }
    // pullFulfilled exists: the controller creates it before the pull.
    controller[kState].pullFulfilled();
  }

  function enqueueValue(value) {
    const controller = stream[kState].controller;
    try {
      readableStreamDefaultControllerEnqueue(controller, value);
    } catch (error) {
      rejectPull(error);
      return;
    }
    controller[kState].pullFulfilled();
  }

  function processPendingIterResult() {
    const iterResult = pendingIterResult;
    pendingIterResult = undefined;
    processIterResult(iterResult);
  }

  function pullAlgorithm() {
    let nextResult;
    try {
      nextResult = FunctionPrototypeCall(nextMethod, iterator);
    } catch (error) {
      return PromiseReject(error);
    }
    if (nextResult !== null &&
        (typeof nextResult === 'object' || typeof nextResult === 'function')) {
      // Mirrors `await iterator.next()`: processIterResult runs at the
      // microtask position the await resumed.
      PromisePrototypeThen(
        PromiseResolve(nextResult), processIterResult, rejectPull);
      return kParkedAlgorithmResult;
    }
    // A non-thenable next() result fails validation a microtask later.
    pendingIterResult = nextResult;
    PromisePrototypeThen(kResolvedPromise, processPendingIterResult);
    return kParkedAlgorithmResult;
  }

  async function cancelAlgorithm(reason) {
    const returnMethod = iterator.return;
    if (returnMethod === undefined) {
      return;
    }
    const iterResult = await FunctionPrototypeCall(returnMethod, iterator, reason);
    if (typeof iterResult !== 'object' || iterResult === null) {
      throw new ERR_INVALID_STATE.TypeError(
        'The promise returned by the iterator.return() method must fulfill with an object');
    }
  }

  stream = createReadableStream(
    startAlgorithm,
    pullAlgorithm,
    cancelAlgorithm,
    0,
  );

  return stream;
}

function readableStreamPipeTo(
  source,
  dest,
  preventClose,
  preventAbort,
  preventCancel,
  signal) {

  let reader;
  let writer;
  let disposable;
  // Both of these can throw synchronously. We want to capture
  // the error and return a rejected promise instead.
  try {
    reader = new ReadableStreamDefaultReader(source);
    writer = new WritableStreamDefaultWriter(dest);
  } catch (error) {
    return PromiseReject(error);
  }

  source[kState].disturbed = true;

  let shuttingDown = false;

  if (signal !== undefined) {
    try {
      validateAbortSignal(signal, 'options.signal');
    } catch (error) {
      return PromiseReject(error);
    }
  }

  const promise = PromiseWithResolvers();

  // One shared write request tracks every chunk written to the
  // destination, instead of a { promise, resolve, reject } record per
  // write. `stall` is armed by waitForPendingWrites() during shutdown;
  // `failed`/`failure` latch a write that could not proceed.
  const writeTracker = {
    // Non-undefined: queue entries are discriminated from kNilRequest
    // by `promise === undefined`.
    promise: null,
    pending: 0,
    failed: false,
    failure: undefined,
    stall: null,
    resolve() {
      if (--this.pending === 0 && this.stall !== null) {
        const stall = this.stall;
        this.stall = null;
        if (this.failed)
          stall.reject(this.failure);
        else
          stall.resolve();
      }
    },
    reject(error) {
      this.pending--;
      this.failed = true;
      this.failure = error;
      if (this.stall !== null) {
        const stall = this.stall;
        this.stall = null;
        stall.reject(error);
      }
    },
  };

  // The error here can be undefined. The rejected arg
  // tells us that the promise must be rejected even
  // when error is undefine.
  function finalize(rejected, error) {
    writableStreamDefaultWriterRelease(writer);
    readableStreamReaderGenericRelease(reader);
    if (signal !== undefined)
      disposable?.[SymbolDispose]();
    if (rejected)
      promise.reject(error);
    else
      promise.resolve();
  }

  function waitForPendingWrites() {
    if (writeTracker.pending === 0) {
      return writeTracker.failed ?
        PromiseReject(writeTracker.failure) :
        PromiseResolve();
    }
    writeTracker.stall = PromiseWithResolvers();
    return writeTracker.stall.promise;
  }

  function shutdownWithAnAction(action, rejected, originalError) {
    if (shuttingDown) return;
    shuttingDown = true;
    if (dest[kState].state === 'writable' &&
        !writableStreamCloseQueuedOrInFlight(dest)) {
      PromisePrototypeThen(
        waitForPendingWrites(),
        complete,
        (error) => finalize(true, error));
      return;
    }
    complete();

    function complete() {
      PromisePrototypeThen(
        action(),
        () => finalize(rejected, originalError),
        (error) => finalize(true, error));
    }
  }

  function shutdown(rejected, error) {
    if (shuttingDown) return;
    shuttingDown = true;
    if (dest[kState].state === 'writable' &&
        !writableStreamCloseQueuedOrInFlight(dest)) {
      PromisePrototypeThen(
        waitForPendingWrites(),
        () => finalize(rejected, error),
        (error) => finalize(true, error));
      return;
    }
    finalize(rejected, error);
  }

  function abortAlgorithm() {
    let error;
    if (signal.reason instanceof AbortError) {
      // Cannot use the AbortError class here. It must be a DOMException.
      error = new DOMException(signal.reason.message, 'AbortError');
    } else {
      error = signal.reason;
    }

    const actions = [];
    if (!preventAbort) {
      ArrayPrototypePush(
        actions,
        () => {
          if (dest[kState].state === 'writable')
            return writableStreamAbort(dest, error);
          return PromiseResolve();
        });
    }
    if (!preventCancel) {
      ArrayPrototypePush(
        actions,
        () => {
          if (source[kState].state === 'readable')
            return readableStreamCancel(source, error);
          return PromiseResolve();
        });
    }

    shutdownWithAnAction(
      () => SafePromiseAll(actions, (action) => action()),
      true,
      error);
  }

  function watchErrored(stream, promise, action) {
    if (stream[kState].state === 'errored')
      action(stream[kState].storedError);
    else
      PromisePrototypeThen(promise, undefined, action);
  }

  function watchClosed(stream, promise, action) {
    if (stream[kState].state === 'closed')
      action();
    else
      PromisePrototypeThen(promise, action, () => {});
  }

  // The pump loop is callback-driven to avoid per-iteration promise
  // allocations. At most one read is in flight at a time, so one read
  // request and one forwarding function are reused for every chunk;
  // the chunk travels through `pendingChunk`.
  let pendingChunk;
  let readRequest;
  let readyHook;

  // Ready promise rejection is handled by the destination-errored
  // watcher.
  function ignoreReadyRejection() {}

  // Parks the pump on the destination's backpressure by installing a
  // record that duck-types the writer's lazily-materialized
  // [[readyPromise]] record: writableStreamUpdateBackpressure resolves it
  // when backpressure clears (after publishing the new backpressure
  // state), which re-enters the pump directly instead of rotating a
  // fresh promise record plus reaction per flip. The pipe holds the only
  // reference to the writer, so the record is never observable as a real
  // ready promise; the erroring/release paths probe `promise` via
  // isPromisePending() and call `reject`, so it carries a real
  // forever-pending promise and a no-op reject.
  function parkOnReady() {
    readyHook ??= {
      promise: new Promise(nonOpCallback),
      resolve: pump,
      reject: ignoreReadyRejection,
    };
    writer[kState].ready = readyHook;
  }

  function forwardChunk() {
    const chunk = pendingChunk;
    pendingChunk = undefined;
    writableStreamDefaultWriterWriteWithRequest(writer, chunk, writeTracker);
    pump();
  }

  function pump() {
    if (shuttingDown) return;

    if (dest[kState].backpressure) {
      parkOnReady();
      return;
    }

    const controller = source[kState].controller;

    // Fast path: batch reads when data is buffered in a default controller.
    // This avoids parking read requests and reduces promise allocation
    // overhead.
    if (source[kState].state === 'readable' &&
        isReadableStreamDefaultController(controller) &&
        controller[kState].queue.length > 0) {

      while (controller[kState].queue.length > 0) {
        if (shuttingDown) return;

        const chunk = dequeueValue(controller);

        if (controller[kState].closeRequested && !controller[kState].queue.length) {
          readableStreamDefaultControllerClearAlgorithms(controller);
          readableStreamClose(source);
        }

        // Write the chunk - we're already in a separate microtask from enqueue
        // because we awaited the writer ready promise above.
        writableStreamDefaultWriterWriteWithRequest(writer, chunk, writeTracker);

        // Check backpressure after each write
        if (dest[kState].backpressure) {
          // Backpressure - stop batch and wait for ready
          break;
        } else if (dest[kState].state !== 'writable' || writableStreamCloseQueuedOrInFlight(dest)) {
          // Closing or erroring - stop batch and wait for shutdown
          break;
        }
      }

      // Trigger pull if needed after batch
      readableStreamDefaultControllerCallPullIfNeeded(controller);

      // Check if stream closed during batch
      if (source[kState].state === 'closed') {
        return;
      }

      // Park on backpressure directly: the ready hook resumes the pump
      // when a completed write clears it.
      if (dest[kState].backpressure) {
        parkOnReady();
        return;
      }

      // Yield to the microtask queue between batches so completed-write
      // reactions and events/signals fire; a shared resolved promise
      // enqueues the continuation at the same position as queueMicrotask
      // without the per-batch scheduling overhead.
      PromisePrototypeThen(kResolvedPromise, pump);
      return;
    }

    // Slow path: park a lazily materialized read request. Close and
    // error are handled by the source watchers.
    readRequest ??= {
      [kChunk](chunk) {
        // Per spec, pipeTo must queue a microtask for the write to avoid
        // synchronous write during enqueue(). See WHATWG Streams spec
        // "ReadableStreamPipeTo" step 15's "chunk steps".
        pendingChunk = chunk;
        PromisePrototypeThen(kResolvedPromise, forwardChunk);
      },
      [kClose]() {},
      [kError]() {},
    };
    readableStreamDefaultReaderRead(reader, readRequest);
  }

  if (signal !== undefined) {
    if (signal.aborted) {
      abortAlgorithm();
      return promise.promise;
    }
    addAbortListener ??= require('internal/events/abort_listener').addAbortListener;
    disposable = addAbortListener(signal, abortAlgorithm);
  }

  pump();

  watchErrored(source, readerClosedPromise(reader).promise, (error) => {
    if (!preventAbort) {
      return shutdownWithAnAction(
        () => writableStreamAbort(dest, error),
        true,
        error);
    }
    shutdown(true, error);
  });

  watchErrored(dest, writerClosedPromise(writer).promise, (error) => {
    if (!preventCancel) {
      return shutdownWithAnAction(
        () => readableStreamCancel(source, error),
        true,
        error);
    }
    shutdown(true, error);
  });

  watchClosed(source, readerClosedPromise(reader).promise, () => {
    if (!preventClose) {
      return shutdownWithAnAction(
        () => writableStreamDefaultWriterCloseWithErrorPropagation(writer));
    }
    shutdown();
  });

  if (writableStreamCloseQueuedOrInFlight(dest) ||
      dest[kState].state === 'closed') {
    const error = new ERR_INVALID_STATE.TypeError(
      'Destination WritableStream is closed');
    if (!preventCancel) {
      shutdownWithAnAction(
        () => readableStreamCancel(source, error), true, error);
    } else {
      shutdown(true, error);
    }
  }

  return promise.promise;
}

function readableStreamTee(stream, cloneForBranch2) {
  if (isReadableByteStreamController(stream[kState].controller)) {
    return readableByteStreamTee(stream);
  }
  return readableStreamDefaultTee(stream, cloneForBranch2);
}

function readableStreamDefaultTee(stream, cloneForBranch2) {
  const reader = new ReadableStreamDefaultReader(stream);
  let reading = false;
  let canceled1 = false;
  let canceled2 = false;
  let reason1;
  let reason2;
  let branch1;
  let branch2;
  const cancelPromise = PromiseWithResolvers();

  // At most one read is ever in flight (`reading` guards pullAlgorithm),
  // so one read request object and one forwarding microtask function are
  // reused for every chunk; the chunk travels through `pendingChunk`.
  // The request is materialized lazily on the first pull so that tee()
  // itself stays allocation-light.
  let pendingChunk;
  let readRequest;
  function forwardChunk() {
    reading = false;
    const value1 = pendingChunk;
    let value2 = pendingChunk;
    pendingChunk = undefined;
    if (!canceled2 && cloneForBranch2) {
      value2 = structuredClone(value2);
    }
    if (!canceled1) {
      readableStreamDefaultControllerEnqueue(
        branch1[kState].controller,
        value1);
    }
    if (!canceled2) {
      readableStreamDefaultControllerEnqueue(
        branch2[kState].controller,
        value2);
    }
  }

  async function pullAlgorithm() {
    if (reading) return;
    reading = true;
    readRequest ??= {
      [kChunk](value) {
        // The microtask is required by the spec (ReadableStreamTee's
        // "chunk steps" queue one).
        pendingChunk = value;
        PromisePrototypeThen(kResolvedPromise, forwardChunk);
      },
      [kClose]() {
        // The `process.nextTick()` is not part of the spec.
        // This approach was needed to avoid a race condition working with esm
        // Further information, see: https://github.com/nodejs/node/issues/39758
        process.nextTick(() => {
          reading = false;
          if (!canceled1)
            readableStreamDefaultControllerClose(branch1[kState].controller);
          if (!canceled2)
            readableStreamDefaultControllerClose(branch2[kState].controller);
          if (!canceled1 || !canceled2)
            cancelPromise.resolve();
        });
      },
      [kError]() {
        reading = false;
      },
    };
    readableStreamDefaultReaderRead(reader, readRequest);
  }

  function cancel1Algorithm(reason) {
    canceled1 = true;
    reason1 = reason;
    if (canceled2) {
      const compositeReason = [reason1, reason2];
      cancelPromise.resolve(readableStreamCancel(stream, compositeReason));
    }
    return cancelPromise.promise;
  }

  function cancel2Algorithm(reason) {
    canceled2 = true;
    reason2 = reason;
    if (canceled1) {
      const compositeReason = [reason1, reason2];
      cancelPromise.resolve(readableStreamCancel(stream, compositeReason));
    }
    return cancelPromise.promise;
  }

  branch1 =
    createReadableStream(nonOpCallback, pullAlgorithm, cancel1Algorithm);
  branch2 =
    createReadableStream(nonOpCallback, pullAlgorithm, cancel2Algorithm);

  PromisePrototypeThen(
    readerClosedPromise(reader).promise,
    undefined,
    (error) => {
      readableStreamDefaultControllerError(branch1[kState].controller, error);
      readableStreamDefaultControllerError(branch2[kState].controller, error);
      if (!canceled1 || !canceled2)
        cancelPromise.resolve();
    });

  return [branch1, branch2];
}
