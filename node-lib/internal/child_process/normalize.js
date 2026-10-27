'use strict';

const {
  ArrayIsArray,
  ArrayPrototypeFilter,
  ArrayPrototypeJoin,
  ArrayPrototypePush,
  ArrayPrototypeSlice,
  ArrayPrototypeSort,
  ArrayPrototypeUnshift,
  ObjectAssign,
  ObjectPrototypeHasOwnProperty,
  RegExpPrototypeExec,
  SafeSet,
  StringPrototypeIncludes,
  StringPrototypeToUpperCase,
} = primordials;

const { kEmptyObject } = require('internal/util');
const {
  codes: {
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_ARG_VALUE,
  },
} = require('internal/errors');
const {
  validateBoolean,
  validateFunction,
  validateInteger,
  validateInt32,
  validateNumber,
  validateObject,
  validateString,
} = require('internal/validators');
const { convertToValidSignal, getValidatedPath } = require('internal/child_process/support');

function normalizeExecArgs(command, options, callback) {
  validateString(command, 'command');
  validateArgumentNullCheck(command, 'command');

  if (typeof options === 'function') {
    callback = options;
    options = undefined;
  }

  // Make a shallow copy so we don't clobber the user's options object.
  options = { __proto__: null, ...options };
  options.shell = typeof options.shell === 'string' ? options.shell : true;

  return {
    file: command,
    options: options,
    callback: callback,
  };
}

function normalizeExecFileArgs(file, args, options, callback) {
  if (ArrayIsArray(args)) {
    args = ArrayPrototypeSlice(args);
  } else if (args != null && typeof args === 'object') {
    callback = options;
    options = args;
    args = null;
  } else if (typeof args === 'function') {
    callback = args;
    options = null;
    args = null;
  }

  args ??= [];

  if (typeof options === 'function') {
    callback = options;
  } else if (options != null) {
    validateObject(options, 'options');
  }

  options ??= kEmptyObject;

  if (callback != null) {
    validateFunction(callback, 'callback');
  }

  // Validate argv0, if present.
  if (options.argv0 != null) {
    validateString(options.argv0, 'options.argv0');
    validateArgumentNullCheck(options.argv0, 'options.argv0');
  }

  return { file, args, options, callback };
}

function copyProcessEnvToEnv(env, name, optionEnv) {
  if (process.env[name] &&
      (!optionEnv ||
       !ObjectPrototypeHasOwnProperty(optionEnv, name))) {
    env[name] = process.env[name];
  }
}

