
function readableByteStreamTee(stream) {
  assert(isReadableStream(stream));
  assert(isReadableByteStreamController(stream[kState].controller));

  let reader = new ReadableStreamDefaultReader(stream);
  let reading = false;
  let readAgainForBranch1 = false;
  let readAgainForBranch2 = false;
  let canceled1 = false;
  let canceled2 = false;
  let reason1;
  let reason2;
  let branch1;
  let branch2;
  const cancelDeferred = PromiseWithResolvers();

  function forwardReaderError(thisReader) {
    PromisePrototypeThen(
      readerClosedPromise(thisReader).promise,
      undefined,
      (error) => {
        if (thisReader !== reader) {
          return;
        }
        readableStreamDefaultControllerError(branch1[kState].controller, error);
        readableStreamDefaultControllerError(branch2[kState].controller, error);
        if (!canceled1 || !canceled2) {
          cancelDeferred.resolve();
        }
      },
    );
  }

  // As in readableStreamDefaultTee, only one read is ever in flight, so
  // the default-reader read request and its forwarding microtask are
  // shared across all chunks.
  let pendingChunk;
  function forwardChunk() {
    readAgainForBranch1 = false;
    readAgainForBranch2 = false;
    const chunk1 = pendingChunk;
    let chunk2 = pendingChunk;
    pendingChunk = undefined;

    if (!canceled1 && !canceled2) {
      try {
        chunk2 = cloneAsUint8Array(chunk1);
      } catch (error) {
        readableByteStreamControllerError(
          branch1[kState].controller,
          error,
        );
        readableByteStreamControllerError(
          branch2[kState].controller,
          error,
        );
        cancelDeferred.resolve(readableStreamCancel(stream, error));
        return;
      }
    }
    if (!canceled1) {
      readableByteStreamControllerEnqueue(
        branch1[kState].controller,
        chunk1,
      );
    }
    if (!canceled2) {
      readableByteStreamControllerEnqueue(
        branch2[kState].controller,
        chunk2,
      );
    }
    reading = false;

    if (readAgainForBranch1) {
      pull1Algorithm();
    } else if (readAgainForBranch2) {
      pull2Algorithm();
    }
  }

  let defaultReadRequest;
  function pullWithDefaultReader() {
    if (isReadableStreamBYOBReader(reader)) {
      readableStreamBYOBReaderRelease(reader);
      reader = new ReadableStreamDefaultReader(stream);
      forwardReaderError(reader);
    }

    defaultReadRequest ??= {
      [kChunk](chunk) {
        pendingChunk = chunk;
        PromisePrototypeThen(kResolvedPromise, forwardChunk);
      },
      [kClose]() {
        reading = false;

        if (!canceled1) {
          readableByteStreamControllerClose(branch1[kState].controller);
        }
        if (!canceled2) {
          readableByteStreamControllerClose(branch2[kState].controller);
        }
        if (branch1[kState].controller[kState].pendingPullIntos.length > 0) {
          readableByteStreamControllerRespond(branch1[kState].controller, 0);
        }
        if (branch2[kState].controller[kState].pendingPullIntos.length > 0) {
          readableByteStreamControllerRespond(branch2[kState].controller, 0);
        }
        if (!canceled1 || !canceled2) {
          cancelDeferred.resolve();
        }
      },
      [kError]() {
        reading = false;
      },
    };
    readableStreamDefaultReaderRead(reader, defaultReadRequest);
  }

  function pullWithBYOBReader(view, forBranch2) {
    if (isReadableStreamDefaultReader(reader)) {
      readableStreamDefaultReaderRelease(reader);
      reader = new ReadableStreamBYOBReader(stream);
      forwardReaderError(reader);
    }

    const byobBranch = forBranch2 === true ? branch2 : branch1;
    const otherBranch = forBranch2 === false ? branch2 : branch1;
    const readIntoRequest = {
      [kChunk](chunk) {
        queueMicrotask(() => {
          readAgainForBranch1 = false;
          readAgainForBranch2 = false;
          const byobCanceled = forBranch2 === true ? canceled2 : canceled1;
          const otherCanceled = forBranch2 === false ? canceled2 : canceled1;

          if (!otherCanceled) {
            let clonedChunk;

            try {
              clonedChunk = cloneAsUint8Array(chunk);
            } catch (error) {
              readableByteStreamControllerError(
                byobBranch[kState].controller,
                error,
              );
              readableByteStreamControllerError(
                otherBranch[kState].controller,
                error,
              );
              cancelDeferred.resolve(readableStreamCancel(stream, error));
              return;
            }
            if (!byobCanceled) {
              readableByteStreamControllerRespondWithNewView(
                byobBranch[kState].controller,
                chunk,
              );
            }

            readableByteStreamControllerEnqueue(
              otherBranch[kState].controller,
              clonedChunk,
            );
          } else if (!byobCanceled) {
            readableByteStreamControllerRespondWithNewView(
              byobBranch[kState].controller,
              chunk,
            );
          }
          reading = false;

          if (readAgainForBranch1) {
            pull1Algorithm();
          } else if (readAgainForBranch2) {
            pull2Algorithm();
          }
        });
      },
      [kClose](chunk) {
        reading = false;

        const byobCanceled = forBranch2 === true ? canceled2 : canceled1;
        const otherCanceled = forBranch2 === false ? canceled2 : canceled1;

        if (!byobCanceled) {
          readableByteStreamControllerClose(byobBranch[kState].controller);
        }
        if (!otherCanceled) {
          readableByteStreamControllerClose(otherBranch[kState].controller);
        }
        if (chunk !== undefined) {
          if (!byobCanceled) {
            readableByteStreamControllerRespondWithNewView(
              byobBranch[kState].controller,
              chunk,
            );
          }
          if (
            !otherCanceled &&
            otherBranch[kState].controller[kState].pendingPullIntos.length > 0
          ) {
            readableByteStreamControllerRespond(
              otherBranch[kState].controller,
              0,
            );
          }
        }
        if (!byobCanceled || !otherCanceled) {
          cancelDeferred.resolve();
        }
      },
      [kError]() {
        reading = false;
      },
    };
    readableStreamBYOBReaderRead(reader, view, 1, readIntoRequest);
  }

  function pull1Algorithm() {
    if (reading) {
      readAgainForBranch1 = true;
      return PromiseResolve();
    }
    reading = true;

    const byobRequest = branch1[kState].controller.byobRequest;
    if (byobRequest === null) {
      pullWithDefaultReader();
    } else {
      pullWithBYOBReader(byobRequest[kState].view, false);
    }
    return PromiseResolve();
  }

  function pull2Algorithm() {
    if (reading) {
      readAgainForBranch2 = true;
      return PromiseResolve();
    }
    reading = true;

    const byobRequest = branch2[kState].controller.byobRequest;
    if (byobRequest === null) {
      pullWithDefaultReader();
    } else {
      pullWithBYOBReader(byobRequest[kState].view, true);
    }
    return PromiseResolve();
  }

  function cancel1Algorithm(reason) {
    canceled1 = true;
    reason1 = reason;
    if (canceled2) {
      cancelDeferred.resolve(readableStreamCancel(stream, [reason1, reason2]));
    }
    return cancelDeferred.promise;
  }

  function cancel2Algorithm(reason) {
    canceled2 = true;
    reason2 = reason;
    if (canceled1) {
      cancelDeferred.resolve(readableStreamCancel(stream, [reason1, reason2]));
    }
    return cancelDeferred.promise;
  }

  branch1 =
    createReadableByteStream(nonOpCallback, pull1Algorithm, cancel1Algorithm);
  branch2 =
    createReadableByteStream(nonOpCallback, pull2Algorithm, cancel2Algorithm);

  forwardReaderError(reader);

  return [branch1, branch2];
}

