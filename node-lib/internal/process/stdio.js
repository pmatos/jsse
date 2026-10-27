'use strict';

// process.stdin, process.stdout and process.stderr, created on first access as
// lib/internal/bootstrap/switches/is_main_thread.js does. Node picks the class
// by the kind of handle behind the descriptor: a TTY gets tty.WriteStream or
// tty.ReadStream, a file SyncWriteStream, a pipe or socket a net.Socket (here
// the Socket of internal/stdio/socket). console.log does not need the stream
// objects: until process.stdout is touched, nothing can have observed or
// replaced its write(), so console writes the descriptor directly.

const {
  ObjectDefineProperty,
} = primordials;

const host = require('internal/host/tty');

let stdin;
let stdout;
let stderr;

function createWritableStdioStream(fd) {
  let stream;
  switch (host.guessHandleType(fd)) {
    case 'TTY': {
      const tty = require('tty');
      stream = new tty.WriteStream(fd);
      stream._type = 'tty';
      break;
    }

    case 'FILE': {
      const SyncWriteStream = require('internal/fs/sync_write_stream');
      stream = new SyncWriteStream(fd, { autoClose: false });
      stream._type = 'fs';
      break;
    }

    case 'PIPE':
    case 'TCP': {
      const { Socket } = require('internal/stdio/socket');
      stream = new Socket({
        fd,
        readable: false,
        writable: true,
      });
      stream._type = 'pipe';
      break;
    }

    default: {
      const { Writable } = require('stream');
      stream = new Writable({
        write(buf, enc, cb) {
          cb();
        },
      });
    }
  }

  stream.fd = fd;
  stream._isStdio = true;
  return stream;
}

function dummyDestroy(err, cb) {
  cb(err);
  this._undestroy();

  // The fd is never closed, but 'close' must still be observable.
  if (!this._writableState.emitClose) {
    process.nextTick(() => {
      this.emit('close');
    });
  }
}

function neverClose(stream) {
  stream.destroySoon = stream.destroy;
  stream._destroy = dummyDestroy;
}

function getStdout() {
  if (stdout) return stdout;
  stdout = createWritableStdioStream(1);
  neverClose(stdout);
  return stdout;
}

function getStderr() {
  if (stderr) return stderr;
  stderr = createWritableStdioStream(2);
  neverClose(stderr);
  return stderr;
}

function getStdin() {
  if (stdin) return stdin;
  const fd = 0;

  switch (host.guessHandleType(fd)) {
    case 'TTY': {
      const tty = require('tty');
      stdin = new tty.ReadStream(fd);
      break;
    }

    case 'FILE': {
      const { ReadStream } = require('internal/stdio/socket');
      stdin = new ReadStream({ fd });
      break;
    }

    case 'PIPE':
    case 'TCP': {
      const { Socket } = require('internal/stdio/socket');
      stdin = new Socket({
        fd,
        readable: true,
        writable: false,
      });
      break;
    }

    default: {
      const { Readable } = require('stream');
      stdin = new Readable({ read() {} });
      stdin.push(null);
    }
  }

  stdin.fd = fd;

  // The stream starts paused: make sure the reader is not running, and stop
  // it again once pause() takes effect so that the process can exit.
  if (stdin._handle?.readStop) {
    stdin._handle.reading = false;
    stdin._readableState.reading = false;
  }

  stdin.on('pause', () => {
    process.nextTick(onpause);
  });

  function onpause() {
    if (!stdin._handle)
      return;
    if (stdin._handle.reading && !stdin.readableFlowing) {
      stdin._readableState.reading = false;
      stdin._handle.readStop();
    }
  }

  return stdin;
}

function defineStream(name, getter) {
  ObjectDefineProperty(process, name, {
    __proto__: null,
    configurable: true,
    enumerable: true,
    get: getter,
  });
}

defineStream('stdout', getStdout);
defineStream('stdin', getStdin);
defineStream('stderr', getStderr);

function noop() {}

// What Console#write does with a stream: a failing write is never thrown.
function consoleWrite(stream, string) {
  const errorHandler = (err) => {
    if (err !== null && !stream._writableState.errorEmitted &&
        stream.listenerCount('error') === 0) {
      stream.once('error', noop);
    }
  };
  try {
    if (stream.listenerCount('error') === 0) {
      stream.once('error', noop);
    }
    stream.write(string, errorHandler);
  } catch {
    // Nowhere to report it.
  } finally {
    stream.removeListener('error', noop);
  }
}

// console.log / console.error: through the stream once it exists, otherwise
// straight to the descriptor.
function write(fd, string) {
  const stream = fd === 1 ? stdout : stderr;
  if (stream !== undefined) {
    consoleWrite(stream, string);
    return;
  }
  try {
    host.write(fd, string);
  } catch {
    // Nowhere to report it.
  }
}

// The stream console decides colors by: a descriptor that is not a TTY never
// colors, and finding that out does not need the stream object.
const isTTY = [undefined, undefined, undefined];

function colorStream(fd) {
  isTTY[fd] ??= host.isTTY(fd);
  if (!isTTY[fd]) return undefined;
  return fd === 1 ? getStdout() : getStderr();
}

module.exports = { write, colorStream };
