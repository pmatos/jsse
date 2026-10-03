// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-map-iterable
description: >
  The Map object under construction stays reachable across the garbage
  collections that can happen while the constructor's Get/GetIterator/Call
  steps run arbitrary user code, even though nothing but the constructor's
  own in-flight locals reference it yet.
info: |
  Map ( [ iterable ] )

  2. Let map be ? OrdinaryCreateFromConstructor(NewTarget, "%Map.prototype%", ...).
  ...
  5. Let adder be ? Get(map, "set").
  ...
  7. Return ? AddEntriesFromIterable(map, iterable, adder).

  AddEntriesFromIterable ( target, iterable, adder )

  3. Let iteratorRecord be ? GetIterator(iterable, sync).
  4. Repeat,
    a. Let next be ? IteratorStepValue(iteratorRecord).
    ...
    e. Let k be Completion(Get(next, "0")).
    f. IfAbruptCloseIterator(k, iteratorRecord).
    g. Let v be Completion(Get(next, "1")).
    h. IfAbruptCloseIterator(v, iteratorRecord).
    i. Let status be Completion(Call(adder, target, « k, v »)).
    j. IfAbruptCloseIterator(status, iteratorRecord).

  `map` and `iteratorRecord` must stay the same object across every step
  above, each of which can run arbitrary code (a getter, [Symbol.iterator],
  or the adder itself) and therefore trigger a collection.
features: [host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

// (a) GC inside [Symbol.iterator]() — exercises the window during which the
// under-construction `map` object (`this_val`) is reachable only from the
// constructor's own Rust local, between Get(map, "set") and the first
// Call(adder, map, ...).
function makeEntryIterableGcOnIteratorCall() {
  return {
    [Symbol.iterator]() {
      collect();
      var i = 0;
      return {
        next() {
          if (i === 0) {
            i++;
            return { done: false, value: [1, "a"] };
          }
          return { done: true, value: undefined };
        },
      };
    },
  };
}

var m1 = new Map(makeEntryIterableGcOnIteratorCall());
assert.sameValue(m1.size, 1, "map under construction survives GC inside [Symbol.iterator]()");
assert.sameValue(m1.get(1), "a", "entry added via adder after the GC is the expected one");

// (b) GC inside a later entry's key getter — exercises the window during
// which the iterator record obtained from GetIterator is reachable only
// from the constructor's own Rust local while a getter triggered by
// Get(next, "0") runs arbitrary code.
function makeEntryIterableGcOnKeyGetter() {
  var i = 0;
  return {
    [Symbol.iterator]() {
      return {
        next() {
          i++;
          if (i === 1) {
            return { done: false, value: [1, "a"] };
          }
          return {
            done: false,
            value: {
              get 0() {
                collect();
                throw new Error("boom");
              },
            },
          };
        },
      };
    },
  };
}

var thrown;
try {
  new Map(makeEntryIterableGcOnKeyGetter());
} catch (e) {
  thrown = e;
}
assert.notSameValue(thrown, undefined, "the key getter's throw propagates");
assert.sameValue(
  thrown.message,
  "boom",
  "the thrown error survives the GC intact, not replaced by a corrupted-object error"
);

