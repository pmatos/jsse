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
  isExternal() { return false; },
  isDate: is('Date'),
  isArgumentsObject: is('Arguments'),
  isBigIntObject: is('BigIntObject'),
  isBooleanObject: is('BooleanObject'),
  isNumberObject: is('NumberObject'),
  isStringObject: is('StringObject'),
  isSymbolObject: is('SymbolObject'),
  isBoxedPrimitive,
  isNativeError: is('Error'),
  isRegExp: is('RegExp'),
  isAsyncFunction,
  isGeneratorFunction,
  isGeneratorObject: is('GeneratorObject'),
  isPromise: is('Promise'),
  isMap: is('Map'),
  isSet: is('Set'),
  isMapIterator: is('MapIterator'),
  isSetIterator: is('SetIterator'),
  isWeakMap: is('WeakMap'),
  isWeakSet: is('WeakSet'),
  isArrayBuffer: is('ArrayBuffer'),
  isDataView: is('DataView'),
  isSharedArrayBuffer: is('SharedArrayBuffer'),
  isProxy: is('Proxy'),
  isModuleNamespaceObject: is('ModuleNamespace'),
  isAnyArrayBuffer,
};
