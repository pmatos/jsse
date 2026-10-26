'use strict';

// The CommonJS loader (#1219), after Node's lib/internal/modules/cjs/loader.js:
// `Module`, the resolution algorithm (relative, absolute, node_modules walk,
// package.json main/exports/imports, extensions, directory index), the module
// wrapper, circular requires and the main-module and `-e` entry points. ESM,
// `.node` addons, source maps, policies and `--preserve-symlinks` are out of
// scope; `require()` of an ES module throws ERR_REQUIRE_ESM.

const {
  ArrayIsArray,
  ArrayPrototypeConcat,
  ArrayPrototypeFilter,
  ArrayPrototypeIncludes,
  ArrayPrototypeIndexOf,
  ArrayPrototypeJoin,
  ArrayPrototypePush,
  ArrayPrototypeSlice,
  ArrayPrototypeSplice,
  ArrayPrototypeUnshift,
  Boolean,
  Function: FunctionCtor,
  JSONParse,
  ObjectCreate,
  ObjectDefineProperty,
  ObjectKeys,
  ReflectApply,
  RegExpPrototypeExec,
  StringPrototypeCharCodeAt,
  StringPrototypeEndsWith,
  StringPrototypeIndexOf,
  StringPrototypeSlice,
  StringPrototypeSplit,
  StringPrototypeStartsWith,
  WeakMap,
  WeakMapPrototypeGet,
  WeakMapPrototypeSet,
} = primordials;

const fs = require('fs');
const path = require('path');
const { canBeRequiredByUsers, loadBuiltin } = require('internal/modules/builtins');
const { getNearestParentPackageJSON, readPackage } = require('internal/modules/package_json_reader');
const { packageExportsResolve, packageImportsResolve } = require('internal/modules/exports');
const {
  codes: {
    ERR_INVALID_ARG_VALUE,
    ERR_REQUIRE_ESM,
    ERR_UNKNOWN_BUILTIN_MODULE,
  },
} = require('internal/errors');
const { validateString } = require('internal/validators');

// Node 26 also matches `module-sync`; it is left out while require() of an
// ES module is unsupported, so such packages fall back to their CJS entry.
const cjsConditions = ['require', 'node'];
const EXPORTS_PATTERN = /^((?:@[^/\\%]+\/)?[^./\\%][^/\\%]*)(\/.*)?$/;
const kNodeModules = 'node_modules';

const moduleParentCache = new WeakMap();
const relativeResolveCache = ObjectCreate(null);
const realpathCache = ObjectCreate(null);
let modulePaths = [];

function Module(id = '', parent) {
  this.id = id;
  this.path = path.dirname(id);
  this.exports = {};
  WeakMapPrototypeSet(moduleParentCache, this, parent);
  updateChildren(parent, this, false);
  this.filename = null;
  this.loaded = false;
  this.children = [];
}

ObjectDefineProperty(Module.prototype, 'parent', {
  get() { return WeakMapPrototypeGet(moduleParentCache, this); },
  set(value) { WeakMapPrototypeSet(moduleParentCache, this, value); },
  configurable: true,
});

Module._cache = ObjectCreate(null);
Module._pathCache = ObjectCreate(null);
Module._extensions = ObjectCreate(null);
Module.globalPaths = [];
Module.Module = Module;

function updateChildren(parent, child, scan) {
  const children = parent && parent.children;
  if (children && !(scan && ArrayPrototypeIncludes(children, child))) {
    ArrayPrototypePush(children, child);
  }
}

function initPaths() {
  const homeDir = process.env.HOME;
  const nodePath = process.env.NODE_PATH;
  const prefixDir = path.resolve(process.execPath, '..', '..');
  let paths = [path.resolve(prefixDir, 'lib', 'node')];
  if (homeDir) {
    ArrayPrototypeUnshift(paths, path.resolve(homeDir, '.node_libraries'));
    ArrayPrototypeUnshift(paths, path.resolve(homeDir, '.node_modules'));
  }
  if (nodePath) {
    paths = ArrayPrototypeConcat(
      ArrayPrototypeFilter(StringPrototypeSplit(nodePath, path.delimiter), Boolean),
      paths);
  }
  modulePaths = paths;
  Module.globalPaths = ArrayPrototypeSlice(modulePaths);
}
Module._initPaths = initPaths;

