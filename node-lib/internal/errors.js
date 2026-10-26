'use strict';

// The subset of Node's lib/internal/errors.js (v26.8.2) the embedded modules
// need. Add codes here with `E(code, Base, message)` as modules are ported.

const {
  ArrayIsArray,
  ArrayPrototypeIncludes,
  ArrayPrototypeIndexOf,
  ArrayPrototypeJoin,
  ArrayPrototypeMap,
  ArrayPrototypePush,
  ArrayPrototypeSlice,
  ArrayPrototypeSplice,
  JSONStringify,
  MathAbs,
  NumberIsInteger,
  ObjectAssign,
  ObjectCreate,
  ObjectDefineProperty,
  ObjectSetPrototypeOf,
  ReflectApply,
  RegExpPrototypeExec,
  StringPrototypeEndsWith,
  StringPrototypeIncludes,
  StringPrototypeIndexOf,
  StringPrototypeSlice,
  StringPrototypeStartsWith,
  StringPrototypeToLowerCase,
} = primordials;

let lazyInspect;
function inspect(...args) {
  lazyInspect ??= require('internal/util/inspect').inspect;
  return lazyInspect(...args);
}

function format(...args) {
  return require('internal/util/inspect').format(...args);
}

const codes = {};

function E(code, Base, message) {
  const proto = ObjectCreate(Base.prototype, {
    constructor: { value: Base, writable: true, configurable: true },
    toString: {
      value() {
        return `${this.name} [${code}]: ${this.message}`;
      },
      writable: true,
      configurable: true,
    },
  });
  function NodeError(...args) {
    const error = new Base();
    ObjectSetPrototypeOf(error, proto);
    error.code = code;
    ObjectDefineProperty(error, 'message', {
      value: typeof message === 'function' ? ReflectApply(message, error, args) : message,
      writable: true,
      enumerable: false,
      configurable: true,
    });
    return error;
  }
  NodeError.prototype = proto;
  NodeError.HideStackFramesError = NodeError;
  codes[code] = NodeError;
}

class SystemError extends Error {
  constructor(context) {
    super();
    let message = `A system error occurred: ${context.syscall} returned ` +
      `${context.code} (${context.message})`;
    if (context.path !== undefined) message += ` ${context.path}`;
    if (context.dest !== undefined) message += ` => ${context.dest}`;
    this.code = 'ERR_SYSTEM_ERROR';
    primordials.ObjectDefineProperties(this, {
      name: { value: 'SystemError', enumerable: false, writable: true, configurable: true },
      message: { value: message, enumerable: false, writable: true, configurable: true },
      info: { value: context, enumerable: true, configurable: true, writable: false },
      errno: {
        get() { return context.errno; },
        set(value) { context.errno = value; },
        enumerable: true,
        configurable: true,
      },
      syscall: {
        get() { return context.syscall; },
        set(value) { context.syscall = value; },
        enumerable: true,
        configurable: true,
      },
    });
  }

  toString() {
    return `${this.name} [${this.code}]: ${this.message}`;
  }
}
codes.ERR_SYSTEM_ERROR = SystemError;

function determineSpecificType(value) {
  if (value === null) {
    return 'null';
  } else if (value === undefined) {
    return 'undefined';
  }

  const type = typeof value;

  switch (type) {
    case 'bigint':
      return `type bigint (${value}n)`;
    case 'number':
      if (value === 0) {
        return 1 / value === -Infinity ? 'type number (-0)' : 'type number (0)';
      } else if (value !== value) {
        return 'type number (NaN)';
      } else if (value === Infinity) {
        return 'type number (Infinity)';
      } else if (value === -Infinity) {
        return 'type number (-Infinity)';
      }
      return `type number (${value})`;
    case 'boolean':
      return value ? 'type boolean (true)' : 'type boolean (false)';
    case 'symbol':
      return `type symbol (${String(value)})`;
    case 'function':
      return `function ${value.name}`;
    case 'object':
      if (value.constructor && 'name' in value.constructor) {
        return `an instance of ${value.constructor.name}`;
      }
      return `${inspect(value, { depth: -1 })}`;
    case 'string':
      value.length > 28 && (value = `${StringPrototypeSlice(value, 0, 25)}...`);
      if (StringPrototypeIndexOf(value, "'") === -1) {
        return `type string ('${value}')`;
      }
      return `type string (${JSONStringify(value)})`;
    default:
      value = inspect(value, { colors: false });
      if (value.length > 28) {
        value = `${StringPrototypeSlice(value, 0, 25)}...`;
      }
      return `type ${type} (${value})`;
  }
}

