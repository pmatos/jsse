'use strict';

// Derived from Node v26.8.2 lib/internal/crypto/random.js (MIT, Copyright
// Joyent, Inc. and other Node contributors). Entropy comes from the OS through
// the binding's `randomBytes` (the prelude's `__host_random_bytes`). Callbacks
// of `randomBytes`/`randomFill` run from `setImmediate` (Node: after the
// threadpool job); `randomInt`'s run from `process.nextTick`, as in Node when
// its entropy cache is warm. `randomUUIDv7` is not provided.

const {
  MathMin,
  NumberIsNaN,
  NumberIsSafeInteger,
  NumberPrototypeToString,
  StringPrototypePadStart,
} = primordials;

const native = require('internal/host/crypto');
const { Buffer } = require('internal/host/buffer');
const { kEmptyObject, lazyDOMException } = require('internal/util');
const {
  codes: { ERR_INVALID_ARG_TYPE, ERR_OUT_OF_RANGE },
} = require('internal/errors');
const {
  validateBoolean,
  validateFunction,
  validateNumber,
  validateObject,
} = require('internal/validators');
const {
  isAnyArrayBuffer,
  isArrayBufferView,
  isFloat16Array,
  isFloat32Array,
  isFloat64Array,
  isTypedArray,
} = require('internal/util/types');

const kMaxPossibleLength = 2 ** 31 - 1;

function assertOffset(offset, elementSize, length) {
  validateNumber(offset, 'offset');
  offset *= elementSize;
  const maxLength = MathMin(length, kMaxPossibleLength);
  if (NumberIsNaN(offset) || offset > maxLength || offset < 0) {
    throw new ERR_OUT_OF_RANGE('offset', `>= 0 && <= ${maxLength}`, offset);
  }
  return offset >>> 0;
}

function assertSize(size, elementSize, offset, length) {
  validateNumber(size, 'size');
  size *= elementSize;
  if (NumberIsNaN(size) || size > kMaxPossibleLength || size < 0) {
    throw new ERR_OUT_OF_RANGE('size', `>= 0 && <= ${kMaxPossibleLength}`, size);
  }
  if (size + offset > length) {
    throw new ERR_OUT_OF_RANGE('size + offset', `<= ${length}`, size + offset);
  }
  return size >>> 0;
}

function randomBytes(size, callback) {
  size = assertSize(size, 1, 0, Infinity);
  if (callback !== undefined) validateFunction(callback, 'callback');
  const bytes = native.randomBytes(size);
  const buf = Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (callback === undefined) return buf;
  setImmediate(function() { callback.call(this, null, buf); });
}

function randomFillSync(buf, offset = 0, size) {
  if (!isAnyArrayBuffer(buf) && !isArrayBufferView(buf)) {
    throw new ERR_INVALID_ARG_TYPE('buf', ['ArrayBuffer', 'ArrayBufferView'], buf);
  }
  const elementSize = buf.BYTES_PER_ELEMENT || 1;
  offset = assertOffset(offset, elementSize, buf.byteLength);
  if (size === undefined) {
    size = buf.byteLength - offset;
  } else {
    size = assertSize(size, elementSize, offset, buf.byteLength);
  }
  native.randomFill(buf, offset, size);
  return buf;
}

function randomFill(buf, offset, size, callback) {
  if (!isAnyArrayBuffer(buf) && !isArrayBufferView(buf)) {
    throw new ERR_INVALID_ARG_TYPE('buf', ['ArrayBuffer', 'ArrayBufferView'], buf);
  }
  const elementSize = buf.BYTES_PER_ELEMENT || 1;
  if (typeof offset === 'function') {
    callback = offset;
    offset = 0;
    size = buf.length;
  } else if (typeof size === 'function') {
    callback = size;
    size = (buf.length ?? buf.byteLength) - offset;
  } else {
    validateFunction(callback, 'callback');
  }
  offset = assertOffset(offset, elementSize, buf.byteLength);
  if (size === undefined) {
    size = buf.byteLength - offset;
  } else {
    size = assertSize(size, elementSize, offset, buf.byteLength);
  }
  if (size === 0) {
    callback(null, buf);
    return;
  }
  setImmediate(function() {
    native.randomFill(buf, offset, size);
    callback.call(this, null, buf);
  });
}