Module._nodeModulePaths = function(from) {
  from = path.resolve(from);
  if (from === '/') return ['/node_modules'];

  const paths = [];
  const nmLen = kNodeModules.length;
  let p = 0;
  let last = from.length;
  for (let i = from.length - 1; i >= 0; --i) {
    const code = StringPrototypeCharCodeAt(from, i);
    if (code === 47) {
      if (p !== nmLen) {
        ArrayPrototypePush(paths, `${StringPrototypeSlice(from, 0, last)}/node_modules`);
      }
      last = i;
      p = 0;
    } else if (p !== -1) {
      if (StringPrototypeCharCodeAt(kNodeModules, nmLen - 1 - p) === code) {
        ++p;
      } else {
        p = -1;
      }
    }
  }
  ArrayPrototypePush(paths, '/node_modules');
  return paths;
};

function normalizeRequirableId(id) {
  if (StringPrototypeStartsWith(id, 'node:')) {
    const rest = StringPrototypeSlice(id, 5);
    if (canBeRequiredByUsers(rest)) return rest;
  } else if (canBeRequiredByUsers(id)) {
    return id;
  }
  return undefined;
}

Module._resolveLookupPaths = function(request, parent) {
  if (normalizeRequirableId(request) !== undefined) return null;

  if (request[0] !== '.' ||
      (request.length > 1 && request[1] !== '.' && request[1] !== '/')) {
    let paths = modulePaths;
    if (parent && parent.paths && parent.paths.length) {
      paths = ArrayPrototypeConcat(parent.paths, paths);
    }
    return paths.length > 0 ? paths : null;
  }

  if (!parent || !parent.id || !parent.filename) {
    return ArrayPrototypeConcat(['.'], Module._nodeModulePaths('.'), modulePaths);
  }
  return [path.dirname(parent.filename)];
};

// 0 for a file, 1 for a directory, negative when the path does not exist.
function stat(filename) {
  let s;
  try {
    s = fs.statSync(filename, { throwIfNoEntry: false });
  } catch {
    return -1;
  }
  if (s === undefined) return -2;
  return s.isDirectory() ? 1 : 0;
}

function toRealPath(requestPath) {
  const cached = realpathCache[requestPath];
  if (cached !== undefined) return cached;
  return (realpathCache[requestPath] = fs.realpathSync(requestPath));
}

function tryFile(requestPath) {
  if (stat(requestPath) !== 0) return false;
  return toRealPath(requestPath);
}

function tryExtensions(basePath, exts) {
  for (let i = 0; i < exts.length; i++) {
    const filename = tryFile(basePath + exts[i]);
    if (filename) return filename;
  }
  return false;
}

function createEsmNotFoundErr(request, pjsonPath) {
  const err = new Error(`Cannot find module '${request}'`);
  err.code = 'MODULE_NOT_FOUND';
  if (pjsonPath) err.path = pjsonPath;
  return err;
}

function finalizeEsmResolution(resolved, errPath) {
  const actual = tryFile(resolved);
  if (actual) return actual;
  throw createEsmNotFoundErr(resolved, errPath);
}

function tryPackage(requestPath, exts, originalPath) {
  const main = readPackage(requestPath).main;
  if (!main) return tryExtensions(path.resolve(requestPath, 'index'), exts);

  const filename = path.resolve(requestPath, main);
  let actual = tryFile(filename) ||
    tryExtensions(filename, exts) ||
    tryExtensions(path.resolve(filename, 'index'), exts);
  if (actual === false) {
    actual = tryExtensions(path.resolve(requestPath, 'index'), exts);
    if (!actual) {
      const err = new Error(
        `Cannot find module '${filename}'. ` +
        'Please verify that the package.json has a valid "main" entry');
      err.code = 'MODULE_NOT_FOUND';
      err.path = path.resolve(requestPath, 'package.json');
      err.requestPath = originalPath;
      throw err;
    }
  }
  return actual;
}

function resolveExports(nmPath, request) {
  const match = RegExpPrototypeExec(EXPORTS_PATTERN, request);
  if (!match) return undefined;
  const name = match[1];
  const expansion = match[2] === undefined ? '' : match[2];
  const pkg = readPackage(path.resolve(nmPath, name));
  if (pkg.exists && pkg.exports != null) {
    try {
      const resolved = packageExportsResolve(pkg, `.${expansion}`, undefined, cjsConditions);
      return finalizeEsmResolution(resolved, pkg.dir);
    } catch (e) {
      if (e.code === 'ERR_MODULE_NOT_FOUND') {
        throw createEsmNotFoundErr(request, `${pkg.dir}/package.json`);
      }
      throw e;
    }
  }
  return undefined;
}

