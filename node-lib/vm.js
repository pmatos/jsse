// Copyright Joyent, Inc. and other Node contributors.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit
// persons to whom the Software is furnished to do so, subject to the
// following conditions:
//
// The above copyright notice and this permission notice shall be included
// in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN
// NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
// OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
// USE OR OTHER DEALINGS IN THE SOFTWARE.

'use strict';

const {
  ArrayPrototypeForEach,
  ObjectFreeze,
  ObjectPrototype,
  ReflectApply,
  SafeSet,
  Symbol,
} = primordials;

const {
  ContextifyScript,
  makeContext,
  constants,
  measure: _measureMemory,
  symbols: {
    vm_dynamic_import_main_context_default,
    vm_context_no_contextify,
  },
} = require('internal/vm/contextify');
const {
  ERR_INVALID_ARG_TYPE,
} = require('internal/errors').codes;
const { isArrayBufferView } = require('internal/util/types');
const {
  validateArray,
  validateBoolean,
  validateInt32,
  validateOneOf,
  validateObject,
  validateString,
  validateStringArray,
  validateUint32,
  kValidateObjectAllowArray,
  kValidateObjectAllowNullable,
} = require('internal/validators');
const { kEmptyObject } = require('internal/util');
const {
  getHostDefinedOptionId,
  internalCompileFunction,
  isContext: _isContext,
  registerImportModuleDynamically,
} = require('internal/vm');
const kParsingContext = Symbol('script parsing context');
const kVmBreakFirstLineSymbol = Symbol('kVmBreakFirstLineSymbol');

function validateBuffer(buffer, name) {
  if (!isArrayBufferView(buffer)) {
    throw new ERR_INVALID_ARG_TYPE(name,
                                   ['Buffer', 'TypedArray', 'DataView'],
                                   buffer);
  }
}

const experimentalWarnings = new SafeSet();
function emitExperimentalWarning(feature) {
  if (experimentalWarnings.has(feature)) return;
  experimentalWarnings.add(feature);
  process.emitWarning(
    `${feature} is an experimental feature and might change at any time`,
    'ExperimentalWarning');
}

/**
 * Check if object is a context object created by vm.createContext().
 * @throws {TypeError} If object is not an object in the first place, throws TypeError.
 * @param {object} object Object to check.
 * @returns {boolean}
 */
function isContext(object) {
  validateObject(object, 'object', kValidateObjectAllowArray);

  return _isContext(object);
}

class Script extends ContextifyScript {
  constructor(code, options = kEmptyObject) {
    code = `${code}`;
    if (typeof options === 'string') {
      options = { filename: options };
    } else {
      validateObject(options, 'options');
    }

    const {
      filename = 'evalmachine.<anonymous>',
      lineOffset = 0,
      columnOffset = 0,
      cachedData,
      produceCachedData = false,
      importModuleDynamically,
      [kParsingContext]: parsingContext,
    } = options;

    validateString(filename, 'options.filename');
    validateInt32(lineOffset, 'options.lineOffset');
    validateInt32(columnOffset, 'options.columnOffset');
    if (cachedData !== undefined) {
      validateBuffer(cachedData, 'options.cachedData');
    }
    validateBoolean(produceCachedData, 'options.produceCachedData');

    const hostDefinedOptionId =
        getHostDefinedOptionId(importModuleDynamically, filename);
    // Calling `ReThrow()` on a native TryCatch does not generate a new
    // abort-on-uncaught-exception check. A dummy try/catch in JS land
    // protects against that.
    try { // eslint-disable-line no-useless-catch
      super(code,
            filename,
            lineOffset,
            columnOffset,
            cachedData,
            produceCachedData,
            parsingContext,
            hostDefinedOptionId);
    } catch (e) {
      throw e; /* node-do-not-add-exception-line */
    }

    registerImportModuleDynamically(this, importModuleDynamically);
  }

  runInThisContext(options) {
    const { args } = getRunInContextArgs(null, options);
    return ReflectApply(super.runInContext, this, args);
  }