// Largest integer readable from six bytes: Buffer.from("ff".repeat(6), "hex").readUIntBE(0, 6).
const RAND_MAX = 0xFFFF_FFFF_FFFF;
const kCacheSize = 6 * 1024;
let randomCache;
let randomCacheOffset = kCacheSize;

function randomInt(min, max, callback) {
  const minNotSpecified = typeof max === 'undefined' || typeof max === 'function';
  if (minNotSpecified) {
    callback = max;
    max = min;
    min = 0;
  }
  const isSync = typeof callback === 'undefined';
  if (!isSync) validateFunction(callback, 'callback');
  if (!NumberIsSafeInteger(min)) {
    throw new ERR_INVALID_ARG_TYPE('min', 'a safe integer', min);
  }
  if (!NumberIsSafeInteger(max)) {
    throw new ERR_INVALID_ARG_TYPE('max', 'a safe integer', max);
  }
  if (max <= min) {
    throw new ERR_OUT_OF_RANGE('max', `greater than the value of "min" (${min})`, max);
  }
  const range = max - min;
  if (!(range <= RAND_MAX)) {
    throw new ERR_OUT_OF_RANGE(`max${minNotSpecified ? '' : ' - min'}`, `<= ${RAND_MAX}`, range);
  }
  // Rejection sampling keeps `x % range` unbiased.
  const randLimit = RAND_MAX - (RAND_MAX % range);
  for (;;) {
    if (randomCacheOffset === kCacheSize) {
      randomCache = Buffer.from(native.randomBytes(kCacheSize).buffer);
      randomCacheOffset = 0;
    }
    const x = randomCache.readUIntBE(randomCacheOffset, 6);
    randomCacheOffset += 6;
    if (x < randLimit) {
      const n = (x % range) + min;
      if (isSync) return n;
      process.nextTick(callback, undefined, n);
      return;
    }
  }
}

function getRandomValues(data) {
  if (!isTypedArray(data) || isFloat16Array(data) || isFloat32Array(data) ||
      isFloat64Array(data)) {
    throw lazyDOMException(
      'The data argument must be an integer-type TypedArray', 'TypeMismatchError');
  }
  if (data.byteLength > 65536) {
    throw lazyDOMException('The requested length exceeds 65,536 bytes', 'QuotaExceededError');
  }
  randomFillSync(data, 0);
  return data;
}

let hexBytes;
function serializeUUID(buf) {
  if (hexBytes === undefined) {
    hexBytes = [];
    for (let i = 0; i < 256; i++) {
      hexBytes.push(StringPrototypePadStart(NumberPrototypeToString(i, 16), 2, '0'));
    }
  }
  const h = hexBytes;
  return h[buf[0]] + h[buf[1]] + h[buf[2]] + h[buf[3]] + '-' +
    h[buf[4]] + h[buf[5]] + '-' +
    h[(buf[6] & 0x0f) | 0x40] + h[buf[7]] + '-' +
    h[(buf[8] & 0x3f) | 0x80] + h[buf[9]] + '-' +
    h[buf[10]] + h[buf[11]] + h[buf[12]] + h[buf[13]] + h[buf[14]] + h[buf[15]];
}

function randomUUID(options) {
  if (options !== undefined) validateObject(options, 'options');
  const { disableEntropyCache = false } = options || kEmptyObject;
  validateBoolean(disableEntropyCache, 'options.disableEntropyCache');
  return serializeUUID(native.randomBytes(16));
}

module.exports = {
  getRandomValues,
  randomBytes,
  randomFill,
  randomFillSync,
  randomInt,
  randomUUID,
};
