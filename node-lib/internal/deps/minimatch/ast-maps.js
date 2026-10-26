// Derived from minimatch (https://github.com/isaacs/minimatch, ISC license,
// Copyright (c) Isaac Z. Schlueter and Contributors) as bundled in Node.js v26.8.2
// deps/minimatch. Split from the bundle at its module boundaries; the extglob adoption tables of ast.js.
'use strict';
var adoptionMap = new Map([
  ["!", ["@"]],
  ["?", ["?", "@"]],
  ["@", ["@"]],
  ["*", ["*", "+", "?", "@"]],
  ["+", ["+", "@"]]
]);
var adoptionWithSpaceMap = new Map([
  ["!", ["?"]],
  ["@", ["?"]],
  ["+", ["?", "*"]]
]);
var adoptionAnyMap = new Map([
  ["!", ["?", "@"]],
  ["?", ["?", "@"]],
  ["@", ["?", "@"]],
  ["*", ["*", "+", "?", "@"]],
  ["+", ["+", "@", "?", "*"]]
]);
var usurpMap = new Map([
  ["!", new Map([["!", "@"]])],
  [
    "?",
    new Map([
      ["*", "*"],
      ["+", "*"]
    ])
  ],
  [
    "@",
    new Map([
      ["!", "!"],
      ["?", "?"],
      ["@", "@"],
      ["*", "*"],
      ["+", "+"]
    ])
  ],
  [
    "+",
    new Map([
      ["?", "*"],
      ["*", "*"]
    ])
  ]
]);
exports.adoptionMap = adoptionMap;
exports.adoptionWithSpaceMap = adoptionWithSpaceMap;
exports.adoptionAnyMap = adoptionAnyMap;
exports.usurpMap = usurpMap;
