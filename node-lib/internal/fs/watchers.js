'use strict';

// `fs.watch` and FSWatcher (issue #1235) over the handles of
// internal/fs/watch_native. Mirrors lib/internal/fs/watchers.js of Node
// v26.8.2: the watch starts synchronously, `change` carries `rename` or
// `change` and the file name, `close` is emitted on the next tick, and an
// unref'd or non-persistent watcher does not keep the process alive.

const EventEmitter = require('events');
const { Buffer } = require('buffer');
const { support: S } = require('internal/host/fs');
const { validateAbortSignal, validateBoolean, validateFunction } = require('internal/validators');
const { codes: { ERR_INVALID_ARG_TYPE } } = require('internal/errors');
const { startWatch, kRename } = require('internal/fs/watch_native');

const kStart = Symbol('kFSWatchStart');
const kEncoding = Symbol('kEncoding');

function decode(name, encoding) {
  const bytes = Buffer.from(name);
  return encoding === 'buffer' ? bytes : bytes.toString(encoding);
}

function createIgnoreMatcher(ignore) {
  if (ignore == null) return null;
  const matchers = [];
  for (const entry of Array.isArray(ignore) ? ignore : [ignore]) {
    if (typeof entry === 'string') {
      const { Minimatch } = require('internal/deps/minimatch/index');
      const compiled = new Minimatch(entry, {
        __proto__: null, nonegate: true, nocomment: true, optimizationLevel: 2, matchBase: true,
        platform: process.platform, windowsPathsNoEscape: true,
      });
      matchers.push((name) => compiled.match(name));
    } else if (entry instanceof RegExp) {
      matchers.push((name) => entry.exec(name) !== null);
    } else if (typeof entry === 'function') {
      matchers.push(entry);
    } else {
      throw new ERR_INVALID_ARG_TYPE('options.ignore', ['string', 'function', 'RegExp'], entry);
    }
  }
  return (name) => matchers.some((matches) => matches(name));
}

function emitClose(watcher) {
  watcher.emit('close');
}

class FSWatcher extends EventEmitter {
  constructor() {
    super();
    this._handle = null;
    this[kEncoding] = 'utf8';
  }

  [kStart](filename, persistent, recursive, encoding, ignore, throwIfNoEntry = true) {
    if (this._handle !== null) return;
    filename = S.getValidatedPath(filename);
    validateBoolean(recursive, 'options.recursive');
    const ignored = createIgnoreMatcher(ignore);
    this[kEncoding] = encoding || 'utf8';
    const filter = recursive ? null : ignored;
    const onevent = (bits, name) => {
      if (this._handle === null) return;
      if (filter !== null && filter(decode(name, 'utf8'))) return;
      this.emit('change', bits & kRename ? 'rename' : 'change', decode(name, this[kEncoding]));
    };
    try {
      if (recursive) {
        const { RecursiveWatch } = require('internal/fs/recursive_watch');
        this._handle = new RecursiveWatch(filename, onevent, persistent !== false, ignored, (error) => {
          this.emit('error', error);
          this.close();
        });
      } else {
        this._handle = startWatch(filename, onevent, persistent !== false);
      }
    } catch (error) {
      if (!throwIfNoEntry && error.code === 'ENOENT') return;
      if (error.code === 'ENOSPC') {
        error.message = error.message.replace('no space left on device',
                                              'System limit for number of file watchers reached');
      }
      error.filename = filename;
      throw error;
    }
  }

  close() {
    if (this._handle === null) return;
    this._handle.close();
    this._handle = null;
    process.nextTick(emitClose, this);
  }

  ref() {
    if (this._handle !== null) this._handle.ref();
    return this;
  }

  unref() {
    if (this._handle !== null) this._handle.unref();
    return this;
  }
}

function watch(filename, options, listener) {
  if (typeof options === 'function') listener = options;
  options = S.getOptions(options, {});
  if (options.persistent === undefined) options.persistent = true;
  if (options.recursive === undefined) options.recursive = false;
  const { signal } = options;
  validateAbortSignal(signal, 'options.signal');
  if (listener !== undefined) validateFunction(listener, 'listener');
  const watcher = new FSWatcher();
  watcher[kStart](filename, options.persistent, options.recursive, options.encoding,
                  options.ignore, options.throwIfNoEntry);
  if (listener) watcher.addListener('change', listener);
  if (signal) {
    if (signal.aborted) {
      process.nextTick(() => watcher.close());
    } else {
      const onabort = () => watcher.close();
      signal.addEventListener('abort', onabort);
      watcher.once('close', () => signal.removeEventListener('abort', onabort));
    }
  }
  return watcher;
}

module.exports = { FSWatcher, watch };
