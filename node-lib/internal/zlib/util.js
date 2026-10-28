'use strict';

// The few internal helpers Node's lib/zlib.js takes from internal/validators,
// internal/util, internal/buffer and internal/async_hooks (v26.8.2).

const {
  NumberIsFinite,
  NumberIsNaN,
  ReflectConstruct,
  SafeSet,
  Symbol,
} = primordials;

const { codes: { ERR_OUT_OF_RANGE } } = require('internal/errors');
const { validateNumber } = require('internal/validators');
const { Buffer } = require('buffer');

const owner_symbol = Symbol('owner_symbol');

// `new FastBuffer()` is an empty Buffer.
function FastBuffer() {
  return Buffer.alloc(0);
}

// 1. Returns false for undefined and NaN
// 2. Returns true for finite numbers
// 3. Throws ERR_INVALID_ARG_TYPE for non-numbers
// 4. Throws ERR_OUT_OF_RANGE for infinite numbers
function validateFiniteNumber(number, name) {
  if (number === undefined) return false;
  if (NumberIsFinite(number)) return true;
  if (NumberIsNaN(number)) return false;
  validateNumber(number, name);
  throw new ERR_OUT_OF_RANGE(name, 'a finite number', number);
}

// 1. Returns def for number when it's undefined or NaN
// 2. Returns number for finite numbers >= lower and <= upper
// 3. Throws ERR_INVALID_ARG_TYPE for non-numbers
// 4. Throws ERR_OUT_OF_RANGE for infinite numbers or numbers > upper or < lower
function checkRangesOrGetDefault(number, name, lower, upper, def) {
  if (!validateFiniteNumber(number, name)) return def;
  if (number < lower || number > upper) {
    throw new ERR_OUT_OF_RANGE(name, `>= ${lower} and <= ${upper}`, number);
  }
  return number;
}

const warned = new SafeSet();
function deprecateInstantiation(target, code, ...args) {
  if (!warned.has(code)) {
    warned.add(code);
    process.emitWarning(
      `Instantiating ${target.name} without the 'new' keyword has been deprecated.`,
      'DeprecationWarning', code, target);
  }
  return ReflectConstruct(target, args);
}

module.exports = {
  FastBuffer,
  checkRangesOrGetDefault,
  deprecateInstantiation,
  owner_symbol,
  validateFiniteNumber,
};
