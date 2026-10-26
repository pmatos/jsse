'use strict';

// The parts of `Module` that tools patch or call and that the loader does not
// need itself, after Node's lib/internal/modules/cjs/loader.js: the
// `wrap`/`wrapper` pair (patching either makes the loader compile through
// `Module.wrap`), `createRequire`, `_preloadModules`, `isPreloading` and
// `runMain`. `module.js` adds the rest of the public `module` API.

const {
  ArrayIsArray,
  ObjectDefineProperty,
  Proxy: ProxyCtor,
  ReflectDefineProperty,
  ReflectSet,
  StringPrototypeEndsWith,
} = primordials;

const path = require('path');
const { codes: { ERR_INVALID_ARG_VALUE } } = require('internal/errors');

const createRequireError = 'must be a file URL object, file URL string, or absolute path string';

const wrapper = [
  '(function (exports, require, module, __filename, __dirname) { ',
  '\n});',
];

let patched = false;
function wrap(script) {
  return `${state.wrapper[0]}${script}${state.wrapper[1]}`;
}
let wrapFn = wrap;
const state = {
  wrapper: new ProxyCtor(wrapper, {
    set(target, property, value, receiver) {
      patched = true;
      return ReflectSet(target, property, value, receiver);
    },
    defineProperty(target, property, descriptor) {
      patched = true;
      return ReflectDefineProperty(target, property, descriptor);
    },
  }),
};

function isPatched() {
  return patched;
}

function install(Module, makeRequireFunction) {
  ObjectDefineProperty(Module, 'wrap', {
    get() { return wrapFn; },
    set(value) { patched = true; wrapFn = value; },
  });
  ObjectDefineProperty(Module, 'wrapper', {
    get() { return state.wrapper; },
    set(value) { patched = true; state.wrapper = value; },
  });

  Module.createRequire = function createRequire(filename) {
    const { isURL, fileURLToPath } = require('internal/url');
    let filepath;
    if (isURL(filename) || (typeof filename === 'string' && !path.isAbsolute(filename))) {
      try {
        filepath = fileURLToPath(filename);
      } catch {
        throw new ERR_INVALID_ARG_VALUE('filename', filename, createRequireError);
      }
    } else if (typeof filename !== 'string') {
      throw new ERR_INVALID_ARG_VALUE('filename', filename, createRequireError);
    } else {
      filepath = filename;
    }
    const proxyPath = StringPrototypeEndsWith(filepath, '/') ?
      path.join(filepath, 'noop.js') : filepath;
    const m = new Module(proxyPath);
    m.filename = proxyPath;
    m.paths = Module._nodeModulePaths(m.path);
    return makeRequireFunction(m);
  };

  let isPreloading = false;
  ObjectDefineProperty(Module.prototype, 'isPreloading', {
    get() { return isPreloading; },
  });

  Module._preloadModules = function(requests) {
    if (!ArrayIsArray(requests)) return;
    isPreloading = true;
    const parent = new Module('internal/preload', null);
    try {
      parent.paths = Module._nodeModulePaths(process.cwd());
    } catch (e) {
      if (e.code !== 'ENOENT') throw e;
    }
    try {
      for (let n = 0; n < requests.length; n++) parent.require(requests[n]);
    } finally {
      isPreloading = false;
    }
  };

  Module.runMain = function executeUserEntryPoint(main = process.argv[1]) {
    Module._load(main, null, true);
  };
}

module.exports = { install, isPatched };
