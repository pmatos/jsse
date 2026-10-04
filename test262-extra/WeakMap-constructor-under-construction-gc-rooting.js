// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-weakmap-iterable
description: >
  The WeakMap object under construction, and the iterator record obtained
  from it, stay reachable across the garbage collections that can happen
  while the constructor's Get/GetIterator/Call steps run arbitrary user
  code, even though nothing but the constructor's own in-flight locals
  reference them yet.
info: |
  WeakMap ( [ iterable ] )

  2. Let map be ? OrdinaryCreateFromConstructor(NewTarget, "%WeakMap.prototype%", ...).
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
function makeEntryIterableGcOnIteratorCall(key, value) {
  return {
    [Symbol.iterator]() {
      collect();
      var i = 0;
      return {
        next() {
          if (i === 0) {
            i++;
            return { done: false, value: [key, value] };
          }
          return { done: true, value: undefined };
        },
      };
    },
  };
}

var key1 = {};
var wm1 = new WeakMap(makeEntryIterableGcOnIteratorCall(key1, "a"));
assert.sameValue(
  wm1.get(key1),
  "a",
  "weakmap under construction survives GC inside [Symbol.iterator]()"
);

// (b) GC inside a later entry's key getter — exercises the window during
// which the iterator record obtained from GetIterator is reachable only
// from the constructor's own Rust local while a getter triggered by
// Get(next, "0") runs arbitrary code.
function makeEntryIterableGcOnKeyGetter(key1, key2) {
  var i = 0;
  return {
    [Symbol.iterator]() {
      return {
        next() {
          i++;
          if (i === 1) {
            return { done: false, value: [key1, "a"] };
          }
          if (i === 2) {
            return {
              done: false,
              value: {
                get 0() {
                  collect();
                  return key2;
                },
                get 1() {
                  return "b";
                },
              },
            };
          }
          return { done: true, value: undefined };
        },
      };
    },
  };
}

var mkey1 = {};
var mkey2 = {};
var wm2 = new WeakMap(makeEntryIterableGcOnKeyGetter(mkey1, mkey2));
assert.sameValue(wm2.get(mkey1), "a", "first entry survives the GC inside the second key getter");
assert.sameValue(wm2.get(mkey2), "b", "second entry, fetched after the GC, also committed");
