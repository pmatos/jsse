// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-createasyncfromsynciterator
description: >
  The Async-from-Sync Iterator wrapper's [[SyncIteratorRecord]] (the sync
  iterator and its cached next method), and the reaction closures built by
  AsyncFromSyncIteratorContinuation, stay reachable across a garbage
  collection even when nothing but the wrapper's native next/return/throw
  methods reference them.
info: |
  CreateAsyncFromSyncIterator ( syncIteratorRecord )

  1. Let asyncIterator be OrdinaryObjectCreate(%AsyncFromSyncIteratorPrototype%,
     « [[SyncIteratorRecord]] »).
  2. Set asyncIterator.[[SyncIteratorRecord]] to syncIteratorRecord.
  ...

  %AsyncFromSyncIteratorPrototype%.next, .return and .throw each read
  [[SyncIteratorRecord]] to drive IteratorNext / GetMethod(syncIterator,
  "return"/"throw"). The engine encodes this internal slot as native-closure
  captures, which the tracer cannot walk unless explicitly pinned.

  AsyncFromSyncIteratorContinuation ( result, promiseCapability,
  syncIteratorRecord, closeOnRejection )

  ...
  9. Let onFulfilled be CreateBuiltinFunction(unwrapClosure, 1, "", « »).
  ...
  11. If closeOnRejection is true, then
      a. Let onRejected be a new Abstract Closure ... that captures
         syncIteratorRecord and performs ... IteratorClose ...
  ...
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, generators, host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

var log = [];

asyncTest(async function () {
  // next(): wrapper's cached sync_iter/cached_next survive a collection
  // between every next() call, with nothing else holding the array iterator.
  var seen = [];
  for await (const v of [1, 2, 3]) {
    collect();
    seen.push(v);
  }
  assert.compareArray(seen, [1, 2, 3], "for-await-of over an array literal");

  // return(): breaking out of the loop drives the wrapper's return() method,
  // which reads the same [[SyncIteratorRecord]].
  log = [];
  function returningIterable() {
    var state = 0;
    return {
      [Symbol.iterator]() {
        return {
          next() {
            state++;
            return { done: false, value: state };
          },
          return(v) {
            log.push("returned:" + state);
            return { done: true, value: v };
          },
        };
      },
    };
  }
  for await (const v of returningIterable()) {
    collect();
    if (v === 2) break;
  }
  assert.compareArray(log, ["returned:2"], "break drives the wrapper's return()");

  // throw(): an async generator delegating (yield*) to a sync generator
  // wraps it in the same AsyncFromSyncIterator; .throw() reaches the
  // wrapper's throw() method, which reads [[SyncIteratorRecord]] too.
  log = [];
  function* g() {
    try {
      yield 1;
      yield 2;
    } catch (e) {
      log.push("caught:" + e);
    }
  }
  async function* asyncg() {
    yield* g();
  }
  var it = asyncg();
  await it.next();
  collect();
  await it.throw("boom");
  assert.compareArray(log, ["caught:boom"], "throw() drives the wrapper's throw()");

  // AsyncFromSyncIteratorContinuation's onFulfilled/onRejected reaction
  // closures: a sync iterator whose next() result value is a thenable that
  // settles asynchronously. A collection between the next() call returning
  // and the promise chain settling must not lose the outer promise capability
  // (onFulfilled) or the sync iterator (onRejected's IteratorClose capture).
  log = [];
  function thenableIterable(rejects) {
    var state = 0;
    return {
      [Symbol.iterator]() {
        return {
          next() {
            state++;
            var n = state;
            return {
              done: false,
              value: {
                then(resolve, reject) {
                  Promise.resolve().then(function () {
                    if (rejects && n === 2) {
                      reject("rejected:" + n);
                    } else {
                      resolve("resolved:" + n);
                    }
                  });
                },
              },
            };
          },
          return(v) {
            log.push("closed:" + state);
            return { done: true, value: v };
          },
        };
      },
    };
  }

  log = [];
  seen = [];
  for await (const v of thenableIterable(false)) {
    collect();
    await null;
    collect();
    seen.push(v);
    if (seen.length === 2) break;
  }
  assert.compareArray(seen, ["resolved:1", "resolved:2"], "onFulfilled survives a collection");
  assert.compareArray(log, ["closed:2"], "break after thenable resolution still closes the iterator");

  log = [];
  try {
    for await (const v of thenableIterable(true)) {
      collect();
      await null;
      collect();
    }
    throw new Test262Error("loop should have thrown");
  } catch (e) {
    assert.sameValue(e, "rejected:2", "onRejected still rejects with the thenable's reason");
  }
  assert.compareArray(log, ["closed:2"], "onRejected still closes the sync iterator (IteratorClose)");
});
