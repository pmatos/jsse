'use strict';

const {
  ArrayPrototypePush,
  Error,
  FunctionPrototypeCall,
  ObjectKeys,
  ObjectPrototypeIsPrototypeOf,
  RegExpPrototypeExec,
  String,
} = primordials;

const {
  codes: {
    ERR_AMBIGUOUS_ARGUMENT,
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_ARG_VALUE,
    ERR_INVALID_RETURN_VALUE,
  },
} = require('internal/errors');
const AssertionError = require('internal/assert/assertion_error');
const { inspect } = require('internal/util/inspect');
const {
  isPromise,
  isRegExp,
} = require('internal/util/types');
const { isError } = require('internal/util');
const { innerFail } = require('internal/assert/utils');
const { validateFunction } = require('internal/validators');

let isDeepEqual;
let isDeepStrictEqual;

function lazyLoadComparison() {
  const comparison = require('internal/util/comparisons');
  isDeepEqual = comparison.isDeepEqual;
  isDeepStrictEqual = comparison.isDeepStrictEqual;
}

const NO_EXCEPTION_SENTINEL = {};

let Assert;
function lazyAssert() {
  return Assert ??= require('assert').Assert;
}

class Comparison {
  constructor(obj, keys, actual) {
    for (const key of keys) {
      if (key in obj) {
        if (actual !== undefined &&
            typeof actual[key] === 'string' &&
            isRegExp(obj[key]) &&
            RegExpPrototypeExec(obj[key], actual[key]) !== null) {
          this[key] = actual[key];
        } else {
          this[key] = obj[key];
        }
      }
    }
  }
}

function compareExceptionKey(actual, expected, key, message, keys, fn) {
  if (!(key in actual) || !isDeepStrictEqual(actual[key], expected[key])) {
    if (!message) {
      // Create placeholder objects to create a nice output.
      const a = new Comparison(actual, keys);
      const b = new Comparison(expected, keys, actual);

      const err = new AssertionError({
        actual: a,
        expected: b,
        operator: 'deepStrictEqual',
        stackStartFn: fn,
      });
      err.actual = actual;
      err.expected = expected;
      err.operator = fn.name;
      throw err;
    }
    innerFail({
      actual,
      expected,
      message: [message],
      operator: fn.name,
      stackStartFn: fn,
    });
  }
}

function expectedException(actual, expected, message, fn) {
  let generatedMessage = false;
  let throwError = false;

  if (typeof expected !== 'function') {
    // Handle regular expressions.
    if (isRegExp(expected)) {
      const str = String(actual);
      if (RegExpPrototypeExec(expected, str) !== null)
        return;

      if (!message) {
        generatedMessage = true;
        message = 'The input did not match the regular expression ' +
                  `${inspect(expected)}. Input:\n\n${inspect(str)}\n`;
      }
      throwError = true;
      // Handle primitives properly.
    } else if (typeof actual !== 'object' || actual === null) {
      const err = new AssertionError({
        actual,
        expected,
        message,
        operator: 'deepStrictEqual',
        stackStartFn: fn,
      });
      err.operator = fn.name;
      throw err;
    } else {
      // Handle validation objects.
      const keys = ObjectKeys(expected);
      // Special handle errors to make sure the name and the message are
      // compared as well.
      if (expected instanceof Error) {
        ArrayPrototypePush(keys, 'name', 'message');
      } else if (keys.length === 0) {
        throw new ERR_INVALID_ARG_VALUE('error',
                                        expected, 'may not be an empty object');
      }
      if (isDeepEqual === undefined) lazyLoadComparison();
      for (const key of keys) {
        if (typeof actual[key] === 'string' &&
            isRegExp(expected[key]) &&
            RegExpPrototypeExec(expected[key], actual[key]) !== null) {
          continue;
        }
        compareExceptionKey(actual, expected, key, message, keys, fn);
      }
      return;
    }
  // Guard instanceof against arrow functions as they don't have a prototype.
  // Check for matching Error classes.
  } else if (expected.prototype !== undefined && actual instanceof expected) {
    return;
  } else if (ObjectPrototypeIsPrototypeOf(Error, expected)) {
    if (!message) {
      generatedMessage = true;
      message = 'The error is expected to be an instance of ' +
        `"${expected.name}". Received `;
      if (isError(actual)) {
        const name = (actual.constructor?.name) ||
                     actual.name;
        if (expected.name === name) {
          message += 'an error with identical name but a different prototype.';
        } else {
          message += `"${name}"`;
        }
        if (actual.message) {
          message += `\n\nError message:\n\n${actual.message}`;
        }
      } else {
        message += `"${inspect(actual, { depth: -1 })}"`;
      }
    }
    throwError = true;
  } else {
    // Check validation functions return value.
    const res = FunctionPrototypeCall(expected, {}, actual);
    if (res !== true) {
      if (!message) {
        generatedMessage = true;
        const name = expected.name ? `"${expected.name}" ` : '';
        message = `The ${name}validation function is expected to return` +
          ` "true". Received ${inspect(res)}`;

        if (isError(actual)) {
          message += `\n\nCaught error:\n\n${actual}`;
        }
      }
      throwError = true;
    }
  }

  if (throwError) {
    const err = new AssertionError({
      actual,
      expected,
      message,
      operator: fn.name,
      stackStartFn: fn,
    });
    err.generatedMessage = generatedMessage;
    throw err;
  }
}

