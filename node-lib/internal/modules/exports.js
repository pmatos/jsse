'use strict';

// "exports" and "imports" resolution of package.json (Node's
// lib/internal/modules/esm/resolve.js), over absolute paths instead of file
// URLs. A target that names another package ("imports" only) is returned as
// `{ bare: specifier }` for the caller to resolve.

const {
  ArrayIsArray,
  ArrayPrototypeIncludes,
  ObjectGetOwnPropertyNames,
  ObjectPrototypeHasOwnProperty,
  RegExpPrototypeExec,
  RegExpPrototypeSymbolReplace,
  StringPrototypeEndsWith,
  StringPrototypeIncludes,
  StringPrototypeIndexOf,
  StringPrototypeLastIndexOf,
  StringPrototypeReplace,
  StringPrototypeSlice,
  StringPrototypeStartsWith,
} = primordials;

const { getNearestParentPackageJSON } = require('internal/modules/package_json_reader');
const {
  codes: {
    ERR_INVALID_MODULE_SPECIFIER,
    ERR_INVALID_PACKAGE_CONFIG,
    ERR_INVALID_PACKAGE_TARGET,
    ERR_PACKAGE_IMPORT_NOT_DEFINED,
    ERR_PACKAGE_PATH_NOT_EXPORTED,
  },
} = require('internal/errors');

const invalidSegmentRegEx = /(^|\\|\/)((\.|%2e)(\.|%2e)?|(n|%6e|%4e)(o|%6f|%4f)(d|%64|%44)(e|%65|%45)(_|%5f)(m|%6d|%4d)(o|%6f|%4f)(d|%64|%44)(u|%75|%55)(l|%6c|%4c)(e|%65|%45)(s|%73|%53))?(\\|\/|$)/i;
const deprecatedInvalidSegmentRegEx = /(^|\\|\/)((\.|%2e)(\.|%2e)?|(n|%6e|%4e)(o|%6f|%4f)(d|%64|%44)(e|%65|%45)(_|%5f)(m|%6d|%4d)(o|%6f|%4f)(d|%64|%44)(u|%75|%55)(l|%6c|%4c)(e|%65|%45)(s|%73|%53))(\\|\/|$)/i;
const patternRegEx = /\*/g;
const urlSchemeRegEx = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;
const numericKeyRegEx = /^(?:0|[1-9][0-9]*)$/;

function invalidPackageTarget(match, target, pkg, internal, base) {
  return new ERR_INVALID_PACKAGE_TARGET(`${pkg.dir}/`, match, target, internal, base);
}

function exportsNotFound(subpath, pkg, base) {
  return new ERR_PACKAGE_PATH_NOT_EXPORTED(`${pkg.dir}/`, subpath, base);
}

function throwInvalidSubpath(request, match, pkg, internal, base) {
  const reason = `request is not a valid match in pattern "${match}" for the ` +
    `"${internal ? 'imports' : 'exports'}" resolution of ${pkg.pjsonPath}`;
  throw new ERR_INVALID_MODULE_SPECIFIER(request, reason, base);
}

function resolvePackageTargetString(target, subpath, match, pkg, base, pattern, internal) {
  if (subpath !== '' && !pattern && target[target.length - 1] !== '/') {
    throw invalidPackageTarget(match, target, pkg, internal, base);
  }

  if (!StringPrototypeStartsWith(target, './')) {
    if (internal && !StringPrototypeStartsWith(target, '../') &&
        !StringPrototypeStartsWith(target, '/') &&
        RegExpPrototypeExec(urlSchemeRegEx, target) === null) {
      return {
        bare: pattern ?
          RegExpPrototypeSymbolReplace(patternRegEx, target, () => subpath) :
          target + subpath,
      };
    }
    throw invalidPackageTarget(match, target, pkg, internal, base);
  }

  const inner = StringPrototypeSlice(target, 2);
  if (RegExpPrototypeExec(invalidSegmentRegEx, inner) !== null &&
      RegExpPrototypeExec(deprecatedInvalidSegmentRegEx, inner) !== null) {
    throw invalidPackageTarget(match, target, pkg, internal, base);
  }

  const resolved = `${pkg.dir}/${inner}`;
  if (subpath === '') return resolved;

  if (RegExpPrototypeExec(invalidSegmentRegEx, subpath) !== null &&
      RegExpPrototypeExec(deprecatedInvalidSegmentRegEx, subpath) !== null) {
    const request = pattern ?
      StringPrototypeReplace(match, '*', () => subpath) : match + subpath;
    throwInvalidSubpath(request, match, pkg, internal, base);
  }

  if (pattern) {
    return RegExpPrototypeSymbolReplace(patternRegEx, resolved, () => subpath);
  }
  return resolved + subpath;
}