function formatList(array, type = 'and') {
  switch (array.length) {
    case 0: return '';
    case 1: return `${array[0]}`;
    case 2: return `${array[0]} ${type} ${array[1]}`;
    case 3: return `${array[0]}, ${array[1]}, ${type} ${array[2]}`;
    default:
      return `${ArrayPrototypeJoin(ArrayPrototypeSlice(array, 0, -1), ', ')}, ${type} ${array[array.length - 1]}`;
  }
}

const kTypes = [
  'string',
  'function',
  'number',
  'object',
  'Function',
  'Object',
  'boolean',
  'bigint',
  'symbol',
];

const classRegExp = /^[A-Z][a-zA-Z0-9]*$/;

E('ERR_INVALID_ARG_TYPE', TypeError, (name, expected, actual) => {
  if (!ArrayIsArray(expected)) {
    expected = [expected];
  }

  let msg = 'The ';
  if (StringPrototypeEndsWith(name, ' argument')) {
    msg += `${name} `;
  } else {
    const type = StringPrototypeIncludes(name, '.') ? 'property' : 'argument';
    msg += `"${name}" ${type} `;
  }
  msg += 'must be ';

  const types = [];
  const instances = [];
  const other = [];

  for (const value of expected) {
    if (ArrayPrototypeIncludes(kTypes, value)) {
      ArrayPrototypePush(types, StringPrototypeToLowerCase(value));
    } else if (RegExpPrototypeExec(classRegExp, value) !== null) {
      ArrayPrototypePush(instances, value);
    } else {
      ArrayPrototypePush(other, value);
    }
  }

  if (instances.length > 0) {
    const pos = ArrayPrototypeIndexOf(types, 'object');
    if (pos !== -1) {
      ArrayPrototypeSplice(types, pos, 1);
      ArrayPrototypePush(instances, 'Object');
    }
  }

  if (types.length > 0) {
    msg += `${types.length > 1 ? 'one of type' : 'of type'} ${formatList(types, 'or')}`;
    if (instances.length > 0 || other.length > 0)
      msg += ' or ';
  }

  if (instances.length > 0) {
    msg += `an instance of ${formatList(instances, 'or')}`;
    if (other.length > 0)
      msg += ' or ';
  }

  if (other.length > 0) {
    if (other.length > 1) {
      msg += `one of ${formatList(other, 'or')}`;
    } else {
      if (StringPrototypeToLowerCase(other[0]) !== other[0])
        msg += 'an ';
      msg += `${other[0]}`;
    }
  }

  msg += `. Received ${determineSpecificType(actual)}`;

  return msg;
});

E('ERR_EVENT_RECURSION', Error, (name) => `The event "${name}" is already being dispatched`);
E('ERR_ILLEGAL_CONSTRUCTOR', TypeError, 'Illegal constructor');
E('ERR_INVALID_ARG_VALUE', TypeError, (name, value, reason = 'is invalid') => {
  let inspected = inspect(value);
  if (inspected.length > 128) {
    inspected = `${StringPrototypeSlice(inspected, 0, 128)}...`;
  }
  const type = StringPrototypeIncludes(name, '.') ? 'property' : 'argument';
  return `The ${type} '${name}' ${reason}. Received ${inspected}`;
});
E('ERR_INVALID_RETURN_VALUE', TypeError, (input, name, value) =>
  `Expected ${input} to be returned from the "${name}" function but got ${determineSpecificType(value)}.`);
E('ERR_INVALID_STATE', Error, (message) => format('Invalid state: %s', message));
E('ERR_METHOD_NOT_IMPLEMENTED', Error, (method) => format('The %s method is not implemented', method));
E('ERR_MULTIPLE_CALLBACK', Error, 'Callback called multiple times');
E('ERR_OPERATION_FAILED', Error, (message) => format('Operation failed: %s', message));
E('ERR_PERFORMANCE_INVALID_TIMESTAMP', TypeError, (timestamp) => format('%d is not a valid timestamp', timestamp));
E('ERR_PERFORMANCE_MEASURE_INVALID_OPTIONS', TypeError, (message) => message);
E('ERR_STREAM_ALREADY_FINISHED', Error,
  (method) => format('Cannot call %s after a stream was finished', method));
E('ERR_STREAM_CANNOT_PIPE', Error, 'Cannot pipe, not readable');
E('ERR_STREAM_DESTROYED', Error,
  (method) => format('Cannot call %s after a stream was destroyed', method));
