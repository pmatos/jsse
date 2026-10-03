// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-weakset-iterable
description: >
  The WeakSet object under construction, and the iterator record obtained
  from it, stay reachable across the garbage collections that can happen
  while the constructor's Get/GetIterator/Call steps run arbitrary user
  code, even though nothing but the constructor's own in-flight locals
  reference them yet.
info: |
  WeakSet ( [ iterable ] )

  2. Let set be ? OrdinaryCreateFromConstructor(NewTarget, "%WeakSet.prototype%", ...).
  ...
  5. Let adder be ? Get(set, "add").
  ...
  7. Let iteratorRecord be ? GetIterator(iterable, sync).
  8. Repeat,
    a. Let next be ? IteratorStepValue(iteratorRecord).
    ...
    c. Let status be Completion(Call(adder, set, « next »)).
    d. IfAbruptCloseIterator(status, iteratorRecord).

  `set` and `iteratorRecord` must stay the same object across every step
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
// under-construction `set` object (`this_val`) is reachable only from the
// constructor's own Rust local, between Get(set, "add") and the first
// Call(adder, set, ...).
function makeValueIterableGcOnIteratorCall(value) {
  return {
    [Symbol.iterator]() {
      collect();
      var i = 0;
      return {
        next() {
          if (i === 0) {
            i++;
            return { done: false, value: value };
          }
          return { done: true, value: undefined };
        },
      };
    },
  };
}

var obj1 = {};
var ws1 = new WeakSet(makeValueIterableGcOnIteratorCall(obj1));
assert.sameValue(
  ws1.has(obj1),
  true,
  "weakset under construction survives GC inside [Symbol.iterator]()"
);

// (b) GC inside a later entry's value getter — exercises the window during
// which the iterator record obtained from GetIterator is reachable only
// from the constructor's own Rust local while a getter triggered by
// IteratorStepValue's Get(next, "value") runs arbitrary code.
function makeValueIterableGcOnValueGetter(entries) {
  var i = 0;
  return {
    [Symbol.iterator]() {
      return {
        next() {
          if (i >= entries.length) {
            return { done: true, value: undefined };
          }
          var idx = i;
          i++;
          return {
            done: false,
            get value() {
              if (idx === 0) {
                collect();
              }
              return entries[idx];
            },
          };
        },
      };
    },
  };
}

var wobj1 = {};
var wobj2 = {};
var ws2 = new WeakSet(makeValueIterableGcOnValueGetter([wobj1, wobj2]));
assert.sameValue(ws2.has(wobj1), true, "first entry survives the GC inside the second getter");
assert.sameValue(ws2.has(wobj2), true, "second entry, fetched after the GC, also committed");
