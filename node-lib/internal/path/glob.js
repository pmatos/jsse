'use strict';

// The matchGlobPattern half of Node's lib/internal/fs/glob.js (v26.8.2):
// createMatcher and its pattern cache. The directory-walking Glob class is
// not embedded.

const { Map: SafeMap } = primordials;
const { isMacOS, isWindows } = require('internal/util');
const { validateString } = require('internal/validators');

const kGlobPatternCacheLimit = 250;
const patternFnCache = new SafeMap();

let minimatch;
function lazyMinimatch() {
  minimatch ??= require('internal/deps/minimatch/index');
  return minimatch;
}

function createMatcher(pattern, options) {
  const opts = {
    __proto__: null,
    nocase: isWindows || isMacOS,
    windowsPathsNoEscape: true,
    nonegate: true,
    nocomment: true,
    optimizationLevel: 2,
    platform: process.platform,
    nocaseMagicOnly: true,
    ...options,
  };
  return new (lazyMinimatch().Minimatch)(pattern, opts);
}

function matchGlobPattern(path, pattern, windows = isWindows) {
  validateString(path, 'path');
  validateString(pattern, 'pattern');

  const cacheKey = `${windows ? 'win32' : 'posix'}:${pattern}`;
  let matcher;
  if (patternFnCache.has(cacheKey)) {
    matcher = patternFnCache.get(cacheKey);
  } else {
    matcher = createMatcher(pattern, {
      platform: windows ? 'win32' : 'posix',
    });
    patternFnCache.set(cacheKey, matcher);

    if (patternFnCache.size >= kGlobPatternCacheLimit) {
      patternFnCache.delete(patternFnCache.keys().next().value);
    }
  }

  return matcher.match(path);
}

module.exports = { createMatcher, matchGlobPattern };