E('ERR_STREAM_NULL_VALUES', TypeError, 'May not write null values to stream');
E('ERR_STREAM_PREMATURE_CLOSE', Error, 'Premature close');
E('ERR_STREAM_PUSH_AFTER_EOF', Error, 'stream.push() after EOF');
E('ERR_STREAM_UNABLE_TO_PIPE', Error, 'Cannot pipe to a closed or destroyed stream');
E('ERR_STREAM_UNSHIFT_AFTER_END_EVENT', Error, 'stream.unshift() after end event');
E('ERR_STREAM_WRITE_AFTER_END', Error, 'write after end');
E('ERR_UNKNOWN_ENCODING', TypeError, (encoding) => format('Unknown encoding: %s', encoding));
E('ERR_INVALID_THIS', TypeError, (type) => `Value of "this" must be of type ${type}`);
E('ERR_MISSING_ARGS', TypeError, (...args) => {
  let msg = 'The ';
  const len = args.length;
  const wrap = (a) => `"${a}"`;
  args = ArrayPrototypeMap(
    args,
    (a) => (ArrayIsArray(a) ?
      ArrayPrototypeJoin(ArrayPrototypeMap(a, wrap), ' or ') :
      wrap(a)),
  );
  msg += `${formatList(args)} argument${len > 1 ? 's' : ''}`;
  return `${msg} must be specified`;
});

function addNumericalSeparator(val) {
  let res = '';
  let i = val.length;
  const start = val[0] === '-' ? 1 : 0;
  for (; i >= start + 4; i -= 3) {
    res = `_${StringPrototypeSlice(val, i - 3, i)}${res}`;
  }
  return `${StringPrototypeSlice(val, 0, i)}${res}`;
}

E('ERR_OUT_OF_RANGE', RangeError, (str, range, input, replaceDefaultBoolean = false) => {
  let msg = replaceDefaultBoolean ? str :
    `The value of "${str}" is out of range.`;
  let received;
  if (NumberIsInteger(input) && MathAbs(input) > 2 ** 32) {
    received = addNumericalSeparator(String(input));
  } else if (typeof input === 'bigint') {
    received = String(input);
    if (input > 2n ** 32n || input < -(2n ** 32n)) {
      received = addNumericalSeparator(received);
    }
    received += 'n';
  } else {
    received = inspect(input);
  }
  msg += ` It must be ${range}. Received ${received}`;
  return msg;
});
E('ERR_UNHANDLED_ERROR', Error, (err = undefined) => {
  const msg = 'Unhandled error.';
  if (err === undefined) return msg;
  return `${msg} (${err})`;
});
E('ERR_FALSY_VALUE_REJECTION', Error, function(reason) {
  this.reason = reason;
  return 'Promise was rejected with falsy value';
});
E('ERR_INTERNAL_ASSERTION', Error, (message) => {
  const suffix = 'This is caused by either a bug in Node.js ' +
    'or incorrect usage of Node.js internals.\n' +
    'Please open an issue with this stack trace at ' +
    'https://github.com/nodejs/node/issues\n';
  return message === undefined ? suffix : `${message}\n${suffix}`;
});
E('ERR_INVALID_MIME_SYNTAX', TypeError, (production, str, invalidIndex) => {
  const msg = invalidIndex !== -1 ? ` at ${invalidIndex}` : '';
  return `The MIME syntax for a ${production} in "${str}" is invalid` + msg;
});
E('ERR_PARSE_ARGS_INVALID_OPTION_VALUE', TypeError, (message) => message);
E('ERR_PARSE_ARGS_UNEXPECTED_POSITIONAL', TypeError, (positional) =>
  `Unexpected argument '${positional}'. This command does not take positional arguments`);
E('ERR_PARSE_ARGS_UNKNOWN_OPTION', TypeError, (option, allowPositionals) => {
  const suggestDashDash = allowPositionals ? '. To specify a positional ' +
    "argument starting with a '-', place it at the end of the command after " +
    `'--', as in '-- ${JSONStringify(option)}` : '';
  return `Unknown option '${option}'${suggestDashDash}`;
});
E('ERR_UNKNOWN_BUILTIN_MODULE', Error, (id) => `No such built-in module: ${id}`);

E('ERR_INVALID_MODULE_SPECIFIER', TypeError, (request, reason, base = undefined) => {
  return `Invalid module "${request}" ${reason}${base ? ` imported from ${base}` : ''}`;
});

