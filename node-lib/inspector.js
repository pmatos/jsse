'use strict';

// jsse has no V8 inspector and no sampling profiler. This module keeps the
// shape tools such as tsc rely on: a Session whose Profiler domain records
// nothing, so Profiler.stop returns a valid profile with no samples. Every
// other protocol method is reported as not found, as V8 does for a domain it
// lacks.

const { EventEmitter } = require('events');
const { genericNodeError } = require('internal/errors');
const {
  validateFunction,
  validateObject,
  validateString,
} = require('internal/validators');

const kConnected = Symbol('connected');
const kProfiler = Symbol('profiler');

function inspectorError(code, message) {
  return genericNodeError(message, { code });
}

function commandError(code, message) {
  return inspectorError('ERR_INSPECTOR_COMMAND', `Inspector error ${code}: ${message}`);
}

function newProfilerState() {
  return { enabled: false, startTime: undefined };
}

function nowMicros() {
  return Number(process.hrtime.bigint() / 1000n);
}

function emptyProfile(startTime) {
  const callFrame = (functionName) => ({
    functionName,
    scriptId: '0',
    url: '',
    lineNumber: -1,
    columnNumber: -1,
  });
  return {
    nodes: [
      { id: 1, callFrame: callFrame('(root)'), hitCount: 0, children: [2] },
      { id: 2, callFrame: callFrame('(program)'), hitCount: 0 },
    ],
    startTime,
    endTime: nowMicros(),
    samples: [],
    timeDeltas: [],
  };
}

function profilerCommand(state, method) {
  switch (method) {
    case 'Profiler.enable':
      state.enabled = true;
      return {};
    case 'Profiler.disable':
      state.enabled = false;
      state.startTime = undefined;
      return {};
    case 'Profiler.setSamplingInterval':
      return {};
    case 'Profiler.start':
      if (!state.enabled) throw commandError(-32000, 'Profiler is not enabled');
      state.startTime ??= nowMicros();
      return {};
    case 'Profiler.stop': {
      if (state.startTime === undefined) {
        throw commandError(-32000, 'No recording profiles found');
      }
      const profile = emptyProfile(state.startTime);
      state.startTime = undefined;
      return { profile };
    }
    default:
      throw commandError(-32601, `'${method}' wasn't found`);
  }
}

class Session extends EventEmitter {
  constructor() {
    super();
    this[kConnected] = false;
    this[kProfiler] = newProfilerState();
  }

  connect() {
    if (this[kConnected]) {
      throw inspectorError('ERR_INSPECTOR_ALREADY_CONNECTED',
                           'The inspector session is already connected');
    }
    this[kConnected] = true;
  }

  connectToMainThread() {
    throw inspectorError('ERR_INSPECTOR_NOT_WORKER', 'Current thread is not a worker');
  }

  post(method, params, callback) {
    validateString(method, 'method');
    if (!callback && typeof params === 'function') {
      callback = params;
      params = null;
    }
    if (params) validateObject(params, 'params');
    if (callback) validateFunction(callback, 'callback');
    if (!this[kConnected]) {
      throw inspectorError('ERR_INSPECTOR_NOT_CONNECTED', 'Session is not connected');
    }
    let error = null;
    let result;
    try {
      result = profilerCommand(this[kProfiler], method);
    } catch (e) {
      error = e;
    }
    if (callback) {
      if (error) callback(error);
      else callback(null, result);
    }
  }

  disconnect() {
    if (!this[kConnected]) return;
    this[kConnected] = false;
    this[kProfiler] = newProfilerState();
  }
}

function open() {
  throw inspectorError('ERR_INSPECTOR_NOT_AVAILABLE', 'Inspector is not available');
}

function waitForDebugger() {
  throw inspectorError('ERR_INSPECTOR_NOT_ACTIVE', 'Inspector is not active');
}

module.exports = {
  open,
  close() {},
  url() {},
  waitForDebugger,
  Session,
};
