'use strict';

// `fs.watchFile`, `fs.unwatchFile` and StatWatcher (issue #1235): libuv's
// uv_fs_poll over a timer. The first poll stats at once and reports only a
// failure; a later poll reports a change of ctime, mtime, birthtime, size,
// mode, uid, gid, ino or dev as `change(current, previous)`. A path that does
// not exist reports all-zero Stats, as in Node.

const EventEmitter = require('events');
const path = require('path');
const { fs, support: S } = require('internal/host/fs');
const { validateFunction, validateUint32 } = require('internal/validators');

const kStart = Symbol('kFSStatWatcherStart');
const kBigint = Symbol('kUseBigint');
const kRefs = Symbol('kFSStatWatcherRefCount');
const kMaxRefs = Symbol('kFSStatWatcherMaxRefCount');
const kAddOrCleanRef = Symbol('kFSStatWatcherAddOrCleanRef');
const watchers = new Map();

const COMMON = ['size', 'mode', 'uid', 'gid', 'ino', 'dev'];
const TIMES = {
  number: ['ctimeMs', 'mtimeMs', 'birthtimeMs'],
  bigint: ['ctimeNs', 'mtimeNs', 'birthtimeNs'],
};

function zeroStats(bigint) {
  if (bigint) return new S.stats.BigIntStats(...new Array(14).fill(0n));
  return new S.stats.Stats(...new Array(18).fill(0));
}

class Poller {
  constructor(file, interval, bigint, onchange) {
    this.file = file;
    this.interval = interval;
    this.bigint = bigint;
    this.onchange = onchange;
    this.fields = COMMON.concat(TIMES[bigint ? 'bigint' : 'number']);
    this.previous = zeroStats(bigint);
    this.busy = 0;
    this.refed = true;
    this.closed = false;
    this.timer = null;
    this.schedule(0);
  }

  schedule(delay) {
    this.timer = setTimeout(() => this.poll(), delay);
    if (!this.refed) this.timer.unref();
  }

  same(a, b) {
    return this.fields.every((field) => a[field] === b[field]);
  }

  poll() {
    let current = null;
    let errno = 0;
    let report = null;
    try {
      current = fs.statSync(this.file, { bigint: this.bigint });
    } catch (error) {
      errno = error.errno || -1;
    }
    if (errno !== 0) {
      if (this.busy !== errno) {
        this.busy = errno;
        report = [zeroStats(this.bigint), this.previous];
      }
    } else {
      const changed = this.busy < 0 || (this.busy !== 0 && !this.same(this.previous, current));
      const before = this.previous;
      this.previous = current;
      this.busy = 1;
      if (changed) report = [current, before];
    }
    if (!this.closed) this.schedule(this.interval);
    if (report !== null) this.onchange(report[0], report[1]);
  }

  ref() {
    this.refed = true;
    if (this.timer !== null) this.timer.ref();
  }

  unref() {
    this.refed = false;
    if (this.timer !== null) this.timer.unref();
  }

  close() {
    this.closed = true;
    clearTimeout(this.timer);
  }
}

class StatWatcher extends EventEmitter {
  constructor(bigint) {
    super();
    this._handle = null;
    this[kBigint] = Boolean(bigint);
    this[kRefs] = 1;
    this[kMaxRefs] = 1;
  }

  [kStart](filename, persistent, interval) {
    if (this._handle !== null) return;
    this._handle = new Poller(filename, interval, this[kBigint], (current, previous) => {
      this.emit('change', current, previous);
    });
    if (!persistent) this.unref();
  }

  [kAddOrCleanRef](operate) {
    if (operate === 'add') {
      this[kRefs]++;
      this[kMaxRefs]++;
    } else if (operate === 'clean') {
      this[kMaxRefs]--;
      this.unref();
    } else if (operate === 'cleanAll') {
      this[kMaxRefs] = 0;
      this[kRefs] = 0;
      if (this._handle !== null) this._handle.unref();
    }
  }

  stop() {
    if (this._handle === null) return;
    process.nextTick(() => this.emit('stop'));
    this._handle.close();
    this._handle = null;
  }

  ref() {
    if (this[kRefs] === this[kMaxRefs]) return this;
    if (this._handle && this[kRefs]++ === 0) this._handle.ref();
    return this;
  }

  unref() {
    if (this[kRefs] === 0) return this;
    if (this._handle && --this[kRefs] === 0) this._handle.unref();
    return this;
  }
}

function watchFile(filename, options, listener) {
  filename = path.resolve(S.getValidatedPath(filename).toString());
  if (options === null || typeof options !== 'object') {
    listener = options;
    options = null;
  }
  options = { interval: 5007, persistent: true, ...options };
  validateFunction(listener, 'listener');
  validateUint32(options.interval, 'interval');
  let watcher = watchers.get(filename);
  if (watcher === undefined) {
    watcher = new StatWatcher(options.bigint);
    watcher[kStart](filename, options.persistent, options.interval);
    watchers.set(filename, watcher);
  } else {
    watcher[kAddOrCleanRef]('add');
  }
  watcher.addListener('change', listener);
  return watcher;
}

function unwatchFile(filename, listener) {
  filename = path.resolve(S.getValidatedPath(filename).toString());
  const watcher = watchers.get(filename);
  if (watcher === undefined) return;
  if (typeof listener === 'function') {
    const before = watcher.listenerCount('change');
    watcher.removeListener('change', listener);
    if (watcher.listenerCount('change') < before) watcher[kAddOrCleanRef]('clean');
  } else {
    watcher.removeAllListeners('change');
    watcher[kAddOrCleanRef]('cleanAll');
  }
  if (watcher.listenerCount('change') === 0) {
    watcher.stop();
    watchers.delete(filename);
  }
}

module.exports = { StatWatcher, watchFile, unwatchFile };
