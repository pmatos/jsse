'use strict';

// The `subtle` half of Web Crypto for `crypto.webcrypto` and the global
// `crypto`. Only `digest` is implemented (SHA-1, SHA-2 and SHA-3 as in Node
// v26.8.2); the other SubtleCrypto methods reject with NotSupportedError.
// Error shapes follow Node's webidl layer (lib/internal/crypto/webcrypto.js).

const {
  ArrayBufferIsView,
  PromiseReject,
  StringPrototypeToLowerCase,
  TypedArrayPrototypeGetBuffer,
  TypedArrayPrototypeGetByteLength,
  TypedArrayPrototypeGetByteOffset,
} = primordials;

const native = require('internal/host/crypto');
const { lazyDOMException } = require('internal/util');
const { codes: { ERR_INVALID_THIS } } = require('internal/errors');
const { isArrayBuffer } = require('internal/util/types');

const kDigests = {
  'sha-1': 'sha1',
  'sha-256': 'sha256',
  'sha-384': 'sha384',
  'sha-512': 'sha512',
  'sha3-256': 'sha3-256',
  'sha3-384': 'sha3-384',
  'sha3-512': 'sha3-512',
};

function codedTypeError(message, code) {
  const err = new TypeError(message);
  err.code = code;
  return err;
}

function normalizeAlgorithm(algorithm) {
  let name = algorithm;
  if (typeof algorithm === 'object' && algorithm !== null) {
    name = algorithm.name;
    if (name === undefined) {
      throw codedTypeError(
        'Failed to normalize algorithm: passed algorithm cannot be converted to ' +
        '\'Algorithm\' because \'name\' is required in \'Algorithm\'.',
        'ERR_MISSING_OPTION');
    }
  }
  const digest = kDigests[StringPrototypeToLowerCase(`${name}`)];
  if (digest === undefined) {
    throw lazyDOMException('Unrecognized algorithm name', 'NotSupportedError');
  }
  return digest;
}

class SubtleCrypto {
  digest(algorithm, data) {
    try {
      if (this !== subtle) {
        throw new ERR_INVALID_THIS('SubtleCrypto');
      }
      if (arguments.length < 2) {
        throw codedTypeError(
          `Failed to execute 'digest' on 'SubtleCrypto': 2 arguments required, ` +
          `but only ${arguments.length} present.`, 'ERR_MISSING_ARGS');
      }
      const name = normalizeAlgorithm(algorithm);
      let bytes;
      if (isArrayBuffer(data)) {
        bytes = new Uint8Array(data);
      } else if (ArrayBufferIsView(data)) {
        bytes = new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
      } else {
        throw codedTypeError(
          'Failed to execute \'digest\' on \'SubtleCrypto\': 2nd argument is not ' +
          'instance of ArrayBuffer, Buffer, TypedArray, or DataView.',
          'ERR_INVALID_ARG_TYPE');
      }
      const handle = native.create(name);
      native.update(handle, bytes);
      const out = native.digest(handle);
      return Promise.resolve(TypedArrayPrototypeGetBuffer(out).slice(
        TypedArrayPrototypeGetByteOffset(out),
        TypedArrayPrototypeGetByteOffset(out) + TypedArrayPrototypeGetByteLength(out)));
    } catch (err) {
      return PromiseReject(err);
    }
  }
}

for (const method of [
  'encrypt', 'decrypt', 'sign', 'verify', 'generateKey', 'deriveKey', 'deriveBits',
  'importKey', 'exportKey', 'wrapKey', 'unwrapKey',
]) {
  Object.defineProperty(SubtleCrypto.prototype, method, {
    value: function() {
      return PromiseReject(lazyDOMException(
        `SubtleCrypto.${method} is not supported`, 'NotSupportedError'));
    },
    writable: true,
    enumerable: true,
    configurable: true,
  });
}

Object.defineProperty(SubtleCrypto.prototype, Symbol.toStringTag, {
  value: 'SubtleCrypto',
  configurable: true,
});

const subtle = new SubtleCrypto();

module.exports = { SubtleCrypto, subtle };
