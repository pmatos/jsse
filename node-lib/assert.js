// Originally from narwhal.js (http://narwhaljs.org)
// Copyright (c) 2009 Thomas Robinson <280north.com>
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the 'Software'), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED 'AS IS', WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN
// ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
// WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

'use strict';

const {
  ArrayPrototypeForEach,
  NumberIsNaN,
  ObjectAssign,
  ObjectDefineProperty,
  ObjectIs,
  Symbol,
} = primordials;

const {
  codes: {
    ERR_CONSTRUCT_CALL_REQUIRED,
    ERR_MISSING_ARGS,
  },
} = require('internal/errors');
const AssertionError = require('internal/assert/assertion_error');
const { isError, setOwnProperty } = require('internal/util');
const { innerOk, innerFail } = require('internal/assert/utils');

const {
  validateOneOf,
} = require('internal/validators');

const kOptions = Symbol('options');

let isDeepEqual;
let isDeepStrictEqual;
let isPartialStrictEqual;

function lazyLoadComparison() {
  const comparison = require('internal/util/comparisons');
  isDeepEqual = comparison.isDeepEqual;
  isDeepStrictEqual = comparison.isDeepStrictEqual;
  isPartialStrictEqual = comparison.isPartialStrictEqual;
}

// The assert module provides functions that throw
// AssertionError's when particular conditions are not met. The
// assert module must conform to the following interface.

module.exports = assert;

/**
 * Assert options.
 * @typedef {object} AssertOptions
 * @property {'full'|'simple'} [diff='simple'] - If set to 'full', shows the full diff in assertion errors.
 * @property {boolean} [strict=true] - If set to true, non-strict methods behave like their corresponding
 *   strict methods.
 * @property {boolean} [skipPrototype=false] - If set to true, skips comparing prototypes
 *   in deep equality checks.
 */

/**
 * @class Assert
 * @param {AssertOptions} [options] - Optional configuration for assertions.
 * @throws {ERR_CONSTRUCT_CALL_REQUIRED} If not called with `new`.
 */
function Assert(options) {
  if (!new.target) {
    throw new ERR_CONSTRUCT_CALL_REQUIRED('Assert');
  }

  options = ObjectAssign({ __proto__: null, strict: true, skipPrototype: false }, options);

  const allowedDiffs = ['simple', 'full'];
  if (options.diff !== undefined) {
    validateOneOf(options.diff, 'options.diff', allowedDiffs);
  }

  this.AssertionError = AssertionError;
  ObjectDefineProperty(this, kOptions, {
    __proto__: null,
    value: options,
    enumerable: false,
    configurable: false,
    writable: false,
  });

  if (options.strict) {
    this.equal = this.strictEqual;
    this.deepEqual = this.deepStrictEqual;
    this.notEqual = this.notStrictEqual;
    this.notDeepEqual = this.notDeepStrictEqual;
  }
}

// All of the following functions must throw an AssertionError
// when a corresponding condition is not met, with a message that
// may be undefined if not provided. All assertion methods provide
// both the actual and expected values to the assertion error for
// display purposes.

// DESTRUCTURING WARNING: All Assert.prototype methods use optional chaining
// (this?.[kOptions]) to safely access instance configuration. When methods are
// destructured from an Assert instance (e.g., const {strictEqual} = myAssert),
// they lose their `this` context and will use default behavior instead of the
// instance's custom options.

/**
 * Throws an AssertionError with the given message.
 * @param {any | Error} [message]
 */
Assert.prototype.fail = function fail(message) {
  if (isError(message)) throw message;

  let internalMessage = false;
  if (message === undefined) {
    message = 'Failed';
    internalMessage = true;
  }

  // IMPORTANT: When adding new references to `this`, ensure they use optional chaining
  // (this?.[kOptions]?.diff) to handle cases where the method is destructured from an
  // Assert instance and loses its context. Destructured methods will fall back
  // to default behavior when `this` is undefined.
  const errArgs = {
    operator: 'fail',
    stackStartFn: fail,
    message,
    diff: this?.[kOptions]?.diff,
  };
  const err = new AssertionError(errArgs);
  if (internalMessage) {
    err.generatedMessage = true;
  }
  throw err;
};

