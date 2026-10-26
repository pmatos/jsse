'use strict';

// Derived from Node v26.8.2 lib/internal/crypto/hash.js (MIT, Copyright Joyent,
// Inc. and other Node contributors). The OpenSSL-backed handles are the native
// `__host_crypto` binding (seeded as `internal/host/crypto`). Deviations: Hash
// and Hmac are plain objects, not Transform streams (jsse has no `stream`
// module yet); a key must be a string or a buffer source (no KeyObject). The
// native state of a Hash that is dropped without `digest()` is not reclaimed:
// jsse never runs FinalizationRegistry callbacks.

const {
  ArrayBufferIsView,
  ObjectCreate,
  StringPrototypeToLowerCase,
  Symbol,
  TypedArrayPrototypeGetBuffer,
  TypedArrayPrototypeGetByteLength,
  TypedArrayPrototypeGetByteOffset,
} = primordials;

const native = require('internal/host/crypto');
const { Buffer } = require('internal/host/buffer');
const {
  codes: {
    ERR_CRYPTO_HASH_FINALIZED,
    ERR_CRYPTO_INVALID_DIGEST,
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_ARG_VALUE,
    ERR_UNKNOWN_ENCODING,
  },
} = require('internal/errors');
const {
  validateObject,
  validateString,
  validateUint32,
} = require('internal/validators');
const { isAnyArrayBuffer } = require('internal/util/types');

const kHandle = Symbol('kHandle');
const kState = Symbol('kState');
const kFinalized = Symbol('kFinalized');

const kHashes = [
  'md5', 'sha1', 'sha224', 'sha256', 'sha384', 'sha512', 'sha512-224',
  'sha512-256', 'sha3-224', 'sha3-256', 'sha3-384', 'sha3-512', 'shake128',
  'shake256',
];

function opensslError(reason, library, hex, code) {
  const err = new Error(`error:${hex}:${library}::${reason}`);
  err.library = library;
  err.reason = reason;
  err.code = code;
  return err;
}

let warnedShake = false;
function maybeWarnShake(info) {
  if (warnedShake || !info[2] || typeof process.emitWarning !== 'function') return;
  warnedShake = true;
  process.emitWarning(
    'Creating SHAKE128/256 digests without an explicit options.outputLength is deprecated.',
    'DeprecationWarning', 'DEP0198');
}

function lookup(algorithm, message = 'Digest method not supported') {
  const info = native.info(algorithm);
  if (info === undefined) throw new Error(message);
  return info;
}

function outputLengthOf(options) {
  let len = typeof options === 'object' && options !== null ?
    options.outputLength : undefined;
  if (len !== undefined) {
    validateUint32(len, 'options.outputLength');
    len += 0;
  }
  return len;
}

function checkOutputLength(info, len, plain) {
  if (len !== undefined && !info[2] && len !== info[1]) {
    if (plain) {
      throw new Error(`Output length ${len} is invalid for ${plain}, which does not support XOF`);
    }
    throw opensslError('not XOF or invalid length', 'digital envelope routines',
                       '030000B2', 'ERR_OSSL_EVP_NOT_XOF_OR_INVALID_LENGTH');
  }
}

function init(self, handle, info) {
  self[kHandle] = handle;
  self[kState] = { [kFinalized]: false, info };
  return self;
}

function Hash(algorithm, options) {
  if (!new.target) return new Hash(algorithm, options);
  validateString(algorithm, 'algorithm');
  const len = outputLengthOf(options);
  const info = lookup(algorithm);
  checkOutputLength(info, len);
  const handle = native.create(info[0], len);
  init(this, handle, info);
  if (len === undefined) maybeWarnShake(info);
}

function toBytes(data, encoding, name) {
  if (typeof data === 'string') {
    if (encoding === undefined || encoding === null ||
        StringPrototypeToLowerCase(`${encoding}`) === 'utf8' ||
        StringPrototypeToLowerCase(`${encoding}`) === 'utf-8' ||
        !Buffer.isEncoding(encoding)) {
      return data;
    }
    if (StringPrototypeToLowerCase(`${encoding}`) === 'hex' && data.length % 2 !== 0) {
      throw new ERR_INVALID_ARG_VALUE('encoding', encoding,
                                      `is invalid for data of length ${data.length}`);
    }
    return Buffer.from(data, encoding);
  }
  if (!ArrayBufferIsView(data)) {
    throw new ERR_INVALID_ARG_TYPE(
      name, ['string', 'Buffer', 'TypedArray', 'DataView'], data);
  }
  return data;
}

// The native side reads the bytes of any typed array; a DataView has none.
function asUint8Array(view) {
  if (typeof view === 'string') return view;
  return new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
}

