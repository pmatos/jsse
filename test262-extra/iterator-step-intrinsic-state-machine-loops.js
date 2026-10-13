// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  for-of loops and array destructuring that run inside generators, async
  functions and async generators (which the engine drives through state
  machines) consume Array, Map, Set, String and generator iterators with the
  same observable behaviour as in plain functions, including a `yield` or
  `await` in the middle of the loop and replacement of a captured next.
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [Symbol.iterator, generators, async-iteration]
---*/

var ArrayIteratorPrototype = Object.getPrototypeOf([][Symbol.iterator]());
var originalNext = ArrayIteratorPrototype.next;

function* inner() {
  yield "i1";
  yield "i2";
}

function* syncGenerator() {
  var out = [];
  for (var v of [1, 2, 3]) {
    out.push(yield v);
  }
  for (var [k, val] of new Map([["a", 1]])) out.push(k + val);
  for (var s of new Set([7])) out.push(s);
  for (var c of "xy") out.push(c);
  for (var g of inner()) out.push(g);
  var [a, , b, ...rest] = [10, 20, 30, 40, 50];
  out.push(a, b, rest.length);
  var [p, q = yield "mid"] = [1];
  out.push(p, q);
  return out;
}

async function asyncFunction() {
  var out = [];
  for (var v of [1, 2, 3]) {
    out.push(await v);
    if (v === 1) {
      ArrayIteratorPrototype.next = function () {
        throw new Test262Error("replacement must not affect the running loop");
      };
    }
  }
  ArrayIteratorPrototype.next = originalNext;
  for (var [k, val] of new Map([["a", 1]])) out.push(k + val);
  for (var g of inner()) out.push(await g);
  var [a, , b, ...rest] = [10, 20, 30, 40, 50];
  out.push(a, b, rest.length);
  var [x, y] = inner();
  out.push(x, y);
  return out;
}

async function* asyncGenerator() {
  for (var v of [1, 2]) yield v;
  for (var g of inner()) yield await g;
  var [a, b] = new Set([5, 6]);
  yield a + b;
  for await (var w of [Promise.resolve("w1"), "w2"]) yield w;
}

var sg = syncGenerator();
var sent = [];
var step = sg.next();
while (!step.done) {
  sent.push(step.value);
  step = sg.next("sent-" + sent.length);
}
assert.compareArray(sent, [1, 2, 3, "mid"], "yields of the sync generator");
assert.compareArray(
  step.value,
  ["sent-1", "sent-2", "sent-3", "a1", 7, "x", "y", "i1", "i2", 10, 30, 2, 1, "sent-4"],
  "values collected by the sync generator"
);

asyncFunction()
  .then(function (out) {
    assert.compareArray(out, [1, 2, 3, "a1", "i1", "i2", 10, 30, 2, "i1", "i2"]);
    return Array.fromAsync(asyncGenerator());
  })
  .then(function (out) {
    assert.compareArray(out, [1, 2, "i1", "i2", 11, "w1", "w2"]);
  })
  .then($DONE, $DONE);
