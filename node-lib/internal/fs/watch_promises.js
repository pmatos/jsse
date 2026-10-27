'use strict';

// `fs.promises.watch` (issue #1235): an async iterator of `{ eventType,
// filename }` over an FSWatcher, with Node's `maxQueue`/`overflow` bound on
// the events waiting for the consumer and `signal` for cancellation.

const { support: S } = require('internal/host/fs');
const {
  validateAbortSignal, validateBoolean, validateInteger, validateObject, validateOneOf,
} = require('internal/validators');
const { AbortError, genericNodeError } = require('internal/errors');
const { watch } = require('internal/fs/watchers');

async function* watchIterator(filename, options = {}) {
  S.getValidatedPath(filename);
  validateObject(options, 'options');
  const {
    persistent = true, recursive = false, encoding = 'utf8', signal,
    overflow = 'ignore', maxQueue = 2048, ignore,
  } = options;
  validateBoolean(persistent, 'options.persistent');
  validateBoolean(recursive, 'options.recursive');
  S.getOptions(encoding, {});
  validateAbortSignal(signal, 'options.signal');
  if (signal && signal.aborted) throw new AbortError(undefined, { cause: signal.reason });
  validateOneOf(overflow, 'options.overflow', ['ignore', 'error']);
  validateInteger(maxQueue, 'options.maxQueue', 1);

  const watcher = watch(filename, { persistent, recursive, encoding, ignore });
  const queue = [];
  let wake = null;
  let failure = null;
  let closed = false;
  const notify = () => {
    if (wake !== null) wake();
    wake = null;
  };
  watcher.on('change', (eventType, name) => {
    if (queue.length < maxQueue) {
      queue.push({ __proto__: null, eventType, filename: name });
    } else if (overflow === 'error' && failure === null) {
      failure = genericNodeError(`fs.watch() queued more than ${maxQueue} events`, {
        code: 'ERR_FS_WATCH_QUEUE_OVERFLOW',
      });
    }
    notify();
  });
  watcher.on('error', (error) => {
    failure = error;
    notify();
  });
  watcher.on('close', () => {
    closed = true;
    notify();
  });
  const onabort = () => {
    failure = new AbortError(undefined, { cause: signal.reason });
    queue.length = 0;
    watcher.close();
    notify();
  };
  if (signal) signal.addEventListener('abort', onabort);
  try {
    for (;;) {
      if (queue.length > 0) yield queue.shift();
      else if (failure !== null) throw failure;
      else if (closed) return;
      else await new Promise((resolve) => { wake = resolve; });
    }
  } finally {
    if (signal) signal.removeEventListener('abort', onabort);
    watcher.close();
  }
}

module.exports = { watch: watchIterator };