function resolvePackageTarget(target, subpath, packageSubpath, pkg, base, pattern, internal, conditions) {
  if (typeof target === 'string') {
    return resolvePackageTargetString(target, subpath, packageSubpath, pkg, base, pattern, internal);
  }
  if (ArrayIsArray(target)) {
    if (target.length === 0) return null;
    let lastException;
    for (let i = 0; i < target.length; i++) {
      let resolveResult;
      try {
        resolveResult = resolvePackageTarget(target[i], subpath, packageSubpath,
                                             pkg, base, pattern, internal, conditions);
      } catch (e) {
        lastException = e;
        if (e.code === 'ERR_INVALID_PACKAGE_TARGET') continue;
        throw e;
      }
      if (resolveResult === undefined) continue;
      if (resolveResult === null) {
        lastException = null;
        continue;
      }
      return resolveResult;
    }
    if (lastException === undefined || lastException === null) {
      return lastException;
    }
    throw lastException;
  }
  if (typeof target === 'object' && target !== null) {
    const keys = ObjectGetOwnPropertyNames(target);
    for (let i = 0; i < keys.length; i++) {
      if (RegExpPrototypeExec(numericKeyRegEx, keys[i]) !== null) {
        throw new ERR_INVALID_PACKAGE_CONFIG(
          pkg.pjsonPath, base, '"exports" cannot contain numeric property keys.');
      }
    }
    for (let i = 0; i < keys.length; i++) {
      const key = keys[i];
      if (key === 'default' || ArrayPrototypeIncludes(conditions, key)) {
        const resolveResult = resolvePackageTarget(target[key], subpath, packageSubpath,
                                                   pkg, base, pattern, internal, conditions);
        if (resolveResult === undefined) continue;
        return resolveResult;
      }
    }
    return undefined;
  }
  if (target === null) return null;
  throw invalidPackageTarget(packageSubpath, target, pkg, internal, base);
}

function isConditionalExportsMainSugar(exports, pkg, base) {
  if (typeof exports === 'string' || ArrayIsArray(exports)) return true;
  if (typeof exports !== 'object' || exports === null) return false;

  const keys = ObjectGetOwnPropertyNames(exports);
  let isConditionalSugar = false;
  for (let i = 0; i < keys.length; i++) {
    const key = keys[i];
    const curIsConditionalSugar = key === '' || key[0] !== '.';
    if (i === 0) {
      isConditionalSugar = curIsConditionalSugar;
    } else if (isConditionalSugar !== curIsConditionalSugar) {
      throw new ERR_INVALID_PACKAGE_CONFIG(
        pkg.pjsonPath, base,
        '"exports" cannot contain some keys starting with \'.\' and some not.' +
        ' The exports object must either be an object of package subpath keys' +
        ' or an object of main entry condition name keys only.');
    }
  }
  return isConditionalSugar;
}

