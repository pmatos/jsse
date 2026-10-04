// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  The iterator of a for-of or for-await-of loop inside an async function, and
  the destructuring iterator of an array pattern, stay reachable across a
  garbage collection at every await, including the step right after the
  iterator is created, even though nothing but the suspended function
  references them.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind,
  lhsKind, labelSet [ , iteratorKind ] )

  The iteratorRecord is consulted again after every Await in the loop body and
  by IteratorClose when the loop exits.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, destructuring-assignment, host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

var log = [];

function makeIterable(tag, count) {
  return {
    [Symbol.iterator]() {
      var state = { index: 0, payload: [tag, tag + tag] };
      return {
        next() {
          collect();
          if (state.index >= count) {
            return { done: true, value: undefined };
          }
          state.index++;
          return { done: false, value: state.payload[0] + state.index };
        },
        return() {
          collect();
          log.push("return:" + tag + ":" + state.payload[1]);
          return {};
        },
      };
    },
  };
}

function makeAsyncIterable(tag, count) {
  return {
    [Symbol.asyncIterator]() {
      var state = { index: 0, payload: [tag, tag + tag] };
      return {
        async next() {
          collect();
          await null;
          collect();
          if (state.index >= count) {
            return { done: true, value: undefined };
          }
          state.index++;
          return { done: false, value: state.payload[0] + state.index };
        },
        async return() {
          collect();
          log.push("return:" + tag + ":" + state.payload[1]);
          return {};
        },
      };
    },
  };
}

asyncTest(async function () {
  var seen = [];
  for (const v of makeIterable("a", 3)) {
    collect();
    await null;
    collect();
    seen.push(v);
  }
  assert.compareArray(seen, ["a1", "a2", "a3"], "for-of in async function");

  seen = [];
  for await (const v of makeAsyncIterable("b", 3)) {
    collect();
    await null;
    seen.push(v);
  }
  assert.compareArray(seen, ["b1", "b2", "b3"], "for-await-of in async function");

  seen = [];
  for await (const v of makeIterable("s", 2)) {
    collect();
    seen.push(v);
  }
  assert.compareArray(seen, ["s1", "s2"], "for-await-of over a sync iterable");

  log = [];
  for (const v of makeIterable("e", 5)) {
    await null;
    collect();
    break;
  }
  assert.compareArray(log, ["return:e:ee"], "break closes the iterator");

  log = [];
  for await (const v of makeAsyncIterable("f", 5)) {
    await null;
    collect();
    break;
  }
  assert.compareArray(log, ["return:f:ff"], "break closes the async iterator");

  log = [];
  try {
    for (const v of makeIterable("t", 5)) {
      await null;
      collect();
      throw "boom";
    }
  } catch (e) {
    assert.sameValue(e, "boom");
  }
  assert.compareArray(log, ["return:t:tt"], "throw closes the iterator");

  var a, b, rest;
  [a, b, ...rest] = makeIterable("p", 4);
  await null;
  collect();
  assert.sameValue(a + b + rest.join(), "p1p2p3,p4", "array assignment pattern");

  var [first, second] = makeIterable("d", 4);
  await null;
  collect();
  assert.sameValue(first + second, "d1d2", "array binding pattern");
  assert.compareArray(log, ["return:t:tt", "return:d:dd"], "unfinished pattern closes the iterator");

  async function inner() {
    var out = [];
    for (const v of makeIterable("n", 2)) {
      for (const w of makeIterable("m", 2)) {
        await null;
        collect();
        out.push(v + w);
      }
    }
    return out;
  }
  assert.compareArray(await inner(), ["n1m1", "n1m2", "n2m1", "n2m2"], "nested loops");
});
