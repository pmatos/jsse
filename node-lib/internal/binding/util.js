'use strict';

// The JavaScript stand-in for the parts of Node's `internalBinding('util')`
// that util.inspect and the util module use, over the engine's
// `__host_util_*` introspection primitives.

const { ArrayPrototypeFilter } = primordials;

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

module.exports = {
  constants: { ALL_PROPERTIES, ONLY_ENUMERABLE, SKIP_SYMBOLS, kPending, kFulfilled, kRejected },
  getOwnNonIndexProperties(object, filter) {
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
