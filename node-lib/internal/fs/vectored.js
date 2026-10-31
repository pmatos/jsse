'use strict';

// Vectored I/O and the remaining fs entry points of issue #1341: `readv`,
// `writev` and their synchronous forms, `FileHandle.readv/writev/readLines`
// and `openAsBlob`. A vector call is one `read`/`write` of the buffers'
// combined length on a temporary, scattered to or gathered from the caller's
// buffers, which is what `readv(2)`/`writev(2)` do for a single call.

const { support: S, async: hostAsync } = require('internal/host/fs');
const { kCustomPromisifyArgsSymbol } = require('internal/util');

const host = S.host;

function validateBufferArray(buffers) {
  if (!Array.isArray(buffers)) {
    throw S.invalidArgType('buffers', 'an ArrayBufferView[]', buffers);
  }
  for (let i = 0; i < buffers.length; i++) {
    if (!ArrayBuffer.isView(buffers[i])) {
      throw S.invalidArgType('buffers', 'an ArrayBufferView[]', buffers);
    }
  }
}

function checkCallback(cb) {
  if (typeof cb !== 'function') {
    throw S.invalidArgType('cb', 'of type function', cb);
  }
}

const positionOf = (position) => (typeof position === 'number' ? position : -1);
const totalOf = (buffers) => buffers.reduce((sum, b) => sum + b.byteLength, 0);

function gather(buffers, total) {
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const view of buffers) {
    bytes.set(new Uint8Array(view.buffer, view.byteOffset, view.byteLength), offset);
    offset += view.byteLength;
  }
  return bytes;
}

function scatter(bytes, count, buffers) {
  let offset = 0;
  for (const view of buffers) {
    if (offset >= count) break;
    const take = Math.min(view.byteLength, count - offset);
    new Uint8Array(view.buffer, view.byteOffset, take).set(bytes.subarray(offset, offset + take));
    offset += take;
  }
}

// libuv refuses a vector read of no buffers.
function emptyVectorError() {
  const error = new Error('EINVAL: invalid argument, read');
  error.errno = -22;
  error.code = 'EINVAL';
  error.syscall = 'read';
  return error;
}

function readvSync(fd, buffers, position) {
  S.getValidatedFd(fd);
  validateBufferArray(buffers);
  if (buffers.length === 0) throw emptyVectorError();
  const total = totalOf(buffers);
  const bytes = new Uint8Array(total);
  const count = host.read(fd, bytes, 0, total, positionOf(position));
  scatter(bytes, count, buffers);
  return count;
}

function writevSync(fd, buffers, position) {
  S.getValidatedFd(fd);
  validateBufferArray(buffers);
  if (buffers.length === 0) return 0;
  const total = totalOf(buffers);
  return host.write(fd, gather(buffers, total), 0, total, positionOf(position));
}

function readv(fd, buffers, position, callback) {
  S.getValidatedFd(fd);
  validateBufferArray(buffers);
  callback ||= position;
  checkCallback(callback);
  if (buffers.length === 0) {
    process.nextTick(callback, emptyVectorError(), 0, buffers);
    return;
  }
  const total = totalOf(buffers);
  const bytes = new Uint8Array(total);
  hostAsync('read', (error, count) => {
    if (error) {
      callback(error, 0, buffers);
      return;
    }
    scatter(bytes, count, buffers);
    callback(null, count, buffers);
  }, fd, bytes, 0, total, positionOf(position));
}

function writev(fd, buffers, position, callback) {
  S.getValidatedFd(fd);
  validateBufferArray(buffers);
  callback ||= position;
  checkCallback(callback);
  if (buffers.length === 0) {
    process.nextTick(callback, null, 0, buffers);
    return;
  }
  const total = totalOf(buffers);
  hostAsync('write', (error, count) => {
    if (error) callback(error, 0, buffers);
    else callback(null, count, buffers);
  }, fd, gather(buffers, total), 0, total, positionOf(position));
}

readv[kCustomPromisifyArgsSymbol] = ['bytesRead', 'buffers'];
writev[kCustomPromisifyArgsSymbol] = ['bytesWritten', 'buffer'];

// Node opens the file when called (a missing file throws at once) and reads it
// lazily; here the bytes are copied when the Blob is made.
function openAsBlob(path, options) {
  path = S.getValidatedPath(path);
  if (options !== undefined) S.validateObject(options, 'options');
  const type = (options && options.type) || '';
  if (typeof type !== 'string') {
    throw S.invalidArgType('options.type', 'of type string', type);
  }
  try {
    host.access(path, 4);
  } catch {
    const error = new TypeError('Unable to open file as blob');
    error.code = 'ERR_INVALID_ARG_VALUE';
    throw error;
  }
  const { Blob } = require('buffer');
  return new Promise((resolve, reject) => {
    hostAsync('read_file', (error, bytes) => {
      if (error) reject(error);
      else resolve(new Blob([bytes], { type }));
    }, path, S.stringToFlags('r'), false);
  });
}

function installFileHandle(FileHandle) {
  const vector = (call) => function(buffers, position) {
    return new Promise((resolve, reject) => {
      try {
        call(this.fd, buffers, position, (error, count) => {
          if (error) reject(error);
          else resolve({ count, buffers });
        });
      } catch (error) {
        reject(error);
      }
    });
  };
  const readvAsync = vector(readv);
  const writevAsync = vector(writev);
  FileHandle.prototype.readv = async function readvOnHandle(buffers, position) {
    const { count } = await readvAsync.call(this, buffers, position);
    return { bytesRead: count, buffers };
  };
  FileHandle.prototype.writev = async function writevOnHandle(buffers, position) {
    const { count } = await writevAsync.call(this, buffers, position);
    return { bytesWritten: count, buffers };
  };
  FileHandle.prototype.readLines = function readLines(options) {
    return require('readline').createInterface({
      input: this.createReadStream(options),
      crlfDelay: Infinity,
    });
  };
}

module.exports = { readv, writev, readvSync, writevSync, openAsBlob, installFileHandle };
