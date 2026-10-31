'use strict';

// The global queueMicrotask, captured before user code runs.
module.exports = {
  queueMicrotask: require('internal/host/webstreams').queueMicrotask,
};