// The AssertionError is defined in internal/error.
assert.AssertionError = AssertionError;

/**
 * Pure assertion tests whether a value is truthy, as determined
 * by !!value.
 * @param {...any} args
 * @returns {void}
 */
function assert(...args) {
  innerOk(assert, ...args);
}

/**
 * Pure assertion tests whether a value is truthy, as determined
 * by !!value.
 * Duplicated as the other `ok` function is supercharged and exposed as default export.
 * @param {...any} args
 * @returns {void}
 */
Assert.prototype.ok = function ok(...args) {
  innerOk(ok, ...args);
};

/**
 * The equality assertion tests shallow, coercive equality with ==.
 * @param {any} actual
 * @param {any} expected
 * @param {string | Error | MessageFactory} [message]
 * @returns {void}
 */
Assert.prototype.equal = function equal(actual, expected, ...message) {
  if (arguments.length < 2) {
    throw new ERR_MISSING_ARGS('actual', 'expected');
  }
  // eslint-disable-next-line eqeqeq
  if (actual != expected && (!NumberIsNaN(actual) || !NumberIsNaN(expected))) {
    innerFail({
      actual,
      expected,
      message,
      operator: '==',
      stackStartFn: equal,
      diff: this?.[kOptions]?.diff,
    });
  }
};

/**
 * The non-equality assertion tests for whether two objects are not
 * equal with !=.
 * @param {any} actual
 * @param {any} expected
 * @param {string | Error | MessageFactory} [message]
 * @returns {void}
 */
Assert.prototype.notEqual = function notEqual(actual, expected, ...message) {
  if (arguments.length < 2) {
    throw new ERR_MISSING_ARGS('actual', 'expected');
  }
  // eslint-disable-next-line eqeqeq
  if (actual == expected || (NumberIsNaN(actual) && NumberIsNaN(expected))) {
    innerFail({
      actual,
      expected,
      message,
      operator: '!=',
      stackStartFn: notEqual,
      diff: this?.[kOptions]?.diff,
    });
  }
};

/**
 * The deep equivalence assertion tests a deep equality relation.
 * @param {any} actual
 * @param {any} expected
 * @param {string | Error | MessageFactory} [message]
 * @returns {void}
 */
Assert.prototype.deepEqual = function deepEqual(actual, expected, ...message) {
  if (arguments.length < 2) {
    throw new ERR_MISSING_ARGS('actual', 'expected');
  }
  if (isDeepEqual === undefined) lazyLoadComparison();
  if (!isDeepEqual(actual, expected)) {
    innerFail({
      actual,
      expected,
      message,
      operator: 'deepEqual',
      stackStartFn: deepEqual,
      diff: this?.[kOptions]?.diff,
    });
  }
};

/**
 * The deep non-equivalence assertion tests for any deep inequality.
 * @param {any} actual
 * @param {any} expected
 * @param {string | Error | MessageFactory} [message]
 * @returns {void}
 */
Assert.prototype.notDeepEqual = function notDeepEqual(actual, expected, ...message) {
  if (arguments.length < 2) {
    throw new ERR_MISSING_ARGS('actual', 'expected');
  }
  if (isDeepEqual === undefined) lazyLoadComparison();
  if (isDeepEqual(actual, expected)) {
    innerFail({
      actual,
      expected,
      message,
      operator: 'notDeepEqual',
      stackStartFn: notDeepEqual,
      diff: this?.[kOptions]?.diff,
    });
  }
};

/**
 * The deep strict equivalence assertion tests a deep strict equality
 * relation.
 * @param {any} actual
 * @param {any} expected
 * @param {string | Error | MessageFactory} [message]
 * @returns {void}
 */
Assert.prototype.deepStrictEqual = function deepStrictEqual(actual, expected, ...message) {
  if (arguments.length < 2) {
    throw new ERR_MISSING_ARGS('actual', 'expected');
  }
  if (isDeepEqual === undefined) lazyLoadComparison();
  if (!isDeepStrictEqual(actual, expected, this?.[kOptions]?.skipPrototype)) {
    innerFail({
      actual,
      expected,
      message,
      operator: 'deepStrictEqual',
      stackStartFn: deepStrictEqual,
      diff: this?.[kOptions]?.diff,
    });
  }
};

