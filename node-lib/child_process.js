'use strict';

const {
  ArrayIsArray,
  ArrayPrototypeJoin,
  ArrayPrototypePush,
  ArrayPrototypePushApply,
  ObjectAssign,
  ObjectDefineProperty,
  PromiseWithResolvers,
  StringPrototypeSlice,
  SymbolDispose,
} = primordials;

const {
  assignFunctionName,
  getSystemErrorName,
  kEmptyObject,
  promisify,
} = require('internal/util');
const { isArrayBufferView } = require('internal/util/types');
let debug = require('internal/util/debuglog').debuglog(
  'child_process',
  (fn) => {
    debug = fn;
  },
);
const { Buffer } = require('internal/host/buffer');

const {
  AbortError,
  codes: {
    ERR_CHILD_PROCESS_STDIO_MAXBUFFER,
    ERR_INVALID_ARG_TYPE,
  },
  genericNodeError,
} = require('internal/errors');
const { clearTimeout, setTimeout } = require('timers');
const { validateAbortSignal } = require('internal/validators');
const child_process = require('internal/child_process');
const {
  getValidStdio,
  ChildProcess,
} = child_process;
const {
  normalizeExecArgs,
  normalizeExecFileArgs,
  normalizeSpawnArguments,
  sanitizeKillSignal,
  validateMaxBuffer,
  validateTimeout,
} = require('internal/child_process/normalize');

const MAX_BUFFER = 1024 * 1024;

let addAbortListener;

function exec(command, options, callback) {
  const opts = normalizeExecArgs(command, options, callback);
  return module.exports.execFile(opts.file,
                                 opts.options,
                                 opts.callback);
}

const customPromiseExecFunction = (orig) => {
  return assignFunctionName(orig.name, function(...args) {
    const { promise, resolve, reject } = PromiseWithResolvers();

    promise.child = orig(...args, (err, stdout, stderr) => {
      if (err !== null) {
        err.stdout = stdout;
        err.stderr = stderr;
        reject(err);
      } else {
        resolve({ stdout, stderr });
      }
    });

    return promise;
  });
};

ObjectDefineProperty(exec, promisify.custom, {
  __proto__: null,
  enumerable: false,
  value: customPromiseExecFunction(exec),
});