function readableByteStreamControllerConvertPullIntoDescriptor(desc) {
  const {
    buffer,
    bytesFilled,
    byteLength,
    byteOffset,
    ctor,
    elementSize,
  } = desc;
  if (bytesFilled > byteLength)
    throw new ERR_INVALID_STATE.RangeError('The buffer size is invalid');
  assert(!(bytesFilled % elementSize));
  const transferredBuffer = ArrayBufferPrototypeTransfer(buffer);

  if (ctor === Buffer) {
    return Buffer.from(transferredBuffer, byteOffset, bytesFilled / elementSize);
  }

  return new ctor(transferredBuffer, byteOffset, bytesFilled / elementSize);
}

function isReadableStreamLocked(stream) {
  return stream[kState].reader !== undefined;
}

function readableStreamCancel(stream, reason) {
  stream[kState].disturbed = true;
  switch (stream[kState].state) {
    case 'closed':
      return PromiseResolve();
    case 'errored':
      return PromiseReject(stream[kState].storedError);
  }
  readableStreamClose(stream);
  const {
    reader,
  } = stream[kState];
  if (reader !== undefined && readableStreamHasBYOBReader(stream)) {
    const readIntoRequests = reader[kState].readIntoRequests;
    while (readIntoRequests.length)
      readIntoRequests.shift()[kClose]();
  }

  return PromisePrototypeThen(
    stream[kState].controller[kCancel](reason),
    () => {});
}

