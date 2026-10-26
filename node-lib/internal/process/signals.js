'use strict';

// process.kill and the process 'SIGxxx' events (#1236). A signal is watched
// while process has listeners for it; the native side restores the default
// action when the last listener goes.

const {
  ObjectKeys,
} = primordials;

const {
  codes: {
    ERR_INVALID_ARG_TYPE,
    ERR_UNKNOWN_SIGNAL,
  },
} = require('internal/errors');
const binding = require('internal/host/child_process');
const {
  ErrnoException,
  getSignals,
  signalName,
} = require('internal/child_process/support');

const watched = { __proto__: null };

function kill(pid, sig) {
  let err;

  if (pid != (pid | 0)) {
    throw new ERR_INVALID_ARG_TYPE('pid', 'number', pid);
  }

  if (sig === (sig | 0)) {
    err = binding.kill(pid, sig);
  } else {
    sig ||= 'SIGTERM';
    const number = getSignals()[sig];
    if (number) {
      err = binding.kill(pid, number);
    } else {
      throw new ERR_UNKNOWN_SIGNAL(sig);
    }
  }

  if (err)
    throw new ErrnoException(err, 'kill');

  return true;
}

function syncWatch(proc, type) {
  const number = getSignals()[type];
  if (number === undefined) return;
  const want = proc.listenerCount(type) > 0;
  if (want === (watched[number] === true)) return;
  const err = binding.signalWatch(number, want);
  if (err && want) throw new ErrnoException(err, 'uv_signal_start');
  watched[number] = want;
}

function syncAll(proc) {
  const signals = getSignals();
  for (const name of ObjectKeys(signals)) {
    if (watched[signals[name]]) syncWatch(proc, name);
  }
}

function emitSignal(proc, number) {
  const name = signalName(number);
  if (name === undefined) return;
  proc.emit(name, name, number);
  syncWatch(proc, name);
}

module.exports = { emitSignal, kill, syncAll, syncWatch };