function execFile(file, args, options, callback) {
  ({ file, args, options, callback } = normalizeExecFileArgs(file, args, options, callback));

  options = {
    __proto__: null,
    encoding: 'utf8',
    timeout: 0,
    maxBuffer: MAX_BUFFER,
    killSignal: 'SIGTERM',
    cwd: null,
    env: null,
    shell: false,
    ...options,
  };

  // Validate the timeout, if present.
  validateTimeout(options.timeout);

  // Validate maxBuffer, if present.
  validateMaxBuffer(options.maxBuffer);

  options.killSignal = sanitizeKillSignal(options.killSignal);

  const child = spawn(file, args, {
    cwd: options.cwd,
    env: options.env,
    gid: options.gid,
    shell: options.shell,
    signal: options.signal,
    uid: options.uid,
    windowsHide: !!options.windowsHide,
    windowsVerbatimArguments: !!options.windowsVerbatimArguments,
  });

  let encoding;
  const _stdout = [];
  const _stderr = [];
  if (options.encoding !== 'buffer' && Buffer.isEncoding(options.encoding)) {
    encoding = options.encoding;
  } else {
    encoding = null;
  }
  let stdoutLen = 0;
  let stderrLen = 0;
  let killed = false;
  let exited = false;
  let timeoutId;

  let ex = null;

  let cmd = file;

  function exithandler(code, signal) {
    if (exited) return;
    exited = true;

    if (timeoutId) {
      clearTimeout(timeoutId);
      timeoutId = null;
    }

    if (!callback) return;

    // merge chunks
    let stdout;
    let stderr;
    if (encoding ||
      (
        child.stdout?.readableEncoding
      )) {
      stdout = ArrayPrototypeJoin(_stdout, '');
    } else {
      stdout = Buffer.concat(_stdout);
    }
    if (encoding ||
      (
        child.stderr?.readableEncoding
      )) {
      stderr = ArrayPrototypeJoin(_stderr, '');
    } else {
      stderr = Buffer.concat(_stderr);
    }

    if (!ex && code === 0 && signal === null) {
      callback(null, stdout, stderr);
      return;
    }

    if (args?.length)
      cmd += ` ${ArrayPrototypeJoin(args, ' ')}`;

    ex ||= genericNodeError(`Command failed: ${cmd}\n${stderr}`, {
      code: code < 0 ? getSystemErrorName(code) : code,
      killed: child.killed || killed,
      signal: signal,
    });

    ex.cmd = cmd;
    callback(ex, stdout, stderr);
  }

  function errorhandler(e) {
    ex = e;

    if (child.stdout)
      child.stdout.destroy();

    if (child.stderr)
      child.stderr.destroy();

    exithandler();
  }

  function kill() {
    if (child.stdout)
      child.stdout.destroy();

    if (child.stderr)
      child.stderr.destroy();

    killed = true;
    try {
      child.kill(options.killSignal);
    } catch (e) {
      ex = e;
      exithandler();
    }
  }

  if (options.timeout > 0) {
    timeoutId = setTimeout(function delayedKill() {
      kill();
      timeoutId = null;
    }, options.timeout);
  }

  if (child.stdout) {
    if (encoding)
      child.stdout.setEncoding(encoding);

    child.stdout.on('data', function onChildStdout(chunk) {
      // Do not need to count the length
      if (options.maxBuffer === Infinity) {
        ArrayPrototypePush(_stdout, chunk);
        return;
      }
      const encoding = child.stdout.readableEncoding;
      const length = encoding ?
        Buffer.byteLength(chunk, encoding) :
        chunk.length;
      const slice = encoding ? StringPrototypeSlice :
        (buf, ...args) => buf.slice(...args);
      stdoutLen += length;

      if (stdoutLen > options.maxBuffer) {
        const truncatedLen = options.maxBuffer - (stdoutLen - length);
        ArrayPrototypePush(_stdout, slice(chunk, 0, truncatedLen));

        ex = new ERR_CHILD_PROCESS_STDIO_MAXBUFFER('stdout');
        kill();
      } else {
        ArrayPrototypePush(_stdout, chunk);
      }
    });
  }

  if (child.stderr) {
    if (encoding)
      child.stderr.setEncoding(encoding);

    child.stderr.on('data', function onChildStderr(chunk) {
      // Do not need to count the length
      if (options.maxBuffer === Infinity) {
        ArrayPrototypePush(_stderr, chunk);
        return;
      }
      const encoding = child.stderr.readableEncoding;
      const length = encoding ?
        Buffer.byteLength(chunk, encoding) :
        chunk.length;
      stderrLen += length;

      if (stderrLen > options.maxBuffer) {
        const truncatedLen = options.maxBuffer - (stderrLen - length);
        ArrayPrototypePush(_stderr,
                           chunk.slice(0, truncatedLen));

        ex = new ERR_CHILD_PROCESS_STDIO_MAXBUFFER('stderr');
        kill();
      } else {
        ArrayPrototypePush(_stderr, chunk);
      }
    });
  }

  child.addListener('close', exithandler);
  child.addListener('error', errorhandler);

  return child;
}

ObjectDefineProperty(execFile, promisify.custom, {
  __proto__: null,
  enumerable: false,
  value: customPromiseExecFunction(execFile),
});

function abortChildProcess(child, killSignal, reason) {
  if (!child)
    return;
  try {
    if (child.kill(killSignal)) {
      child.emit('error', new AbortError(undefined, { cause: reason }));
    }
  } catch (err) {
    child.emit('error', err);
  }
}

function spawn(file, args, options) {
  options = normalizeSpawnArguments(file, args, options);
  validateTimeout(options.timeout);
  validateAbortSignal(options.signal, 'options.signal');
  const killSignal = sanitizeKillSignal(options.killSignal);
  const child = new ChildProcess();

  debug('spawn', options);
  child.spawn(options);

  if (options.timeout > 0) {
    let timeoutId = setTimeout(() => {
      if (timeoutId) {
        try {
          child.kill(killSignal);
        } catch (err) {
          child.emit('error', err);
        }
        timeoutId = null;
      }
    }, options.timeout);

    child.once('exit', () => {
      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }
    });
  }

  if (options.signal) {
    const signal = options.signal;
    if (signal.aborted) {
      process.nextTick(onAbortListener);
    } else {
      addAbortListener ??= require('internal/events/abort_listener').addAbortListener;
      const disposable = addAbortListener(signal, onAbortListener);
      child.once('exit', disposable[SymbolDispose]);
    }

    function onAbortListener() {
      abortChildProcess(child, killSignal, options.signal.reason);
    }
  }

  return child;
}