function getActual(fn) {
  validateFunction(fn, 'fn');
  try {
    fn();
  } catch (e) {
    return e;
  }
  return NO_EXCEPTION_SENTINEL;
}

function checkIsPromise(obj) {
  // Accept native ES6 promises and promises that are implemented in a similar
  // way. Do not accept thenables that use a function as `obj` and that have no
  // `catch` handler.
  return isPromise(obj) ||
    (obj !== null && typeof obj === 'object' &&
    typeof obj.then === 'function' &&
    typeof obj.catch === 'function');
}

async function waitForActual(promiseFn) {
  let resultPromise;
  if (typeof promiseFn === 'function') {
    // Return a rejected promise if `promiseFn` throws synchronously.
    resultPromise = promiseFn();
    // Fail in case no promise is returned.
    if (!checkIsPromise(resultPromise)) {
      throw new ERR_INVALID_RETURN_VALUE('instance of Promise',
                                         'promiseFn', resultPromise);
    }
  } else if (checkIsPromise(promiseFn)) {
    resultPromise = promiseFn;
  } else {
    throw new ERR_INVALID_ARG_TYPE(
      'promiseFn', ['Function', 'Promise'], promiseFn);
  }

  try {
    await resultPromise;
  } catch (e) {
    return e;
  }
  return NO_EXCEPTION_SENTINEL;
}

function expectsError(stackStartFn, actual, error, message) {
  if (typeof error === 'string') {
    if (arguments.length === 4) {
      throw new ERR_INVALID_ARG_TYPE('error',
                                     ['Object', 'Error', 'Function', 'RegExp'],
                                     error);
    }
    if (typeof actual === 'object' && actual !== null) {
      if (actual.message === error) {
        throw new ERR_AMBIGUOUS_ARGUMENT(
          'error/message',
          `The error message "${actual.message}" is identical to the message.`,
        );
      }
    } else if (actual === error) {
      throw new ERR_AMBIGUOUS_ARGUMENT(
        'error/message',
        `The error "${actual}" is identical to the message.`,
      );
    }
    message = error;
    error = undefined;
  } else if (error != null &&
             typeof error !== 'object' &&
             typeof error !== 'function') {
    throw new ERR_INVALID_ARG_TYPE('error',
                                   ['Object', 'Error', 'Function', 'RegExp'],
                                   error);
  }

  if (actual === NO_EXCEPTION_SENTINEL) {
    let details = '';
    if (error?.name) {
      details += ` (${error.name})`;
    }
    details += message ? `: ${message}` : '.';
    const fnType = stackStartFn === lazyAssert().prototype.rejects ? 'rejection' : 'exception';
    innerFail({
      actual: undefined,
      expected: error,
      operator: stackStartFn.name,
      message: [`Missing expected ${fnType}${details}`],
      stackStartFn,
    });
  }

  if (!error)
    return;

  expectedException.call(this, actual, error, message, stackStartFn);
}

function hasMatchingError(actual, expected) {
  if (typeof expected !== 'function') {
    if (isRegExp(expected)) {
      const str = String(actual);
      return RegExpPrototypeExec(expected, str) !== null;
    }
    throw new ERR_INVALID_ARG_TYPE(
      'expected', ['Function', 'RegExp'], expected,
    );
  }
  // Guard instanceof against arrow functions as they don't have a prototype.
  if (expected.prototype !== undefined && actual instanceof expected) {
    return true;
  }
  if (ObjectPrototypeIsPrototypeOf(Error, expected)) {
    return false;
  }
  return FunctionPrototypeCall(expected, {}, actual) === true;
}

function expectsNoError(stackStartFn, actual, error, message) {
  if (actual === NO_EXCEPTION_SENTINEL)
    return;

  if (typeof error === 'string') {
    message = error;
    error = undefined;
  }

  if (!error || hasMatchingError(actual, error)) {
    const details = message ? `: ${message}` : '.';
    const fnType = stackStartFn === lazyAssert().prototype.doesNotReject ?
      'rejection' : 'exception';
    innerFail({
      actual,
      expected: error,
      operator: stackStartFn.name,
      message: [`Got unwanted ${fnType}${details}\n` +
               `Actual message: "${actual?.message}"`],
      stackStartFn,
    });
  }
  throw actual;
}

module.exports = {
  NO_EXCEPTION_SENTINEL,
  getActual,
  waitForActual,
  expectsError,
  expectsNoError,
};