E('ERR_INVALID_PACKAGE_CONFIG', Error, (path, base, message) => {
  return `Invalid package config ${path}${base ? ` while importing ${base}` : ''}${message ? `. ${message}` : ''}`;
});

E('ERR_INVALID_PACKAGE_TARGET', Error, (pkgPath, key, target, isImport = false, base = undefined) => {
  const relatedError = typeof target === 'string' && !isImport &&
    target.length && !StringPrototypeStartsWith(target, './');
  target = JSONStringify(target);
  if (key === '.') {
    return `Invalid "exports" main target ${target} defined ` +
      `in the package config ${pkgPath}package.json${base ? ` imported from ${base}` : ''}` +
      `${relatedError ? '; targets must start with "./"' : ''}`;
  }
  return `Invalid "${isImport ? 'imports' : 'exports'}" target ${target} defined for '${key}' in the package config ${pkgPath}package.json${base ? ` imported from ${base}` : ''}${relatedError ? '; targets must start with "./"' : ''}`;
});

E('ERR_PACKAGE_IMPORT_NOT_DEFINED', TypeError, (specifier, packagePath, base) => {
  return `Package import specifier "${specifier}" is not defined${packagePath ? ` in package ${packagePath}package.json` : ''} imported from ${base}`;
});

E('ERR_PACKAGE_PATH_NOT_EXPORTED', Error, (pkgPath, subpath, base = undefined) => {
  if (subpath === '.') {
    return `No "exports" main defined in ${pkgPath}package.json${base ? ` imported from ${base}` : ''}`;
  }
  return `Package subpath '${subpath}' is not defined by "exports" in ${pkgPath}package.json${base ? ` imported from ${base}` : ''}`;
});

E('ERR_REQUIRE_ESM', Error, (filename) => {
  return `require() of ES Module ${filename} not supported: ES modules cannot be loaded by require() in jsse yet.`;
});

E('ERR_ARG_NOT_ITERABLE', TypeError, (x) => `${x} must be iterable`);
E('ERR_INVALID_FILE_URL_HOST', TypeError,
  (platform) => `File URL host must be "localhost" or empty on ${platform}`);
E('ERR_INVALID_FILE_URL_PATH', TypeError, function(reason, input) {
  this.input = input;
  return `File URL path ${reason}`;
});
E('ERR_INVALID_TUPLE', TypeError, (x, y) => `${x} must be an iterable ${y} tuple`);
E('ERR_INVALID_URI', URIError, 'URI malformed');
E('ERR_INVALID_URL', TypeError, function(input, base = null) {
  this.input = input;
  if (base != null) {
    this.base = base;
  }
  return 'Invalid URL';
});
E('ERR_INVALID_URL_SCHEME', TypeError, (expected) => {
  if (typeof expected === 'string')
    expected = [expected];
  const res = expected.length === 2 ?
    `one of scheme ${expected[0]} or ${expected[1]}` :
    `of scheme ${expected[0]}`;
  return `The URL must be ${res}`;
});

// Node's AbortError is a plain Error with .code and .name, not a DOMException.
class AbortError extends Error {
  constructor(message = 'The operation was aborted', options = undefined) {
    if (options !== undefined && typeof options !== 'object') {
      throw new codes.ERR_INVALID_ARG_TYPE('options', 'Object', options);
    }
    super(message, options);
    this.code = 'ABORT_ERR';
    this.name = 'AbortError';
  }
}

function aggregateTwoErrors(innerError, outerError) {
  if (innerError && outerError && innerError !== outerError) {
    if (ArrayIsArray(outerError.errors)) {
      ArrayPrototypePush(outerError.errors, innerError);
      return outerError;
    }
    const err = new AggregateError([outerError, innerError], outerError.message);
    err.code = outerError.code;
    return err;
  }
  return innerError || outerError;
}

function genericNodeError(message, errorProperties) {
  const err = new Error(message);
  ObjectAssign(err, errorProperties);
  return err;
}

let maxStackErrorName;
let maxStackErrorMessage;
function isStackOverflowError(err) {
  if (!err || err.name !== 'RangeError') return false;
  if (maxStackErrorMessage === undefined) {
    try {
      function overflowStack() { overflowStack(); }
      overflowStack();
    } catch (e) {
      maxStackErrorMessage = e.message;
      maxStackErrorName = e.name;
    }
  }
  return err.name === maxStackErrorName && err.message === maxStackErrorMessage;
}

module.exports = { AbortError, codes, genericNodeError, isStackOverflowError };
module.exports.aggregateTwoErrors = aggregateTwoErrors;
