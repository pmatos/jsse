'use strict';

// require() of an ES module and the format decision for `.js` files (Node's
// loadESMFromCJS and populateCJSExportsFromESM). The engine loads and
// evaluates the graph; this turns the namespace into `module.exports`.

const { ObjectHasOwn } = primordials;

const path = require('path');
const { esmRequire, esmFacade } = require('internal/host/module');
const { getNearestParentPackageJSON } = require('internal/modules/package_json_reader');
const { detectsEsmSyntax } = require('internal/modules/esm/resolve');
const {
  codes: { ERR_REQUIRE_ASYNC_MODULE, ERR_REQUIRE_CYCLE_MODULE },
} = require('internal/errors');

// 'module', 'commonjs', or 'detect' (a typeless `.js`: CommonJS unless its
// source only compiles as an ES module).
function getFormat(filename) {
  switch (path.extname(filename)) {
    case '.mjs': return 'module';
    case '.cjs': return 'commonjs';
    case '.js': {
      const pkg = getNearestParentPackageJSON(filename);
      if (pkg === undefined || pkg.type === undefined || pkg.type === 'none') return 'detect';
      return pkg.type === 'module' ? 'module' : 'commonjs';
    }
    default: return 'commonjs';
  }
}

function canRetryAsEsm(error, filename) {
  return error instanceof SyntaxError && detectsEsmSyntax(filename);
}

// A main file detected as ES module syntax runs through the ES loader, which
// allows top-level await.
function runMain(filename) {
  const url = require('url').pathToFileURL(filename).href;
  import(url).catch((error) => {
    process.nextTick(() => { throw error; });
  });
}

function load(mod, filename) {
  if (mod.id === '.') {
    runMain(filename);
    return;
  }
  const parent = mod.parent;
  const ns = esmRequire(filename);
  if (ns === 'async') throw new ERR_REQUIRE_ASYNC_MODULE(filename, parent);
  if (ns === 'cycle') throw new ERR_REQUIRE_CYCLE_MODULE(mod.id, parent);
  if (ObjectHasOwn(ns, 'module.exports')) {
    mod.exports = ns['module.exports'];
  } else if (!ObjectHasOwn(ns, 'default') || ObjectHasOwn(ns, '__esModule')) {
    mod.exports = ns;
  } else {
    mod.exports = esmFacade(ns);
  }
}

module.exports = { getFormat, canRetryAsEsm, load };
