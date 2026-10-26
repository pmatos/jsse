// Copyright Joyent, Inc. and other Node contributors.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit
// persons to whom the Software is furnished to do so, subject to the
// following conditions:
//
// The above copyright notice and this permission notice shall be included
// in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN
// NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
// OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
// USE OR OTHER DEALINGS IN THE SOFTWARE.

'use strict';

const {
  ArrayIsArray,
  ArrayPrototypePop,
  ArrayPrototypePush,
  ArrayPrototypeSlice,
  Error,
  ErrorCaptureStackTrace,
  FunctionPrototypeBind,
  NumberIsSafeInteger,
  ObjectDefineProperties,
  ObjectDefineProperty,
  ObjectGetOwnPropertyDescriptors,
  ObjectKeys,
  ObjectSetPrototypeOf,
  ObjectValues,
  ReflectApply,
  RegExpPrototypeExec,
  StringPrototypeSplit,
  StringPrototypeToWellFormed,
} = primordials;

const {
  codes: {
    ERR_FALSY_VALUE_REJECTION,
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_ARG_VALUE,
    ERR_OUT_OF_RANGE,
  },
} = require('internal/errors');
const {
  format,
  formatWithOptions,
  inspect,
  stripVTControlCharacters,
} = require('internal/util/inspect');
const { debuglog } = require('internal/util/debuglog');
const {
  validateFunction,
  validateInteger,
  validateNumber,
  validateString,
} = require('internal/validators');
const types = require('internal/util/types');
const { styleText } = require('internal/util/style_text');

const {
  convertProcessSignalToExitCode,
  deprecate: internalDeprecate,
  getSystemErrorMap,
  getSystemErrorName: internalErrorName,
  getSystemErrorMessage: internalErrorMessage,
  promisify,
  defineLazyProperties,
  kEmptyObject,
} = require('internal/util');

let abortController;

function lazyAbortController() {
  abortController ??= require('internal/abort_controller');
  return abortController;
}

let internalDeepEqual;


/**
 * Inherit the prototype methods from one constructor into another.
 *
 * The Function.prototype.inherits from lang.js rewritten as a standalone
 * function (not on Function.prototype). NOTE: If this file is to be loaded
 * during bootstrapping this function needs to be rewritten using some native
 * functions as prototype setup using normal JavaScript does not work as
 * expected during bootstrapping (see mirror.js in r114903).
 * @param {Function} ctor Constructor function which needs to inherit the
 *   prototype.
 * @param {Function} superCtor Constructor function to inherit prototype from.
 * @throws {TypeError} Will error if either constructor is null, or if
 * the super constructor lacks a prototype.
 */
function inherits(ctor, superCtor) {

  if (ctor === undefined || ctor === null)
    throw new ERR_INVALID_ARG_TYPE('ctor', 'Function', ctor);

  if (superCtor === undefined || superCtor === null)
    throw new ERR_INVALID_ARG_TYPE('superCtor', 'Function', superCtor);

  if (superCtor.prototype === undefined) {
    throw new ERR_INVALID_ARG_TYPE('superCtor.prototype',
                                   'Object', superCtor.prototype);
  }
  ObjectDefineProperty(ctor, 'super_', {
    __proto__: null,
    value: superCtor,
    writable: true,
    configurable: true,
  });
  ObjectSetPrototypeOf(ctor.prototype, superCtor.prototype);
}

/**
 * @deprecated since v6.0.0
 * @template T
 * @template S
 * @param {T} target
 * @param {S} source
 * @returns {(T & S) | null}
 */
function _extend(target, source) {
  // Don't do anything if source isn't an object
  if (source === null || typeof source !== 'object') return target;

  const keys = ObjectKeys(source);
  let i = keys.length;
  while (i--) {
    target[keys[i]] = source[keys[i]];
  }
  return target;
}

const callbackifyOnRejected = (reason, cb) => {
  // `!reason` guard inspired by bluebird (Ref: https://goo.gl/t5IS6M).
  // Because `null` is a special error value in callbacks which means "no error
  // occurred", we error-wrap so the callback consumer can distinguish between
  // "the promise rejected with null" or "the promise fulfilled with undefined".
  if (!reason) {
    reason = new ERR_FALSY_VALUE_REJECTION.HideStackFramesError(reason);
    ErrorCaptureStackTrace(reason, callbackifyOnRejected);
  }
  return cb(reason);
};

/**
 * Converts a Promise-returning function to callback style
 * @param {Function} original
 * @returns {Function}
 */
function callbackify(original) {
  validateFunction(original, 'original');

  // We DO NOT return the promise as it gives the user a false sense that
  // the promise is actually somehow related to the callback's execution
  // and that the callback throwing will reject the promise.
  function callbackified(...args) {
    const maybeCb = ArrayPrototypePop(args);
    validateFunction(maybeCb, 'last argument');
    const cb = FunctionPrototypeBind(maybeCb, this);
    // In true node style we process the callback on `nextTick` with all the
    // implications (stack, `uncaughtException`, `async_hooks`)
    ReflectApply(original, this, args)
      .then((ret) => process.nextTick(cb, null, ret),
            (rej) => process.nextTick(callbackifyOnRejected, rej, cb));
  }

  const descriptors = ObjectGetOwnPropertyDescriptors(original);
  // It is possible to manipulate a functions `length` or `name` property. This
  // guards against the manipulation.
  if (typeof descriptors.length.value === 'number') {
    descriptors.length.value++;
  }
  if (typeof descriptors.name.value === 'string') {
    descriptors.name.value += 'Callbackified';
  }
  const propertiesValues = ObjectValues(descriptors);
  for (let i = 0; i < propertiesValues.length; i++) {
  // We want to use null-prototype objects to not rely on globally mutable
  // %Object.prototype%.
    ObjectSetPrototypeOf(propertiesValues[i], null);
  }
  ObjectDefineProperties(callbackified, descriptors);
  return callbackified;
}

