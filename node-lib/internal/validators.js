'use strict';

const {
  ArrayIsArray,
  NumberIsInteger,
  NumberIsNaN,
  NumberMAX_SAFE_INTEGER,
  NumberMIN_SAFE_INTEGER,
  ObjectPrototypeHasOwnProperty,
} = primordials;
const {
  ERR_INVALID_ARG_TYPE,
  ERR_INVALID_THIS,
  ERR_OUT_OF_RANGE,
} = require('internal/errors').codes;

const kValidateObjectNone = 0;
const kValidateObjectAllowNullable = 1 << 0;
const kValidateObjectAllowArray = 1 << 1;
const kValidateObjectAllowFunction = 1 << 2;
const kValidateObjectAllowObjects = kValidateObjectAllowArray |
  kValidateObjectAllowFunction;
const { ArrayIsArray } = primordials;
const { ERR_INVALID_ARG_TYPE, ERR_OUT_OF_RANGE } = require('internal/errors').codes;

function validateString(value, name) {
  if (typeof value !== 'string')
    throw new ERR_INVALID_ARG_TYPE(name, 'string', value);
}

function validateObject(value, name, options = kValidateObjectNone) {
  if (options === kValidateObjectNone) {
    if (value === null || ArrayIsArray(value) || typeof value !== 'object') {
      throw new ERR_INVALID_ARG_TYPE(name, 'Object', value);
    }
    return;
  }
  const throwOnNullable = (kValidateObjectAllowNullable & options) === 0;
  if (throwOnNullable && value === null) {
function validateInt32(value, name, min = -2147483648, max = 2147483647) {
  if (typeof value !== 'number')
    throw new ERR_INVALID_ARG_TYPE(name, 'number', value);
  if (!primordials.NumberIsInteger(value))
    throw new ERR_OUT_OF_RANGE(name, 'an integer', value);
  if (value < min || value > max)
    throw new ERR_OUT_OF_RANGE(name, `>= ${min} && <= ${max}`, value);
}

function validateObject(value, name) {
  if (value === null || ArrayIsArray(value) || typeof value !== 'object')
    throw new ERR_INVALID_ARG_TYPE(name, 'Object', value);
  }
  const throwOnArray = (kValidateObjectAllowArray & options) === 0;
  if (throwOnArray && ArrayIsArray(value)) {
    throw new ERR_INVALID_ARG_TYPE(name, 'Object', value);
  }
  const throwOnFunction = (kValidateObjectAllowFunction & options) === 0;
  const typeofValue = typeof value;
  if (typeofValue !== 'object' && (throwOnFunction || typeofValue !== 'function')) {
    throw new ERR_INVALID_ARG_TYPE(name, 'Object', value);
  }
}

function validateInteger(
  value, name, min = NumberMIN_SAFE_INTEGER, max = NumberMAX_SAFE_INTEGER) {
  if (typeof value !== 'number')
    throw new ERR_INVALID_ARG_TYPE(name, 'number', value);
  if (!NumberIsInteger(value))
    throw new ERR_OUT_OF_RANGE(name, 'an integer', value);
  if (value < min || value > max)
    throw new ERR_OUT_OF_RANGE(name, `>= ${min} && <= ${max}`, value);
}

function validateUint32(value, name, positive = false) {
  if (typeof value !== 'number') {
    throw new ERR_INVALID_ARG_TYPE(name, 'number', value);
  }
  if (!NumberIsInteger(value)) {
    throw new ERR_OUT_OF_RANGE(name, 'an integer', value);
  }
  const min = positive ? 1 : 0;
  const max = 4_294_967_295;
  if (value < min || value > max) {
    throw new ERR_OUT_OF_RANGE(name, `>= ${min} && <= ${max}`, value);
  }
}

function validateNumber(value, name, min = undefined, max) {
  if (typeof value !== 'number')
    throw new ERR_INVALID_ARG_TYPE(name, 'number', value);

  if ((min != null && value < min) || (max != null && value > max) ||
    ((min != null || max != null) && NumberIsNaN(value))) {
    throw new ERR_OUT_OF_RANGE(
      name,
      `${min != null ? `>= ${min}` : ''}${min != null && max != null ? ' && ' : ''}${max != null ? `<= ${max}` : ''}`,
      value);
  }
}

function validateBoolean(value, name) {
  if (typeof value !== 'boolean')
    throw new ERR_INVALID_ARG_TYPE(name, 'boolean', value);
}

function validateFunction(value, name) {
  if (typeof value !== 'function')
    throw new ERR_INVALID_ARG_TYPE(name, 'Function', value);
}

function validateAbortSignal(signal, name) {
  if (signal !== undefined &&
      (signal === null ||
       typeof signal !== 'object' ||
       !('aborted' in signal))) {
    throw new ERR_INVALID_ARG_TYPE(name, 'AbortSignal', signal);
  }
}

function validateThisInternalField(object, fieldKey, className) {
  if (typeof object !== 'object' || object === null || !ObjectPrototypeHasOwnProperty(object, fieldKey)) {
    throw new ERR_INVALID_THIS(className);
  }
}

module.exports = {
  kValidateObjectAllowObjects,
  validateAbortSignal,
  validateBoolean,
  validateFunction,
  validateInteger,
  validateNumber,
  validateInt32,
  validateObject,
  validateString,
  validateThisInternalField,
  validateUint32,
};
