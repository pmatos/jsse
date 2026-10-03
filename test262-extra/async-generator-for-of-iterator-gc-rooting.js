// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  The iterator of a for-of or for-await-of loop inside an async generator, the
  destructuring iterator of an array pattern and the iterator of a yield*
  delegation stay reachable across a garbage collection at every yield and
  await, including the step right after the iterator is created, even though
  nothing but the suspended generator references them.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iteratorRecord, iterationKind,
  lhsKind, labelSet [ , iteratorKind ] )

  The iteratorRecord is consulted again after every Await or yield in the loop
  body and by IteratorClose when the loop exits.
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

async function drain(iterator) {
  var out = [];
  for (var r = await iterator.next(); !r.done; r = await iterator.next()) {
    collect();
    out.push(r.value);
  }
  return out;
}

async function* forOf() {
  for (const v of makeIterable("a", 3)) {
    collect();
    await null;
    yield v;
    collect();
  }
}

async function* forAwaitOf() {
  for await (const v of makeAsyncIterable("b", 3)) {
    collect();
    yield v;
    collect();
  }
}

async function* forAwaitOverSync() {
  for await (const v of makeIterable("s", 2)) {
    yield v;
  }
}

async function* nested() {
  for (const outer of makeIterable("o", 2)) {
    for await (const inner of makeAsyncIterable("i", 2)) {
      collect();
      yield outer + inner;
    }
  }
}

async function* delegating() {
  yield* makeAsyncIterable("y", 2);
  for (const v of makeIterable("z", 2)) {
    yield v;
  }
}

async function* pattern() {
  var a, b, rest;
  [a, b, ...rest] = makeIterable("p", 4);
  yield a;
  collect();
  yield b;
  collect();
  yield rest.join();
}

async function* declPattern() {
  var [first, second] = makeIterable("d", 4);
  yield first;
  collect();
  await null;
  yield second;
}

async function* early() {
  for (const v of makeIterable("e", 5)) {
    yield v;
    break;
  }
  yield "after";
}

async function* closedByReturn() {
  for await (const v of makeAsyncIterable("r", 5)) {
    yield v;
  }
}

asyncTest(async function () {
  assert.compareArray(await drain(forOf()), ["a1", "a2", "a3"], "for-of");
  assert.compareArray(await drain(forAwaitOf()), ["b1", "b2", "b3"], "for-await-of");
  assert.compareArray(await drain(forAwaitOverSync()), ["s1", "s2"], "for-await-of over sync");
  assert.compareArray(
    await drain(nested()),
    ["o1i1", "o1i2", "o2i1", "o2i2"],
    "nested for-of and for-await-of"
  );
  assert.compareArray(await drain(delegating()), ["y1", "y2", "z1", "z2"], "yield* then for-of");
  assert.compareArray(await drain(pattern()), ["p1", "p2", "p3,p4"], "assignment pattern");

  log = [];
  assert.compareArray(await drain(declPattern()), ["d1", "d2"], "binding pattern");
  assert.compareArray(log, ["return:d:dd"], "unfinished pattern closes the iterator");

  log = [];
  assert.compareArray(await drain(early()), ["e1", "after"], "break");
  assert.compareArray(log, ["return:e:ee"], "break closes the iterator");

  log = [];
  var it = closedByReturn();
  await it.next();
  collect();
  await it.return("done");
  assert.compareArray(log, ["return:r:rr"], "return() closes the async iterator");
});
