'use strict';

// The host function behind the engine's module loader (src/interpreter/
// node_esm.rs): `resolve` answers `[format, target]`, `check` validates
// import attributes, `load` runs a CommonJS module or fetches a builtin's
// exports.

const { defaultResolve, validateAttributes } = require('internal/modules/esm/resolve');
const { loadBuiltin } = require('internal/modules/builtins');

function host(op, a, b, c) {
  switch (op) {
    case 'resolve': {
      const { format, target } = defaultResolve(a, b);
      return [format, target];
    }
    case 'check':
      validateAttributes(a, b, c);
      return undefined;
    case 'load':
      if (a === 'builtin') return loadBuiltin(b);
      return require('internal/modules/cjs/loader').Module._load(b, null, false);
  }
  return undefined;
}

module.exports = { host };
