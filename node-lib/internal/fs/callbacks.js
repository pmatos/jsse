'use strict';

// The callback API (issues #1234, #1341). Every function runs the matching
// synchronous function against a script of floor calls. The first run has no
// script, so its first floor call is replaced by a probe that records its
// arguments and stops: that validates the call exactly as the synchronous
// function does and throws its errors synchronously. The recorded call is
// submitted to the worker pool; when its result arrives the function runs again
// with the results so far replayed, so post-processing (Stats, Dirent,
// encodings) is shared code, and stops at its next floor call, until a run
// completes. Functions with a handful of floor calls (`rm`, `truncate`,
// `realpath`) therefore never block the loop, and the tree walks (`rm`
// recursive, recursive `readdir`) are one floor call each.

const { sync, support: S, async: hostAsync } = require('internal/host/fs');

const { ObjectDefineProperty, SymbolFor } = primordials;
const { kCustomPromisifyArgsSymbol } = require('internal/util');

const host = S.host;
const STOP = { stop: true };
const OFFLOADED = new Set([
  'open', 'close', 'read', 'write', 'read_file', 'write_file', 'stat', 'access',
  'readdir', 'readdir_types', 'readlink', 'realpath', 'mkdir', 'mkdirp',
  'rmdir', 'unlink', 'rename', 'copyfile', 'ftruncate', 'fsync', 'chmod',
  'chown', 'utimes', 'link', 'symlink', 'mkdtemp', 'statfs', 'rm_all',
  'readdir_tree',
]);
const PROBED = Object.keys(host).filter(
  (name) => name !== 'constants' && name !== 'exists' && typeof host[name] === 'function');

function checkCallback(cb) {
  if (typeof cb !== 'function') {
    throw S.invalidArgType('cb', 'of type function', cb);
  }
}

let probeHit = null;
let script = null;
let scriptIndex = 0;
const stubs = {};
for (const name of PROBED) {
  stubs[name] = function() {
    if (scriptIndex < script.length) {
      const step = script[scriptIndex++];
      if (step.failed) throw step.error;
      return step.raw;
    }
    // A `finally` block may make further floor calls while STOP unwinds; the
    // first one is the one to submit.
    if (probeHit === null) {
      probeHit = { op: name, args: Array.prototype.slice.call(arguments) };
    }
    throw STOP;
  };
}

// Run `syncFn` once with the floor calls in `steps` replayed; `hit` is the
// next floor call it made (null if it ran to completion with `value`). The
// state is saved and restored so a getter that calls `fs` while the function
// runs does not disturb it.
function execute(syncFn, args, steps) {
  const outer = { probeHit, script, scriptIndex };
  const originals = {};
  probeHit = null;
  script = steps;
  scriptIndex = 0;
  for (const name of PROBED) {
    originals[name] = host[name];
    host[name] = stubs[name];
  }
  let value;
  let hit;
  try {
    value = syncFn(...args);
  } catch (e) {
    if (e !== STOP) throw e;
  } finally {
    hit = probeHit;
    for (const name of PROBED) host[name] = originals[name];
    ({ probeHit, script, scriptIndex } = outer);
  }
  return { hit, value };
}

function settle(cb, value, extra) {
  if (extra !== undefined) cb(null, value, extra);
  else if (value === undefined) cb(null);
  else cb(null, value);
}

function submit(hit, done) {
  if (OFFLOADED.has(hit.op)) {
    hostAsync(hit.op, done, ...hit.args);
    return;
  }
  setImmediate(() => {
    let raw;
    let error;
    try {
      raw = host[hit.op](...hit.args);
    } catch (e) {
      error = e;
    }
    done(error, raw);
  });
}

// Every run reads its arguments again, so a plain options object is copied once
// (its getters run once) and a later change by the caller is not seen.
const snapshot = (value) => (
  value !== null && typeof value === 'object' && Object.getPrototypeOf(value) === Object.prototype ?
    { ...value } : value);

function run(syncFn, args, cb, extra) {
  args = args.map(snapshot);
  const steps = [];
  const advance = (first) => {
    let outcome;
    try {
      outcome = execute(syncFn, args, steps);
    } catch (e) {
      if (first) throw e;
      cb(e);
      return;
    }
    const { hit, value } = outcome;
    if (hit === null) {
      if (first) setImmediate(() => settle(cb, value, extra));
      else settle(cb, value, extra);
      return;
    }
    const done = (error, raw) => {
      steps.push(error ? { failed: true, error } : { failed: false, raw });
      advance(false);
    };
    try {
      submit(hit, done);
    } catch (e) {
      if (first) throw e;
      cb(e);
    }
  };
  advance(true);
}

// options.signal: readFile/writeFile/appendFile are Node's own sequence of
// steps with the signal checked between them; stat/lstat/fstat fail as soon as
// it fires (signal.js). Returns the callback to run with, or null if handled.
const SIGNAL_AT = { readFile: 1, writeFile: 2, appendFile: 2, stat: 1, lstat: 1, fstat: 1 };

