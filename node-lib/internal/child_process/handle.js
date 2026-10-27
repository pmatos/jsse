'use strict';

// The libuv-style handles `lib/internal/child_process.js` expects, over the
// `__host_cp_*` floor: a Process handle and the table that routes native events
// (exit, data, eof, written) to it and to its stdio pipes.

const binding = require('internal/host/child_process');
const { signalName } = require('internal/child_process/support');

const UV_ESRCH = -3;
const kExit = 1;
const kData = 2;
const kEof = 3;
const kWritten = 4;

const handles = new Map();

// Keeps the event loop alive while `active` and ref'd, like a uv handle.
class Activity {
  constructor() {
    this.active = false;
    this.refed = true;
  }

  activate() {
    if (!this.active) {
      this.active = true;
      if (this.refed) binding.loopRef(1);
    }
  }

  deactivate() {
    if (this.active) {
      this.active = false;
      if (this.refed) binding.loopRef(-1);
    }
  }

  ref() {
    if (!this.refed) {
      this.refed = true;
      if (this.active) binding.loopRef(1);
    }
  }

  unref() {
    if (this.refed) {
      this.refed = false;
      if (this.active) binding.loopRef(-1);
    }
  }
}

function nativeStdio(entry) {
  switch (entry.type) {
    case 'ignore': return -1;
    case 'pipe':
    case 'overlapped': return -2;
    default: return entry.fd;
  }
}

class Process extends Activity {
  constructor() {
    super();
    this.id = 0;
    this.pid = undefined;
    this.onexit = undefined;
    this.pipes = [];
    this.openPipes = 0;
    this.exited = false;
  }

  spawn(file, args, cwd, envPairs, stdio, flags, uid, gid) {
    const result = binding.spawn(file, args, cwd, envPairs, stdio.map(nativeStdio),
                                 flags, uid, gid);
    if (typeof result === 'number') return result;
    this.id = result[0];
    this.pid = result[1];
    handles.set(this.id, this);
    this.activate();
    return 0;
  }

  kill(signal) {
    if (this.id === 0 || this.exited) return UV_ESRCH;
    return binding.killChild(this.id, signal);
  }

  addPipe(index, pipe) {
    this.pipes[index] = pipe;
    this.openPipes++;
  }

  pipeClosed(index) {
    if (this.pipes[index] === undefined) return;
    this.pipes[index] = undefined;
    this.openPipes--;
    this.release();
  }

  release() {
    if (this.id !== 0 && this.exited && this.openPipes === 0) {
      handles.delete(this.id);
      binding.release(this.id);
    }
  }

  close() {}

  exit(code, signal) {
    this.exited = true;
    this.deactivate();
    this.release();
    this.onexit(code, signal ? signalName(signal) : null);
  }
}

function dispatch(kind, id, x, y) {
  const handle = handles.get(id);
  if (handle === undefined) return;
  if (kind === kExit) {
    handle.exit(x, y);
    return;
  }
  const pipe = handle.pipes[x];
  if (pipe === undefined) return;
  if (kind === kData) pipe.onData(y);
  else if (kind === kEof) pipe.onEof();
  else if (kind === kWritten) pipe.onWritten(y);
}

binding.onEvent(dispatch);

module.exports = { Activity, Process, UV_ESRCH };
