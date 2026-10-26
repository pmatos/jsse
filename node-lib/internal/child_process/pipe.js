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
    let pending = [];
    if (!this._closed) {
      this._closed = true;
      this._activity.deactivate();
      pending = this._pending;
      this._pending = [];
      for (let i = 0; i < pending.length; i++) binding.loopRef(-1);
      if (this._handle.id !== 0) binding.close(this._handle.id, this._index);
      this._handle.pipeClosed(this._index);
    }
    // Writes still in flight when the pipe goes (the child exited) fail with
    // EPIPE, as libuv reports them.
    const failure = pending.length > 0 ? new ErrnoException(-32, 'write') : null;
    callback(error ?? failure);
    for (let i = 0; i < pending.length; i++) pending[i](failure);
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