function isRelativeRequest(request) {
  return StringPrototypeCharCodeAt(request, 0) === 46 &&
    (request.length === 1 || request[1] === '/' ||
      (request[1] === '.' && (request.length === 2 || request[2] === '/')));
}

Module._findPath = function(request, paths) {
  const absoluteRequest = path.isAbsolute(request);
  if (absoluteRequest) {
    paths = [''];
  } else if (!paths || paths.length === 0) {
    return false;
  }

  const cacheKey = `${request}\x00${ArrayPrototypeJoin(paths, '\x00')}`;
  const entry = Module._pathCache[cacheKey];
  if (entry) return entry;

  let exts;
  const last = request.length - 1;
  const trailingSlash = request.length > 0 &&
    (request[last] === '/' ||
      (request[last] === '.' &&
        (request.length === 1 || request[last - 1] === '/' ||
          (request[last - 1] === '.' &&
            (request.length === 2 || request[last - 2] === '/')))));

  let insidePath = true;
  if (isRelativeRequest(request)) {
    insidePath = !StringPrototypeStartsWith(path.normalize(request), '..');
  }

  for (let i = 0; i < paths.length; i++) {
    const curPath = paths[i];
    if (insidePath && curPath && stat(curPath) < 1) continue;

    if (!absoluteRequest) {
      const exportsResolved = resolveExports(curPath, request);
      if (exportsResolved) return exportsResolved;
    }

    const basePath = path.resolve(curPath, request);
    let filename;
    const rc = stat(basePath);
    if (!trailingSlash) {
      if (rc === 0) filename = toRealPath(basePath);
      if (!filename) {
        if (exts === undefined) exts = ObjectKeys(Module._extensions);
        filename = tryExtensions(basePath, exts);
      }
    }
    if (!filename && rc === 1) {
      if (exts === undefined) exts = ObjectKeys(Module._extensions);
      filename = tryPackage(basePath, exts, request);
    }
    if (filename) {
      Module._pathCache[cacheKey] = filename;
      return filename;
    }
  }
  return false;
};

function trySelf(parentPath, request) {
  if (!parentPath) return false;
  const pkg = getNearestParentPackageJSON(parentPath);
  if (pkg === undefined || pkg.exports == null || pkg.name === undefined) {
    return false;
  }
  let expansion;
  if (request === pkg.name) {
    expansion = '.';
  } else if (StringPrototypeStartsWith(request, `${pkg.name}/`)) {
    expansion = `.${StringPrototypeSlice(request, pkg.name.length)}`;
  } else {
    return false;
  }
  try {
    const resolved = packageExportsResolve(pkg, expansion, parentPath, cjsConditions);
    return finalizeEsmResolution(resolved, pkg.dir);
  } catch (e) {
    if (e.code === 'ERR_MODULE_NOT_FOUND') {
      throw createEsmNotFoundErr(request, pkg.pjsonPath);
    }
    throw e;
  }
}

