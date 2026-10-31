'use strict';

// `options.signal` of readFile/writeFile/appendFile in the callback API, the
// promise API and the FileHandle forms (issues #1341, #1505). A call that
// carries a signal is not one pool job: it is Node's own sequence of steps
// (open, fstat, chunked read or write, close) with the signal checked at the
// same points, so an abort lands between steps and leaves on disk what Node
// leaves. Calls without a signal keep the whole-file jobs of callbacks.js.
//
// Callback API (lib/fs.js): readFile checks before the open and before every
// chunk read (or once after a file of at most one chunk, which Node reads in a
// single pool job); writeFile checks before the open and before every write,
// and a file it opened is closed before the error is reported.
// Promise API (lib/internal/fs/promises.js): the same, with writes in chunks of
// 512 KiB and the check after each.

const { support: S } = require('internal/host/fs');
const { AbortError, aggregateTwoErrors } = require('internal/errors');
const { validateAbortSignal } = require('internal/validators');

const CHUNK = 512 * 1024;
const UNKNOWN_CHUNK = 64 * 1024;
const Buffer = S.Buffer;

let cbs;
const fsCb = () => (cbs ??= require('internal/fs/callbacks'));
const lift = (name) => (...args) => new Promise((resolve, reject) => {
  fsCb()[name](...args, (error, value) => (error ? reject(error) : resolve(value)));
});
const openFd = lift('open');
const closeFd = lift('close');
const fstatFd = lift('fstat');
const fsyncFd = lift('fsync');
const readFd = (fd, buffer, length) => new Promise((resolve, reject) => {
  fsCb().read(fd, buffer, 0, length, null, (error, n) => (error ? reject(error) : resolve(n)));
});
const writeFd = (fd, buffer, offset, length) => new Promise((resolve, reject) => {
  fsCb().write(fd, buffer, offset, length, null, (error, n) => (error ? reject(error) : resolve(n)));
});

const abortError = (signal) => new AbortError(undefined, { cause: signal.reason });
function checkAborted(signal) {
  if (signal?.aborted) throw abortError(signal);
}

const hasSignal = (options) => (
  options !== null && typeof options === 'object' && options.signal !== undefined);

function signalOf(options) {
  validateAbortSignal(options.signal, 'options.signal');
  return options.signal;
}

const isCustomIterable = (data) => (
  data != null && typeof data !== 'string' && !S.isView(data) &&
  (typeof data[Symbol.iterator] === 'function' || typeof data[Symbol.asyncIterator] === 'function'));

// The step after an operation on a file this call opened: close it, and keep
// the operation's error ahead of a close error.
async function closing(fd, work) {
  let result;
  try {
    result = await work();
  } catch (error) {
    try {
      await closeFd(fd);
    } catch (closeError) {
      throw aggregateTwoErrors(closeError, error);
    }
    throw error;
  }
  await closeFd(fd);
  return result;
}

// ---- readFile ----------------------------------------------------------

// A read can block for good (a FIFO nobody writes to). Node only notices an
// abort between steps, so such a read never fails there; here the abort is
// delivered when it happens and the read finishes, and closes its file, behind it.
function raceAbort(promise, signal) {
  return new Promise((resolve, reject) => {
    const onAbort = () => reject(abortError(signal));
    signal.addEventListener('abort', onAbort, { once: true });
    const done = (settle) => (value) => {
      signal.removeEventListener('abort', onAbort);
      settle(value);
    };
    promise.then(done(resolve), done(reject));
  });
}

function decode(buffer, encoding) {
  return encoding ? buffer.toString(encoding) : buffer;
}

// Read an open file to its end. `owned`: the call opened it and Node reads
// files of at most one chunk in one job, so only a completed read is checked.
async function readContents(fd, signal, state) {
  if (!state.owned) checkAborted(signal);
  const stats = await fstatFd(fd);
  if (!state.owned) checkAborted(signal);
  const size = stats.isFile() ? stats.size : 0;
  state.oneShot = state.owned && size <= CHUNK;
  const chunks = [];
  let total = 0;
  for (;;) {
    if (!state.oneShot) checkAborted(signal);
    const want = size === 0 ? UNKNOWN_CHUNK : Math.min(CHUNK, size - total);
    const buffer = Buffer.allocUnsafe(want);
    const n = await readFd(fd, buffer, want);
    if (n === 0) break;
    total += n;
    chunks.push(n === want ? buffer : buffer.subarray(0, n));
    if (size !== 0 && total === size) break;
  }
  return chunks.length === 1 ? chunks[0] : Buffer.concat(chunks, total);
}

async function readFilePath(path, flag, encoding, signal) {
  checkAborted(signal);
  path = S.getValidatedPath(path);
  flag = S.stringToFlags(flag || 'r', 'options.flag');
  const state = { owned: true, oneShot: false };
  const fd = await openFd(path, flag, 0o666);
  const buffer = await closing(fd, () => readContents(fd, signal, state));
  if (state.oneShot) checkAborted(signal);
  return decode(buffer, encoding);
}

async function underRef(handle, name, work) {
  if (handle.fd === -1) {
    const error = new Error('file closed');
    error.code = 'EBADF';
    error.syscall = name;
    throw error;
  }
  const { kRef, kUnref } = require('internal/fs/promises');
  handle[kRef]();
  try {
    return await work(handle.fd);
  } finally {
    handle[kUnref]();
  }
}

// fs.promises.readFile(path | handle, options) and FileHandle.readFile.
async function readFile(path, options, handle) {
  options = S.getOptions(options, { flag: 'r' });
  const signal = signalOf(options);
  if (handle) {
    return raceAbort(underRef(handle, 'readFile', async (fd) => decode(
      await readContents(fd, signal, { owned: false }), options.encoding)), signal);
  }
  return raceAbort(readFilePath(path, options.flag, options.encoding, signal), signal);
}