function readableStreamClose(stream) {
  assert(stream[kState].state === 'readable');
  stream[kState].state = 'closed';
  stream[kState].closedPromise?.resolve();
  const {
    reader,
  } = stream[kState];

  if (reader === undefined)
    return;

  reader[kState].close?.resolve();

  if (readableStreamHasDefaultReader(stream)) {
    const readRequests = reader[kState].readRequests;
    while (readRequests.length)
      readRequests.shift()[kClose]();
  }
}

function readableStreamError(stream, error) {
  assert(stream[kState].state === 'readable');
  stream[kState].state = 'errored';
  stream[kState].storedError = error;
  const closedPromiseCache = stream[kState].closedPromise;
  if (closedPromiseCache !== undefined) {
    setPromiseHandled(closedPromiseCache.promise);
    closedPromiseCache.reject(error);
  }

  const {
    reader,
  } = stream[kState];

  if (reader === undefined)
    return;

  const closeCache = reader[kState].close;
  if (closeCache !== undefined) {
    setPromiseHandled(closeCache.promise);
    closeCache.reject(error);
  }

  if (readableStreamHasDefaultReader(stream)) {
    const readRequests = reader[kState].readRequests;
    while (readRequests.length)
      readRequests.shift()[kError](error);
  } else {
    assert(readableStreamHasBYOBReader(stream));
    const readIntoRequests = reader[kState].readIntoRequests;
    while (readIntoRequests.length)
      readIntoRequests.shift()[kError](error);
  }
}

function readableStreamHasDefaultReader(stream) {
  const {
    reader,
  } = stream[kState];

  if (reader === undefined)
    return false;

  return reader[kState] !== undefined &&
         reader[kType] === 'ReadableStreamDefaultReader';
}

function readableStreamGetNumReadRequests(stream) {
  assert(readableStreamHasDefaultReader(stream));
  return stream[kState].reader[kState].readRequests.length;
}

function readableStreamHasBYOBReader(stream) {
  const {
    reader,
  } = stream[kState];

  if (reader === undefined)
    return false;

  return reader[kState] !== undefined &&
         reader[kType] === 'ReadableStreamBYOBReader';
}

function readableStreamGetNumReadIntoRequests(stream) {
  assert(readableStreamHasBYOBReader(stream));
  return stream[kState].reader[kState].readIntoRequests.length;
}

function readableStreamFulfillReadRequest(stream, chunk, done) {
  assert(readableStreamHasDefaultReader(stream));
  const {
    reader,
  } = stream[kState];
  assert(reader[kState].readRequests.length);
  const readRequest = reader[kState].readRequests.shift();

  // TODO(@jasnell): It's not clear under what exact conditions done
  // will be true here. The spec requires this check but none of the
  // WPT's or other tests trigger it. Will need to investigate how to
  // get coverage for this.
  if (done)
    readRequest[kClose]();
  else
    readRequest[kChunk](chunk);
}

function readableStreamFulfillReadIntoRequest(stream, chunk, done) {
  assert(readableStreamHasBYOBReader(stream));
  const {
    reader,
  } = stream[kState];
  assert(reader[kState].readIntoRequests.length);
  const readIntoRequest = reader[kState].readIntoRequests.shift();
  if (done)
    readIntoRequest[kClose](chunk);
  else
    readIntoRequest[kChunk](chunk);
}

function readableStreamAddReadRequest(stream, readRequest) {
  assert(readableStreamHasDefaultReader(stream));
  assert(stream[kState].state === 'readable');
  const readerState = stream[kState].reader[kState];
  let readRequests = readerState.readRequests;
  if (readRequests === kEmptyQueue)
    readRequests = readerState.readRequests = new Queue();
  readRequests.push(readRequest);
}

function readableStreamAddReadIntoRequest(stream, readIntoRequest) {
  assert(readableStreamHasBYOBReader(stream));
  assert(stream[kState].state !== 'errored');
  const readerState = stream[kState].reader[kState];
  let readIntoRequests = readerState.readIntoRequests;
  if (readIntoRequests === kEmptyQueue)
    readIntoRequests = readerState.readIntoRequests = new Queue();
  readIntoRequests.push(readIntoRequest);
}

function readableStreamReaderGenericCancel(reader, reason) {
  const {
    stream,
  } = reader[kState];
  assert(stream !== undefined);
  return readableStreamCancel(stream, reason);
}

function readableStreamReaderGenericInitialize(reader, stream) {
  reader[kState].stream = stream;
  stream[kState].reader = reader;
  // The reader's closed promise (reader[kState].close) is materialized
  // lazily by readerClosedPromise(); its settlement is fully derivable
  // from the reader/stream state, so nothing is allocated here.
}

