'use strict';

// The libuv `Pipe(IPC)` handle of an IPC channel (Node's `channel` argument of
// `setupChannel`): `onread` gets the text of each whole message a chunk held
// and then `null` at EOF, `write` takes one message and reports whether the
// pipe took it at once, `writeQueueSize` counts what the pipe has not taken.
// The native side frames messages. The channel keeps the loop alive while it
// reads and is ref'd, so `ref()`/`unref()` are the `Control` ones.

const binding = require('internal/host/child_process');
const { Activity } = require('internal/child_process/handle');

const UV_EPIPE = -32;
const UV_ECANCELED = -125;

class IpcPipe extends Activity {
  constructor(handle, index, serialization) {
    super();
    this.mode = serialization === 'advanced' ? 1 : 0;
    this.handle = handle;
    this.index = index;
    this.fd = index;
    this.onread = undefined;
    this.writeQueueSize = 0;
    this.lastWriteWasAsync = false;
    this.closed = false;
    this.reading = false;
    this.pending = [];
    handle.addPipe(index, this);
  }

  readStart() {
    if (this.closed || this.reading) return;
    this.reading = true;
    this.activate();
    if (this.handle.id === 0) {
      process.nextTick(() => this.onEof());
      return;
    }
    binding.ipcRead(this.handle.id, this.index, this.mode);
  }

  onMessages(items) {
    if (!this.closed) this.onread(items);
  }

  onEof() {
    if (this.closed) return;
    this.deactivate();
    this.onread(null);
  }

  write(text, oncomplete) {
    if (this.closed || this.handle.id === 0) return UV_EPIPE;
    const result = binding.ipcWrite(this.handle.id, this.index, this.mode, text);
    if (result === 0) {
      this.lastWriteWasAsync = false;
      return 0;
    }
    if (result === false) return UV_EPIPE;
    this.lastWriteWasAsync = true;
    this.writeQueueSize += text.length;
    this.pending.push({ size: text.length, oncomplete });
    return 0;
  }

  onWritten(errno) {
    const request = this.pending.shift();
    if (request === undefined) return;
    this.writeQueueSize -= request.size;
    request.oncomplete(errno ? -errno : 0);
  }

  close() {
    if (this.closed) return;
    this.closed = true;
    this.deactivate();
    const pending = this.pending;
    this.pending = [];
    this.writeQueueSize = 0;
    if (this.handle.id !== 0) binding.close(this.handle.id, this.index);
    this.handle.pipeClosed(this.index);
    for (let i = 0; i < pending.length; i++) pending[i].oncomplete(UV_ECANCELED);
  }
}

module.exports = { IpcPipe };
