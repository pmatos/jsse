'use strict';

// jsse runs one thread. This module is what a tool sees from the main thread of
// a process that cannot start workers: the thread identity, an environment-data
// store and the untransferable marks work, and constructing a Worker fails the
// way Node does when the platform lacks the feature. ESLint loads the module
// for `isMainThread`/`threadId` and only uses `Worker` for `--concurrency`.

const {
  codes: { ERR_FEATURE_UNAVAILABLE_ON_PLATFORM },
} = require('internal/errors');

const SHARE_ENV = Symbol.for('nodejs.worker_threads.SHARE_ENV');

const environmentData = new Map();
const untransferable = new WeakSet();

class Worker {
  constructor() {
    throw new ERR_FEATURE_UNAVAILABLE_ON_PLATFORM('Worker');
  }
}

function setEnvironmentData(key, value) {
  if (value === undefined) environmentData.delete(key);
  else environmentData.set(key, value);
}

function getEnvironmentData(key) {
  return environmentData.get(key);
}

function isObjectLike(value) {
  return (typeof value === 'object' && value !== null) || typeof value === 'function';
}

function markAsUntransferable(object) {
  if (isObjectLike(object)) untransferable.add(object);
}

function isMarkedAsUntransferable(object) {
  return isObjectLike(object) && untransferable.has(object);
}

// structuredClone does not consult the mark, so there is nothing to record.
function markAsUncloneable() {}

module.exports = {
  isMainThread: true,
  isInternalThread: false,
  threadId: 0,
  threadName: 'main',
  parentPort: null,
  workerData: null,
  resourceLimits: {},
  SHARE_ENV,
  Worker,
  setEnvironmentData,
  getEnvironmentData,
  markAsUntransferable,
  isMarkedAsUntransferable,
  markAsUncloneable,
};
