// Derived from minimatch (https://github.com/isaacs/minimatch, ISC license,
// Copyright (c) Isaac Z. Schlueter and Contributors) as bundled in Node.js v26.8.2
// deps/minimatch. Split from the bundle at its module boundaries; dist/commonjs/index.js.
'use strict';
Object.defineProperty(exports, "__esModule", { value: true });
exports.unescape = exports.escape = exports.AST = exports.Minimatch = exports.match = exports.makeRe = exports.braceExpand = exports.defaults = exports.filter = exports.GLOBSTAR = exports.sep = exports.minimatch = void 0;
var assert_valid_pattern_js_1 = require('internal/deps/minimatch/assert-valid-pattern');
var ast_js_1 = require('internal/deps/minimatch/ast');
var escape_js_1 = require('internal/deps/minimatch/escape');
var unescape_js_1 = require('internal/deps/minimatch/unescape');
var { Minimatch } = require('internal/deps/minimatch/minimatch');
var { GLOBSTAR, braceExpand, sep } = require('internal/deps/minimatch/matchers');
var minimatch = (p, pattern, options = {}) => {
  (0, assert_valid_pattern_js_1.assertValidPattern)(pattern);
  if (!options.nocomment && pattern.charAt(0) === "#") {
    return false;
  }
  return new Minimatch(pattern, options).match(p);
};
exports.minimatch = minimatch;
exports.sep = sep;
exports.minimatch.sep = exports.sep;
exports.GLOBSTAR = GLOBSTAR;
exports.minimatch.GLOBSTAR = exports.GLOBSTAR;
exports.braceExpand = braceExpand;
exports.minimatch.braceExpand = exports.braceExpand;
var filter = (pattern, options = {}) => (p) => (0, exports.minimatch)(p, pattern, options);
exports.filter = filter;
exports.minimatch.filter = exports.filter;
var ext = (a, b = {}) => Object.assign({}, a, b);
var defaults = (def) => {
  if (!def || typeof def !== "object" || !Object.keys(def).length) {
    return exports.minimatch;
  }
  const orig = exports.minimatch;
  const m = (p, pattern, options = {}) => orig(p, pattern, ext(def, options));
  return Object.assign(m, {
    Minimatch: class Minimatch extends orig.Minimatch {
      constructor(pattern, options = {}) {
        super(pattern, ext(def, options));
      }
      static defaults(options) {
        return orig.defaults(ext(def, options)).Minimatch;
      }
    },
    AST: class AST extends orig.AST {
      /* c8 ignore start */
      constructor(type, parent, options = {}) {
        super(type, parent, ext(def, options));
      }
      /* c8 ignore stop */
      static fromGlob(pattern, options = {}) {
        return orig.AST.fromGlob(pattern, ext(def, options));
      }
    },
    unescape: (s, options = {}) => orig.unescape(s, ext(def, options)),
    escape: (s, options = {}) => orig.escape(s, ext(def, options)),
    filter: (pattern, options = {}) => orig.filter(pattern, ext(def, options)),
    defaults: (options) => orig.defaults(ext(def, options)),
    makeRe: (pattern, options = {}) => orig.makeRe(pattern, ext(def, options)),
    braceExpand: (pattern, options = {}) => orig.braceExpand(pattern, ext(def, options)),
    match: (list, pattern, options = {}) => orig.match(list, pattern, ext(def, options)),
    sep: orig.sep,
    GLOBSTAR: exports.GLOBSTAR
  });
};
exports.defaults = defaults;
exports.minimatch.defaults = exports.defaults;
var makeRe = (pattern, options = {}) => new Minimatch(pattern, options).makeRe();
exports.makeRe = makeRe;
exports.minimatch.makeRe = exports.makeRe;
var match = (list, pattern, options = {}) => {
  const mm = new Minimatch(pattern, options);
  list = list.filter((f) => mm.match(f));
  if (mm.options.nonull && !list.length) {
    list.push(pattern);
  }
  return list;
};
exports.match = match;
exports.minimatch.match = exports.match;
exports.Minimatch = Minimatch;
var ast_js_2 = require('internal/deps/minimatch/ast');
Object.defineProperty(exports, "AST", { enumerable: true, get: function() {
  return ast_js_2.AST;
} });
var escape_js_2 = require('internal/deps/minimatch/escape');
Object.defineProperty(exports, "escape", { enumerable: true, get: function() {
  return escape_js_2.escape;
} });
var unescape_js_2 = require('internal/deps/minimatch/unescape');
Object.defineProperty(exports, "unescape", { enumerable: true, get: function() {
  return unescape_js_2.unescape;
} });
exports.minimatch.AST = ast_js_1.AST;
exports.minimatch.Minimatch = Minimatch;
exports.minimatch.escape = escape_js_1.escape;
exports.minimatch.unescape = unescape_js_1.unescape;
