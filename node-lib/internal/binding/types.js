'use strict';

// The JavaScript stand-in for Node's `internalBinding('types')`: each
// predicate asks the engine for an object's internal-slot brand.

const { kind } = require('internal/host/util');

function is(name) {
  return function(value) {
    return kind(value) === name;
  };
}

function isAsyncFunction(value) {
  const k = kind(value);
  return k === 'AsyncFunction' || k === 'AsyncGeneratorFunction';
}

function isGeneratorFunction(value) {
  const k = kind(value);
  return k === 'GeneratorFunction' || k === 'AsyncGeneratorFunction';
}

function isBoxedPrimitive(value) {
  switch (kind(value)) {
    case 'BooleanObject':
    case 'NumberObject':
    case 'StringObject':
    case 'SymbolObject':
    case 'BigIntObject':
      return true;
    default:
      return false;
  }
}

function isAnyArrayBuffer(value) {
  const k = kind(value);
  return k === 'ArrayBuffer' || k === 'SharedArrayBuffer';
}

module.exports = {
  isArgumentsObject: is('Arguments'),
  isArrayBuffer: is('ArrayBuffer'),
  isAsyncFunction,
  isBigIntObject: is('BigIntObject'),
  isBooleanObject: is('BooleanObject'),
  isDate: is('Date'),
  isExternal() { return false; },
  isGeneratorFunction,
  isGeneratorObject: is('GeneratorObject'),
  isMap: is('Map'),
  isMapIterator: is('MapIterator'),
  isModuleNamespaceObject: is('ModuleNamespace'),
  isNativeError: is('Error'),
  isNumberObject: is('NumberObject'),
  isPromise: is('Promise'),
  isProxy: is('Proxy'),
  isRegExp: is('RegExp'),
  isSet: is('Set'),
  isSetIterator: is('SetIterator'),
  isSharedArrayBuffer: is('SharedArrayBuffer'),
  isStringObject: is('StringObject'),
  isSymbolObject: is('SymbolObject'),
  isWeakMap: is('WeakMap'),
  isWeakSet: is('WeakSet'),
  isAnyArrayBuffer,
  isBoxedPrimitive,
};
