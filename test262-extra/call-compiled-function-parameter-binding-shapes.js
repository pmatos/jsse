// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-functiondeclarationinstantiation
description: >
  The engine binds the parameters of a repeatedly called function from a
  per-literal plan. Every parameter list shape must bind the same names and
  values on the tenth call as on the first: no parameters, one, sixteen and
  seventeen or more, duplicate names, a parameter named `arguments`, missing
  and extra arguments, and parameters captured by closures.
flags: [noStrict]
---*/

function times(n, f) {
  for (var i = 0; i < n; i++) f(i);
}

function none() {
  return arguments.length;
}
function one(a) {
  return a;
}
function two(a, b) {
  return [a, b];
}
function sixteen(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15) {
  return a0 + a15 + a7;
}
function seventeen(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16) {
  return a0 + a16 + a15 + a8;
}
function twenty(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17, a18, a19) {
  return a0 + a19 + a16;
}
function sixteenWithVar(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15) {
  var local = a15;
  return local + a0;
}
function sixteenWithClosure(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15) {
  return function () {
    return a15 + a0;
  };
}
function sixteenWithFunction(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15) {
  function inner() {
    return a15;
  }
  return inner() + a14;
}
function readsArguments(a, b) {
  return arguments.length + a + b;
}
function duplicate(a, a) {
  return a;
}
function namedArguments(arguments) {
  return arguments;
}
function captured(a, b) {
  return function () {
    return a + b;
  };
}

times(10, function (i) {
  assert.sameValue(none(i, i), 2, "arguments.length, call " + i);
  assert.sameValue(one(i), i, "one parameter, call " + i);
  assert.sameValue(one(), undefined, "one parameter missing, call " + i);
  assert.sameValue(one(i, "extra"), i, "one parameter, extra argument, call " + i);

  var pair = two(i);
  assert.sameValue(pair[0], i, "first of two, call " + i);
  assert.sameValue(pair[1], undefined, "second of two missing, call " + i);

  var args16 = [];
  var args17 = [];
  var args20 = [];
  for (var k = 0; k < 20; k++) {
    if (k < 16) args16.push(i + k);
    if (k < 17) args17.push(i + k);
    args20.push(i + k);
  }
  assert.sameValue(sixteen.apply(undefined, args16), 3 * i + 15 + 7, "sixteen parameters, call " + i);
  assert.sameValue(seventeen.apply(undefined, args17), 4 * i + 16 + 15 + 8, "seventeen parameters, call " + i);
  assert.sameValue(twenty.apply(undefined, args20), 3 * i + 19 + 16, "twenty parameters, call " + i);
  assert.sameValue(seventeen(1, 2, 3), NaN, "seventeen parameters, mostly missing, call " + i);

  assert.sameValue(sixteenWithVar.apply(undefined, args16), i + (i + 15), "var beside sixteen parameters, call " + i);
  assert.sameValue(sixteenWithClosure.apply(undefined, args16)(), i + 15 + i, "closure over sixteen parameters, call " + i);
  assert.sameValue(sixteenWithFunction.apply(undefined, args16), i + 15 + i + 14, "function beside sixteen parameters, call " + i);
  assert.sameValue(readsArguments(i, 1), 2 + i + 1, "parameters beside an arguments object, call " + i);
  assert.sameValue(duplicate(i, i + 1), i + 1, "the last duplicate name wins, call " + i);
  assert.sameValue(duplicate(i), undefined, "a missing duplicate shadows an earlier one, call " + i);
  assert.sameValue(namedArguments(i), i, "a parameter named arguments, call " + i);
  assert.sameValue(captured(i, 1)(), i + 1, "captured parameters, call " + i);
});

function reassign(a, b) {
  a = a + 1;
  b = b + 1;
  return a * 100 + b;
}
times(10, function (i) {
  assert.sameValue(reassign(i, i), (i + 1) * 100 + i + 1, "parameters are mutable bindings, call " + i);
});

function shadowsByVar(a) {
  var a;
  return a;
}
function shadowsByFunction(a) {
  function a() {}
  return typeof a;
}
times(10, function (i) {
  assert.sameValue(shadowsByVar(i), i, "a var redeclaring a parameter keeps its value, call " + i);
  assert.sameValue(shadowsByFunction(i), "function", "a function declaration replaces a parameter, call " + i);
});
