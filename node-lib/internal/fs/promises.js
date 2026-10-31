'use strict';

// `fs.promises` (issue #1234): the callback API wrapped in promises, plus
// FileHandle. A synchronous validation failure of the callback function
// becomes a rejection, as it does for Node's async functions.

const { fs, support: S } = require('internal/host/fs');
const cbs = require('internal/fs/callbacks');
const EventEmitter = require('events');

const kRef = Symbol('kRef');
const kUnref = Symbol('kUnref');
const kFd = Symbol('kFd');
const kRefs = Symbol('kRefs');
const kClosePromise = Symbol('kClosePromise');
const kCloseResolve = Symbol('kCloseResolve');
const CHUNK = 64 * 1024;

function promisify(fn) {
  return function(...args) {
    return new Promise((resolve, reject) => {
      fn(...args, (error, value) => (error ? reject(error) : resolve(value)));
    });
  };
}

const call = {};
for (const name of [
  'access', 'chmod', 'chown', 'close', 'copyFile', 'cp', 'fchmod', 'fchown',
  'fdatasync', 'fstat', 'fsync', 'ftruncate', 'futimes', 'lchown', 'link',
  'lstat', 'lutimes', 'mkdir', 'mkdtemp', 'open', 'opendir', 'readdir',
  'readlink', 'rename', 'rm', 'rmdir', 'stat', 'statfs',
  'symlink', 'truncate', 'unlink', 'utimes',
]) {
  call[name] = promisify(cbs[name]);
}
call.realpath = promisify(cbs.realpath.native);
const readdirLevels = call.readdir;
const readdirDepthFirst = promisify(cbs.readdirDepthFirst);
call.readdir = (path, options) => (
  options !== null && typeof options === 'object' && options.recursive ?
    readdirDepthFirst(path, options) : readdirLevels(path, options));
const readFd = (fd, buffer, offset, length, position) => new Promise((resolve, reject) => {
  cbs.read(fd, buffer, offset, length, position, (error, bytesRead) => {
    if (error) reject(error);
    else resolve(bytesRead);
  });
});
const writeFd = (fd, data, a, b, c) => new Promise((resolve, reject) => {
  cbs.write(fd, data, a, b, c, (error, bytesWritten) => {
    if (error) reject(error);
    else resolve(bytesWritten);
  });
});

function encodeResult(buffer, options) {
  const encoding = typeof options === 'string' ? options : options && options.encoding;
  return encoding && encoding !== 'buffer' ? buffer.toString(encoding) : buffer;
}

async function readAll(fd) {
  const chunks = [];
  for (;;) {
    const chunk = S.Buffer.allocUnsafe(CHUNK);
    const n = await readFd(fd, chunk, 0, CHUNK, null);
    if (n === 0) break;
    chunks.push(n === CHUNK ? chunk : chunk.subarray(0, n));
  }
  return S.Buffer.concat(chunks);
}

async function writeAll(fd, data) {
  let view = data;
  while (view.byteLength > 0) {
    const n = await writeFd(fd, view, 0, view.byteLength, null);
    view = new Uint8Array(view.buffer, view.byteOffset + n, view.byteLength - n);
  }
}

function toBytes(data, options) {
  if (typeof data === 'string') {
    const encoding = (typeof options === 'string' ? options : options && options.encoding) || 'utf8';
    return S.Buffer.from(data, encoding);
  }
  S.validateDataArg(data, 'data');
  return S.bytesOf(data);
}

function closeFd(handle) {
  const fd = handle[kFd];
  handle[kFd] = -1;
  return call.close(fd);
}

class FileHandle extends EventEmitter {
  constructor(fd) {
    super();
    this[kFd] = fd;
    this[kRefs] = 1;
    this[kClosePromise] = null;
    this[kCloseResolve] = null;
  }

  get fd() { return this[kFd]; }

  [kRef]() { this[kRefs]++; }

  [kUnref]() {
    if (--this[kRefs] !== 0) return;
    const closing = closeFd(this);
    const pending = this[kCloseResolve];
    if (pending) {
      this[kCloseResolve] = null;
      closing.then(pending.resolve, pending.reject);
    } else {
      this[kClosePromise] = closing.finally(() => { this[kClosePromise] = null; });
    }
  }

  close() {
    if (this[kFd] === -1) return Promise.resolve();
    if (this[kClosePromise]) return this[kClosePromise];
    if (--this[kRefs] === 0) {
      this[kClosePromise] = closeFd(this).finally(() => { this[kClosePromise] = null; });
    } else {
      this[kClosePromise] = new Promise((resolve, reject) => {
        this[kCloseResolve] = { resolve, reject };
      }).finally(() => { this[kClosePromise] = null; });
    }
    this.emit('close');
    return this[kClosePromise];
  }

