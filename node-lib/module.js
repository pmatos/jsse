'use strict';

// The public `module` builtin (#1220); what jsse leaves out is in the ADR.

const {
  ArrayPrototypeIncludes,
  ObjectFreeze,
  StringPrototypeSlice,
  StringPrototypeStartsWith,
} = primordials;

const { Module } = require('internal/modules/cjs/loader');
const { findPackageJSON } = require('internal/modules/find_package_json');
const { SourceMap } = require('internal/source_map/source_map');
const {
  findSourceMap,
  getSourceMapsSupport,
  setSourceMapsSupport,
} = require('internal/source_map/support');
const { codes: { ERR_INVALID_ARG_TYPE, ERR_METHOD_NOT_IMPLEMENTED } } = require('internal/errors');
const { validateObject } = require('internal/validators');

const publicIds = [
  '_http_agent', '_http_client', '_http_common', '_http_incoming',
  '_http_outgoing', '_http_server', '_tls_common', '_tls_wrap', 'assert',
  'assert/strict', 'async_hooks', 'buffer', 'child_process', 'cluster',
  'console', 'constants', 'crypto', 'dgram', 'diagnostics_channel', 'dns',
  'dns/promises', 'domain', 'events', 'fs', 'fs/promises', 'http', 'http2',
  'https', 'inspector', 'inspector/promises', 'module', 'net', 'os', 'path',
  'path/posix', 'path/win32', 'perf_hooks', 'process', 'punycode',
  'querystring', 'readline', 'readline/promises', 'repl', 'stream',
  'stream/consumers', 'stream/promises', 'stream/web', 'string_decoder',
  'sys', 'timers', 'timers/promises', 'tls', 'trace_events', 'tty', 'url',
  'util', 'util/types', 'v8', 'vm', 'wasi', 'worker_threads', 'zlib',
];
const schemeOnlyIds = ['sea', 'sqlite', 'test', 'test/reporters'];

// Node's list, not what jsse ships: tools use it to decide what is external.
const builtinModules = [...publicIds, 'node:sea', 'node:sqlite', 'node:test', 'node:test/reporters'];

function isBuiltin(moduleName) {
  if (typeof moduleName !== 'string') return false;
  if (StringPrototypeStartsWith(moduleName, 'node:')) {
    const id = StringPrototypeSlice(moduleName, 5);
    return ArrayPrototypeIncludes(publicIds, id) || ArrayPrototypeIncludes(schemeOnlyIds, id);
  }
  return ArrayPrototypeIncludes(publicIds, moduleName);
}

const compileCacheStatus = ObjectFreeze({
  __proto__: null,
  FAILED: 0,
  ENABLED: 1,
  ALREADY_ENABLED: 2,
  DISABLED: 3,
});

function enableCompileCache(options) {
  if (options !== undefined && typeof options !== 'string') {
    validateObject(options, 'options');
    if (options.directory !== undefined && typeof options.directory !== 'string') {
      throw new ERR_INVALID_ARG_TYPE('options.directory', 'string', options.directory);
    }
  }
  return { status: compileCacheStatus.DISABLED, message: 'The compile cache is not supported by jsse' };
}

function getCompileCacheDir() {
  return undefined;
}

function flushCompileCache() {}

function stripTypeScriptTypes(code) {
  throw new ERR_METHOD_NOT_IMPLEMENTED('stripTypeScriptTypes()');
}

function syncBuiltinESMExports() {}

Module.isBuiltin = isBuiltin;
Module.syncBuiltinESMExports = syncBuiltinESMExports;
Module.builtinModules = ObjectFreeze(builtinModules);
Module.constants = ObjectFreeze({ __proto__: null, compileCacheStatus });
Module.enableCompileCache = enableCompileCache;
Module.findPackageJSON = findPackageJSON;
Module.flushCompileCache = flushCompileCache;
Module.getCompileCacheDir = getCompileCacheDir;
Module.stripTypeScriptTypes = stripTypeScriptTypes;
Module.findSourceMap = findSourceMap;
Module.SourceMap = SourceMap;
Module.getSourceMapsSupport = getSourceMapsSupport;
Module.setSourceMapsSupport = setSourceMapsSupport;

module.exports = Module;
