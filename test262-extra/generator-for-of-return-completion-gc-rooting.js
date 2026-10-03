// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-generator-function-definitions-runtime-semantics-evaluatebody
description: >
  A heap-allocated value returned from inside a generator's own open for-of
  loop stays reachable across the garbage collection that can happen while
  the loop is closed by IteratorClose's return() call, even though nothing
  but the in-flight return completion references the value.
info: |
  GeneratorBody : FunctionBody

  Evaluating `return expr;` inside a for-of loop body produces a Return
  completion; ForIn/OfBodyEvaluation's abrupt-exit handling for Return calls
  IteratorClose with the loop's iterator before the completion resumes
  outward through the generator machinery, and IteratorClose's `return()`
  call can run arbitrary user code.
features: [generators, host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

function makeIterable() {
  return {
    [Symbol.iterator]() {
      var i = 0;
      return {
        next() {
          i++;
          return { done: false, value: i };
        },
        return(v) {
          collect();
          return { done: true };
        },
      };
    },
  };
}

// The generator's own `return` statement closes the open for-of loop in the
// same `.next()` call that evaluates it — no injected `.return()` needed.
function* gen() {
  for (const x of makeIterable()) {
    return { tag: "inline-return-payload" };
  }
}

var result = gen().next();
assert.sameValue(result.done, true, "the for-of's return exits the generator");
assert.sameValue(
  result.value.tag,
  "inline-return-payload",
  "the generator's own for-of return value survives IteratorClose's return() call"
);

// A `.return(value)` injected into a generator suspended inside an open
// for-of loop must also survive closing that loop.
function* suspended() {
  for (const x of makeIterable()) {
    yield x;
  }
}

var it = suspended();
it.next();
var injected = it.return({ tag: "injected-return-payload" });
assert.sameValue(injected.done, true, "the injected return completes the generator");
assert.sameValue(
  injected.value.tag,
  "injected-return-payload",
  "generator.return()'s value survives closing the open for-of's IteratorClose return() call"
);
