'use strict';

// Node's `crypto` for `--node` (issue #1226): hashing, HMAC, random and
// `webcrypto.subtle.digest`, over the native `__host_crypto` binding. Ciphers,
// keys, signatures, KDFs (pbkdf2, scrypt, hkdf) and `KeyObject` are not
// provided, and `Hash`/`Hmac` are not streams. See
// docs/adr/2026-10-26-1930-node-host-crypto.md.

const {
  ArrayBufferIsView,
  ObjectDefineProperties,
} = primordials;

const native = require('internal/host/crypto');
const { Hash, Hmac, hash, kHashes } = require('internal/crypto/hash');
const {
  getRandomValues,
  randomBytes,
  randomFill,
  randomFillSync,
  randomInt,
  randomUUID,
} = require('internal/crypto/random');
const { codes: { ERR_CRYPTO_TIMING_SAFE_EQUAL_LENGTH } } = require('internal/errors');
const { isAnyArrayBuffer } = require('internal/util/types');

function createHash(algorithm, options) {
  return new Hash(algorithm, options);
}

function createHmac(hmac, key, options) {
  return new Hmac(hmac, key, options);
}

function getHashes() {
  return kHashes.slice();
}

function timingSafeEqual(buf1, buf2) {
  for (const [name, value] of [['buf1', buf1], ['buf2', buf2]]) {
    if (!isAnyArrayBuffer(value) && !ArrayBufferIsView(value)) {
      const err = new TypeError(
        `The "${name}" argument must be an instance of ArrayBuffer, Buffer, TypedArray, or DataView.`);
      err.code = 'ERR_INVALID_ARG_TYPE';
      throw err;
    }
  }
  const equal = native.timingSafeEqual(buf1, buf2);
  if (equal === null) throw new ERR_CRYPTO_TIMING_SAFE_EQUAL_LENGTH();
  return equal;
}

module.exports = {
  createHash,
  createHmac,
  getHashes,
  hash,
  randomBytes,
  randomFill,
  randomFillSync,
  randomInt,
  randomUUID,
  timingSafeEqual,
  Hash,
  Hmac,
  getRandomValues,
};

ObjectDefineProperties(module.exports, {
  webcrypto: {
    __proto__: null,
    configurable: false,
    enumerable: true,
    get() { return native.webcrypto; },
  },
  subtle: {
    __proto__: null,
    configurable: false,
    enumerable: true,
    get() { return native.webcrypto.subtle; },
  },
});
