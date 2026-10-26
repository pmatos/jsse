// Differential corpus for the embedded `assert` module (issue #1225): the
// message, code, operator and generatedMessage of a matrix of failing
// assertions, printed identically by Node and jsse --node. Run by
// scripts/run-node-assert-oracle.sh as corpus-a.js + corpus-b.js + corpus-run.js.
var assert = require("assert");
var strict = require("assert/strict");
var util = require("util");

var lines = [];
var scenarios = [];

function out(text) { lines.push(text); }

function report(label, value, err, threw) {
  if (!threw) {
    out(label + " => " + (value === undefined ? "no error" : "value " + util.inspect(value)));
    return;
  }
  var head = [err && err.name, err && err.code, err && err.operator, err && err.generatedMessage].join("|");
  out(label + " => " + head + " " + JSON.stringify(err && err.message));
}

function add(label, fn) { scenarios.push([label, fn]); }

function expectThrow(fn) {
  return function () {
    try { fn(); } catch (e) { throw e; }
    return "no throw";
  };
}

function cases(prefix, fn, pairs) {
  pairs.forEach(function (p, i) {
    add(prefix + " #" + i, expectThrow(function () { fn(p[0], p[1]); }));
  });
}

function Foo() { this.a = 1; }
function Bar() { this.a = 1; }
class Point { constructor(x, y) { this.x = x; this.y = y; } }
var circular = { name: "c" };
circular.self = circular;
var big = [];
for (var n = 0; n < 60; n++) big.push({ id: n, label: "item " + n });
var big2 = big.map(function (v, i) { return i === 45 ? { id: 45, label: "changed" } : v; });
var sym = Symbol("s");

var valuePairs = [
  [1, 2], [0, -0], [NaN, 0], ["a", "b"], ["abc", "abe"], ["hello world", "hello there"],
  ["a".repeat(40), "a".repeat(39) + "b"], ["line1\nline2", "line1\nline3"], [1n, 2n], [1, "1"],
  [true, false], [null, undefined], [sym, Symbol("s")], [undefined, null],
  [{ a: 1 }, { a: 2 }], [{ a: 1, b: 2 }, { a: 1 }], [[1, 2, 3], [1, 2, 4]], [[1, 2], [1, 2, 3]],
  [[], {}], [{ a: [1, { b: 2 }] }, { a: [1, { b: 3 }] }], [new Foo(), new Bar()], [new Foo(), { a: 1 }],
  [new Point(1, 2), new Point(1, 3)], [new Map([[1, 2]]), new Map([[1, 3]])], [new Set([1, 2]), new Set([1, 3])],
  [new Date(0), new Date(1)], [/a/g, /a/i], [new Error("x"), new Error("y")], [[1, , 3], [1, undefined, 3]],
  [big, big2], [circular, { name: "c", self: {} }], [{ f: function () {} }, { f: function () {} }],
  [new Uint8Array([1, 2]), new Uint8Array([1, 3])], [Object.create(null), {}], [new Number(1), new Number(2)],
  [[{ a: 1 }], [{ a: 1 }]], [{}, {}], [function a() {}, function a() {}], [{ a: 1 }, { a: 1 }],
];

cases("strictEqual", function (a, b) { assert.strictEqual(a, b); }, valuePairs);
cases("notStrictEqual", function (a, b) { assert.notStrictEqual(a, a); }, valuePairs.slice(0, 20).map(function (p) { return [p[0], p[0]]; }));
cases("deepStrictEqual", function (a, b) { assert.deepStrictEqual(a, b); }, valuePairs);
cases("notDeepStrictEqual", function (a) { assert.notDeepStrictEqual(a, a); }, valuePairs.slice(14, 30).map(function (p) { return [p[0], p[0]]; }));
cases("deepEqual", function (a, b) { assert.deepEqual(a, b); }, valuePairs);
cases("notDeepEqual", function (a) { assert.notDeepEqual(a, a); }, valuePairs.slice(0, 30).map(function (p) { return [p[0], p[0]]; }));
cases("equal", function (a, b) { assert.equal(a, b); }, valuePairs.slice(0, 20));
cases("notEqual", function (a) { assert.notEqual(a, a); }, [[1, 1], ["a", "a"], [{}, {}], [NaN, NaN], [null, undefined]].map(function (p, i) { return i === 2 ? [p[0], p[0]] : p; }));
cases("partialDeepStrictEqual", function (a, b) { assert.partialDeepStrictEqual(a, b); }, [
  [{ a: 1 }, { a: 2 }], [{ a: 1 }, { b: 1 }], [[1, 2, 3], [4]], [new Set([1]), new Set([2])], [new Map([[1, 2]]), new Map([[1, 3]])],
  [{ a: { b: 1, c: 2 } }, { a: { b: 2 } }], [[1, 2, 3], [3, 1]], ["abc", "abe"], [1, 2], [{ a: new Date(0) }, { a: new Date(1) }],
]);
cases("strict.equal", function (a, b) { strict.equal(a, b); }, valuePairs.slice(0, 16));
cases("strict.deepEqual", function (a, b) { strict.deepEqual(a, b); }, valuePairs.slice(14, 36));
