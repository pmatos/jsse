'use strict';

// import.meta for a module on disk (Node's initializeImportMeta and
// createImportMetaResolve), called by the engine through the host function.

const {
  RegExpPrototypeExec,
  StringPrototypeStartsWith,
} = primordials;

const path = require('path');
const { canBeRequiredByUsers } = require('internal/modules/builtins');
const { defaultResolve } = require('internal/modules/esm/resolve');

const schemeRegEx = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;

function createResolve(filename) {
  return function resolve(specifier, parentURL) {
    specifier = `${specifier}`;
    if (StringPrototypeStartsWith(specifier, 'node:')) {
      return specifier;
    }
    if (!StringPrototypeStartsWith(specifier, 'file:') &&
        RegExpPrototypeExec(schemeRegEx, specifier) !== null) {
      return new (require('internal/url').URL)(specifier).href;
    }
    let resolved;
    try {
      resolved = defaultResolve(specifier, filename, true);
    } catch (error) {
      if ((error.code === 'ERR_MODULE_NOT_FOUND' ||
           error.code === 'ERR_UNSUPPORTED_DIR_IMPORT') && error.url) {
        return error.url;
      }
      throw error;
    }
    if (resolved.format === 'builtin' || canBeRequiredByUsers(resolved.target)) {
      return `node:${resolved.target}`;
    }
    return require('url').pathToFileURL(resolved.target).href;
  };
}

function initializeImportMeta(meta, filename, isMain) {
  meta.dirname = path.dirname(filename);
  meta.filename = filename;
  meta.main = isMain;
  meta.resolve = createResolve(filename);
  meta.url = require('url').pathToFileURL(filename).href;
}

module.exports = { initializeImportMeta };
