'use strict';

// Node's lib/internal/vm.js over ./vm/contextify. `importModuleDynamically`
// is validated and then ignored: `import()` inside a vm script uses the
// engine's default behaviour.

const { Symbol } = primordials;

const contextify = require('internal/vm/contextify');
const { validateFunction } = require('internal/validators');

const {
  vm_dynamic_import_main_context_default,
} = contextify.symbols;

function isContext(object) {
  return contextify.isContext(object);
}

function getHostDefinedOptionId(importModuleDynamically, hint) {
  if (importModuleDynamically === vm_dynamic_import_main_context_default) {
    return importModuleDynamically;
  }
  if (importModuleDynamically !== undefined) {
    validateFunction(importModuleDynamically,
                     'options.importModuleDynamically');
  }
  return Symbol(hint);
}

function registerImportModuleDynamically() {}

function internalCompileFunction(
  code, filename, lineOffset, columnOffset,
  cachedData, produceCachedData, parsingContext, contextExtensions,
  params, hostDefinedOptionId, importModuleDynamically) {
  const result = contextify.compileFunction(
    code, filename, lineOffset, columnOffset, cachedData, produceCachedData,
    parsingContext, contextExtensions, params);
  if (produceCachedData) {
    result.function.cachedDataProduced = true;
    result.function.cachedData = Buffer.from('jsse-code-cache');
  }
  if (typeof result.cachedDataRejected === 'boolean') {
    result.function.cachedDataRejected = result.cachedDataRejected;
  }
  registerImportModuleDynamically(result.function, importModuleDynamically);
  return result;
}

function runScriptInThisContext(script, displayErrors, breakOnFirstLine) {
  return script.runInContext(null, -1, displayErrors, false, breakOnFirstLine);
}

module.exports = {
  getHostDefinedOptionId,
  internalCompileFunction,
  isContext,
  registerImportModuleDynamically,
  runScriptInThisContext,
};
