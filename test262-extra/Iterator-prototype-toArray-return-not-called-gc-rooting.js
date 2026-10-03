// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-iterator.prototype.toarray
description: >
  IteratorStepValue never performs IteratorClose, whether IteratorStep
  throws (next() or its done getter) or IteratorValue throws (the value
  getter) -- so Iterator.prototype.toArray must never invoke the
  underlying iterator's return() on any of these paths, and the
  propagated error must retain its original identity across whatever
  garbage collection the engine performs while completing that step.
info: |
  %Iterator.prototype%.toArray ( )
  5. Repeat,
    a. Let value be ? IteratorStepValue(iterated).

  IteratorStepValue ( iteratorRecord )
  1. Let result be ? IteratorStep(iteratorRecord).
  3. Let value be Completion(IteratorValue(result)).
  4. If value is a throw completion, then
    a. Set iteratorRecord.[[Done]] to true.
  5. Return ? value.
features: [host-gc-required]
---*/
class Marker extends Error {}

function makeIterator(nextImpl) {
  var state = { returnCalled: false };
  class T extends Iterator {
    next() {
      return nextImpl();
    }
    return() {
      state.returnCalled = true;
      // If this ever runs, arbitrary script (and therefore a GC) must not
      // be able to disturb the error already produced by IteratorStepValue.
      $262.gc();
      for (var i = 0; i < 64; i++) {
        [{}, {}, {}];
      }
      throw new Error("return() must not be called here");
    }
  }
  return { iterator: new T(), state: state };
}

function run(nextImpl) {
  var made = makeIterator(nextImpl);
  var thrown;
  try {
    made.iterator.toArray();
  } catch (e) {
    thrown = e;
  }
  return { returnCalled: made.state.returnCalled, thrown: thrown };
}

// Case 1: next() itself throws.
var r1 = run(function () {
  throw new Marker("next");
});
assert.sameValue(r1.returnCalled, false, "next()-throws: must not call return()");
assert(r1.thrown instanceof Marker, "next()-throws: must propagate the original error");

// Case 2: the done getter throws.
var r2 = run(function () {
  return {
    get done() {
      throw new Marker("done");
    },
    value: 1,
  };
});
assert.sameValue(r2.returnCalled, false, "done-getter-throws: must not call return()");
assert(r2.thrown instanceof Marker, "done-getter-throws: must propagate the original error");

// Case 3: the value getter throws (the issue's exact scenario).
var r3 = run(function () {
  return {
    done: false,
    get value() {
      throw new Marker("value");
    },
  };
});
assert.sameValue(r3.returnCalled, false, "value-getter-throws: must not call return()");
assert(r3.thrown instanceof Marker, "value-getter-throws: must propagate the original error");

// Case 4: next() returns a non-object (no user value to lose, but return()
// must still not be called).
var r4 = run(function () {
  return null;
});
assert.sameValue(r4.returnCalled, false, "non-object-result: must not call return()");
assert(r4.thrown instanceof TypeError, "non-object-result: must throw a TypeError");