function readFileCallback(path, options, callback) {
  options = S.getOptions(options, { flag: 'r' });
  const signal = signalOf(options);
  let run;
  if (S.isInt32(path)) {
    S.getValidatedFd(path);
    run = async () => decode(await readContents(path, signal, { owned: false }), options.encoding);
  } else {
    if (signal.aborted) {
      callback(abortError(signal));
      return;
    }
    const target = S.getValidatedPath(path);
    const flag = S.stringToFlags(options.flag, 'options.flag');
    run = () => readFilePath(target, flag, options.encoding, signal);
  }
  settle(raceAbort(run(), signal), callback);
}

function settle(promise, callback) {
  promise.then(
    (value) => process.nextTick(callback, null, value),
    (error) => process.nextTick(callback, error));
}

// ---- writeFile / appendFile --------------------------------------------

function normalizeWrite(data, options, append) {
  options = S.getOptions(options, { encoding: 'utf8', mode: 0o666, flag: append ? 'a' : 'w' });
  const flush = options.flush ?? false;
  S.validateBoolean(flush, 'options.flush');
  const mode = S.parseFileMode(options.mode, 'mode', 0o666);
  return { options, flush, mode };
}

const toBuffer = (data, options) => {
  if (S.isView(data)) return data;
  S.validateDataArg(data, 'data');
  return Buffer.from(data, options.encoding || 'utf8');
};

// Callback writeFile: one write of the whole buffer (more only after a short
// write), the signal checked before each.
function writeAll(fd, isUserFd, buffer, offset, length, signal, flush, callback) {
  const finish = (error) => {
    if (isUserFd) {
      callback(error);
    } else {
      fsCb().close(fd, (closeError) => callback(error ? aggregateTwoErrors(closeError, error) : closeError));
    }
  };
  if (signal.aborted) {
    finish(abortError(signal));
    return;
  }
  fsCb().write(fd, buffer, offset, length, null, (writeError, written) => {
    if (writeError) {
      finish(writeError);
    } else if (written === length) {
      if (!flush) {
        if (isUserFd) callback(null);
        else fsCb().close(fd, callback);
      } else {
        fsCb().fsync(fd, (syncError) => (syncError ? finish(syncError) : (isUserFd ? callback(null) : fsCb().close(fd, callback))));
      }
    } else {
      writeAll(fd, isUserFd, buffer, offset + written, length - written, signal, flush, callback);
    }
  });
}

function writeFileCallback(path, data, options, callback, append) {
  const { options: opts, flush, mode } = normalizeWrite(data, options, append);
  const signal = signalOf(opts);
  let flag = opts.flag || (append ? 'a' : 'w');
  if (append && S.isInt32(path)) flag = 'a';
  const buffer = toBuffer(data, opts);
  if (S.isInt32(path)) {
    writeAll(path, true, buffer, 0, buffer.byteLength, signal, flush, callback);
    return;
  }
  if (signal.aborted) {
    callback(abortError(signal));
    return;
  }
  fsCb().open(path, flag, mode, (openError, fd) => {
    if (openError) callback(openError);
    else writeAll(fd, false, buffer, 0, buffer.byteLength, signal, flush, callback);
  });
}

// Node's writeFileHandle: the signal is checked before the first write and
// after each, writes are at most 512 KiB, an iterable's chunks are each written.
async function writeContents(fd, data, signal, encoding) {
  checkAborted(signal);
  if (isCustomIterable(data)) {
    for await (const chunk of data) {
      checkAborted(signal);
      const bytes = S.isView(chunk) ? chunk : Buffer.from(chunk, encoding || 'utf8');
      let remaining = bytes.byteLength;
      while (remaining > 0) {
        const n = await writeFd(fd, bytes, bytes.byteLength - remaining, Math.min(CHUNK, remaining));
        remaining -= n;
        checkAborted(signal);
      }
    }
    return;
  }
  let view = new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  let remaining = view.byteLength;
  if (remaining === 0) return;
  do {
    checkAborted(signal);
    const n = await writeFd(fd, view, 0, Math.min(CHUNK, view.byteLength));
    remaining -= n;
    view = new Uint8Array(view.buffer, view.byteOffset + n, view.byteLength - n);
  } while (remaining > 0);
}

// fs.promises.writeFile / appendFile (path | handle) and the FileHandle methods.
async function writeFile(path, data, options, append, handle) {
  const { options: opts, flush, mode } = normalizeWrite(data, options, append);
  const signal = signalOf(opts);
  const flag = opts.flag || (append ? 'a' : 'w');
  if (!S.isView(data) && !isCustomIterable(data)) data = toBuffer(data, opts);
  const write = (fd) => writeContents(fd, data, signal, opts.encoding);
  if (handle) return underRef(handle, append ? 'appendFile' : 'writeFile', write);
  checkAborted(signal);
  const fd = await openFd(path, flag, mode);
  return closing(fd, async () => {
    await write(fd);
    if (flush) await fsyncFd(fd);
  });
}

// stat/lstat/fstat: an aborted signal fails the call before it starts, one that
// fires first fails it at once and the result that follows is dropped.
function bindStat(options, callback) {
  const signal = signalOf(options);
  if (signal.aborted) {
    callback(abortError(signal));
    return null;
  }
  let aborted = false;
  const onAbort = () => {
    aborted = true;
    callback(abortError(signal));
  };
  signal.addEventListener('abort', onAbort, { once: true });
  return (error, ...rest) => {
    signal.removeEventListener('abort', onAbort);
    if (!aborted) callback(error, ...rest);
  };
}

module.exports = {
  bindStat,
  hasSignal,
  readFile,
  readFileCallback,
  writeFile,
  writeFileCallback,
};