/**
 * The deep strict non-equivalence assertion tests for any deep strict
 * inequality.
 * @param {any} actual
 * @param {any} expected
 * @param {string | Error | MessageFactory} [message]
 * @returns {void}
 */
Assert.prototype.notDeepStrictEqual = notDeepStrictEqual;
function notDeepStrictEqual(actual, expected, ...message) {
  if (arguments.length < 2) {
    throw new ERR_MISSING_ARGS('actual', 'expected');
  }
  if (isDeepEqual === undefined) lazyLoadComparison();
  if (isDeepStrictEqual(actual, expected, this?.[kOptions]?.skipPrototype)) {
    innerFail({
      actual,
      expected,
      message,
      operator: 'notDeepStrictEqual',
      stackStartFn: notDeepStrictEqual,
      diff: this?.[kOptions]?.diff,
    });
  }
}

/**
 * The strict equivalence assertion tests a strict equality relation.
 * @param {any} actual
 * @param {any} expected
 * @param {string | Error | MessageFactory} [message]
 * @returns {void}
 */
Assert.prototype.strictEqual = function strictEqual(actual, expected, ...message) {
  if (arguments.length < 2) {
    throw new ERR_MISSING_ARGS('actual', 'expected');
  }
  if (!ObjectIs(actual, expected)) {
    innerFail({
      actual,
      expected,
      message,
      operator: 'strictEqual',
      stackStartFn: strictEqual,
      diff: this?.[kOptions]?.diff,
    });
  }
};

/**
 * The strict non-equivalence assertion tests for any strict inequality.
 * @param {any} actual
 * @param {any} expected
 * @param {string | Error | MessageFactory} [message]
 * @returns {void}
 */
Assert.prototype.notStrictEqual = function notStrictEqual(actual, expected, ...message) {
  if (arguments.length < 2) {
    throw new ERR_MISSING_ARGS('actual', 'expected');
  }
  if (ObjectIs(actual, expected)) {
    innerFail({
      actual,
      expected,
      message,
      operator: 'notStrictEqual',
      stackStartFn: notStrictEqual,
      diff: this?.[kOptions]?.diff,
    });
  }
};

/**
 * The strict equivalence assertion test between two objects
 * @param {any} actual
 * @param {any} expected
 * @param {string | Error | MessageFactory} [message]
 * @returns {void}
 */
Assert.prototype.partialDeepStrictEqual = function partialDeepStrictEqual(
  actual,
  expected,
  ...message
) {
  if (arguments.length < 2) {
    throw new ERR_MISSING_ARGS('actual', 'expected');
  }
  if (isDeepEqual === undefined) lazyLoadComparison();
  if (!isPartialStrictEqual(actual, expected)) {
    innerFail({
      actual,
      expected,
      message,
      operator: 'partialDeepStrictEqual',
      stackStartFn: partialDeepStrictEqual,
      diff: this?.[kOptions]?.diff,
    });
  }
};

require('internal/assert/methods')(Assert, kOptions);

/**
 * Expose a strict only variant of assert.
 * @param {...any} args
 * @returns {void}
 */
function strict(...args) {
  innerOk(strict, ...args);
}

ArrayPrototypeForEach([
  'ok', 'fail', 'equal', 'notEqual', 'deepEqual', 'notDeepEqual',
  'deepStrictEqual', 'notDeepStrictEqual', 'strictEqual',
  'notStrictEqual', 'partialDeepStrictEqual', 'match', 'doesNotMatch',
  'throws', 'rejects', 'doesNotThrow', 'doesNotReject', 'ifError',
], (name) => {
  setOwnProperty(assert, name, Assert.prototype[name]);
});

assert.strict = ObjectAssign(strict, assert, {
  equal: assert.strictEqual,
  deepEqual: assert.deepStrictEqual,
  notEqual: assert.notStrictEqual,
  notDeepEqual: assert.notDeepStrictEqual,
});

assert.strict.Assert = Assert;
assert.strict.strict = assert.strict;

assert.Assert = Assert;
