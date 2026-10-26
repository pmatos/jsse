'use strict';

// package.json lookup for the CommonJS loader, after Node's
// lib/internal/modules/package_json_reader.js (the native reader is replaced
// by a JSON.parse of the file). Results are cached per directory for the life
// of the process, as in Node.

const {
  JSONParse,
  ObjectCreate,
  StringPrototypeCharCodeAt,
  StringPrototypeSlice,
} = primordials;

const fs = require('fs');
const path = require('path');
const { codes: { ERR_INVALID_PACKAGE_CONFIG } } = require('internal/errors');

const cache = ObjectCreate(null);

function stringOrUndefined(value) {
  return typeof value === 'string' ? value : undefined;
}

function readPackage(dir) {
  const cached = cache[dir];
  if (cached !== undefined) return cached;
  const pjsonPath = path.join(dir, 'package.json');
  let source;
  try {
    source = fs.readFileSync(pjsonPath, 'utf8');
  } catch (e) {
    if (e.code !== 'ENOENT' && e.code !== 'ENOTDIR' && e.code !== 'EISDIR') {
      throw e;
    }
    return (cache[dir] = { exists: false, pjsonPath, dir });
  }
  if (StringPrototypeCharCodeAt(source, 0) === 0xFEFF) {
    source = StringPrototypeSlice(source, 1);
  }
  let data;
  try {
    data = JSONParse(source);
  } catch (e) {
    throw new ERR_INVALID_PACKAGE_CONFIG(pjsonPath, undefined, e.message);
  }
  if (data === null || typeof data !== 'object') data = {};
  const type = data.type;
  return (cache[dir] = {
    exists: true,
    pjsonPath,
    dir,
    name: stringOrUndefined(data.name),
    main: stringOrUndefined(data.main),
    type: type === 'module' || type === 'commonjs' ? type : 'none',
    exports: data.exports,
    imports: data.imports,
  });
}

// The closest package.json at or above the directory of `checkPath`; the
// search does not cross a `node_modules` directory.
function getNearestParentPackageJSON(checkPath) {
  let dir = path.dirname(checkPath);
  for (;;) {
    const parent = path.dirname(dir);
    if (parent === dir || path.basename(dir) === 'node_modules') {
      return undefined;
    }
    const pkg = readPackage(dir);
    if (pkg.exists) return pkg;
    dir = parent;
  }
}

module.exports = { getNearestParentPackageJSON, readPackage };
