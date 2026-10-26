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
  RegExpPrototypeExec,
  StringPrototypeEndsWith,
  StringPrototypeIncludes,
  StringPrototypeIndexOf,
  StringPrototypeSlice,
  StringPrototypeStartsWith,
  StringPrototypeToLowerCase,
} = primordials;

const { inspect } = require('internal/util/inspect');

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
    ObjectDefineProperty(error, 'message', {
      value: typeof message === 'function' ? message(...args) : message,
      writable: true,
      enumerable: false,
      configurable: true,
    });
    error.code = code;
    return error;
  }
  NodeError.prototype = proto;
  codes[code] = NodeError;
}

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

module.exports = { codes };
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

function genericNodeError(message, errorProperties) {
  const err = new Error(message);
  ObjectAssign(err, errorProperties);
  return err;
}

module.exports = { AbortError, codes, genericNodeError };
