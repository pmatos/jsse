'use strict';

// The JavaScript stand-in for the parts of Node's `internalBinding('util')`
// that util.inspect and the util module use, over the engine's
// `__host_util_*` introspection primitives.

const {
  ArrayPrototypeFilter,
  ArrayPrototypePush,
  ReflectGetOwnPropertyDescriptor,
  ReflectOwnKeys,
} = primordials;

const host = require('internal/host/util');

const ALL_PROPERTIES = 0;
const ONLY_ENUMERABLE = 2;
const SKIP_SYMBOLS = 16;
const kPending = 0;
const kFulfilled = 1;
const kRejected = 2;

function getProxyDetails(value, showProxy = true) {
  const details = host.proxyDetails(value);
  if (details === undefined) return undefined;
  return showProxy ? details : details[0];
}

function isArrayIndex(key) {
  if (typeof key !== 'string') return false;
  const n = key >>> 0;
  return n !== 4294967295 && `${n}` === key;
}

// V8 collects a Proxy's keys through its [[OwnPropertyKeys]] trap, keeps the
// trap's order, drops array indices and, for ONLY_ENUMERABLE, asks the
// [[GetOwnProperty]] trap about each remaining key.
function getProxyOwnNonIndexProperties(proxy, filter) {
  const skipSymbols = (filter & SKIP_SYMBOLS) !== 0;
  const onlyEnumerable = (filter & ONLY_ENUMERABLE) !== 0;
  const result = [];
  const keys = ReflectOwnKeys(proxy);
  for (let i = 0; i < keys.length; i++) {
    const key = keys[i];
    if (skipSymbols && typeof key === 'symbol') continue;
    if (isArrayIndex(key)) continue;
    if (onlyEnumerable) {
      const desc = ReflectGetOwnPropertyDescriptor(proxy, key);
      if (desc === undefined || !desc.enumerable) continue;
    }
    ArrayPrototypePush(result, key);
  }
  return result;
}

module.exports = {
  constants: { ALL_PROPERTIES, ONLY_ENUMERABLE, SKIP_SYMBOLS, kPending, kFulfilled, kRejected },
  getOwnNonIndexProperties(object, filter) {
    if (host.proxyDetails(object) !== undefined) {
      return getProxyOwnNonIndexProperties(object, filter);
    }
    const keys = host.ownNonIndex(object, (filter & ONLY_ENUMERABLE) !== 0);
    return (filter & SKIP_SYMBOLS) === 0 ?
      keys : ArrayPrototypeFilter(keys, (key) => typeof key !== 'symbol');
  },
  getPromiseDetails: host.promiseDetails,
  getProxyDetails,
  previewEntries: host.previewEntries,
  getConstructorName: host.className,
  getExternalValue() { return 0n; },
  builtinExists(id) { return host.builtinSource(id) !== undefined; },
};