// Materializes the reader's [[closedPromise]] record on first
// observation. While unobserved, its settlement is derivable: a
// detached reader was released (rejected with the shared released
// error); otherwise it follows the stream state. The settle sites
// (readableStreamClose/Error, readableStreamReaderGenericRelease)
// only touch the record when it has been materialized.
function readerClosedPromise(reader) {
  let close = reader[kState].close;
  if (close === undefined) {
    const stream = reader[kState].stream;
    if (stream === undefined) {
      close = {
        promise: PromiseReject(lazyReadableReleasedError()),
        resolve: undefined,
        reject: undefined,
      };
      setPromiseHandled(close.promise);
    } else {
      switch (stream[kState].state) {
        case 'readable':
          close = PromiseWithResolvers();
          break;
        case 'closed':
          close = {
            promise: PromiseResolve(),
            resolve: undefined,
            reject: undefined,
          };
          break;
        case 'errored':
          close = {
            promise: PromiseReject(stream[kState].storedError),
            resolve: undefined,
            reject: undefined,
          };
          setPromiseHandled(close.promise);
          break;
      }
    }
    reader[kState].close = close;
  }
  return close;
}

function readableStreamDefaultReaderRelease(reader) {
  readableStreamReaderGenericRelease(reader);
  readableStreamDefaultReaderErrorReadRequests(
    reader,
    lazyReadableReleasingError(),
  );
}

function readableStreamDefaultReaderErrorReadRequests(reader, e) {
  const readRequests = reader[kState].readRequests;
  while (readRequests.length)
    readRequests.shift()[kError](e);
}

function readableStreamBYOBReaderRelease(reader) {
  readableStreamReaderGenericRelease(reader);
  readableStreamBYOBReaderErrorReadIntoRequests(
    reader,
    lazyReadableReleasingError(),
  );
}

function readableStreamBYOBReaderErrorReadIntoRequests(reader, e) {
  const readIntoRequests = reader[kState].readIntoRequests;
  while (readIntoRequests.length)
    readIntoRequests.shift()[kError](e);
}

function readableStreamReaderGenericRelease(reader) {
  const {
    stream,
  } = reader[kState];
  assert(stream !== undefined);
  assert(stream[kState].reader === reader);

  const closeCache = reader[kState].close;
  if (stream[kState].state === 'readable') {
    if (closeCache !== undefined) {
      closeCache.reject(lazyReadableReleasedError());
      setPromiseHandled(closeCache.promise);
    }
  } else {
    // The spec replaces [[closedPromise]] here. Dropping the cache makes
    // the next observation derive the released rejection.
    reader[kState].close = undefined;
  }

  stream[kState].controller[kRelease]();

  stream[kState].reader = undefined;
  reader[kState].stream = undefined;
}

function readableStreamBYOBReaderRead(reader, view, min, readIntoRequest) {
  const {
    stream,
  } = reader[kState];
  assert(stream !== undefined);
  stream[kState].disturbed = true;
  if (stream[kState].state === 'errored') {
    readIntoRequest[kError](stream[kState].storedError);
    return;
  }
  readableByteStreamControllerPullInto(
    stream[kState].controller,
    view,
    min,
    readIntoRequest);
}

function readableStreamDefaultReaderRead(reader, readRequest) {
  const {
    stream,
  } = reader[kState];
  assert(stream !== undefined);
  stream[kState].disturbed = true;
  switch (stream[kState].state) {
    case 'closed':
      readRequest[kClose]();
      break;
    case 'errored':
      readRequest[kError](stream[kState].storedError);
      break;
    case 'readable':
      stream[kState].controller[kPull](readRequest);
  }
}

function setupReadableStreamBYOBReader(reader, stream) {
  if (isReadableStreamLocked(stream))
    throw new ERR_INVALID_STATE.TypeError('ReadableStream is locked');
  const {
    controller,
  } = stream[kState];
  if (!isReadableByteStreamController(controller))
    throw new ERR_INVALID_ARG_VALUE('stream', stream, 'must be a byte stream');
  readableStreamReaderGenericInitialize(reader, stream);
  // The read-request queues use the same ring buffer as [[queue]], drained
  // from a moving head rather than with ArrayPrototypeShift. Start from the
  // shared immutable empty queue so acquiring a reader allocates no request
  // storage until a read actually parks.
  reader[kState].readIntoRequests = kEmptyQueue;
}

function setupReadableStreamDefaultReader(reader, stream) {
  if (isReadableStreamLocked(stream))
    throw new ERR_INVALID_STATE.TypeError('ReadableStream is locked');
  readableStreamReaderGenericInitialize(reader, stream);
  reader[kState].readRequests = kEmptyQueue;
}

function readableStreamDefaultControllerClose(controller) {
  if (!readableStreamDefaultControllerCanCloseOrEnqueue(controller))
    return;
  controller[kState].closeRequested = true;
  if (!controller[kState].queue.length) {
    readableStreamDefaultControllerClearAlgorithms(controller);
    readableStreamClose(controller[kState].stream);
  }
}
