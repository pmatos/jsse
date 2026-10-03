// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  The iterator of a for-of loop inside a generator, and the destructuring
  iterator of an array pattern, stay reachable across a garbage collection at
  every suspension point, including the step right after the iterator is
  created, even though nothing but the suspended generator references them.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind,
  lhsKind, labelSet [ , iteratorKind ] )

  The iteratorRecord created by GetIterator is used by every iteration and by
  IteratorClose until the loop exits, and a generator may be suspended at any
  yield inside the body in between.
features: [generators, destructuring-assignment, host-gc-required]
includes: [compareArray.js]
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

function* loop() {
  for (const v of makeIterable("a", 3)) {
    collect();
    yield v;
    collect();
  }
}

var seen = [];
var it = loop();
for (var r = it.next(); !r.done; r = it.next()) {
  collect();
  seen.push(r.value);
}
assert.compareArray(seen, ["a1", "a2", "a3"], "for-of in generator");
assert.compareArray(log, [], "exhausted iterator is not closed");

function* nested() {
  for (const outer of makeIterable("o", 2)) {
    for (const inner of makeIterable("i", 2)) {
      collect();
      yield outer + inner;
    }
  }
}
seen = [];
for (var value of nested()) {
  seen.push(value);
}
assert.compareArray(seen, ["o1i1", "o1i2", "o2i1", "o2i2"], "nested for-of in generator");

function* early() {
  for (const v of makeIterable("e", 5)) {
    yield v;
    break;
  }
  yield "after";
}
log = [];
it = early();
assert.sameValue(it.next().value, "e1", "early: first");
collect();
assert.sameValue(it.next().value, "after", "early: after break");
assert.compareArray(log, ["return:e:ee"], "break closes the iterator");

function* returned() {
  for (const v of makeIterable("r", 5)) {
    yield v;
  }
}
log = [];
it = returned();
it.next();
collect();
it.return("done");
assert.compareArray(log, ["return:r:rr"], "generator.return() closes the iterator");

function* thrown() {
  try {
    for (const v of makeIterable("t", 5)) {
      yield v;
    }
  } catch (e) {
    yield "caught:" + e;
  }
}
log = [];
it = thrown();
it.next();
collect();
assert.sameValue(it.throw("boom").value, "caught:boom", "throw routes to catch");
assert.compareArray(log, ["return:t:tt"], "throw closes the iterator");

function* pattern() {
  var a, b, rest;
  [a, b, ...rest] = makeIterable("p", 4);
  yield a;
  collect();
  yield b;
  collect();
  yield rest.join();
}
seen = [];
for (var value of pattern()) {
  seen.push(value);
}
assert.compareArray(seen, ["p1", "p2", "p3,p4"], "array assignment pattern in generator");

function* declPattern() {
  var [first, second] = makeIterable("d", 4);
  yield first;
  collect();
  yield second;
}
seen = [];
log = [];
for (var value of declPattern()) {
  seen.push(value);
}
assert.compareArray(seen, ["d1", "d2"], "array binding pattern in generator");
assert.compareArray(log, ["return:d:dd"], "unfinished pattern closes the iterator");

function* delegating() {
  yield* makeIterable("y", 2);
  for (const v of makeIterable("z", 2)) {
    yield v;
  }
}
seen = [];
for (var value of delegating()) {
  seen.push(value);
}
assert.compareArray(seen, ["y1", "y2", "z1", "z2"], "yield* followed by for-of");
