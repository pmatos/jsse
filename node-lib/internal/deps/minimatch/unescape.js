// Derived from minimatch (https://github.com/isaacs/minimatch, ISC license,
// Copyright (c) Isaac Z. Schlueter and Contributors) as bundled in Node.js v26.8.2
// deps/minimatch. Split from the bundle at its module boundaries; dist/commonjs/unescape.js.
'use strict';
Object.defineProperty(exports, "__esModule", { value: true });
exports.unescape = void 0;
var unescape = (s, { windowsPathsNoEscape = false, magicalBraces = true } = {}) => {
  if (magicalBraces) {
    return windowsPathsNoEscape ? s.replace(/\[([^/\\])\]/g, "$1") : s.replace(/((?!\\).|^)\[([^/\\])\]/g, "$1$2").replace(/\\([^/])/g, "$1");
  }
  return windowsPathsNoEscape ? s.replace(/\[([^/\\{}])\]/g, "$1") : s.replace(/((?!\\).|^)\[([^/\\{}])\]/g, "$1$2").replace(/\\([^/{}])/g, "$1");
};
exports.unescape = unescape;