  async read(buffer, offset, length, position) {
    let options;
    if (!S.isView(buffer)) {
      options = buffer || {};
      buffer = options.buffer || S.Buffer.alloc(16384);
    } else if (offset !== null && typeof offset === 'object') {
      options = offset;
    } else {
      options = { offset, length, position };
    }
    const bytesRead = await readFd(this[kFd], buffer, options.offset === undefined ? 0 : options.offset,
                                   options.length === undefined ? buffer.byteLength - (options.offset || 0) : options.length,
                                   options.position === undefined ? null : options.position);
    return { bytesRead, buffer };
  }

  async write(buffer, ...rest) {
    const bytesWritten = await writeFd(this[kFd], buffer, ...rest);
    return { bytesWritten, buffer };
  }

  async readFile(options) {
    return encodeResult(await readAll(this[kFd]), options);
  }

  async writeFile(data, options) {
    await writeAll(this[kFd], toBytes(data, options));
  }

  appendFile(data, options) { return this.writeFile(data, options); }
  stat(options) { return call.fstat(this[kFd], options); }
  chmod(mode) { return call.fchmod(this[kFd], mode); }
  chown(uid, gid) { return call.fchown(this[kFd], uid, gid); }
  utimes(atime, mtime) { return call.futimes(this[kFd], atime, mtime); }
  truncate(len) { return call.ftruncate(this[kFd], len); }
  sync() { return call.fsync(this[kFd]); }
  datasync() { return call.fdatasync(this[kFd]); }

  createReadStream(options) {
    return new (require('internal/fs/streams').ReadStream)(undefined, { ...options, fd: this });
  }

  createWriteStream(options) {
    return new (require('internal/fs/streams').WriteStream)(undefined, { ...options, fd: this });
  }
}

if (Symbol.asyncDispose) {
  FileHandle.prototype[Symbol.asyncDispose] = function() { return this.close(); };
}

require('internal/fs/vectored').installFileHandle(FileHandle);

async function openHandle(path, flags, mode) {
  return new FileHandle(await call.open(path, flags, mode));
}

async function withFile(path, flags, fn) {
  const handle = await openHandle(path, flags);
  try {
    return await fn(handle);
  } finally {
    await handle.close();
  }
}

function readFile(path, options) {
  if (path instanceof FileHandle) return path.readFile(options);
  return promisify(cbs.readFile)(path, options);
}

async function writeFile(path, data, options) {
  if (path instanceof FileHandle) return path.writeFile(data, options);
  if (typeof data === 'string' || S.isView(data)) {
    return promisify(cbs.writeFile)(path, data, options);
  }
  if (data == null || (typeof data[Symbol.asyncIterator] !== 'function' &&
                       typeof data[Symbol.iterator] !== 'function')) {
    S.validateDataArg(data, 'data');
  }
  const flags = (options && options.flag) || 'w';
  return withFile(path, flags, async (handle) => {
    for await (const chunk of data) await handle.writeFile(chunk, options);
  });
}

function appendFile(path, data, options) {
  if (path instanceof FileHandle) return path.appendFile(data, options);
  return promisify(cbs.appendFile)(path, data, options);
}

module.exports = {
  kRef,
  kUnref,
  FileHandle,
  exports: {
    access: call.access,
    appendFile,
    chmod: call.chmod,
    chown: call.chown,
    constants: fs.constants,
    copyFile: call.copyFile,
    cp: call.cp,
    lchown: call.lchown,
    link: call.link,
    lstat: call.lstat,
    lutimes: call.lutimes,
    mkdir: call.mkdir,
    glob: (pattern, options) => new (require('internal/fs/glob').Glob)(pattern, options).glob(),
    mkdtemp: call.mkdtemp,
    open: openHandle,
    opendir: call.opendir,
    readFile,
    readdir: call.readdir,
    readlink: call.readlink,
    realpath: call.realpath,
    rename: call.rename,
    rm: call.rm,
    rmdir: call.rmdir,
    stat: call.stat,
    statfs: call.statfs,
    symlink: call.symlink,
    truncate: call.truncate,
    unlink: call.unlink,
    utimes: call.utimes,
    watch: (filename, options) => require('internal/fs/watch_promises').watch(filename, options),
    writeFile,
  },
};