function spawnSync(file, args, options) {
  options = {
    __proto__: null,
    maxBuffer: MAX_BUFFER,
    ...normalizeSpawnArguments(file, args, options),
  };

  debug('spawnSync', options);

  // Validate the timeout, if present.
  validateTimeout(options.timeout);

  // Validate maxBuffer, if present.
  validateMaxBuffer(options.maxBuffer);

  // Validate and translate the kill signal, if present.
  options.killSignal = sanitizeKillSignal(options.killSignal);

  options.stdio = getValidStdio(options.stdio || 'pipe', true).stdio;

  if (options.input) {
    const stdin = options.stdio[0] = { ...options.stdio[0] };
    stdin.input = options.input;
  }

  // We may want to pass data in on any given fd, ensure it is a valid buffer
  for (let i = 0; i < options.stdio.length; i++) {
    const input = options.stdio[i]?.input;
    if (input != null) {
      const pipe = options.stdio[i] = { ...options.stdio[i] };
      if (isArrayBufferView(input)) {
        pipe.input = input;
      } else if (typeof input === 'string') {
        pipe.input = Buffer.from(input, options.encoding);
      } else {
        throw new ERR_INVALID_ARG_TYPE(`options.stdio[${i}]`,
                                       ['Buffer',
                                        'TypedArray',
                                        'DataView',
                                        'string'],
                                       input);
      }
    }
  }

  return child_process.spawnSync(options);
}


function checkExecSyncError(ret, args, cmd) {
  let err;
  if (ret.error) {
    err = ret.error;
    ObjectAssign(err, ret);
  } else if (ret.status !== 0) {
    let msg = 'Command failed: ';
    msg += cmd || ArrayPrototypeJoin(args, ' ');
    if (ret.stderr && ret.stderr.length > 0)
      msg += `\n${ret.stderr.toString()}`;
    err = genericNodeError(msg, ret);
  }
  return err;
}

function execFileSync(file, args, options) {
  ({ file, args, options } = normalizeExecFileArgs(file, args, options));

  const inheritStderr = !options.stdio;
  const ret = spawnSync(file, args, options);

  if (inheritStderr && ret.stderr)
    process.stderr.write(ret.stderr);

  const errArgs = [options.argv0 || file];
  ArrayPrototypePushApply(errArgs, args);
  const err = checkExecSyncError(ret, errArgs);

  if (err)
    throw err;

  return ret.stdout;
}

function execSync(command, options) {
  const opts = normalizeExecArgs(command, options, null);
  const inheritStderr = !opts.options.stdio;

  const ret = spawnSync(opts.file, opts.options);

  if (inheritStderr && ret.stderr)
    process.stderr.write(ret.stderr);

  const err = checkExecSyncError(ret, undefined, command);

  if (err)
    throw err;

  return ret.stdout;
}


function validateArgumentNullCheck(arg, propName) {
  if (typeof arg === 'string' && StringPrototypeIncludes(arg, '\u0000')) {
    throw new ERR_INVALID_ARG_VALUE(propName, arg, 'must be a string without null bytes');
  }
}


function validateArgumentsNullCheck(args, propName) {
  for (let i = 0; i < args.length; ++i) {
    validateArgumentNullCheck(args[i], `${propName}[${i}]`);
  }
}


function validateTimeout(timeout) {
  if (timeout != null) {
    validateInteger(timeout, 'timeout', 0);
  }
}


function validateMaxBuffer(maxBuffer) {
  if (maxBuffer != null) {
    validateNumber(maxBuffer, 'options.maxBuffer', 0);
  }
}


function sanitizeKillSignal(killSignal) {
  if (typeof killSignal === 'string' || typeof killSignal === 'number') {
    return convertToValidSignal(killSignal);
  } else if (killSignal != null) {
    throw new ERR_INVALID_ARG_TYPE('options.killSignal',
                                   ['string', 'number'],
                                   killSignal);
  }
}

module.exports = {
  ChildProcess,
  exec,
  execFile,
  execFileSync,
  execSync,
  spawn,
  spawnSync,
};
