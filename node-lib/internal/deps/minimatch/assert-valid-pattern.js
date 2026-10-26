// Derived from minimatch (https://github.com/isaacs/minimatch, ISC license,
// Copyright (c) Isaac Z. Schlueter and Contributors) as bundled in Node.js v26.8.2
// deps/minimatch. Split from the bundle at its module boundaries; dist/commonjs/assert-valid-pattern.js.
'use strict';
Object.defineProperty(exports, "__esModule", { value: true });
exports.assertValidPattern = void 0;
var MAX_PATTERN_LENGTH = 1024 * 64;
var assertValidPattern = (pattern) => {
  if (typeof pattern !== "string") {
    throw new TypeError("invalid pattern");
  }
  if (pattern.length > MAX_PATTERN_LENGTH) {
    throw new TypeError("pattern is too long");
  }
};
exports.assertValidPattern = assertValidPattern;
