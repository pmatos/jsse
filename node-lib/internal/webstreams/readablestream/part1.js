'use strict';

const {
  ArrayBuffer,
  ArrayBufferPrototypeGetByteLength,
  ArrayBufferPrototypeGetDetached,
  ArrayBufferPrototypeSlice,
  ArrayBufferPrototypeTransfer,
  ArrayPrototypePush,
  DataView,
  FunctionPrototypeBind,
  FunctionPrototypeCall,
  MathMin,
  NumberIsInteger,
  ObjectDefineProperties,
  ObjectSetPrototypeOf,
  Promise,
  PromisePrototypeThen,
  PromiseReject,
  PromiseResolve,
  PromiseWithResolvers,
  SafePromiseAll,
  Symbol,
  SymbolAsyncIterator,
  SymbolDispose,
  SymbolIterator,
  SymbolToStringTag,
  TypedArrayPrototypeGetLength,
  Uint8Array,
} = primordials;

const {
  AbortError,
  codes: {
    ERR_ARG_NOT_ITERABLE,
    ERR_ILLEGAL_CONSTRUCTOR,
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_ARG_VALUE,
    ERR_INVALID_STATE,
    ERR_INVALID_THIS,
    ERR_OUT_OF_RANGE,
  },
} = require('internal/errors');

const {
  DOMException,
} = require('internal/per_context/domexception');

const {
  isArrayBufferView,
  isDataView,
  isSharedArrayBuffer,
} = require('internal/util/types');

const {
  customInspectSymbol: kInspect,
  kEmptyObject,
  kEnumerableProperty,
  SideEffectFreeRegExpPrototypeSymbolReplace,
} = require('internal/util');

const {
  validateAbortSignal,
  validateBuffer,
  validateNumber,
  validateObject,
  kValidateObjectAllowObjects,
  kValidateObjectAllowObjectsAndNull,
} = require('internal/validators');

const {
  MessageChannel,
} = require('internal/worker/io');

const {
  kDeserialize,
  kTransfer,
  kTransferList,
  markTransferMode,
  structuredClone,
} = require('internal/worker/js_transferable');

const {
  queueMicrotask,
} = require('internal/process/task_queues');

const {
  kIsDisturbed,
  kIsErrored,
  kIsReadable,
  kIsClosedPromise,
  kControllerErrorFunction,
} = require('internal/streams/utils');

const {
  ArrayBufferViewGetBuffer,
  ArrayBufferViewGetByteLength,
  ArrayBufferViewGetByteOffset,
  AsyncIterator,
  Queue,
  canCopyArrayBuffer,
  cloneAsUint8Array,
  copyArrayBuffer,
  createPromiseCallback1Param,
  createRawCallback1Param,
  customInspect,
  defaultSizeAlgorithm,
  dequeueValue,
  enqueueValueWithSize,
  extractHighWaterMark,
  extractSizeAlgorithm,
  getNonWritablePropertyDescriptor,
  isBrandCheck,
  kEmptyQueue,
  kParkedAlgorithmResult,
  kResolvedPromise,
  kState,
  kType,
  lazyTransfer,
  materializeQueue,
  nonOpCallback,
  nonOpCancel,
  rejectedHandledRecord,
  resetQueue,
  resolvedRecord,
  setPromiseHandled,
  thenAlgorithmResult,
} = require('internal/webstreams/util');

const {
  WritableStreamDefaultWriter,

  isWritableStream,
  isWritableStreamLocked,
  isWritableStreamDefaultController,
  isWritableStreamDefaultWriter,

  writableStreamAbort,
  writableStreamCloseQueuedOrInFlight,
  writableStreamDefaultWriterCloseWithErrorPropagation,
  writableStreamDefaultWriterRelease,
  writableStreamDefaultWriterWriteWithRequest,
  writerClosedPromise,
} = require('internal/webstreams/writablestream');

const { Buffer } = require('buffer');

const assert = require('internal/assert');

const kCancel = Symbol('kCancel');
const kClose = Symbol('kClose');
const kChunk = Symbol('kChunk');
const kError = Symbol('kError');
const kPull = Symbol('kPull');
const kRelease = Symbol('kRelease');
const kSkipThrow = Symbol('kSkipThrow');

let releasedError;
let releasingError;
let addAbortListener;

const userModuleRegExp = /^ {4}at (?:[^/\\(]+ \()(?!node:(.+):\d+:\d+\)$).*/gm;

function lazyReadableReleasedError() {
  if (releasedError) {
    return releasedError;
  }

  releasedError = new ERR_INVALID_STATE.TypeError('Reader released');
  // Avoid V8 leak and remove userland stackstrace
  releasedError.stack = SideEffectFreeRegExpPrototypeSymbolReplace(userModuleRegExp, releasedError.stack, '');
  return releasedError;
}

function lazyReadableReleasingError() {
  if (releasingError) {
    return releasingError;
  }
  releasingError = new ERR_INVALID_STATE.TypeError('Releasing reader');
  // Avoid V8 leak and remove userland stackstrace
  releasingError.stack = SideEffectFreeRegExpPrototypeSymbolReplace(userModuleRegExp, releasingError.stack, '');
  return releasingError;
}

/**
 * @typedef {import('../abort_controller').AbortSignal} AbortSignal
 * @typedef {import('./queuingstrategies').QueuingStrategy} QueuingStrategy
 * @typedef {import('./queuingstrategies').QueuingStrategySize
 * } QueuingStrategySize
 * @typedef {import('./writablestream').WritableStream} WritableStream
 */

/**
 * @typedef {ReadableStreamDefaultController | ReadableByteStreamController
 * } ReadableStreamController
 */

/**
 * @typedef {ReadableStreamDefaultReader | ReadableStreamBYOBReader
 * } ReadableStreamReader
 */

/**
 * @callback UnderlyingSourceStartCallback
 * @param {ReadableStreamController} controller
 * @returns { any | Promise<void> }
 */

/**
 * @callback UnderlyingSourcePullCallback
 * @param {ReadableStreamController} controller
 * @returns { Promise<void> }
 */

/**
 * @callback UnderlyingSourceCancelCallback
 * @param {any} reason
 * @returns { Promise<void> }
 */

/**
 * @typedef {{
 *   readable: ReadableStream,
 *   writable: WritableStream,
 * }} ReadableWritablePair
 */

/**
 * @typedef {{
 *   preventClose? : boolean,
 *   preventAbort? : boolean,
 *   preventCancel? : boolean,
 *   signal? : AbortSignal,
 * }} StreamPipeOptions
 */

/**
 * @typedef {{
 *   start? : UnderlyingSourceStartCallback,
 *   pull? : UnderlyingSourcePullCallback,
 *   cancel? : UnderlyingSourceCancelCallback,
 *   type? : "bytes",
 *   autoAllocateChunkSize? : number
 * }} UnderlyingSource
 */
