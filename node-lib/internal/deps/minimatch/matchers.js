// Derived from minimatch (https://github.com/isaacs/minimatch, ISC license,
// Copyright (c) Isaac Z. Schlueter and Contributors) as bundled in Node.js v26.8.2
// deps/minimatch. Split from the bundle at its module boundaries; the matcher helpers and constants of dist/commonjs/index.js.
'use strict';
const assert_valid_pattern_js_1 = require('internal/deps/minimatch/assert-valid-pattern');
const brace_expansion_1 = require('internal/deps/minimatch/brace-expansion');
var starDotExtRE = /^\*+([^+@!?*[(]*)$/;
var starDotExtTest = (ext2) => (f) => !f.startsWith(".") && f.endsWith(ext2);
var starDotExtTestDot = (ext2) => (f) => f.endsWith(ext2);
var starDotExtTestNocase = (ext2) => {
  ext2 = ext2.toLowerCase();
  return (f) => !f.startsWith(".") && f.toLowerCase().endsWith(ext2);
};
var starDotExtTestNocaseDot = (ext2) => {
  ext2 = ext2.toLowerCase();
  return (f) => f.toLowerCase().endsWith(ext2);
};
var starDotStarRE = /^\*+\.\*+$/;
var starDotStarTest = (f) => !f.startsWith(".") && f.includes(".");
var starDotStarTestDot = (f) => f !== "." && f !== ".." && f.includes(".");
var dotStarRE = /^\.\*+$/;
var dotStarTest = (f) => f !== "." && f !== ".." && f.startsWith(".");
var starRE = /^\*+$/;
var starTest = (f) => f.length !== 0 && !f.startsWith(".");
var starTestDot = (f) => f.length !== 0 && f !== "." && f !== "..";
var qmarksRE = /^\?+([^+@!?*[(]*)?$/;
var qmarksTestNocase = ([$0, ext2 = ""]) => {
  const noext = qmarksTestNoExt([$0]);
  if (!ext2)
    return noext;
  ext2 = ext2.toLowerCase();
  return (f) => noext(f) && f.toLowerCase().endsWith(ext2);
};
var qmarksTestNocaseDot = ([$0, ext2 = ""]) => {
  const noext = qmarksTestNoExtDot([$0]);
  if (!ext2)
    return noext;
  ext2 = ext2.toLowerCase();
  return (f) => noext(f) && f.toLowerCase().endsWith(ext2);
};
var qmarksTestDot = ([$0, ext2 = ""]) => {
  const noext = qmarksTestNoExtDot([$0]);
  return !ext2 ? noext : (f) => noext(f) && f.endsWith(ext2);
};
var qmarksTest = ([$0, ext2 = ""]) => {
  const noext = qmarksTestNoExt([$0]);
  return !ext2 ? noext : (f) => noext(f) && f.endsWith(ext2);
};
var qmarksTestNoExt = ([$0]) => {
  const len = $0.length;
  return (f) => f.length === len && !f.startsWith(".");
};
var qmarksTestNoExtDot = ([$0]) => {
  const len = $0.length;
  return (f) => f.length === len && f !== "." && f !== "..";
};
var defaultPlatform = typeof process === "object" && process ? typeof process.env === "object" && process.env && process.env.__MINIMATCH_TESTING_PLATFORM__ || process.platform : "posix";
var path = {
  win32: { sep: "\\" },
  posix: { sep: "/" }
};
exports.sep = defaultPlatform === "win32" ? path.win32.sep : path.posix.sep;
const GLOBSTAR = Symbol("globstar **");
exports.GLOBSTAR = GLOBSTAR;
var qmark = "[^/]";
var star = qmark + "*?";
var twoStarDot = "(?:(?!(?:\\/|^)(?:\\.{1,2})($|\\/)).)*?";
var twoStarNoDot = "(?:(?!(?:\\/|^)\\.).)*?";
var globMagic = /[?*]|[+@!]\(.*?\)|\[|\]/;
var regExpEscape = (s) => s.replace(/[-[\]{}()*+?.,\\^$|#\s]/g, "\\$&");
var braceExpand = (pattern, options = {}) => {
  (0, assert_valid_pattern_js_1.assertValidPattern)(pattern);
  if (options.nobrace || !/\{(?:(?!\{).)*\}/.test(pattern)) {
    return [pattern];
  }
  return (0, brace_expansion_1.expand)(pattern, { max: options.braceExpandMax });
};
exports.braceExpand = braceExpand;
exports.starDotExtTest = starDotExtTest;
exports.starDotExtTestDot = starDotExtTestDot;
exports.starDotExtTestNocase = starDotExtTestNocase;
exports.starDotExtTestNocaseDot = starDotExtTestNocaseDot;
exports.starDotStarRE = starDotStarRE;
exports.starDotStarTest = starDotStarTest;
exports.starDotStarTestDot = starDotStarTestDot;
exports.dotStarRE = dotStarRE;
exports.dotStarTest = dotStarTest;
exports.starRE = starRE;
exports.starTest = starTest;
exports.starTestDot = starTestDot;
exports.starDotExtRE = starDotExtRE;
exports.qmarksRE = qmarksRE;
exports.qmarksTestNocase = qmarksTestNocase;
exports.qmarksTestNocaseDot = qmarksTestNocaseDot;
exports.qmarksTestDot = qmarksTestDot;
exports.qmarksTest = qmarksTest;
exports.qmarksTestNoExt = qmarksTestNoExt;
exports.qmarksTestNoExtDot = qmarksTestNoExtDot;
exports.defaultPlatform = defaultPlatform;
exports.path = path;
exports.qmark = qmark;
exports.star = star;
exports.twoStarDot = twoStarDot;
exports.twoStarNoDot = twoStarNoDot;
exports.globMagic = globMagic;
exports.regExpEscape = regExpEscape;
