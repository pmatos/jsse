'use strict';

// ES module resolution for the default loader (Node's
// lib/internal/modules/esm/resolve.js and translators.js), over absolute
// paths. `defaultResolve` answers `{ format, target }`: `target` is the real
// path of a file, or the id of a builtin. `validateAttributes` is the import
// attribute check.

const {
  RegExpPrototypeExec,
  StringPrototypeCharAt,
  StringPrototypeIndexOf,
  StringPrototypeSlice,
  StringPrototypeStartsWith,
} = primordials;

const fs = require('fs');
const path = require('path');
const { canBeRequiredByUsers } = require('internal/modules/builtins');
const { getNearestParentPackageJSON, readPackage } = require('internal/modules/package_json_reader');
const { packageExportsResolve, packageImportsResolve } = require('internal/modules/exports');
const {
  codes: {
    ERR_IMPORT_ATTRIBUTE_MISSING,
    ERR_IMPORT_ATTRIBUTE_TYPE_INCOMPATIBLE,
    ERR_IMPORT_ATTRIBUTE_UNSUPPORTED,
    ERR_INVALID_MODULE_SPECIFIER,
    ERR_MODULE_NOT_FOUND,
    ERR_UNKNOWN_BUILTIN_MODULE,
    ERR_UNKNOWN_FILE_EXTENSION,
    ERR_UNSUPPORTED_DIR_IMPORT,
    ERR_UNSUPPORTED_ESM_URL_SCHEME,
  },
} = require('internal/errors');