  runInContext(contextifiedObject, options) {
    validateContext(contextifiedObject);
    const { args } = getRunInContextArgs(contextifiedObject, options);
    return ReflectApply(super.runInContext, this, args);
  }

  runInNewContext(contextObject, options) {
    const context = createContext(contextObject, getContextOptions(options));
    return this.runInContext(context, options);
  }
}

function validateContext(contextifiedObject) {
  if (!isContext(contextifiedObject)) {
    throw new ERR_INVALID_ARG_TYPE('contextifiedObject', 'vm.Context',
                                   contextifiedObject);
  }
}

function getRunInContextArgs(contextifiedObject, options = kEmptyObject) {
  validateObject(options, 'options');

  let timeout = options.timeout;
  if (timeout === undefined) {
    timeout = -1;
  } else {
    validateUint32(timeout, 'options.timeout', true);
  }

  const {
    displayErrors = true,
    breakOnSigint = false,
    [kVmBreakFirstLineSymbol]: breakFirstLine = false,
  } = options;

  validateBoolean(displayErrors, 'options.displayErrors');
  validateBoolean(breakOnSigint, 'options.breakOnSigint');

  return {
    breakOnSigint,
    args: [
      contextifiedObject,
      timeout,
      displayErrors,
      breakOnSigint,
      breakFirstLine,
    ],
  };
}

function getContextOptions(options) {
  if (!options)
    return {};
  const contextOptions = {
    name: options.contextName,
    origin: options.contextOrigin,
    codeGeneration: undefined,
    microtaskMode: options.microtaskMode,
  };
  if (contextOptions.name !== undefined)
    validateString(contextOptions.name, 'options.contextName');
  if (contextOptions.origin !== undefined)
    validateString(contextOptions.origin, 'options.contextOrigin');
  if (options.contextCodeGeneration !== undefined) {
    validateObject(options.contextCodeGeneration,
                   'options.contextCodeGeneration');
    const { strings, wasm } = options.contextCodeGeneration;
    if (strings !== undefined)
      validateBoolean(strings, 'options.contextCodeGeneration.strings');
    if (wasm !== undefined)
      validateBoolean(wasm, 'options.contextCodeGeneration.wasm');
    contextOptions.codeGeneration = { strings, wasm };
  }
  if (options.microtaskMode !== undefined)
    validateString(options.microtaskMode, 'options.microtaskMode');
  return contextOptions;
}

let defaultContextNameIndex = 1;
function createContext(contextObject = { __proto__: ObjectPrototype }, options = kEmptyObject) {
  if (contextObject !== vm_context_no_contextify && isContext(contextObject)) {
    return contextObject;
  }

  validateObject(options, 'options');

  const {
    name = `VM Context ${defaultContextNameIndex++}`,
    origin,
    codeGeneration,
    microtaskMode,
    importModuleDynamically,
  } = options;

  validateString(name, 'options.name');
  if (origin !== undefined)
    validateString(origin, 'options.origin');
  if (codeGeneration !== undefined)
    validateObject(codeGeneration, 'options.codeGeneration');

  let strings = true;
  let wasm = true;
  if (codeGeneration !== undefined) {
    ({ strings = true, wasm = true } = codeGeneration);
    validateBoolean(strings, 'options.codeGeneration.strings');
    validateBoolean(wasm, 'options.codeGeneration.wasm');
  }

  validateOneOf(microtaskMode,
                'options.microtaskMode',
                ['afterEvaluate', undefined]);
  const microtaskQueue = (microtaskMode === 'afterEvaluate');

  const hostDefinedOptionId =
    getHostDefinedOptionId(importModuleDynamically, name);

  const result = makeContext(contextObject, name, origin, strings, wasm, microtaskQueue, hostDefinedOptionId);
  // Register the context scope callback after the context was initialized.
  registerImportModuleDynamically(result, importModuleDynamically);
  return result;
}

function createScript(code, options) {
  return new Script(code, options);
}