function patternKeyCompare(a, b) {
  const aPatternIndex = StringPrototypeIndexOf(a, '*');
  const bPatternIndex = StringPrototypeIndexOf(b, '*');
  const baseLenA = aPatternIndex === -1 ? a.length : aPatternIndex + 1;
  const baseLenB = bPatternIndex === -1 ? b.length : bPatternIndex + 1;
  if (baseLenA > baseLenB) return -1;
  if (baseLenB > baseLenA) return 1;
  if (aPatternIndex === -1) return 1;
  if (bPatternIndex === -1) return -1;
  if (a.length > b.length) return -1;
  if (b.length > a.length) return 1;
  return 0;
}

// The longest matching "key/*/suffix" pattern of `map` for `name`.
function bestPatternMatch(map, name) {
  let bestMatch = '';
  let bestMatchSubpath;
  const keys = ObjectGetOwnPropertyNames(map);
  for (let i = 0; i < keys.length; i++) {
    const key = keys[i];
    const patternIndex = StringPrototypeIndexOf(key, '*');
    if (patternIndex !== -1 &&
        StringPrototypeStartsWith(name, StringPrototypeSlice(key, 0, patternIndex))) {
      const patternTrailer = StringPrototypeSlice(key, patternIndex + 1);
      if (name.length >= key.length &&
          StringPrototypeEndsWith(name, patternTrailer) &&
          patternKeyCompare(bestMatch, key) === 1 &&
          StringPrototypeLastIndexOf(key, '*') === patternIndex) {
        bestMatch = key;
        bestMatchSubpath = StringPrototypeSlice(
          name, patternIndex, name.length - patternTrailer.length);
      }
    }
  }
  return { bestMatch, bestMatchSubpath };
}

function packageExportsResolve(pkg, packageSubpath, base, conditions) {
  let exports = pkg.exports;
  if (isConditionalExportsMainSugar(exports, pkg, base)) {
    exports = { '.': exports };
  }

  if (ObjectPrototypeHasOwnProperty(exports, packageSubpath) &&
      !StringPrototypeIncludes(packageSubpath, '*') &&
      !StringPrototypeEndsWith(packageSubpath, '/')) {
    const resolveResult = resolvePackageTarget(
      exports[packageSubpath], '', packageSubpath, pkg, base, false, false, conditions);
    if (resolveResult == null) throw exportsNotFound(packageSubpath, pkg, base);
    return resolveResult;
  }

  const { bestMatch, bestMatchSubpath } = bestPatternMatch(exports, packageSubpath);
  if (bestMatch) {
    const resolveResult = resolvePackageTarget(
      exports[bestMatch], bestMatchSubpath, bestMatch, pkg, base, true, false, conditions);
    if (resolveResult == null) throw exportsNotFound(packageSubpath, pkg, base);
    return resolveResult;
  }

  throw exportsNotFound(packageSubpath, pkg, base);
}

function packageImportsResolve(name, base, conditions) {
  if (name === '#' || StringPrototypeEndsWith(name, '/')) {
    throw new ERR_INVALID_MODULE_SPECIFIER(
      name, 'is not a valid internal imports specifier name', base);
  }
  const pkg = getNearestParentPackageJSON(base);
  if (pkg !== undefined) {
    const imports = pkg.imports;
    if (imports !== undefined && imports !== null && typeof imports === 'object') {
      if (ObjectPrototypeHasOwnProperty(imports, name) &&
          !StringPrototypeIncludes(name, '*')) {
        const resolveResult = resolvePackageTarget(
          imports[name], '', name, pkg, base, false, true, conditions);
        if (resolveResult != null) return resolveResult;
      } else {
        const { bestMatch, bestMatchSubpath } = bestPatternMatch(imports, name);
        if (bestMatch) {
          const resolveResult = resolvePackageTarget(
            imports[bestMatch], bestMatchSubpath, bestMatch, pkg, base, true, true, conditions);
          if (resolveResult != null) return resolveResult;
        }
      }
    }
  }
  throw new ERR_PACKAGE_IMPORT_NOT_DEFINED(name, pkg && `${pkg.dir}/`, base);
}

module.exports = { packageExportsResolve, packageImportsResolve };
