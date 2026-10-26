'use strict';

// module.findPackageJSON(specifier, base) after Node's
// lib/internal/modules/package_json_reader.js: the path of the package.json
// that governs the module `specifier` resolves to from `base`, or of the
// package a bare specifier names.

const {
  RegExpPrototypeExec,
  StringPrototypeCharAt,
  StringPrototypeIndexOf,
  StringPrototypeSlice,
} = primordials;

const fs = require('fs');
const path = require('path');
const { getNearestParentPackageJSON } = require('internal/modules/package_json_reader');
const {
  codes: {
    ERR_INVALID_URL,
    ERR_MISSING_ARGS,
    ERR_MODULE_NOT_FOUND,
    ERR_UNSUPPORTED_RESOLVE_REQUEST,
  },
} = require('internal/errors');
const { validateString } = require('internal/validators');

const dotSegment = /(?:^|\/)\.{1,2}$/;
const schemeRegEx = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;

function canParse(specifier) {
  return RegExpPrototypeExec(schemeRegEx, specifier) !== null;
}

function toFilePath(url) {
  const { fileURLToPath } = require('internal/url');
  return fileURLToPath(url);
}

function packageNameOf(specifier) {
  let end = StringPrototypeIndexOf(specifier, '/');
  if (StringPrototypeCharAt(specifier, 0) === '@' && end !== -1) {
    end = StringPrototypeIndexOf(specifier, '/', end + 1);
  }
  return end === -1 ? specifier : StringPrototypeSlice(specifier, 0, end);
}

function findBarePackage(specifier, basePath) {
  const name = packageNameOf(specifier);
  let dir = path.dirname(basePath);
  for (;;) {
    const candidate = path.join(dir, 'node_modules', name, 'package.json');
    if (fs.existsSync(candidate)) return candidate;
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  throw new ERR_MODULE_NOT_FOUND(name, basePath);
}

function findPackageJSON(specifier, base = 'data:') {
  if (arguments.length === 0) throw new ERR_MISSING_ARGS('specifier');
  try {
    specifier = `${specifier}`;
  } catch {
    validateString(specifier, 'specifier');
  }

  let basePath;
  if (base === 'data:') {
    basePath = undefined;
  } else if (typeof base === 'object' && base !== null && typeof base.href === 'string') {
    basePath = toFilePath(base);
  } else {
    validateString(base, 'base');
    if (path.isAbsolute(base)) {
      basePath = base[base.length - 1] === '/' ? base + 'noop.js' : base;
    } else if (canParse(base)) {
      basePath = toFilePath(base);
    } else {
      throw new ERR_INVALID_URL(base);
    }
  }

  if (specifier && specifier[0] !== '.' && specifier[0] !== '/' && !canParse(specifier)) {
    if (basePath === undefined) {
      throw new ERR_UNSUPPORTED_RESOLVE_REQUEST(specifier, 'data:');
    }
    return findBarePackage(specifier, basePath);
  }

  let target;
  if (canParse(specifier)) {
    target = toFilePath(specifier);
  } else {
    if (basePath === undefined) {
      throw new ERR_UNSUPPORTED_RESOLVE_REQUEST(specifier, 'data:');
    }
    const { defaultResolve } = require('internal/modules/esm/resolve');
    try {
      target = defaultResolve(specifier, basePath).target;
    } catch (cause) {
      if (cause.code !== 'ERR_UNSUPPORTED_DIR_IMPORT') throw cause;
      target = toFilePath(cause.url);
      if (RegExpPrototypeExec(dotSegment, specifier) !== null) target += '/';
    }
  }
  if (target[target.length - 1] === '/') target += 'noop.js';
  const pkg = getNearestParentPackageJSON(target);
  return pkg === undefined ? undefined : pkg.pjsonPath;
}

module.exports = { findPackageJSON };
