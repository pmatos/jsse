'use strict';

// The slice of Node's lib/internal/webidl.js that EventTarget and AbortSignal
// use: DOMString, interface and sequence converters.

const {
  ArrayPrototypePush,
  FunctionPrototypeCall,
  ObjectPrototypeIsPrototypeOf,
  String,
  SymbolIterator,
  TypeError,
} = primordials;
const { kEmptyObject } = require('internal/util');

const converters = { __proto__: null };

function makeException(message, options = kEmptyObject) {
  const prefix = options.prefix ? options.prefix + ': ' : '';
  const context = options.context?.length === 0 ?
    '' : (options.context ?? 'Value') + ' ';
  const err = new TypeError(`${prefix}${context}${message}`);
  err.code = options.code || 'ERR_INVALID_ARG_TYPE';
  return err;
}

function type(V) {
  switch (typeof V) {
    case 'undefined': return 'Undefined';
    case 'boolean': return 'Boolean';
    case 'string': return 'String';
    case 'symbol': return 'Symbol';
    case 'number': return 'Number';
    case 'bigint': return 'BigInt';
    default: return V === null ? 'Null' : 'Object';
  }
}

converters.DOMString = function DOMString(V, options = kEmptyObject) {
  if (typeof V === 'symbol') {
    throw makeException(
      'is a Symbol and cannot be converted to a string.',
      options);
  }
  return String(V);
};

function createSequenceConverter(converter) {
  return function(V, options = kEmptyObject) {
    if (type(V) !== 'Object') {
      throw makeException('cannot be converted to sequence.', options);
    }
    const method = V[SymbolIterator];
    if (typeof method !== 'function') {
      throw makeException('cannot be converted to sequence.', options);
    }
    const iterator = FunctionPrototypeCall(method, V);
    const nextMethod = iterator?.next;
    if (typeof nextMethod !== 'function') {
      throw makeException('cannot be converted to sequence.', options);
    }
    const idlSequence = [];
    while (true) {
      const next = FunctionPrototypeCall(nextMethod, iterator);
      if (type(next) !== 'Object') {
        throw makeException('cannot be converted to sequence.', options);
      }
      if (next.done) {
        break;
      }
      ArrayPrototypePush(idlSequence, converter(next.value, {
        prefix: options.prefix,
        context: `${options.context ?? 'Value'}[${idlSequence.length}]`,
        code: options.code,
      }));
    }
    return idlSequence;
  };
}

function createInterfaceConverter(name, prototype) {
  return (V, options = kEmptyObject) => {
    if (ObjectPrototypeIsPrototypeOf(prototype, V)) {
      return V;
    }
    throw makeException(`is not of type ${name}.`, options);
  };
}

module.exports = {
  converters,
  createInterfaceConverter,
  createSequenceConverter,
};
