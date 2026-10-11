// Closures created from one function literal share literal-derived metadata
// (parameter list, whether the body references `arguments`, whether the
// parameter list is simple) but every evaluation must still produce a fresh
// function object with its own environment, `prototype` object and `arguments`.
// Regression test for sharing that metadata across closures (issue #900).
// Spec: ECMAScript 2024, sec-functiondeclarationinstantiation,
// sec-runtime-semantics-instantiateordinaryfunctionexpression,
// sec-runtime-semantics-instantiatearrowfunctionexpression,
// sec-runtime-semantics-instantiateordinaryfunctionobject,
// sec-static-semantics-issimpleparameterlist, sec-createmappedargumentsobject,
// sec-createunmappedargumentsobject, sec-runtime-semantics-classdefinitionevaluation.

/*---
description: Closures of one function literal share metadata but not state
esid: sec-functiondeclarationinstantiation
flags: [async]
---*/

function check(cond, msg) {
  if (!cond) {
    throw new Test262Error(msg);
  }
}

// Distinct identity, independent captured bindings and independent prototypes.
function makeCounter(start) {
  var n = start;
  return function (a, b) {
    n += 1;
    return n;
  };
}
var c1 = makeCounter(10);
var c2 = makeCounter(100);
check(c1 !== c2, 'closures of one literal must be distinct objects');
check(c1.prototype !== c2.prototype, 'each closure has its own prototype object');
check(c1.length === 2 && c2.length === 2, 'length is derived from the shared parameter list');
check(c1() === 11 && c2() === 101 && c1() === 12 && c2() === 102,
  'captured bindings must be independent across interleaved calls');
c1.prototype.marker = 1;
check(c2.prototype.marker === undefined, 'prototype objects must not be shared');
c1.own = 1;
check(c2.own === undefined, 'own properties must not be shared');

// `arguments` is present exactly when referenced, including in a default
// initialiser; arrows see the enclosing function's `arguments`.
function makers() {
  return {
    body: function () { return arguments.length; },
    init: function (n = arguments.length) { return n; },
    none: function (a) { return typeof a; },
    arrow: function () { return (() => arguments[0])(); },
  };
}
var m1 = makers();
var m2 = makers();
check(m1.body !== m2.body, 'literals evaluated twice yield distinct functions');
check(m1.body(1, 2, 3) === 3 && m2.body() === 0, 'arguments object per call');
check(m1.init() === 0 && m2.init(undefined, 1) === 2, 'arguments in a default initialiser');
check(m1.none(7) === 'number', 'plain parameter binding');
check(m1.arrow('x') === 'x' && m2.arrow('y') === 'y', 'arrow sees the outer arguments');

// Mapped arguments for simple parameter lists, unmapped otherwise.
function mappedMaker() {
  return function (a) { a = 2; return arguments[0]; };
}
function unmappedMaker() {
  return function (a = 0) { a = 2; return arguments[0]; };
}
var isStrict = (function () { return this === undefined; })();
var mm1 = mappedMaker();
var mm2 = mappedMaker();
var mappedResult = isStrict ? 1 : 2;
check(mm1(1) === mappedResult && mm2(1) === mappedResult,
  'simple parameters give a mapped arguments object in sloppy code only');
var um1 = unmappedMaker();
var um2 = unmappedMaker();
check(um1(1) === 1 && um2(1) === 1, 'non-simple parameters give an unmapped arguments object');

// Created in a loop and called interleaved, for every function kind.
var fns = [];
for (var i = 0; i < 4; i++) {
  fns.push((function (k) {
    return {
      plain: function (x) { return x + k; },
      arrow: (x) => x * k,
      method: ({ m() { return k; } }).m,
    };
  })(i));
}
for (var j = 0; j < 4; j++) {
  check(fns[j].plain(10) === 10 + j, 'plain closure ' + j);
  check(fns[j].arrow(10) === 10 * j, 'arrow closure ' + j);
  check(fns[j].method() === j, 'method closure ' + j);
}

// Generator closures from one literal run independent instances.
function makeGen(k) {
  return function* (n) { for (var x = 0; x < n; x++) { yield x + k; } };
}
var g1 = makeGen(0)(3);
var g2 = makeGen(100)(3);
check(g1.next().value === 0 && g2.next().value === 100 && g1.next().value === 1 &&
  g2.next().value === 101, 'generator instances are independent');

// Async function closures from one literal.
var log = [];
function makeAsync(k) {
  return async function (v) { log.push(k); return v + k; };
}
var a1 = makeAsync(1);
var a2 = makeAsync(2);
var p1 = a1(10);
var p2 = a2(10);
check(a1 !== a2, 'async closures are distinct objects');

// A class evaluated twice yields distinct constructors and methods that
// behave the same.
function makeClass(k) {
  return class {
    constructor(v) { this.v = v + k; }
    get g() { return this.v; }
    static s(x) { return x + k; }
    m(x) { return x + this.v; }
  };
}
var K1 = makeClass(1);
var K2 = makeClass(2);
check(K1 !== K2 && K1.prototype !== K2.prototype && K1.prototype.m !== K2.prototype.m,
  'class evaluation yields fresh constructors and methods');
check(new K1(10).g === 11 && new K2(10).g === 12, 'constructor and getter see own binding');
check(K1.s(5) === 6 && K2.s(5) === 7, 'static method sees own binding');
check(new K1(0).m(1) === 2 && new K2(0).m(1) === 3, 'method sees own binding');

Promise.all([p1, p2]).then(function (vals) {
  check(vals[0] === 11 && vals[1] === 12, 'async results are per closure');
  check(log.join() === '1,2', 'async bodies ran in call order');
}).then($DONE, $DONE);
