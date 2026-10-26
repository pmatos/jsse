'use strict';

// Small helpers shared by the url modules (the parts of Node's
// internal/util and internal/validators they use).

const {
  ArrayIsArray,
  ArrayPrototypeSymbolIterator,
  ObjectFreeze,
  ObjectGetOwnPropertyDescriptor,
  ObjectGetPrototypeOf,
  SymbolFor,
} = primordials;
const { ERR_INVALID_ARG_TYPE } = require('internal/errors').codes;

const kInspect = SymbolFor('nodejs.util.inspect.custom');
const kEnumerableProperty = { __proto__: null, enumerable: true };
ObjectFreeze(kEnumerableProperty);
const kEmptyObject = ObjectFreeze({ __proto__: null });

// eslint-disable-next-line no-control-regex
const colorRegExp = /\u001b\[\d\d?m/g;

function removeColors(str) {
  return str.replace(colorRegExp, '');
}

function getConstructorOf(obj) {
  while (obj) {
    const descriptor = ObjectGetOwnPropertyDescriptor(obj, 'constructor');
    if (descriptor !== undefined &&
        typeof descriptor.value === 'function' &&
        descriptor.value.name !== '') {
      return descriptor.value;
    }
    obj = ObjectGetPrototypeOf(obj);
  }
  return null;
}

function validateFunction(value, name) {
  if (typeof value !== 'function')
    throw new ERR_INVALID_ARG_TYPE(name, 'Function', value);
}

function validateObjectAllowObjects(value, name) {
  if (value === null || typeof value !== 'object' && typeof value !== 'function')
    throw new ERR_INVALID_ARG_TYPE(name, 'Object', value);
}

function validateString(value, name) {
  if (typeof value !== 'string')
    throw new ERR_INVALID_ARG_TYPE(name, 'string', value);
}

function validateObject(value, name) {
  if (value === null || ArrayIsArray(value) || typeof value !== 'object')
    throw new ERR_INVALID_ARG_TYPE(name, 'Object', value);
}

function spliceOne(list, index) {
  for (; index + 1 < list.length; index++)
    list[index] = list[index + 1];
  list.pop();
}

const IteratorPrototype = ObjectGetPrototypeOf(
  ObjectGetPrototypeOf(ArrayPrototypeSymbolIterator([])));

// State the URL and URLSearchParams classes hand to each other: they live in
// separate files but need each other's private fields.
const hooks = { __proto__: null };

module.exports = {
  IteratorPrototype,
  getConstructorOf,
  hooks,
  kEmptyObject,
  kEnumerableProperty,
  kInspect,
  removeColors,
  spliceOne,
  validateFunction,
  validateObject,
  validateObjectAllowObjects,
  validateString,
};