const conditions = ['node', 'import', 'module-sync', 'node-addons'];
const schemeRegEx = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;
const esmSyntaxRegEx = /(^|[\n;])\s*(import\s*[\w${*"']|export\s*[\w${*]|export\s+default\b)|\bimport\.meta\b/;
const invalidPackageNameRegEx = /^\.|%|\\/;
const extensionRegEx = /\.[^./\\]*$/;

function statKind(file) {
  try {
    const stat = fs.statSync(file);
    return stat.isDirectory() ? 'dir' : 'file';
  } catch {
    return undefined;
  }
}

function lazyUrl() {
  return require('internal/url');
}

function toPath(url) {
  return lazyUrl().fileURLToPath(url);
}

function parsePackageName(specifier, base) {
  let separatorIndex = StringPrototypeIndexOf(specifier, '/');
  let validPackageName = true;
  let isScoped = false;
  if (specifier[0] === '@') {
    isScoped = true;
    if (separatorIndex === -1 || specifier.length === 0) {
      validPackageName = false;
    } else {
      separatorIndex = StringPrototypeIndexOf(specifier, '/', separatorIndex + 1);
    }
  }
  const packageName = separatorIndex === -1 ? specifier : StringPrototypeSlice(specifier, 0, separatorIndex);
  if (RegExpPrototypeExec(invalidPackageNameRegEx, packageName) !== null) {
    validPackageName = false;
  }
  if (!validPackageName) {
    throw new ERR_INVALID_MODULE_SPECIFIER(specifier, 'is not a valid package name', base);
  }
  const packageSubpath = '.' + (separatorIndex === -1 ? '' : StringPrototypeSlice(specifier, separatorIndex));
  return { packageName, packageSubpath, isScoped };
}

function legacyMainResolve(pkgDir, pkg, base) {
  const suffixes = ['', '.js', '.json', '.node', '/index.js', '/index.json', '/index.node'];
  if (pkg.exists && pkg.main !== undefined) {
    for (let i = 0; i < suffixes.length; i++) {
      const candidate = path.resolve(pkgDir, pkg.main + suffixes[i]);
      if (statKind(candidate) === 'file') return candidate;
    }
  }
  const indexes = ['index.js', 'index.json', 'index.node'];
  for (let i = 0; i < indexes.length; i++) {
    const candidate = path.resolve(pkgDir, indexes[i]);
    if (statKind(candidate) === 'file') return candidate;
  }
  throw new ERR_MODULE_NOT_FOUND(pkgDir, base);
}

function packageResolve(specifier, base) {
  const { packageName, packageSubpath } = parsePackageName(specifier, base);

  const self = getNearestParentPackageJSON(base);
  if (self !== undefined && self.exports != null && self.name === packageName) {
    return packageExportsResolve(self, packageSubpath, base, conditions);
  }

  let dir = path.dirname(base);
  for (;;) {
    const pkgDir = path.join(dir, 'node_modules', packageName);
    if (statKind(pkgDir) === 'dir') {
      const pkg = readPackage(pkgDir);
      if (pkg.exists && pkg.exports != null) {
        return packageExportsResolve(pkg, packageSubpath, base, conditions);
      }
      if (packageSubpath === '.') return legacyMainResolve(pkgDir, pkg, base);
      return path.join(pkgDir, packageSubpath);
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  throw new ERR_MODULE_NOT_FOUND(packageName, base);
}

function builtinId(specifier) {
  if (StringPrototypeStartsWith(specifier, 'node:')) {
    const id = StringPrototypeSlice(specifier, 5);
    if (!canBeRequiredByUsers(id)) throw new ERR_UNKNOWN_BUILTIN_MODULE(specifier);
    return id;
  }
  return canBeRequiredByUsers(specifier) ? specifier : undefined;
}

function isPathSpecifier(specifier) {
  if (specifier[0] === '/') return true;
  if (specifier[0] !== '.') return false;
  return specifier.length === 1 || specifier[1] === '/' ||
    (specifier[1] === '.' && (specifier.length === 2 || specifier[2] === '/'));
}

// A relative URL reference as a path: query and fragment are dropped and
// percent escapes decoded, which is what `new URL(specifier, parent)` yields.
function relativePath(specifier, parentDir) {
  let ref = specifier;
  const cut = ref.search(/[?#]/);
  if (cut !== -1) ref = StringPrototypeSlice(ref, 0, cut);
  if (StringPrototypeIndexOf(ref, '%') !== -1) {
    try {
      ref = decodeURIComponent(ref);
    } catch {
      throw new ERR_INVALID_MODULE_SPECIFIER(specifier, 'is not a valid URL', parentDir);
    }
  }
  const resolved = path.resolve(parentDir, ref);
  return StringPrototypeCharAt(ref, ref.length - 1) === '/' && resolved !== '/' ? resolved + '/' : resolved;
}

function finalizeResolution(resolved, base) {
  const trailingSlash = StringPrototypeCharAt(resolved, resolved.length - 1) === '/';
  const file = trailingSlash ? StringPrototypeSlice(resolved, 0, -1) || '/' : resolved;
  const kind = statKind(file);
  if (kind === 'dir') throw new ERR_UNSUPPORTED_DIR_IMPORT(file, base, resolved);
  if (kind !== 'file' || trailingSlash) throw new ERR_MODULE_NOT_FOUND(file, base, resolved);
  return fs.realpathSync(file);
}

function detectsEsmSyntax(file) {
  const FunctionCtor = Function;
  let source;
  try {
    source = fs.readFileSync(file, 'utf8');
    new FunctionCtor('exports', 'require', 'module', '__filename', '__dirname', source);
    return false;
  } catch (e) {
    return e instanceof SyntaxError && source !== undefined &&
      RegExpPrototypeExec(esmSyntaxRegEx, source) !== null;
  }
}

function defaultGetFormat(file) {
  const match = RegExpPrototypeExec(extensionRegEx, path.basename(file));
  const ext = match === null ? '' : match[0];
  switch (ext) {
    case '.mjs': return 'module';
    case '.cjs': return 'commonjs';
    case '.json': return 'json';
    case '.js':
    case '': {
      const pkg = getNearestParentPackageJSON(file);
      const type = pkg === undefined ? 'none' : pkg.type;
      if (type === 'module') return 'module';
      if (type === 'commonjs' || ext === '') return 'commonjs';
      return detectsEsmSyntax(file) ? 'module' : 'commonjs';
    }
    default:
      throw new ERR_UNKNOWN_FILE_EXTENSION(ext, file);
  }
}

function defaultResolve(specifier, parentPath) {
  const base = parentPath === undefined ? path.join(process.cwd(), '[eval1]') : parentPath;
  const parentDir = path.dirname(base);

  const builtin = builtinId(specifier);
  if (builtin !== undefined) return { format: 'builtin', target: builtin };

  let resolved;
  if (StringPrototypeStartsWith(specifier, 'file:')) {
    resolved = toPath(specifier);
  } else if (isPathSpecifier(specifier)) {
    resolved = relativePath(specifier, parentDir);
  } else if (specifier[0] === '#') {
    resolved = packageImportsResolve(specifier, base, conditions);
    if (typeof resolved === 'object' && resolved !== null) {
      resolved = bareOrBuiltin(resolved.bare, base);
      if (typeof resolved === 'object') return resolved;
    }
  } else if (RegExpPrototypeExec(schemeRegEx, specifier) !== null) {
    const protocol = StringPrototypeSlice(specifier, 0, StringPrototypeIndexOf(specifier, ':') + 1);
    const supported = ['file', 'data'];
    if (protocol !== 'http:' && protocol !== 'https:') supported.push('node');
    throw new ERR_UNSUPPORTED_ESM_URL_SCHEME(protocol, supported);
  } else {
    resolved = packageResolve(specifier, base);
  }
  const target = finalizeResolution(resolved, base);
  return { format: defaultGetFormat(target), target };
}

function bareOrBuiltin(bare, base) {
  const builtin = builtinId(bare);
  if (builtin !== undefined) return { format: 'builtin', target: builtin };
  return packageResolve(bare, base);
}

function validateAttributes(format, target, type) {
  const url = format === 'builtin' ? `node:${target}` : lazyUrl().pathToFileURL(target).href;
  if (type === undefined) {
    if (format === 'json') throw new ERR_IMPORT_ATTRIBUTE_MISSING(url, 'type', 'json');
    return;
  }
  if (type !== 'json') throw new ERR_IMPORT_ATTRIBUTE_UNSUPPORTED('type', type, url);
  if (format !== 'json') throw new ERR_IMPORT_ATTRIBUTE_TYPE_INCOMPATIBLE(url, 'json');
}

module.exports = { defaultResolve, validateAttributes };
