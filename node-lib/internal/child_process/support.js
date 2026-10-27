'use strict';

const {
  ObjectKeys,
  StringPrototypeIncludes,
  StringPrototypeToUpperCase,
} = primordials;

const {
  codes: {
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_ARG_VALUE,
    ERR_UNKNOWN_SIGNAL,
  },
} = require('internal/errors');
const { getSystemErrorName } = require('internal/util');

function ErrnoException(err, syscall, original) {
  const code = getSystemErrorName(err);
  const message = original ? `${syscall} ${code} ${original}` : `${syscall} ${code}`;
  const ex = new Error(message);
  ex.errno = err;
  ex.code = code;
  ex.syscall = syscall;
  return ex;
}

let signals;
let signalNames;

function getSignals() {
  signals ??= require('os').constants.signals;
  return signals;
}

function signalName(number) {
  if (signalNames === undefined) {
    signalNames = { __proto__: null };
    const table = getSignals();
    for (const key of ObjectKeys(table)) {
      if (signalNames[table[key]] === undefined) signalNames[table[key]] = key;
    }
  }
  return signalNames[number];
}

function convertToValidSignal(signal) {
  if (typeof signal === 'number' && signalName(signal) !== undefined) return signal;
  if (typeof signal === 'string') {
    const number = getSignals()[StringPrototypeToUpperCase(signal)];
    if (number) return number;
  }
  throw new ERR_UNKNOWN_SIGNAL(signal);
}

function getValidatedPath(path, name) {
  if (typeof path === 'object' && path !== null) {
    if (typeof path.href === 'string' && path.protocol === 'file:') {
      path = require('url').fileURLToPath(path);
    } else if (typeof path.toString === 'function' && path.buffer !== undefined) {
      path = path.toString();
    }
  }
  if (typeof path !== 'string') {
    throw new ERR_INVALID_ARG_TYPE(name, ['string', 'Buffer', 'URL'], path);
  }
  if (StringPrototypeIncludes(path, '\u0000')) {
    throw new ERR_INVALID_ARG_VALUE(name, path, 'must be a string, Uint8Array, or URL without null bytes');
  }
  return path;
}

module.exports = {
  ErrnoException,
  convertToValidSignal,
  getSignals,
  getValidatedPath,
  signalName,
};