Module._resolveFilename = function(request, parent, isMain, options) {
  if (normalizeRequirableId(request) !== undefined) return request;

  let paths;
  if (typeof options === 'object' && options !== null) {
    if (ArrayIsArray(options.paths)) {
      if (StringPrototypeStartsWith(request, './') ||
          StringPrototypeStartsWith(request, '../')) {
        paths = options.paths;
      } else {
        const fakeParent = new Module('', null);
        paths = [];
        for (let i = 0; i < options.paths.length; i++) {
          fakeParent.paths = Module._nodeModulePaths(options.paths[i]);
          const lookupPaths = Module._resolveLookupPaths(request, fakeParent);
          for (let j = 0; j < lookupPaths.length; j++) {
            if (!ArrayPrototypeIncludes(paths, lookupPaths[j])) {
              ArrayPrototypePush(paths, lookupPaths[j]);
            }
          }
        }
      }
    } else if (options.paths === undefined) {
      paths = Module._resolveLookupPaths(request, parent);
    } else {
      throw new ERR_INVALID_ARG_VALUE('options.paths', options.paths);
    }
  } else {
    paths = Module._resolveLookupPaths(request, parent);
  }

  const parentPath = parent && parent.filename ? parent.filename : undefined;

  if (request[0] === '#' && parentPath) {
    const pkg = getNearestParentPackageJSON(parentPath);
    if (pkg !== undefined && pkg.imports != null) {
      try {
        const resolved = packageImportsResolve(request, parentPath, cjsConditions);
        if (typeof resolved === 'object') {
          return Module._resolveFilename(resolved.bare, parent, isMain);
        }
        return finalizeEsmResolution(resolved, pkg.dir);
      } catch (e) {
        if (e.code === 'ERR_MODULE_NOT_FOUND') throw createEsmNotFoundErr(request);
        throw e;
      }
    }
  }

  const selfResolved = trySelf(parentPath, request);
  if (selfResolved) {
    const cacheKey = `${request}\x00` +
      (paths ? ArrayPrototypeJoin(paths, '\x00') : '');
    Module._pathCache[cacheKey] = selfResolved;
    return selfResolved;
  }

  const filename = Module._findPath(request, paths);
  if (filename) return filename;

  const requireStack = [];
  for (let cursor = parent; cursor; cursor = WeakMapPrototypeGet(moduleParentCache, cursor)) {
    ArrayPrototypePush(requireStack, cursor.filename || cursor.id);
  }
  let message = `Cannot find module '${request}'`;
  if (requireStack.length > 0) {
    message += `\nRequire stack:\n- ${ArrayPrototypeJoin(requireStack, '\n- ')}`;
  }
  const err = new Error(message);
  err.code = 'MODULE_NOT_FOUND';
  err.requireStack = requireStack;
  throw err;
};

Module._load = function(request, parent, isMain) {
  let relResolveCacheIdentifier;
  if (parent) {
    relResolveCacheIdentifier = `${parent.path}\x00${request}`;
    const filename = relativeResolveCache[relResolveCacheIdentifier];
    if (filename !== undefined) {
      const cachedModule = Module._cache[filename];
      if (cachedModule !== undefined) {
        updateChildren(parent, cachedModule, true);
        return cachedModule.exports;
      }
      delete relativeResolveCache[relResolveCacheIdentifier];
    }
  }

  if (StringPrototypeStartsWith(request, 'node:')) {
    const id = StringPrototypeSlice(request, 5);
    if (!canBeRequiredByUsers(id)) throw new ERR_UNKNOWN_BUILTIN_MODULE(request);
    return loadBuiltin(id);
  }

  const filename = Module._resolveFilename(request, parent, isMain);
  const cachedModule = Module._cache[filename];
  if (cachedModule !== undefined) {
    updateChildren(parent, cachedModule, true);
    return cachedModule.exports;
  }

  if (canBeRequiredByUsers(filename)) return loadBuiltin(filename);

  const module = new Module(filename, parent);
  if (isMain) {
    process.mainModule = module;
    module.id = '.';
  }
  Module._cache[filename] = module;
  if (parent) {
    relativeResolveCache[relResolveCacheIdentifier] = filename;
  }

  let threw = true;
  try {
    module.load(filename);
    threw = false;
  } finally {
    if (threw) {
      delete Module._cache[filename];
      if (parent) {
        delete relativeResolveCache[relResolveCacheIdentifier];
        const children = parent.children;
        if (ArrayIsArray(children)) {
          const index = ArrayPrototypeIndexOf(children, module);
          if (index !== -1) ArrayPrototypeSplice(children, index, 1);
        }
      }
    }
  }
  return module.exports;
};

function findLongestRegisteredExtension(filename) {
  const name = path.basename(filename);
  let startIndex = 0;
  let index;
  while ((index = StringPrototypeIndexOf(name, '.', startIndex)) !== -1) {
    startIndex = index + 1;
    if (index === 0) continue;
    const currentExtension = StringPrototypeSlice(name, index);
    if (Module._extensions[currentExtension]) return currentExtension;
  }
  return '.js';
}

Module.prototype.load = function(filename) {
  this.filename = filename;
  this.paths = Module._nodeModulePaths(path.dirname(filename));
  const extension = findLongestRegisteredExtension(filename);
  if (StringPrototypeEndsWith(filename, '.mjs') && !Module._extensions['.mjs']) {
    throw new ERR_REQUIRE_ESM(filename);
  }
  Module._extensions[extension](this, filename);
  this.loaded = true;
};