/**
 * @param {number} err
 * @returns {string}
 */
function getSystemErrorMessage(err) {
  validateNumber(err, 'err');
  if (err >= 0 || !NumberIsSafeInteger(err)) {
    throw new ERR_OUT_OF_RANGE('err', 'a negative integer', err);
  }
  return internalErrorMessage(err);
}

/**
 * @param {number} err
 * @returns {string}
 */
function getSystemErrorName(err) {
  validateNumber(err, 'err');
  if (err >= 0 || !NumberIsSafeInteger(err)) {
    throw new ERR_OUT_OF_RANGE('err', 'a negative integer', err);
  }
  return internalErrorName(err);
}

function _errnoException(err, syscall, original) {
  const code = getSystemErrorName(err);
  const message = original ?
    `${syscall} ${code} ${original}` : `${syscall} ${code}`;
  const e = new Error(message);
  e.errno = err;
  e.code = code;
  e.syscall = syscall;
  return e;
}

function _exceptionWithHostPort(err, syscall, address, port, additional) {
  const code = getSystemErrorName(err);
  let details = '';
  if (port && port > 0) {
    details = ` ${address}:${port}`;
  } else if (address) {
    details = ` ${address}`;
  }
  if (additional) {
    details += ` - Local (${additional})`;
  }
  const e = new Error(`${syscall} ${code}${details}`);
  e.errno = err;
  e.code = code;
  e.syscall = syscall;
  e.address = address;
  if (port) {
    e.port = port;
  }
  return e;
}

function parseEnv(content) {
  validateString(content, 'content');
  return require('internal/util/dotenv').parse(content);
}

const stackFrameRegExp = /^\s*at (?:(.*?) \()?(.*?):(\d+):(\d+)\)?$/;

function getCallSites(frameCount = 10, options) {
  if (typeof frameCount !== 'number') {
    if (frameCount !== null && typeof frameCount === 'object') {
      options = frameCount;
    }
    frameCount = 10;
  }
  validateInteger(frameCount, 'frameCount', 1, 200);
  const lines = StringPrototypeSplit(new Error().stack ?? '', '\n');
  const sites = [];
  for (let i = 1; i < lines.length && sites.length < frameCount; i++) {
    const m = RegExpPrototypeExec(stackFrameRegExp, lines[i]);
    if (m === null) continue;
    ArrayPrototypePush(sites, {
      __proto__: null,
      functionName: m[1] ?? '',
      scriptId: '0',
      scriptName: m[2],
      lineNumber: +m[3],
      columnNumber: +m[4],
      column: +m[4],
    });
  }
  return ArrayPrototypeSlice(sites, 1);
}

// Public util.deprecate API
function deprecate(fn, msg, code, { modifyPrototype } = kEmptyObject) {
  return internalDeprecate(fn, msg, code, undefined, modifyPrototype);
}

// Keep the `exports =` so that various functions can still be monkeypatched
module.exports = {
  _errnoException,
  _exceptionWithHostPort,
  _extend: internalDeprecate(_extend,
                             'The `util._extend` API is deprecated. Please use Object.assign() instead.',
                             'DEP0060'),
  callbackify,
  convertProcessSignalToExitCode,
  debug: debuglog,
  debuglog,
  deprecate,
  format,
  styleText,
  formatWithOptions,
  getCallSites,
  getSystemErrorMap,
  getSystemErrorName,
  getSystemErrorMessage,
  inherits,
  inspect,
  isArray: internalDeprecate(ArrayIsArray,
                             'The `util.isArray` API is deprecated. Please use `Array.isArray()` instead.',
                             'DEP0044'),
  isDeepStrictEqual(a, b, skipPrototype) {
    if (internalDeepEqual === undefined) {
      internalDeepEqual = require('internal/util/comparisons').isDeepStrictEqual;
    }
    return internalDeepEqual(a, b, skipPrototype);
  },
  promisify,
  stripVTControlCharacters,
  toUSVString(input) {
    return StringPrototypeToWellFormed(`${input}`);
  },
  get transferableAbortSignal() {
    return lazyAbortController().transferableAbortSignal;
  },
  get transferableAbortController() {
    return lazyAbortController().transferableAbortController;
  },
  get aborted() {
    return lazyAbortController().aborted;
  },
  types,
  parseEnv,
};

defineLazyProperties(
  module.exports,
  'internal/util/parse_args/parse_args',
  ['parseArgs'],
);

defineLazyProperties(
  module.exports,
  'internal/host/encoding',
  ['TextDecoder', 'TextEncoder'],
);

defineLazyProperties(
  module.exports,
  'internal/mime',
  ['MIMEType', 'MIMEParams'],
);

defineLazyProperties(
  module.exports,
  'internal/util/diff',
  ['diff'],
);

defineLazyProperties(
  module.exports,
  'internal/util/trace_sigint',
  ['setTraceSigInt'],
);