let emittedDEP0190Already = false;
function normalizeSpawnArguments(file, args, options) {
  validateString(file, 'file');
  validateArgumentNullCheck(file, 'file');

  if (file.length === 0)
    throw new ERR_INVALID_ARG_VALUE('file', file, 'cannot be empty');

  if (ArrayIsArray(args)) {
    args = ArrayPrototypeSlice(args);
  } else if (args == null) {
    args = [];
  } else if (typeof args !== 'object') {
    throw new ERR_INVALID_ARG_TYPE('args', 'object', args);
  } else {
    options = args;
    args = [];
  }

  validateArgumentsNullCheck(args, 'args');

  if (options === undefined)
    options = kEmptyObject;
  else
    validateObject(options, 'options');

  options = { __proto__: null, ...options };
  let cwd = options.cwd;

  // Validate the cwd, if present.
  if (cwd != null) {
    cwd = getValidatedPath(cwd, 'options.cwd');
  }

  // Validate detached, if present.
  if (options.detached != null) {
    validateBoolean(options.detached, 'options.detached');
  }

  // Validate the uid, if present.
  if (options.uid != null) {
    validateInt32(options.uid, 'options.uid');
  }

  // Validate the gid, if present.
  if (options.gid != null) {
    validateInt32(options.gid, 'options.gid');
  }

  // Validate the shell, if present.
  if (options.shell != null &&
      typeof options.shell !== 'boolean' &&
      typeof options.shell !== 'string') {
    throw new ERR_INVALID_ARG_TYPE('options.shell',
                                   ['boolean', 'string'], options.shell);
  }

  // Validate argv0, if present.
  if (options.argv0 != null) {
    validateString(options.argv0, 'options.argv0');
    validateArgumentNullCheck(options.argv0, 'options.argv0');
  }

  // Validate windowsHide, if present.
  if (options.windowsHide != null) {
    validateBoolean(options.windowsHide, 'options.windowsHide');
  }

  // Validate windowsVerbatimArguments, if present.
  let { windowsVerbatimArguments } = options;
  if (windowsVerbatimArguments != null) {
    validateBoolean(windowsVerbatimArguments,
                    'options.windowsVerbatimArguments');
  }

  if (options.shell) {
    validateArgumentNullCheck(options.shell, 'options.shell');
    if (args.length > 0 && !emittedDEP0190Already) {
      process.emitWarning(
        'Passing args to a child process with shell option true can lead to security ' +
        'vulnerabilities, as the arguments are not escaped, only concatenated.',
        'DeprecationWarning',
        'DEP0190');
      emittedDEP0190Already = true;
    }

    const command = args.length > 0 ? `${file} ${ArrayPrototypeJoin(args, ' ')}` : file;
    // Set the shell, switches, and commands.
    if (process.platform === 'win32') {
      if (typeof options.shell === 'string')
        file = options.shell;
      else
        file = process.env.comspec || 'cmd.exe';
      // '/d /s /c' is used only for cmd.exe.
      if (RegExpPrototypeExec(/^(?:.*\\)?cmd(?:\.exe)?$/i, file) !== null) {
        args = ['/d', '/s', '/c', `"${command}"`];
        windowsVerbatimArguments = true;
      } else {
        args = ['-c', command];
      }
    } else {
      if (typeof options.shell === 'string')
        file = options.shell;
      else if (process.platform === 'android')
        file = '/system/bin/sh';
      else
        file = '/bin/sh';
      args = ['-c', command];
    }
  }

  if (typeof options.argv0 === 'string') {
    ArrayPrototypeUnshift(args, options.argv0);
  } else {
    ArrayPrototypeUnshift(args, file);
  }

  // Shallow copy to guarantee changes won't impact process.env (not a spread:
  // object spread of a Proxy copies nothing, #1339)
  const env = options.env || ObjectAssign({}, process.env);
  const envPairs = [];

  // process.env.NODE_V8_COVERAGE always propagates, making it possible to
  // collect coverage for programs that spawn with white-listed environment.
  copyProcessEnvToEnv(env, 'NODE_V8_COVERAGE', options.env);

  let envKeys = [];
  // Prototype values are intentionally included.
  for (const key in env) {
    ArrayPrototypePush(envKeys, key);
  }

  if (process.platform === 'win32') {
    // On Windows env keys are case insensitive. Filter out duplicates,
    // keeping only the first one (in lexicographic order)
    const sawKey = new SafeSet();
    envKeys = ArrayPrototypeFilter(
      ArrayPrototypeSort(envKeys),
      (key) => {
        const uppercaseKey = StringPrototypeToUpperCase(key);
        if (sawKey.has(uppercaseKey)) {
          return false;
        }
        sawKey.add(uppercaseKey);
        return true;
      },
    );
  }

  for (const key of envKeys) {
    const value = env[key];
    if (value !== undefined) {
      validateArgumentNullCheck(key, `options.env['${key}']`);
      validateArgumentNullCheck(value, `options.env['${key}']`);
      ArrayPrototypePush(envPairs, `${key}=${value}`);
    }
  }

  return {
    // Make a shallow copy so we don't clobber the user's options object.
    __proto__: null,
    ...options,
    args,
    cwd,
    detached: !!options.detached,
    envPairs,
    file,
    windowsHide: !!options.windowsHide,
    windowsVerbatimArguments: !!windowsVerbatimArguments,
  };
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
  normalizeExecArgs,
  normalizeExecFileArgs,
  normalizeSpawnArguments,
  sanitizeKillSignal,
  validateArgumentNullCheck,
  validateArgumentsNullCheck,
  validateMaxBuffer,
  validateTimeout,
};