Module.prototype.require = function(id) {
  validateString(id, 'id');
  if (id === '') {
    throw new ERR_INVALID_ARG_VALUE('id', id, 'must be a non-empty string');
  }
  return Module._load(id, this, false);
};

function stripBOM(content) {
  return StringPrototypeCharCodeAt(content, 0) === 0xFEFF ?
    StringPrototypeSlice(content, 1) : content;
}

function wrapSafe(content) {
  if (StringPrototypeStartsWith(content, '#!')) {
    content = `//${StringPrototypeSlice(content, 2)}`;
  }
  return new FunctionCtor('exports', 'require', 'module', '__filename', '__dirname', content);
}

Module.prototype._compile = function(content, filename) {
  const compiledWrapper = wrapSafe(content);
  const dirname = path.dirname(filename);
  const require = makeRequireFunction(this);
  return ReflectApply(compiledWrapper, this.exports,
                      [this.exports, require, this, filename, dirname]);
};

function makeRequireFunction(mod) {
  const require = function require(path) {
    return mod.require(path);
  };

  function resolve(request, options) {
    validateString(request, 'request');
    return Module._resolveFilename(request, mod, false, options);
  }
  require.resolve = resolve;

  function paths(request) {
    validateString(request, 'request');
    return Module._resolveLookupPaths(request, mod);
  }
  resolve.paths = paths;

  require.main = process.mainModule;
  require.extensions = Module._extensions;
  require.cache = Module._cache;
  return require;
}

Module._extensions['.js'] = function(module, filename) {
  const content = fs.readFileSync(filename, 'utf8');
  if (StringPrototypeEndsWith(filename, '.js')) {
    const pkg = getNearestParentPackageJSON(filename);
    if (pkg !== undefined && pkg.type === 'module') {
      throw new ERR_REQUIRE_ESM(filename);
    }
  }
  module._compile(content, filename);
};

Module._extensions['.json'] = function(module, filename) {
  const content = fs.readFileSync(filename, 'utf8');
  try {
    module.exports = JSONParse(stripBOM(content));
  } catch (err) {
    err.message = `${filename}: ${err.message}`;
    throw err;
  }
};

Module._extensions['.node'] = function(module, filename) {
  const err = new Error(`Native addons are not supported: ${filename}`);
  err.code = 'ERR_DLOPEN_FAILED';
  throw err;
};

function fileUrlToPath(url) {
  const match = RegExpPrototypeExec(/^file:\/\/(?:localhost)?(\/.*)$/, url);
  return match === null ? url : decodeURIComponent(match[1]);
}

Module.createRequire = function(filename) {
  if (typeof filename === 'object' && filename !== null &&
      typeof filename.href === 'string') {
    filename = filename.href;
  }
  if (typeof filename === 'string' && StringPrototypeStartsWith(filename, 'file:')) {
    filename = fileUrlToPath(filename);
  }
  if (typeof filename !== 'string' || !path.isAbsolute(filename)) {
    throw new ERR_INVALID_ARG_VALUE('filename', filename,
                                    'must be a file URL object, file URL string, or absolute path string');
  }
  const proxyPath = StringPrototypeEndsWith(filename, '/') ?
    path.join(filename, 'noop.js') : filename;
  const m = new Module(proxyPath);
  m.filename = proxyPath;
  m.paths = Module._nodeModulePaths(m.path);
  return makeRequireFunction(m);
};

function runMain() {
  Module._load(path.resolve(process.argv[1]), null, true);
}

// `node -e` and the REPL: `require`, `module`, `exports`, `__filename` and
// `__dirname` are globals of an `[eval]` module rooted at the working directory.
function setupEval() {
  const cwd = process.cwd();
  const module = new Module('[eval]');
  module.filename = path.join(cwd, '[eval]');
  module.paths = Module._nodeModulePaths(cwd);
  const globals = {
    __filename: '[eval]',
    module,
    exports: module.exports,
    __dirname: '.',
    require: makeRequireFunction(module),
  };
  const names = ObjectKeys(globals);
  for (let i = 0; i < names.length; i++) {
    ObjectDefineProperty(globalThis, names[i], {
      value: globals[names[i]],
      writable: true,
      enumerable: true,
      configurable: true,
    });
  }
}

initPaths();

module.exports = { Module, makeRequireFunction, runMain, setupEval };
