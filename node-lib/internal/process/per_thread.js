'use strict';

// Derived from Node v26.8.2 lib/internal/process/per_thread.js (MIT): the
// argument checks and result shapes of process.hrtime, cpuUsage,
// threadCpuUsage, memoryUsage, resourceUsage and umask over the host floor
// (`internal/host/process`). The prelude's `process` methods call in lazily.

const {
  NumberMAX_SAFE_INTEGER,
  RegExpPrototypeExec,
  parseInt,
} = primordials;

const {
  codes: {
    ERR_INVALID_ARG_VALUE,
    ERR_OUT_OF_RANGE,
  },
} = require('internal/errors');
const {
  validateArray,
  validateNumber,
  validateObject,
  validateUint32,
} = require('internal/validators');

const host = require('internal/host/process');

function validateHrtime(time) {
  validateArray(time, 'time');
  if (time.length !== 2) {
    throw new ERR_OUT_OF_RANGE('time', 2, time.length);
  }
}

function previousValueIsValid(num) {
  return typeof num === 'number' &&
      num <= NumberMAX_SAFE_INTEGER &&
      num >= 0;
}

function invalidValueRange(name, value) {
  const err = new RangeError(new ERR_INVALID_ARG_VALUE(name, value).message);
  err.code = 'ERR_INVALID_ARG_VALUE';
  return err;
}

function cpuUsage(prevValue) {
  if (prevValue) {
    if (!previousValueIsValid(prevValue.user)) {
      validateObject(prevValue, 'prevValue');
      validateNumber(prevValue.user, 'prevValue.user');
      throw invalidValueRange('prevValue.user', prevValue.user);
    }
    if (!previousValueIsValid(prevValue.system)) {
      validateNumber(prevValue.system, 'prevValue.system');
      throw invalidValueRange('prevValue.system', prevValue.system);
    }
  }

  const values = host.cpuUsage();
  if (prevValue) {
    return {
      user: values[0] - prevValue.user,
      system: values[1] - prevValue.system,
    };
  }
  return { user: values[0], system: values[1] };
}

function memoryUsage() {
  const values = host.memoryUsage();
  return {
    rss: values[0],
    heapTotal: values[1],
    heapUsed: values[2],
    external: values[3],
    arrayBuffers: values[4],
  };
}

function rss() {
  return host.rss();
}

function resourceUsage() {
  const v = host.resourceUsage();
  return {
    userCPUTime: v[0],
    systemCPUTime: v[1],
    maxRSS: v[2],
    sharedMemorySize: v[3],
    unsharedDataSize: v[4],
    unsharedStackSize: v[5],
    minorPageFault: v[6],
    majorPageFault: v[7],
    swappedOut: v[8],
    fsRead: v[9],
    fsWrite: v[10],
    ipcSent: v[11],
    ipcReceived: v[12],
    signalsCount: v[13],
    voluntaryContextSwitches: v[14],
    involuntaryContextSwitches: v[15],
  };
}

function parseFileMode(value, name) {
  if (typeof value === 'string') {
    if (RegExpPrototypeExec(/^[0-7]+$/, value) === null) {
      throw new ERR_INVALID_ARG_VALUE(name, value,
                                      'must be a 32-bit unsigned integer or an octal string');
    }
    value = parseInt(value, 8);
  }
  validateUint32(value, name);
  return value;
}

function umask(mask) {
  if (mask !== undefined) mask = parseFileMode(mask, 'mask');
  return host.umask(mask);
}

module.exports = {
  validateHrtime,
  cpuUsage,
  memoryUsage,
  rss,
  resourceUsage,
  umask,
};
