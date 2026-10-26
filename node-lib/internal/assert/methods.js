'use strict';

const {
  ArrayPrototypeIndexOf,
  ArrayPrototypeJoin,
  ArrayPrototypeSlice,
  RegExpPrototypeExec,
  StringPrototypeIndexOf,
  StringPrototypeSlice,
  StringPrototypeSplit,
} = primordials;

const {
  codes: {
    ERR_INVALID_ARG_TYPE,
  },
} = require('internal/errors');
const AssertionError = require('internal/assert/assertion_error');
const { inspect } = require('internal/util/inspect');
const { isRegExp } = require('internal/util/types');
const { innerFail } = require('internal/assert/utils');
const {
  getActual,
  waitForActual,
  expectsError,
  expectsNoError,
} = require('internal/assert/exceptions');

function installMethods(Assert, kOptions) {
  /**
   * Expects the function `promiseFn` to throw an error.
   * @param {() => any} promiseFn
   * @param {...any} [args]
   * @returns {void}
   */
  Assert.prototype.throws = function throws(promiseFn, ...args) {
    expectsError(throws, getActual(promiseFn), ...args);
  };

  /**
   * Expects `promiseFn` function or its value to reject.
   * @param {() => Promise<any>} promiseFn
   * @param {...any} [args]
   * @returns {Promise<void>}
   */
  Assert.prototype.rejects = async function rejects(promiseFn, ...args) {
    expectsError(rejects, await waitForActual(promiseFn), ...args);
  };

  /**
   * Asserts that the function `fn` does not throw an error.
   * @param {() => any} fn
   * @param {...any} [args]
   * @returns {void}
   */
  Assert.prototype.doesNotThrow = function doesNotThrow(fn, ...args) {
    expectsNoError(doesNotThrow, getActual(fn), ...args);
  };

  /**
   * Expects `fn` or its value to not reject.
   * @param {() => Promise<any>} fn
   * @param {...any} [args]
   * @returns {Promise<void>}
   */
  Assert.prototype.doesNotReject = async function doesNotReject(fn, ...args) {
    expectsNoError(doesNotReject, await waitForActual(fn), ...args);
  };

  /**
   * Throws `AssertionError` if the value is not `null` or `undefined`.
   * @param {any} err
   * @returns {void}
   */
  Assert.prototype.ifError = function ifError(err) {
    if (err !== null && err !== undefined) {
      let message = 'ifError got unwanted exception: ';
      if (typeof err === 'object' && typeof err.message === 'string') {
        if (err.message.length === 0 && err.constructor) {
          message += err.constructor.name;
        } else {
          message += err.message;
        }
      } else {
        message += inspect(err);
      }

      const newErr = new AssertionError({
        actual: err,
        expected: null,
        operator: 'ifError',
        message,
        stackStartFn: ifError,
        diff: this?.[kOptions]?.diff,
      });

      // Make sure we actually have a stack trace!
      const origStack = err.stack;

      if (typeof origStack === 'string') {
        // This will remove any duplicated frames from the error frames taken
        // from within `ifError` and add the original error frames to the newly
        // created ones.
        const origStackStart = StringPrototypeIndexOf(origStack, '\n    at');
        if (origStackStart !== -1) {
          const originalFrames = StringPrototypeSplit(
            StringPrototypeSlice(origStack, origStackStart + 1),
            '\n',
          );
          // Filter all frames existing in err.stack.
          let newFrames = StringPrototypeSplit(newErr.stack, '\n');
          for (const errFrame of originalFrames) {
            // Find the first occurrence of the frame.
            const pos = ArrayPrototypeIndexOf(newFrames, errFrame);
            if (pos !== -1) {
              // Only keep new frames.
              newFrames = ArrayPrototypeSlice(newFrames, 0, pos);
              break;
            }
          }
          const stackStart = ArrayPrototypeJoin(newFrames, '\n');
          const stackEnd = ArrayPrototypeJoin(originalFrames, '\n');
          newErr.stack = `${stackStart}\n${stackEnd}`;
        }
      }

      throw newErr;
    }
  };

  function internalMatch(string, regexp, message, fn) {
    if (!isRegExp(regexp)) {
      throw new ERR_INVALID_ARG_TYPE(
        'regexp', 'RegExp', regexp,
      );
    }
    const match = fn === Assert.prototype.match;
    if (typeof string !== 'string' ||
        RegExpPrototypeExec(regexp, string) !== null !== match) {

      const generatedMessage = message.length === 0;

      // 'The input was expected to not match the regular expression ' +
      message[0] ||= (typeof string !== 'string' ?
        'The "string" argument must be of type string. Received type ' +
          `${typeof string} (${inspect(string)})` :
        (match ?
          'The input did not match the regular expression ' :
          'The input was expected to not match the regular expression ') +
            `${inspect(regexp)}. Input:\n\n${inspect(string)}\n`);
      innerFail({
        actual: string,
        expected: regexp,
        message,
        operator: fn.name,
        stackStartFn: fn,
        diff: this?.[kOptions]?.diff,
        generatedMessage: generatedMessage,
      });
    }
  }

  /**
   * Expects the `string` input to match the regular expression.
   * @param {string} string
   * @param {RegExp} regexp
   * @param {string | Error | MessageFactory} [message]
   * @returns {void}
   */
  Assert.prototype.match = function match(string, regexp, ...message) {
    internalMatch(string, regexp, message, match);
  };

  /**
   * Expects the `string` input not to match the regular expression.
   * @param {string} string
   * @param {RegExp} regexp
   * @param {string | Error | MessageFactory} [message]
   * @returns {void}
   */
  Assert.prototype.doesNotMatch = function doesNotMatch(string, regexp, ...message) {
    internalMatch(string, regexp, message, doesNotMatch);
  };
}

module.exports = installMethods;
