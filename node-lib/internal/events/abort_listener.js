'use strict';

const {
  ObjectFreeze,
  PromisePrototypeThen,
  PromiseResolve,
  SymbolDispose,
} = primordials;
const {
  validateAbortSignal,
  validateFunction,
} = require('internal/validators');
const {
  codes: {
    ERR_INVALID_ARG_TYPE,
  },
} = require('internal/errors');

let kResistStopPropagation;
let abortListenerOptions;

/**
 * @param {AbortSignal} signal
 * @param {EventListener} listener
 * @returns {Disposable}
 */
function addAbortListener(signal, listener) {
  if (signal === undefined) {
    throw new ERR_INVALID_ARG_TYPE('signal', 'AbortSignal', signal);
  }
  validateAbortSignal(signal, 'signal');
  validateFunction(listener, 'listener');

  let removeEventListener;
  if (signal.aborted) {
    // The global queueMicrotask routes a throw to uncaughtException; the
    // promise fallback covers a prelude that does not define it yet (#1229).
    if (typeof queueMicrotask === 'function') {
      queueMicrotask(() => listener());
    } else {
      PromisePrototypeThen(PromiseResolve(), () => listener());
    }
  } else {
    kResistStopPropagation ??= require('internal/event_target').kResistStopPropagation;
    abortListenerOptions ??= ObjectFreeze({ __proto__: null, once: true, [kResistStopPropagation]: true });
    // TODO(atlowChemi) add { subscription: true } and return directly
    signal.addEventListener('abort', listener, abortListenerOptions);
    removeEventListener = () => {
      signal.removeEventListener('abort', listener);
    };
  }
  return {
    __proto__: null,
    [SymbolDispose]() {
      removeEventListener?.();
    },
  };
}

module.exports = {
  __proto__: null,
  addAbortListener,
};
