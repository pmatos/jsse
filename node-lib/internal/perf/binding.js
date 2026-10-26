'use strict';

// Stands in for Node's internalBinding('performance') over the host floor
// (`internal/host/process`). Times are measured from the process time origin
// (the moment the floor started), which is also the origin of `hrtime`. There
// is no libuv loop and no garbage-collection event source, so loop milestones
// stay at -1, loop idle time is 0 and 'gc' observers never fire.

const host = require('internal/host/process');

// The garbage-collection constants are the public perf_hooks.constants; the
// entry-type and milestone indices are internal and not enumerable.
const constants = {
  NODE_PERFORMANCE_GC_MAJOR: 4,
  NODE_PERFORMANCE_GC_MINOR: 1,
  NODE_PERFORMANCE_GC_MINOR_MARK_SWEEP: 2,
  NODE_PERFORMANCE_GC_INCREMENTAL: 8,
  NODE_PERFORMANCE_GC_WEAKCB: 16,
  NODE_PERFORMANCE_GC_FLAGS_NO: 0,
  NODE_PERFORMANCE_GC_FLAGS_CONSTRUCT_RETAINED: 2,
  NODE_PERFORMANCE_GC_FLAGS_FORCED: 4,
  NODE_PERFORMANCE_GC_FLAGS_SYNCHRONOUS_PHANTOM_PROCESSING: 8,
  NODE_PERFORMANCE_GC_FLAGS_ALL_AVAILABLE_GARBAGE: 16,
  NODE_PERFORMANCE_GC_FLAGS_ALL_EXTERNAL_MEMORY: 32,
  NODE_PERFORMANCE_GC_FLAGS_SCHEDULE_IDLE: 64,
};

const internalConstants = {
  NODE_PERFORMANCE_ENTRY_TYPE_GC: 0,
  NODE_PERFORMANCE_ENTRY_TYPE_HTTP: 1,
  NODE_PERFORMANCE_ENTRY_TYPE_HTTP2: 2,
  NODE_PERFORMANCE_ENTRY_TYPE_NET: 3,
  NODE_PERFORMANCE_ENTRY_TYPE_DNS: 4,
  NODE_PERFORMANCE_ENTRY_TYPE_QUIC: 5,

  NODE_PERFORMANCE_MILESTONE_TIME_ORIGIN_TIMESTAMP: 0,
  NODE_PERFORMANCE_MILESTONE_TIME_ORIGIN: 1,
  NODE_PERFORMANCE_MILESTONE_ENVIRONMENT: 2,
  NODE_PERFORMANCE_MILESTONE_NODE_START: 3,
  NODE_PERFORMANCE_MILESTONE_V8_START: 4,
  NODE_PERFORMANCE_MILESTONE_LOOP_START: 5,
  NODE_PERFORMANCE_MILESTONE_LOOP_EXIT: 6,
  NODE_PERFORMANCE_MILESTONE_BOOTSTRAP_COMPLETE: 7,
};
for (const name of Object.keys(internalConstants)) {
  Object.defineProperty(constants, name, { value: internalConstants[name] });
}

// Nanoseconds from the time origin, except the first entry (epoch microseconds).
const milestones = [
  host.timeOrigin(),
  0,
  0,
  0,
  0,
  -1,
  -1,
  host.now() * 1e6,
];

function nop() {}

module.exports = {
  constants,
  milestones,
  now: host.now,
  observerCounts: [0, 0, 0, 0, 0, 0],
  installGarbageCollectionTracking: nop,
  removeGarbageCollectionTracking: nop,
  setupObservers: nop,
  loopIdleTime: () => 0,
  uvMetricsInfo: () => [0, 0, 0],
};
