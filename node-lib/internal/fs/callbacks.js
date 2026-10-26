'use strict';

// The callback API (issue #1234). Every function runs the matching synchronous
// function twice: first with the floor's first syscall replaced by a probe
// that records its arguments and stops, which validates the call exactly as
// the synchronous function does and throws its errors synchronously; then,
// when the syscall's result arrives, again with that one syscall replaced by
// the result, so post-processing (Stats, Dirent, encodings) is shared too.
// Blocking syscalls run on the worker pool; the rest run on a later loop turn.

const { sync, support: S, async: hostAsync } = require('internal/host/fs');

const { ObjectDefineProperty } = primordials;

const host = S.host;
const STOP = { stop: true };
const OFFLOADED = new Set([
  'open', 'close', 'read', 'write', 'read_file', 'write_file', 'stat', 'access',
  'readdir', 'readdir_types', 'readlink', 'realpath', 'mkdir', 'mkdirp',
  'rmdir', 'unlink', 'rename', 'copyfile', 'ftruncate', 'fsync',
]);
const PROBED = Object.keys(host).filter(
  (name) => name !== 'constants' && name !== 'exists' && typeof host[name] === 'function');

function checkCallback(cb) {
  if (typeof cb !== 'function') {
    throw S.invalidArgType('cb', 'of type function', cb);
  }
}

function probe(syncFn, args) {
  const saved = {};
  let hit = null;
  for (const name of PROBED) {
    saved[name] = host[name];
    host[name] = function() {
      hit = { op: name, args: Array.prototype.slice.call(arguments) };
      throw STOP;
    };
  }
  let value;
  try {
    value = syncFn(...args);
  } catch (e) {
    if (e !== STOP) throw e;
  } finally {
    for (const name of PROBED) host[name] = saved[name];
  }
  return { hit, value };
}

function replay(syncFn, args, hit, error, raw) {
  const saved = host[hit.op];
  host[hit.op] = function() {
    host[hit.op] = saved;
    if (error) throw error;
    return raw;
  };
  try {
    return syncFn(...args);
  } finally {
    host[hit.op] = saved;
  }
}

function settle(cb, value, extra) {
  if (extra !== undefined) cb(null, value, extra);
  else if (value === undefined) cb(null);
  else cb(null, value);
}

function run(syncFn, args, cb, extra) {
  const { hit, value } = probe(syncFn, args);
  const done = (error, raw) => {
    let result;
    try {
      result = hit === null ? value : replay(syncFn, args, hit, error, raw);
    } catch (e) {
      cb(e);
      return;
    }
    settle(cb, result, extra);
  };
  if (hit === null) {
    setImmediate(done);
  } else if (OFFLOADED.has(hit.op)) {
    hostAsync(hit.op, done, ...hit.args);
  } else {
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
}

// Functions that make several syscalls (or are written in JavaScript) cannot
// be probed: they validate and run on a later loop turn, and everything,
// including a validation failure, is reported through the callback.
function deferred(syncFn, args, cb) {
  setImmediate(() => {
    let result;
    try {
      result = syncFn(...args);
    } catch (e) {
      cb(e);
      return;
    }
    settle(cb, result);
  });
}

function define(name, syncFn, options) {
  const required = (options && options.required) || 1;
  const wrapper = function(...args) {
    const explicit = args.length > required;
    const cb = explicit ? args[args.length - 1] : options && options.defaultCallback;
    checkCallback(cb);
    if (explicit) args.pop();
    if (options && options.args) args = options.args(args);
    if (options && options.deferred && options.deferred(args)) {
      deferred(syncFn, args, cb);
    } else {
      run(syncFn, args, cb, options && options.extra && options.extra(args));
    }
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

define('readdir', sync.readdirSync, {
  deferred: (args) => {
    const options = args[1];
    return options !== null && typeof options === 'object' && Boolean(options.recursive);
  },
});
define('cp', sync.cpSync, { deferred: () => true, required: 2 });
define('rm', sync.rmSync, { deferred: () => true });
define('truncate', sync.truncateSync, { deferred: () => true });
define('opendir', sync.opendirSync);
define('realpath', sync.realpathSync, { deferred: () => true }).native =
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
