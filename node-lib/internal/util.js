'use strict';

const { ObjectFreeze, SymbolFor } = primordials;

function getLazy(initializer) {
  let value;
  let initialized = false;
  return function () {
    if (!initialized) {
      value = initializer();
      initialized = true;
    }
    return value;
  };
}

const kEmptyObject = ObjectFreeze({ __proto__: null });
const kEnumerableProperty = { __proto__: null, enumerable: true };
ObjectFreeze(kEnumerableProperty);

function spliceOne(list, index) {
  for (; index + 1 < list.length; index++)
    list[index] = list[index + 1];
  list.pop();
}

let DOMException;
function lazyDOMExceptionClass() {
  DOMException ??= require('internal/per_context/domexception').DOMException;
  return DOMException;
}

function lazyDOMException(message, name) {
  return new (lazyDOMExceptionClass())(message, name);
}

module.exports = {
  customInspectSymbol: SymbolFor('nodejs.util.inspect.custom'),
  getLazy,
  isMacOS: process.platform === 'darwin',
  isWindows: process.platform === 'win32',
  kEmptyObject,
  kEnumerableProperty,
  lazyDOMException,
  lazyDOMExceptionClass,
  spliceOne,
};
