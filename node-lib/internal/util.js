'use strict';

const {
  ArrayPrototypePush,
  Error,
  FunctionPrototypeSymbolHasInstance,
  ObjectDefineProperties,
  ObjectDefineProperty,
  ObjectFreeze,
  ObjectKeys,
  ObjectGetOwnPropertyDescriptor,
  ObjectGetOwnPropertyDescriptors,
  ObjectGetPrototypeOf,
  ObjectSetPrototypeOf,
  ObjectValues,
  Promise,
  ReflectApply,
  ReflectConstruct,
  SafeSet,
  StringPrototypeReplace,
  Symbol,
  SymbolFor,
  SymbolPrototypeGetDescription,
} = primordials;

const { isNativeError, isPromise } = require('internal/binding/types');
const assert = require('internal/assert');

function once(callback, { preserveReturnValue = false } = kEmptyObject) {
  let called = false;
  let returnValue;
  return function(...args) {
    if (called) return returnValue;
    called = true;
    const result = ReflectApply(callback, this, args);
    returnValue = preserveReturnValue ? result : undefined;
    return result;
  };
}

function getLazy(initializer) {
  let value;
  let initialized = false;
  return function () {
    if (!initialized) {
      value = initializer();
      initialized = true;
    }
    return value;
  };
}

const kEmptyObject = ObjectFreeze({ __proto__: null });
const kEnumerableProperty = { __proto__: null, enumerable: true };
ObjectFreeze(kEnumerableProperty);

function spliceOne(list, index) {
  for (; index + 1 < list.length; index++)
    list[index] = list[index + 1];
  list.pop();
}

let DOMException;
function lazyDOMExceptionClass() {
  DOMException ??= require('internal/per_context/domexception').DOMException;
  return DOMException;
}

function lazyDOMException(message, name) {
  return new (lazyDOMExceptionClass())(message, name);
}

