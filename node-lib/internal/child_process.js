'use strict';

// Ported from Node v26.8.2 lib/internal/child_process.js (MIT) without the IPC
// channel, handle passing and diagnostics channels; the process and pipe
// handles come from internal/child_process/{handle,pipe}.

const {
  ArrayIsArray,
  ArrayPrototypePush,
  ArrayPrototypeReduce,
  ArrayPrototypeSlice,
  FunctionPrototypeCall,
  ObjectSetPrototypeOf,
  SymbolDispose,
} = primordials;

const {
  ErrnoException,
  convertToValidSignal,
  signalName,
} = require('internal/child_process/support');
const {
  codes: {
    ERR_FEATURE_UNAVAILABLE_ON_PLATFORM,
    ERR_INVALID_ARG_VALUE,
    ERR_INVALID_SYNC_FORK_INPUT,
    ERR_IPC_ONE_PIPE,
    ERR_IPC_SYNC_FORK,
  },
} = require('internal/errors');
const {
  validateArray,
  validateObject,
  validateString,
} = require('internal/validators');
const EventEmitter = require('events');
const inspect = require('internal/util/inspect').inspect;
const { isArrayBufferView } = require('internal/util/types');
const { Buffer } = require('internal/host/buffer');
const binding = require('internal/host/child_process');
const { Process, UV_ESRCH } = require('internal/child_process/handle');
const { StdioPipe } = require('internal/child_process/pipe');

const UV_EACCES = -13;
const UV_EAGAIN = -11;
const UV_EINVAL = -22;
const UV_EMFILE = -24;
const UV_ENFILE = -23;
const UV_ENOENT = -2;
const UV_ENOSYS = -38;

function stdioStringToArray(stdio, channel) {
  const options = [];

  switch (stdio) {
    case 'ignore':
    case 'overlapped':
    case 'pipe': ArrayPrototypePush(options, stdio, stdio, stdio); break;
    case 'inherit': ArrayPrototypePush(options, 0, 1, 2); break;
    default:
      throw new ERR_INVALID_ARG_VALUE('stdio', stdio);
  }

  if (channel) ArrayPrototypePush(options, channel);

  return options;
}

function ChildProcess() {
  FunctionPrototypeCall(EventEmitter, this);

  this._closesNeeded = 1;
  this._closesGot = 0;
  this.connected = false;

  this.signalCode = null;
  this.exitCode = null;
  this.killed = false;
  this.spawnfile = null;

  this._handle = new Process();

  this._handle.onexit = (exitCode, signalCode) => {
    if (signalCode) {
      this.signalCode = signalCode;
    } else {
      this.exitCode = exitCode;
    }

    if (this.stdin) {
      this.stdin.destroy();
    }

    this._handle.close();
    this._handle = null;

    if (exitCode < 0) {
      const syscall = this.spawnfile ? 'spawn ' + this.spawnfile : 'spawn';
      const err = new ErrnoException(exitCode, syscall);

      if (this.spawnfile)
        err.path = this.spawnfile;

      err.spawnargs = ArrayPrototypeSlice(this.spawnargs, 1);
      this.emit('error', err);
    } else {
      this.emit('exit', this.exitCode, this.signalCode);
    }

    // Pull the data of untouched stdio streams through so they reach EOF and
    // emit 'close'; on nextTick so the user has one last chance to consume it.
    process.nextTick(flushStdio, this);

    maybeClose(this);
  };
}
ObjectSetPrototypeOf(ChildProcess.prototype, EventEmitter.prototype);
ObjectSetPrototypeOf(ChildProcess, EventEmitter);

function flushStdio(subprocess) {
  const stdio = subprocess.stdio;

  if (stdio == null) return;

  for (let i = 0; i < stdio.length; i++) {
    const stream = stdio[i];
    if (!stream || !stream.readable) {
      continue;
    }
    stream.resume();
  }
}

