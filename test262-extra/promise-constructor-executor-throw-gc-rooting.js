// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-promise-executor
description: >
  The promise and its resolving functions stay alive while the executor runs
  and collects, so a throwing executor still rejects the promise
info: |
  Promise ( executor )

  10. Let completion be Completion(Call(executor, undefined, « resolvingFunctions.[[Resolve]], resolvingFunctions.[[Reject]] »)).
  11. If completion is an abrupt completion, then
    a. Perform ? Call(resolvingFunctions.[[Reject]], undefined, « completion.[[Value]] »).
  12. Return promise.
flags: [async]
includes: [asyncHelpers.js]
features: [Promise]
---*/

asyncTest(async function () {
  var promise = new Promise(() => {
    $262.gc();
    throw new Error("rejected after a collection");
  });
  var caught;
  try {
    await promise;
  } catch (error) {
    caught = error;
  }
  assert.sameValue(caught instanceof Error, true, "the executor's error rejects the promise");
  assert.sameValue(caught.message, "rejected after a collection");
});
