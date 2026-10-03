// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-getdisposemethod
description: >
  When an `await using` resource falls back to a synchronous
  [Symbol.dispose], the disposer-wrapping function GetDisposeMethod builds
  around it stays reachable across a garbage collection that happens between
  the `using` declaration (where the wrapper is built) and the block's exit
  (where the wrapper is actually called), even though nothing but the
  resource stack's native closure references the original sync method.
info: |
  GetDisposeMethod ( V, hint )

  1. If hint is async-dispose, then
     a. Let method be ? GetMethod(V, %Symbol.asyncDispose%).
     b. If method is undefined, then
        i. Set method to ? GetMethod(V, %Symbol.dispose%).
        ii. If method is not undefined, then
            1. Let closure be a new Abstract Closure with no parameters that
               captures method and performs the following steps when called:
               a. Let O be the this value.
               b. Let promiseCapability be ! NewPromiseCapability(%Promise%).
               c. Let result be Completion(Call(method, O)).
               d. IfAbruptRejectPromise(result, promiseCapability).
               e. Perform ! Call(promiseCapability.[[Resolve]], undefined,
                  « undefined »).
               f. Return promiseCapability.[[Promise]].
            2. NOTE: This function is not observable to user code. It is
               used to ensure that a Promise returned from a synchronous
               @@dispose method will not be awaited.

  (Explicit Resource Management proposal; not yet in the pinned spec/
  submodule, but already test262-covered under language/statements/using/
  and language/statements/await-using/, and already implemented end to end
  by this engine.)
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

function makeSyncResource(marker, log) {
  return {
    marker: marker,
    [Symbol.dispose]() {
      log.push("disposed:" + marker);
      // Returning a promise here must be ignored, not awaited (GetDisposeMethod's
      // own note) -- unrelated to this test's GC concern, but exercised anyway
      // since it is the same wrapper this test is rooting.
      return Promise.resolve("ignored");
    },
  };
}

asyncTest(async function () {
  // Single resource: a collection between declaration and the implicit
  // block-exit disposal must not lose the wrapped sync dispose method.
  var log = [];
  async function run() {
    await using a = makeSyncResource("a", log);
    log.push("body");
    await null;
    collect();
    await null;
  }
  await run();
  assert.compareArray(
    log,
    ["body", "disposed:a"],
    "the sync-dispose fallback wrapper survives a gc before block exit"
  );

  // Multiple resources: disposal runs in reverse declaration order, and every
  // wrapper (one per resource) must independently survive the collection.
  log = [];
  async function runMultiple() {
    await using a = makeSyncResource("x", log);
    await using b = makeSyncResource("y", log);
    log.push("body");
    await null;
    collect();
    await null;
  }
  await runMultiple();
  assert.compareArray(
    log,
    ["body", "disposed:y", "disposed:x"],
    "both wrappers survive the same collection, disposed in reverse order"
  );

  // A disposer that throws: the wrapper must still be callable post-gc so the
  // abrupt completion itself can observably occur.
  log = [];
  function makeThrowingSyncResource(marker) {
    return {
      marker: marker,
      [Symbol.dispose]() {
        log.push("disposed:" + marker);
        throw new RangeError("boom-" + marker);
      },
    };
  }
  async function runThrowing() {
    await using a = makeThrowingSyncResource("z");
    log.push("body");
    await null;
    collect();
    await null;
  }
  try {
    await runThrowing();
    throw new Test262Error("expected the disposal error to propagate");
  } catch (e) {
    assert.sameValue(e instanceof RangeError, true, "error identity survives the collection");
    assert.sameValue(e.message, "boom-z", "error message survives the collection");
  }
  assert.compareArray(log, ["body", "disposed:z"], "the throwing wrapper still ran post-gc");
});
