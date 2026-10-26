'use strict';

const { ArrayIsArray } = primordials;
const { ERR_INVALID_ARG_TYPE } = require('internal/errors').codes;

function validateString(value, name) {
  if (typeof value !== 'string')
    throw new ERR_INVALID_ARG_TYPE(name, 'string', value);
}

function validateObject(value, name) {
  if (value === null || ArrayIsArray(value) || typeof value !== 'object')
    throw new ERR_INVALID_ARG_TYPE(name, 'Object', value);
}

module.exports = {
  validateObject,
  validateString,
};
