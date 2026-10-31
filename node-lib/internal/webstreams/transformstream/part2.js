
async function transformStreamDefaultSinkAbortAlgorithm(stream, reason) {
  const {
    controller,
    readable,
  } = stream[kState];

  if (controller[kState].finishPromise !== undefined) {
    return controller[kState].finishPromise;
  }

  const { promise, resolve, reject } = PromiseWithResolvers();
  controller[kState].finishPromise = promise;
  const cancelPromise = controller[kState].cancelAlgorithm(reason);
  transformStreamDefaultControllerClearAlgorithms(controller);

  PromisePrototypeThen(
    cancelPromise,
    () => {
      if (readable[kState].state === 'errored')
        reject(readable[kState].storedError);
      else {
        readableStreamDefaultControllerError(readable[kState].controller, reason);
        resolve();
      }
    },
    (error) => {
      readableStreamDefaultControllerError(readable[kState].controller, error);
      reject(error);
    },
  );

  return controller[kState].finishPromise;
}

function transformStreamDefaultSinkCloseAlgorithm(stream) {
  const {
    readable,
    controller,
  } = stream[kState];

  if (controller[kState].finishPromise !== undefined) {
    return controller[kState].finishPromise;
  }
  const { promise, resolve, reject } = PromiseWithResolvers();
  controller[kState].finishPromise = promise;
  const flushPromise = controller[kState].flushAlgorithm(controller);
  transformStreamDefaultControllerClearAlgorithms(controller);
  PromisePrototypeThen(
    flushPromise,
    () => {
      if (readable[kState].state === 'errored')
        reject(readable[kState].storedError);
      else {
        readableStreamDefaultControllerClose(readable[kState].controller);
        resolve();
      }
    },
    (error) => {
      readableStreamDefaultControllerError(readable[kState].controller, error);
      reject(error);
    });
  return controller[kState].finishPromise;
}

function transformStreamDefaultSourcePullAlgorithm(stream) {
  const state = stream[kState];
  assert(state.backpressure);
  transformStreamSetBackpressure(stream, false);
  // Park the pull: the next backpressure -> true flip delivers the
  // pull-fulfilled step (see transformStreamSetBackpressure). The old
  // [[backpressureChangePromise]] this replaces was only ever resolved,
  // so the parked pull needs no rejection delivery.
  state.pullPending = true;
  return kParkedAlgorithmResult;
}

function transformStreamDefaultSourceCancelAlgorithm(stream, reason) {
  const {
    controller,
    writable,
  } = stream[kState];

  if (controller[kState].finishPromise !== undefined) {
    return controller[kState].finishPromise;
  }

  const { promise, resolve, reject } = PromiseWithResolvers();
  controller[kState].finishPromise = promise;
  const cancelPromise = controller[kState].cancelAlgorithm(reason);
  transformStreamDefaultControllerClearAlgorithms(controller);

  PromisePrototypeThen(
    cancelPromise,
    () => {
      if (writable[kState].state === 'errored')
        reject(writable[kState].storedError);
      else {
        writableStreamDefaultControllerErrorIfNeeded(
          writable[kState].controller,
          reason);
        transformStreamUnblockWrite(stream);
        resolve();
      }
    },
    (error) => {
      writableStreamDefaultControllerErrorIfNeeded(
        writable[kState].controller,
        error);
      transformStreamUnblockWrite(stream);
      reject(error);
    },
  );

  return controller[kState].finishPromise;
}

module.exports = {
  TransformStream,
  TransformStreamDefaultController,
  TransferredTransformStream,

  // Exported Brand Checks
  isTransformStream,
  isTransformStreamDefaultController,
};
