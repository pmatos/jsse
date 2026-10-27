'use strict';

const {
  NumberIsInteger,
  ObjectSetPrototypeOf,
} = primordials;

const { Socket } = require('internal/stdio/socket');
const host = require('internal/host/tty');
const {
  codes: {
    ERR_INVALID_FD,
    ERR_INVALID_ARG_VALUE,
    ERR_METHOD_NOT_IMPLEMENTED,
  },
} = require('internal/errors');
const {
  getColorDepth,
  hasColors,
} = require('internal/tty');

// Lazy loaded for startup performance.
let readline;

function isatty(fd) {
  return NumberIsInteger(fd) && fd >= 0 && fd <= 2147483647 &&
         host.isTTY(fd);
}

function ReadStream(fd, options) {
  if (!(this instanceof ReadStream))
    return new ReadStream(fd, options);
  if (fd >> 0 !== fd || fd < 0)
    throw new ERR_INVALID_FD(fd);
  if (fd !== 0)
    throw new ERR_METHOD_NOT_IMPLEMENTED('tty.ReadStream on a descriptor other than 0');

  Socket.call(this, {
    readableHighWaterMark: 0,
    ...options,
    fd,
  });

  this.isRaw = false;
  this.rawMode = false;
  this.isTTY = true;
}

ObjectSetPrototypeOf(ReadStream.prototype, Socket.prototype);
ObjectSetPrototypeOf(ReadStream, Socket);

ReadStream.prototype.setRawMode = function(mode) {
  let rawMode;
  if (mode === 'io' || mode === 'raw') {
    rawMode = mode;
  } else if (typeof mode === 'string') {
    throw new ERR_INVALID_ARG_VALUE(
      'mode', mode, "must be true, false, 'raw', or 'io'");
  } else {
    rawMode = mode ? 'raw' : false;
  }
  const err = host.setRawMode(this.fd, rawMode !== false);
  if (err) {
    const error = new Error(`setRawMode ${err.code}`);
    error.errno = err.errno;
    error.code = err.code;
    error.syscall = 'setRawMode';
    this.emit('error', error);
    return this;
  }
  this.isRaw = rawMode !== false;
  this.rawMode = rawMode;
  return this;
};

function WriteStream(fd) {
  if (!(this instanceof WriteStream))
    return new WriteStream(fd);
  if (fd >> 0 !== fd || fd < 0)
    throw new ERR_INVALID_FD(fd);

  Socket.call(this, {
    readableHighWaterMark: 0,
    fd,
  });

  const winSize = host.getWindowSize(fd);
  if (winSize !== undefined) {
    this.columns = winSize[0];
    this.rows = winSize[1];
  }
}

ObjectSetPrototypeOf(WriteStream.prototype, Socket.prototype);
ObjectSetPrototypeOf(WriteStream, Socket);

WriteStream.prototype.isTTY = true;

WriteStream.prototype.getColorDepth = getColorDepth;

WriteStream.prototype.hasColors = hasColors;

WriteStream.prototype._refreshSize = function() {
  const oldCols = this.columns;
  const oldRows = this.rows;
  const winSize = host.getWindowSize(this.fd);
  if (winSize === undefined) return;
  const { 0: newCols, 1: newRows } = winSize;
  if (oldCols !== newCols || oldRows !== newRows) {
    this.columns = newCols;
    this.rows = newRows;
    this.emit('resize');
  }
};

// Backwards-compat
WriteStream.prototype.cursorTo = function(x, y, callback) {
  if (readline === undefined) readline = require('readline');
  return readline.cursorTo(this, x, y, callback);
};
WriteStream.prototype.moveCursor = function(dx, dy, callback) {
  if (readline === undefined) readline = require('readline');
  return readline.moveCursor(this, dx, dy, callback);
};
WriteStream.prototype.clearLine = function(dir, callback) {
  if (readline === undefined) readline = require('readline');
  return readline.clearLine(this, dir, callback);
};
WriteStream.prototype.clearScreenDown = function(callback) {
  if (readline === undefined) readline = require('readline');
  return readline.clearScreenDown(this, callback);
};
WriteStream.prototype.getWindowSize = function() {
  return [this.columns, this.rows];
};

module.exports = { isatty, ReadStream, WriteStream };
