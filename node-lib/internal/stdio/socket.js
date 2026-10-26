'use strict';

// The part of net.Socket the standard streams use (there is no net module):
// a Duplex over a file descriptor of the host floor. Writes are synchronous,
// as they are for files, pipes and TTYs in Node; reads of fd 0 come from the
// floor's on-demand reader, which keeps the event loop alive while reading.

const {
  Boolean,
  FunctionPrototypeCall,
  ObjectSetPrototypeOf,
} = primordials;

const { Duplex, Readable } = require('stream');
const host = require('internal/host/tty');

function Socket(options) {
  if (!(this instanceof Socket))
    return new Socket(options);

  FunctionPrototypeCall(Duplex, this, {
    readableHighWaterMark: options.readableHighWaterMark,
    allowHalfOpen: Boolean(options.allowHalfOpen),
    autoDestroy: true,
    decodeStrings: false,
    emitClose: false,
    readable: options.readable !== false,
    writable: options.writable !== false,
  });
  this.fd = options.fd;
  this.connecting = false;
  this._handle = new Handle(this, options.fd);
}

ObjectSetPrototypeOf(Socket.prototype, Duplex.prototype);
ObjectSetPrototypeOf(Socket, Duplex);

class Handle {
  constructor(socket, fd) {
    this.fd = fd;
    this.reading = false;
    this.onread = (chunk, err) => onRead(socket, this, chunk, err);
  }

  readStart() {
    this.reading = true;
    host.stdinStart(this.onread);
  }

  readStop() {
    this.reading = false;
    host.stdinStop();
  }

  ref() { if (this.fd === 0) host.stdinRef(true); }
  unref() { if (this.fd === 0) host.stdinRef(false); }
  setBlocking() {}
}

function onRead(socket, handle, chunk, err) {
  if (err) {
    handle.reading = false;
    socket.destroy(err);
    return;
  }
  if (chunk === null) {
    handle.reading = false;
    socket.push(null);
    socket.read(0);
    return;
  }
  const buffer = Buffer.from(chunk.buffer, chunk.byteOffset, chunk.length);
  if (!socket.push(buffer)) {
    handle.readStop();
  }
}

Socket.prototype._read = function(n) {
  const handle = this._handle;
  if (this.fd !== 0 || handle === null || handle.reading || this.destroyed)
    return;
  handle.readStart();
};

function writeError(err) {
  err.message = `${err.syscall} ${err.code}`;
  return err;
}

Socket.prototype._write = function(data, encoding, cb) {
  try {
    if (typeof data === 'string' && encoding !== 'utf8' && encoding !== 'utf-8')
      data = Buffer.from(data, encoding);
    host.write(this.fd, data);
  } catch (err) {
    cb(writeError(err));
    return;
  }
  cb();
};

Socket.prototype._final = function(cb) {
  cb();
};

Socket.prototype._destroy = function(err, cb) {
  const handle = this._handle;
  if (handle !== null && handle.reading) handle.readStop();
  cb(err);
  setImmediate(() => this.emit('close', !!err));
};

Socket.prototype.ref = function() {
  this._handle?.ref();
  return this;
};

Socket.prototype.unref = function() {
  this._handle?.unref();
  return this;
};

Socket.prototype.setBlocking = function() {
  return this;
};

// What Node makes of a regular file on fd 0 (an fs.ReadStream): a plain
// Readable over the same reader.
function ReadStream(options) {
  FunctionPrototypeCall(Readable, this, { emitClose: false });
  this.fd = options.fd;
  this._handle = new Handle(this, options.fd);
}

ObjectSetPrototypeOf(ReadStream.prototype, Readable.prototype);
ObjectSetPrototypeOf(ReadStream, Readable);

ReadStream.prototype._read = Socket.prototype._read;

ReadStream.prototype._destroy = function(err, cb) {
  const handle = this._handle;
  if (handle !== null && handle.reading) handle.readStop();
  cb(err);
};

module.exports = { Socket, ReadStream };