ChildProcess.prototype.spawn = function spawn(options) {
  let i = 0;

  validateObject(options, 'options');

  let stdio = options.stdio || 'pipe';

  stdio = getValidStdio(stdio, false);

  stdio = options.stdio = stdio.stdio;

  validateString(options.file, 'options.file');
  this.spawnfile = options.file;

  if (options.args === undefined) {
    this.spawnargs = [];
  } else {
    validateArray(options.args, 'options.args');
    this.spawnargs = options.args;
  }

  const spawnFlags = options.detached ? 1 : 0;

  const err = this._handle.spawn(
    options.file,
    options.args,
    options.cwd,
    options.envPairs,
    options.stdio,
    spawnFlags,
    options.uid,
    options.gid,
  );

  // Run-time errors should emit an error, not throw an exception.
  if (err === UV_EACCES ||
      err === UV_EAGAIN ||
      err === UV_EMFILE ||
      err === UV_ENFILE ||
      err === UV_ENOENT) {
    process.nextTick(onErrorNT, this, err);

    // There is no point in continuing when we've hit EMFILE or ENFILE
    // because we won't be able to set up the stdio file descriptors.
    if (err === UV_EMFILE || err === UV_ENFILE)
      return err;
  } else if (err) {
    this._handle.close();
    this._handle = null;

    throw new ErrnoException(err, 'spawn');
  } else {
    process.nextTick(onSpawnNT, this);
  }

  this.pid = this._handle.pid;

  for (i = 0; i < stdio.length; i++) {
    const stream = stdio[i];
    if (stream.type === 'ignore') continue;

    if (stream.type === 'pipe' || stream.type === 'overlapped') {
      stream.socket = new StdioPipe(this._handle, i, i > 0, i === 0 || i >= 3);

      if (i > 0) {
        this._closesNeeded++;
        stream.socket.on('close', () => {
          maybeClose(this);
        });
      }
    }
  }

  this.stdin = stdio.length >= 1 && stdio[0].socket !== undefined ?
    stdio[0].socket : null;
  this.stdout = stdio.length >= 2 && stdio[1].socket !== undefined ?
    stdio[1].socket : null;
  this.stderr = stdio.length >= 3 && stdio[2].socket !== undefined ?
    stdio[2].socket : null;

  this.stdio = [];

  for (i = 0; i < stdio.length; i++)
    ArrayPrototypePush(this.stdio,
                       stdio[i].socket === undefined ? null : stdio[i].socket);

  return err;
};

function onErrorNT(self, err) {
  self._handle.onexit(err);
}

function onSpawnNT(self) {
  self.emit('spawn');
}

ChildProcess.prototype.kill = function kill(sig) {
  const signal = sig === 0 ? sig :
    convertToValidSignal(sig === undefined ? 'SIGTERM' : sig);

  if (this._handle) {
    const err = this._handle.kill(signal);
    if (err === 0) {
      /* Success. */
      this.killed = true;
      return true;
    }
    if (err === UV_ESRCH) {
      /* Already dead. */
    } else if (err === UV_EINVAL || err === UV_ENOSYS) {
      /* The underlying platform doesn't support this signal. */
      throw new ErrnoException(err, 'kill');
    } else {
      /* Other error, almost certainly EPERM. */
      this.emit('error', new ErrnoException(err, 'kill'));
    }
  }

  /* Kill didn't succeed. */
  return false;
};

ChildProcess.prototype[SymbolDispose] = function() {
  if (!this.killed) {
    this.kill();
  }
};

ChildProcess.prototype.ref = function ref() {
  if (this._handle) this._handle.ref();
};

ChildProcess.prototype.unref = function unref() {
  if (this._handle) this._handle.unref();
};