const colorRegExp = /\u001b\[\d\d?m/g; // eslint-disable-line no-control-regex

function removeColors(str) {
  return StringPrototypeReplace(str, colorRegExp, '');
}

function isError(e) {
  return isNativeError(e) || FunctionPrototypeSymbolHasInstance(Error, e);
}

const codesWarned = new SafeSet();

function getDeprecationWarningEmitter(code, msg, deprecated, useEmitSync) {
  let warned = false;
  return function() {
    if (!warned) {
      warned = true;
      if (code !== undefined) {
        if (!codesWarned.has(code)) {
          const emitWarning = useEmitSync ?
            require('internal/process/warning').emitWarningSync :
            process.emitWarning;
          emitWarning(msg, 'DeprecationWarning', code, deprecated);
          codesWarned.add(code);
        }
      } else {
        process.emitWarning(msg, 'DeprecationWarning', deprecated);
      }
    }
  };
}

function deprecate(fn, msg, code, useEmitSync, modifyPrototype = true) {
  if (code !== undefined)
    require('internal/validators').validateString(code, 'code');

  const emitDeprecationWarning = getDeprecationWarningEmitter(
    code, msg, deprecated, useEmitSync,
  );

  function deprecated(...args) {
    if (!process.noDeprecation) {
      emitDeprecationWarning();
    }
    if (new.target) {
      return ReflectConstruct(fn, args, new.target);
    }
    return ReflectApply(fn, this, args);
  }

  if (modifyPrototype) {
    ObjectSetPrototypeOf(deprecated, fn);
    if (fn.prototype) {
      deprecated.prototype = fn.prototype;
    }

    ObjectDefineProperty(deprecated, 'length', {
      __proto__: null,
      ...ObjectGetOwnPropertyDescriptor(fn, 'length'),
    });
  }

  return deprecated;
}

function getSystemErrorName(err) {
  const entry = require('internal/util/uv_errmap').errmap.get(err);
  return entry ? entry[0] : `Unknown system error ${err}`;
}

function getSystemErrorMessage(err) {
  const entry = require('internal/util/uv_errmap').errmap.get(err);
  return entry ? entry[1] : `Unknown system error ${err}`;
}

function getSystemErrorMap() {
  return require('internal/util/uv_errmap').errmap;
}

const kCustomPromisifiedSymbol = SymbolFor('nodejs.util.promisify.custom');
const kCustomPromisifyArgsSymbol = Symbol('customPromisifyArgs');

function promisify(original) {
  const { validateFunction } = require('internal/validators');
  validateFunction(original, 'original');

  if (original[kCustomPromisifiedSymbol]) {
    const fn = original[kCustomPromisifiedSymbol];

    validateFunction(fn, 'util.promisify.custom');

    return ObjectDefineProperty(fn, kCustomPromisifiedSymbol, {
      __proto__: null,
      value: fn, enumerable: false, writable: false, configurable: true,
    });
  }

  const argumentNames = original[kCustomPromisifyArgsSymbol];

  function fn(...args) {
    return new Promise((resolve, reject) => {
      ArrayPrototypePush(args, (err, ...values) => {
        if (err) {
          return reject(err);
        }
        if (argumentNames !== undefined && values.length > 1) {
          const obj = {};
          for (let i = 0; i < argumentNames.length; i++)
            obj[argumentNames[i]] = values[i];
          resolve(obj);
        } else {
          resolve(values[0]);
        }
      });
      if (isPromise(ReflectApply(original, this, args))) {
        process.emitWarning('Calling promisify on a function that returns a Promise is likely a mistake.',
                            'DeprecationWarning', 'DEP0174');
      }
    });
  }

  ObjectSetPrototypeOf(fn, ObjectGetPrototypeOf(original));

  ObjectDefineProperty(fn, kCustomPromisifiedSymbol, {
    __proto__: null,
    value: fn, enumerable: false, writable: false, configurable: true,
  });

  const descriptors = ObjectGetOwnPropertyDescriptors(original);
  const propertiesValues = ObjectValues(descriptors);
  for (let i = 0; i < propertiesValues.length; i++) {
    ObjectSetPrototypeOf(propertiesValues[i], null);
  }
  return ObjectDefineProperties(fn, descriptors);
}

promisify.custom = kCustomPromisifiedSymbol;

function convertProcessSignalToExitCode(signalCode) {
  const { signals } = require('internal/util/signals');
  require('internal/validators').validateOneOf(signalCode, 'signalCode', ObjectKeys(signals));
  return 128 + signals[signalCode];
}

function join(output, separator) {
  let str = '';
  if (output.length !== 0) {
    const lastIndex = output.length - 1;
    for (let i = 0; i < lastIndex; i++) {
      str += output[i];
      str += separator;
    }
    str += output[lastIndex];
  }
  return str;
}

function defineLazyProperties(target, id, keys, enumerable = true) {
  const descriptors = { __proto__: null };
  let mod;
  for (let i = 0; i < keys.length; i++) {
    const key = keys[i];
    let lazyLoadedValue;
    function set(value) {
      ObjectDefineProperty(target, key, {
        __proto__: null,
        writable: true,
        value,
      });
    }
    ObjectDefineProperty(set, 'name', {
      __proto__: null,
      value: `set ${key}`,
    });
    function get() {
      mod ??= require(id);
      if (lazyLoadedValue === undefined) {
        lazyLoadedValue = mod[key];
        set(lazyLoadedValue);
      }
      return lazyLoadedValue;
    }
    ObjectDefineProperty(get, 'name', {
      __proto__: null,
      value: `get ${key}`,
    });
    descriptors[key] = {
      __proto__: null,
      configurable: true,
      enumerable,
      get,
      set,
    };
  }
  ObjectDefineProperties(target, descriptors);
}

function assignFunctionName(name, fn, descriptor = kEmptyObject) {
  if (typeof name !== 'string') {
    const symbolDescription = SymbolPrototypeGetDescription(name);
    assert(symbolDescription !== undefined, 'Attempted to name function after descriptionless Symbol');
    name = `[${symbolDescription}]`;
  }
  return ObjectDefineProperty(fn, 'name', {
    __proto__: null,
    writable: false,
    enumerable: false,
    configurable: true,
    ...ObjectGetOwnPropertyDescriptor(fn, 'name'),
    ...descriptor,
    value: name,
  });
}

module.exports = {
  assignFunctionName,
  once,
  customInspectSymbol: SymbolFor('nodejs.util.inspect.custom'),
  defineLazyProperties,
  convertProcessSignalToExitCode,
  deprecate,
  getSystemErrorMap,
  getSystemErrorMessage,
  getSystemErrorName,
  isError,
  join,
  kCustomPromisifiedSymbol,
  kCustomPromisifyArgsSymbol,
  promisify,
  removeColors,
  getLazy,
  isMacOS: process.platform === 'darwin',
  isWindows: process.platform === 'win32',
  kEmptyObject,
  kEnumerableProperty,
  lazyDOMException,
  lazyDOMExceptionClass,
  spliceOne,
};
