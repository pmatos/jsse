'use strict';

const { Duplex } = require('stream');
const { Buffer } = require('internal/host/buffer');
const binding = require('internal/host/child_process');
const { Activity } = require('internal/child_process/handle');
const { ErrnoException } = require('internal/child_process/support');

// One stdio pipe of a child process; stands in for the `net.Socket` Node wraps
// around the pipe handle. A pipe whose handle never started (a failed spawn)
// reads as an immediate EOF.
class StdioPipe extends Duplex {
  constructor(handle, index, readable, writable) {
    super({ readable, writable, allowHalfOpen: false });
    this._handle = handle;
    this._index = index;
    this._activity = new Activity();
    this._pending = [];
    this._closed = false;
    handle.addPipe(index, this);
  }

  _read() {
    if (this._handle.id === 0) {
      process.nextTick(() => this.push(null));
      return;
    }
    this._activity.activate();
    binding.read(this._handle.id, this._index);
  }

  _write(chunk, encoding, callback) {
    if (this._handle.id === 0 || !binding.write(this._handle.id, this._index, chunk)) {
      callback(new ErrnoException(-32, 'write'));
      return;
    }
    binding.loopRef(1);
    this._pending.push(callback);
  }

  _final(callback) {
    if (this._handle.id !== 0) binding.shutdown(this._handle.id, this._index);
    callback();
  }

  _destroy(error, callback) {
    if (!this._closed) {
      this._closed = true;
      this._activity.deactivate();
      for (let i = 0; i < this._pending.length; i++) binding.loopRef(-1);
      this._pending = [];
      if (this._handle.id !== 0) binding.close(this._handle.id, this._index);
      this._handle.pipeClosed(this._index);
    }
    callback(error);
  }

  onData(bytes) {
    this.push(Buffer.from(bytes.buffer, bytes.byteOffset, bytes.length));
  }

  onEof() {
    this._activity.deactivate();
    this.push(null);
  }

  onWritten(errno) {
    const callback = this._pending.shift();
    if (callback === undefined) return;
    binding.loopRef(-1);
    callback(errno ? new ErrnoException(-errno, 'write') : null);
  }

  ref() {
    this._activity.ref();
    return this;
  }

  unref() {
    this._activity.unref();
    return this;
  }
}

module.exports = { StdioPipe };
