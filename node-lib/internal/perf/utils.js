'use strict';

// Milliseconds since the first load of this module. #1227 (perf_hooks) should
// replace this with the process time origin.
const origin = process.hrtime.bigint();

function now() {
  return Number(process.hrtime.bigint() - origin) / 1e6;
}

module.exports = { now };