function getValidStdio(stdio, sync) {
  if (typeof stdio === 'string') {
    stdio = stdioStringToArray(stdio);
  } else if (!ArrayIsArray(stdio)) {
    throw new ERR_INVALID_ARG_VALUE('stdio', stdio);
  }

  // At least 3 stdio will be created. Not concat()ed with a new Array(),
  // which would be sparse and skipped by reduce().
  while (stdio.length < 3) ArrayPrototypePush(stdio, undefined);

  let ipc;
  stdio = ArrayPrototypeReduce(stdio, (acc, stdio, i) => {
    stdio ??= i < 3 ? 'pipe' : 'ignore';

    if (stdio === 'ignore') {
      ArrayPrototypePush(acc, { type: 'ignore' });
    } else if (stdio === 'pipe' || stdio === 'overlapped' ||
               (typeof stdio === 'number' && stdio < 0)) {
      ArrayPrototypePush(acc, {
        type: stdio === 'overlapped' ? 'overlapped' : 'pipe',
        readable: i === 0,
        writable: i !== 0,
      });
    } else if (stdio === 'ipc') {
      if (sync || ipc !== undefined) {
        if (!sync)
          throw new ERR_IPC_ONE_PIPE();
        else
          throw new ERR_IPC_SYNC_FORK();
      }
      throw new ERR_FEATURE_UNAVAILABLE_ON_PLATFORM("stdio 'ipc'");
    } else if (stdio === 'inherit') {
      ArrayPrototypePush(acc, {
        type: 'inherit',
        fd: i,
      });
    } else if (typeof stdio === 'number' || typeof stdio.fd === 'number') {
      ArrayPrototypePush(acc, {
        type: 'fd',
        fd: typeof stdio === 'number' ? stdio : stdio.fd,
      });
    } else if (isArrayBufferView(stdio) || typeof stdio === 'string') {
      if (!sync) {
        throw new ERR_INVALID_SYNC_FORK_INPUT(inspect(stdio));
      }
    } else {
      throw new ERR_INVALID_ARG_VALUE('stdio', stdio);
    }

    return acc;
  }, []);

  return { stdio, ipc };
}

function maybeClose(subprocess) {
  subprocess._closesGot++;

  if (subprocess._closesGot === subprocess._closesNeeded) {
    subprocess.emit('close', subprocess.exitCode, subprocess.signalCode);
  }
}

function toBuffer(bytes) {
  return Buffer.from(bytes.buffer, bytes.byteOffset, bytes.length);
}

function toBytes(view) {
  return view instanceof Uint8Array ? view :
    new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
}

function spawnSync(options) {
  const stdio = options.stdio;
  const slots = [];
  const inputs = [];
  for (let i = 0; i < stdio.length; i++) {
    const entry = stdio[i];
    slots.push(entry.type === 'ignore' ? -1 :
      entry.type === 'pipe' || entry.type === 'overlapped' ? -2 : entry.fd);
    inputs.push(entry.input === undefined ? undefined : toBytes(entry.input));
  }
  const raw = binding.spawnSync(options.file, options.args, options.cwd,
                                options.envPairs, slots, options.detached ? 1 : 0,
                                options.uid, options.gid, inputs,
                                options.timeout || 0,
                                options.killSignal ?? convertToValidSignal('SIGTERM'),
                                options.maxBuffer);
  const result = {};
  if (raw.length === 1) {
    result.error = raw[0];
    result.status = null;
    result.signal = null;
    result.output = null;
    result.pid = 0;
  } else {
    if (raw[0] !== 0) result.error = raw[0];
    result.status = raw[2];
    result.signal = raw[3] === null ? null : signalName(raw[3]);
    result.output = [];
    for (let i = 4; i < raw.length; i++) {
      ArrayPrototypePush(result.output, raw[i] === null ? null : toBuffer(raw[i]));
    }
    result.pid = raw[1];
  }

  if (result.output && options.encoding && options.encoding !== 'buffer') {
    for (let i = 0; i < result.output.length; i++) {
      if (!result.output[i])
        continue;
      result.output[i] = result.output[i].toString(options.encoding);
    }
  }

  result.stdout = result.output?.[1];
  result.stderr = result.output?.[2];

  if (result.error) {
    result.error = new ErrnoException(result.error, 'spawnSync ' + options.file);
    result.error.path = options.file;
    result.error.spawnargs = ArrayPrototypeSlice(options.args, 1);
  }

  return result;
}

module.exports = {
  ChildProcess,
  getValidStdio,
  stdioStringToArray,
  spawnSync,
};
