'use strict';

// The names `import` binds for a CommonJS module (Node's cjsPreparseModuleExports):
// what the lexer finds in the source, plus the names of the modules it
// re-exports.

const {
  ArrayFrom,
  ObjectCreate,
  Set: SafeSet,
  SetPrototypeAdd,
} = primordials;

const fs = require('fs');
const path = require('path');
const { cjsLex } = require('internal/host/module');
const { canBeRequiredByUsers } = require('internal/modules/builtins');

const cache = ObjectCreate(null);

function cjsExportNames(filename) {
  const cached = cache[filename];
  if (cached !== undefined) return cached;
  const names = new SafeSet();
  cache[filename] = names;

  let source;
  try {
    source = fs.readFileSync(filename, 'utf8');
  } catch {
    return names;
  }
  const { 0: exports, 1: reexports } = cjsLex(source);
  for (let i = 0; i < exports.length; i++) SetPrototypeAdd(names, exports[i]);
  if (reexports.length === 0) return names;

  const { Module } = require('internal/modules/cjs/loader');
  const parent = new Module(filename, null);
  parent.filename = filename;
  parent.paths = Module._nodeModulePaths(path.dirname(filename));
  for (let i = 0; i < reexports.length; i++) {
    let resolved;
    try {
      resolved = Module._resolveFilename(reexports[i], parent, false);
    } catch {
      continue;
    }
    if (canBeRequiredByUsers(resolved)) continue;
    const ext = path.extname(resolved);
    if (ext !== '.js' && ext !== '.cjs' && Module._extensions[ext]) continue;
    const inner = cjsExportNames(resolved);
    const list = ArrayFrom(inner);
    for (let j = 0; j < list.length; j++) {
      SetPrototypeAdd(names, list[j]);
    }
  }
  return names;
}

module.exports = { cjsExportNames };