function withSignal(name, args, cb) {
  const options = args[SIGNAL_AT[name]];
  if (options === null || typeof options !== 'object' || options.signal === undefined) {
    return cb;
  }
  const forms = require('internal/fs/signal');
  if (name === 'readFile') {
    forms.readFileCallback(args[0], options, cb);
  } else if (name === 'writeFile' || name === 'appendFile') {
    forms.writeFileCallback(args[0], args[1], options, cb, name === 'appendFile');
  } else {
    if (name !== 'fstat') S.getValidatedPath(args[0]);
    return forms.bindStat(options, cb);
  }
  return null;
}

function define(name, syncFn, options) {
  const required = (options && options.required) || 1;
  const wrapper = function(...args) {
    const explicit = args.length > required;
    let cb = explicit ? args[args.length - 1] : options && options.defaultCallback;
    checkCallback(cb);
    if (explicit) args.pop();
    if (options && options.args) args = options.args(args);
    if (name in SIGNAL_AT) {
      cb = withSignal(name, args, cb);
      if (cb === null) return;
    }
    run(syncFn, args, cb, options && options.extra && options.extra(args));
  };
  ObjectDefineProperty(wrapper, 'name', { value: name, configurable: true });
  if (!(options && options.hidden)) exports[name] = wrapper;
  return wrapper;
}

const ARITY = {
  copyFile: 2, link: 2, symlink: 2, chmod: 2, fchmod: 2, rename: 2,
  chown: 3, fchown: 3, lchown: 3, utimes: 3, futimes: 3, lutimes: 3,
};

for (const name of [
  'access', 'appendFile', 'chmod', 'chown', 'copyFile', 'fchmod',
  'fchown', 'fdatasync', 'fstat', 'fsync', 'ftruncate', 'futimes', 'lchown',
  'link', 'lutimes', 'mkdir', 'mkdtemp', 'open', 'readFile', 'readlink',
  'rename', 'rmdir', 'statfs', 'symlink', 'unlink', 'utimes', 'writeFile',
]) {
  define(name, sync[name + 'Sync'], { required: ARITY[name] });
}

const withoutThrowIfNoEntry = (args) => {
  const options = args[1];
  if (options !== null && typeof options === 'object') {
    args[1] = { ...options, throwIfNoEntry: true };
  }
  return args;
};
define('close', sync.closeSync, {
  defaultCallback: (error) => {
    if (error != null) throw error;
  },
});
define('stat', sync.statSync, { args: withoutThrowIfNoEntry });
define('lstat', sync.lstatSync, { args: withoutThrowIfNoEntry });

define('readdir', sync.readdirSync);
exports.readdirDepthFirst = define('readdir', S.readdirDepthFirst, { hidden: true });

// `cp` is not a script of floor calls: it is Node's promise-based copy.
exports.cp = function cp(src, dest, options, callback) {
  if (typeof options === 'function') {
    callback = options;
    options = undefined;
  }
  checkCallback(callback);
  require('internal/fs/cp').cpCallback(src, dest, options, callback);
};
define('rm', sync.rmSync);
define('truncate', sync.truncateSync, {
  args: (args) => {
    if (args[1] === undefined) args[1] = 0;
    S.validateInteger(args[1], 'len', -S.MAX_SAFE, S.MAX_SAFE);
    return args;
  },
});
define('opendir', sync.opendirSync);
define('realpath', sync.realpathSync, {
  args: (args) => {
    // Node's callback realpath stringifies what is not a path instead of
    // throwing: realpath(1, cb) looks up "1".
    if (typeof args[0] !== 'string') {
      try {
        args[0] = S.getValidatedPath(args[0]) + '';
      } catch {
        args[0] += '';
      }
    }
    args[0] = S.path.resolve(process.cwd(), args[0]);
    return args;
  },
}).native =
  define('realpath', sync.realpathSync.native, { hidden: true });

exports.exists = function exists(path, cb) {
  checkCallback(arguments.length > 1 ? cb : undefined);
  try {
    path = S.getValidatedPath(path);
  } catch {
    process.nextTick(cb, false);
    return;
  }
  hostAsync('access', (error) => cb(!error), path, 0);
};

// fs.read(fd, buffer, offset, length, position, cb), fs.read(fd, buffer,
// options, cb), fs.read(fd, options, cb) and fs.read(fd, cb).
exports.read = function read(fd, ...rest) {
  const cb = rest.pop();
  checkCallback(cb);
  S.getValidatedFd(fd);
  let args;
  if (rest.length === 0 || !S.isView(rest[0])) {
    const options = rest[0];
    if (options !== undefined) S.validateObject(options, 'options');
    const buffer = (options && options.buffer) || S.Buffer.alloc(16384);
    args = [fd, buffer, { ...options }];
  } else {
    args = [fd, ...rest];
  }
  run(sync.readSync, args, cb, args[1]);
};

exports.write = function write(fd, ...rest) {
  const cb = rest.pop();
  checkCallback(cb);
  run(sync.writeSync, [fd, ...rest], cb, rest[0]);
};

exports.read[kCustomPromisifyArgsSymbol] = ['bytesRead', 'buffer'];
exports.write[kCustomPromisifyArgsSymbol] = ['bytesWritten', 'buffer'];
ObjectDefineProperty(exports.exists, SymbolFor('nodejs.util.promisify.custom'), {
  value: function exists(path) {
    return new Promise((resolve) => exports.exists(path, resolve));
  },
  configurable: true,
});