function runInContext(code, contextifiedObject, options) {
  validateContext(contextifiedObject);
  if (typeof options === 'string') {
    options = {
      filename: options,
      [kParsingContext]: contextifiedObject,
    };
  } else {
    options = { ...options, [kParsingContext]: contextifiedObject };
  }
  const script = createScript(code, options);
  try {
    return script.runInContext(contextifiedObject, options);
  } finally {
    ContextifyScript.release(script);
  }
}

function runInNewContext(code, contextObject, options) {
  if (typeof options === 'string') {
    options = { filename: options };
  }
  contextObject = createContext(contextObject, getContextOptions(options));
  options = { ...options, [kParsingContext]: contextObject };
  const script = createScript(code, options);
  try {
    return script.runInNewContext(contextObject, options);
  } finally {
    ContextifyScript.release(script);
  }
}

function runInThisContext(code, options) {
  if (typeof options === 'string') {
    options = { filename: options };
  }
  const script = createScript(code, options);
  try {
    return script.runInThisContext(options);
  } finally {
    ContextifyScript.release(script);
  }
}

function compileFunction(code, params, options = kEmptyObject) {
  validateString(code, 'code');
  validateObject(options, 'options');
  if (params !== undefined) {
    validateStringArray(params, 'params');
  }
  const {
    filename = '',
    columnOffset = 0,
    lineOffset = 0,
    cachedData = undefined,
    produceCachedData = false,
    parsingContext = undefined,
    contextExtensions = [],
    importModuleDynamically,
  } = options;

  validateString(filename, 'options.filename');
  validateInt32(columnOffset, 'options.columnOffset');
  validateInt32(lineOffset, 'options.lineOffset');
  if (cachedData !== undefined)
    validateBuffer(cachedData, 'options.cachedData');
  validateBoolean(produceCachedData, 'options.produceCachedData');
  if (parsingContext !== undefined) {
    if (
      typeof parsingContext !== 'object' ||
      parsingContext === null ||
      !isContext(parsingContext)
    ) {
      throw new ERR_INVALID_ARG_TYPE(
        'options.parsingContext',
        'Context',
        parsingContext,
      );
    }
  }
  validateArray(contextExtensions, 'options.contextExtensions');
  ArrayPrototypeForEach(contextExtensions, (extension, i) => {
    const name = `options.contextExtensions[${i}]`;
    validateObject(extension, name, kValidateObjectAllowNullable);
  });

  const hostDefinedOptionId =
      getHostDefinedOptionId(importModuleDynamically, filename);

  return internalCompileFunction(
    code, filename, lineOffset, columnOffset,
    cachedData, produceCachedData, parsingContext, contextExtensions,
    params, hostDefinedOptionId, importModuleDynamically,
  ).function;
}

const measureMemoryModes = {
  summary: constants.measureMemory.mode.SUMMARY,
  detailed: constants.measureMemory.mode.DETAILED,
};

const measureMemoryExecutions = {
  default: constants.measureMemory.execution.DEFAULT,
  eager: constants.measureMemory.execution.EAGER,
};

function measureMemory(options = kEmptyObject) {
  emitExperimentalWarning('vm.measureMemory');
  validateObject(options, 'options');
  const { mode = 'summary', execution = 'default' } = options;
  validateOneOf(mode, 'options.mode', ['summary', 'detailed']);
  validateOneOf(execution, 'options.execution', ['default', 'eager']);
  return _measureMemory(measureMemoryModes[mode]);
}

const vmConstants = {
  __proto__: null,
  USE_MAIN_CONTEXT_DEFAULT_LOADER: vm_dynamic_import_main_context_default,
  DONT_CONTEXTIFY: vm_context_no_contextify,
};

ObjectFreeze(vmConstants);

module.exports = {
  Script,
  createContext,
  createScript,
  runInContext,
  runInNewContext,
  runInThisContext,
  isContext,
  compileFunction,
  measureMemory,
  constants: vmConstants,
};

// The vm module is patched to include vm.Module, vm.SourceTextModule
// and vm.SyntheticModule in the pre-execution phase when
// --experimental-vm-modules is on.