Hash.prototype.copy = function copy(options) {
  const state = this[kState];
  if (state[kFinalized]) throw new ERR_CRYPTO_HASH_FINALIZED();
  const len = outputLengthOf(options);
  if (len !== undefined) checkOutputLength(state.info, len);
  return init(ObjectCreate(Hash.prototype), native.copy(this[kHandle], len), state.info);
};

Hash.prototype.update = function update(data, encoding) {
  const state = this[kState];
  if (state[kFinalized]) throw new ERR_CRYPTO_HASH_FINALIZED();
  native.update(this[kHandle], asUint8Array(toBytes(data, encoding, 'data')));
  return this;
};

function encode(bytes, outputEncoding) {
  const buf = Buffer.from(TypedArrayPrototypeGetBuffer(bytes),
                          TypedArrayPrototypeGetByteOffset(bytes),
                          TypedArrayPrototypeGetByteLength(bytes));
  const encoding = outputEncoding && `${outputEncoding}`;
  if (encoding && encoding !== 'buffer' && Buffer.isEncoding(encoding)) {
    return buf.toString(encoding);
  }
  return buf;
}

Hash.prototype.digest = function digest(outputEncoding) {
  const state = this[kState];
  if (state[kFinalized]) throw new ERR_CRYPTO_HASH_FINALIZED();
  const bytes = native.digest(this[kHandle]);
  state[kFinalized] = true;
  return encode(bytes, outputEncoding);
};

function secretKey(key, encoding) {
  if (typeof key === 'string') {
    if (encoding === undefined || encoding === 'buffer') return key;
    if (!Buffer.isEncoding(encoding)) throw new ERR_UNKNOWN_ENCODING(encoding);
    return Buffer.from(key, encoding);
  }
  if (isAnyArrayBuffer(key)) return new Uint8Array(key);
  if (ArrayBufferIsView(key)) return asUint8Array(key);
  throw new ERR_INVALID_ARG_TYPE(
    'key',
    ['ArrayBuffer', 'Buffer', 'TypedArray', 'DataView', 'string', 'KeyObject', 'CryptoKey'],
    key);
}

function Hmac(hmac, key, options) {
  if (!(this instanceof Hmac)) return new Hmac(hmac, key, options);
  validateString(hmac, 'hmac');
  const encoding = typeof options === 'object' && options !== null ?
    options.encoding : undefined;
  const bytes = secretKey(key, encoding);
  const info = native.info(hmac);
  if (info === undefined) throw new ERR_CRYPTO_INVALID_DIGEST(hmac);
  if (info[2]) throw new Error('error:00000000:lib(0)::reason(0)');
  init(this, native.create(info[0], undefined, bytes), info);
}

Hmac.prototype.update = Hash.prototype.update;

Hmac.prototype.digest = function digest(outputEncoding) {
  const state = this[kState];
  if (state[kFinalized]) return encode(new Uint8Array(0), outputEncoding);
  const bytes = native.digest(this[kHandle]);
  state[kFinalized] = true;
  return encode(bytes, outputEncoding);
};

function hash(algorithm, input, options) {
  validateString(algorithm, 'algorithm');
  if (typeof input !== 'string' && !ArrayBufferIsView(input)) {
    throw new ERR_INVALID_ARG_TYPE('input', ['Buffer', 'TypedArray', 'DataView', 'string'], input);
  }
  let outputEncoding;
  let outputLength;
  if (typeof options === 'string') {
    outputEncoding = options;
  } else if (options !== undefined) {
    validateObject(options, 'options');
    outputLength = options.outputLength;
    outputEncoding = options.outputEncoding;
  }
  outputEncoding ??= 'hex';
  if (outputEncoding !== 'hex') {
    validateString(outputEncoding, 'outputEncoding');
    if (!Buffer.isEncoding(outputEncoding) &&
        StringPrototypeToLowerCase(outputEncoding) !== 'buffer') {
      throw new ERR_INVALID_ARG_VALUE('outputEncoding', outputEncoding);
    }
  }
  if (outputLength !== undefined) {
    validateUint32(outputLength, 'outputLength');
    outputLength += 0;
  }
  const info = lookup(algorithm, `Digest method ${algorithm} is not supported`);
  if (outputLength === undefined) maybeWarnShake(info);
  checkOutputLength(info, outputLength, algorithm);
  const handle = native.create(info[0], outputLength);
  native.update(handle, asUint8Array(input));
  return encode(native.digest(handle), StringPrototypeToLowerCase(outputEncoding));
}

module.exports = { Hash, Hmac, hash, kHashes };
