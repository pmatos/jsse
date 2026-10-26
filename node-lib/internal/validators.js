'use strict';

const {
  ArrayIsArray,
  ArrayPrototypeIncludes,
  ArrayPrototypeJoin,
  ArrayPrototypeMap,
  NumberIsInteger,
  NumberIsNaN,
  NumberMAX_SAFE_INTEGER,
  NumberMIN_SAFE_INTEGER,
  ObjectPrototypeHasOwnProperty,
} = primordials;
const {
  ERR_INVALID_ARG_TYPE,
  ERR_INVALID_ARG_VALUE,
  ERR_INVALID_THIS,
  ERR_OUT_OF_RANGE,
} = require('internal/errors').codes;

const kValidateObjectNone = 0;
const kValidateObjectAllowNullable = 1 << 0;
const kValidateObjectAllowArray = 1 << 1;
const kValidateObjectAllowFunction = 1 << 2;
const kValidateObjectAllowObjects = kValidateObjectAllowArray |
  kValidateObjectAllowFunction;

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

function validateInt32(value, name, min = -2147483648, max = 2147483647) {
  validateInteger(value, name, min, max);
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

function validateOneOf(value, name, oneOf) {
  if (!ArrayPrototypeIncludes(oneOf, value)) {
    const allowed = ArrayPrototypeJoin(
      ArrayPrototypeMap(oneOf, (v) =>
        (typeof v === 'string' ? `'${v}'` : String(v))),
      ', ');
    const reason = 'must be one of: ' + allowed;
    throw new ERR_INVALID_ARG_VALUE(name, value, reason);
  }
}

function validateArray(value, name, minLength = 0) {
  if (!ArrayIsArray(value)) {
    throw new ERR_INVALID_ARG_TYPE(name, 'Array', value);
  }
  if (value.length < minLength) {
    const reason = `must have a length of at least ${minLength}`;
    throw new ERR_INVALID_ARG_VALUE(name, value, reason);
  }
}

function validateStringArray(value, name) {
  validateArray(value, name);
  for (let i = 0; i < value.length; ++i) {
    if (typeof value[i] !== 'string') {
      throw new ERR_INVALID_ARG_TYPE(`${name}[${i}]`, 'string', value[i]);
    }
  }
}

function validateBooleanArray(value, name) {
  validateArray(value, name);
  for (let i = 0; i < value.length; ++i) {
    if (value[i] !== true && value[i] !== false) {
      throw new ERR_INVALID_ARG_TYPE(`${name}[${i}]`, 'boolean', value[i]);
    }
  }
}

function validateUnion(value, name, union) {
  if (!ArrayPrototypeIncludes(union, value)) {
    throw new ERR_INVALID_ARG_TYPE(name, `('${ArrayPrototypeJoin(union, '|')}')`, value);
  }
}

module.exports = {
  kValidateObjectAllowArray,
  validateArray,
  validateBooleanArray,
  validateOneOf,
  validateStringArray,
  validateUnion,
  kValidateObjectAllowObjects,
  validateAbortSignal,
  validateBoolean,
  validateFunction,
  validateInt32,
  validateInteger,
  validateNumber,
  validateObject,
  validateString,
  validateThisInternalField,
  validateUint32,
};
