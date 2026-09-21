// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
description: >
  `for (await using x of iterable)` iterates with the sync for-of protocol:
  each element is bound as produced, not unwrapped through AsyncFromSyncIterator.
  Only `for await (await using x of iterable)` awaits each element.
info: |
  ForIn/OfBodyEvaluation ( lhs, stmt, iterator, iteratorKind, lhsKind, labelSet [ , iteratorRecordLevel ] )

  1. If iteratorKind is not present, set iteratorKind to sync.
  ...
  8. Repeat,
     a. Let nextResult be ? Call(iteratorRecord.[[NextMethod]], iteratorRecord.[[Iterator]]).
     b. If iteratorKind is async, set nextResult to ? Await(nextResult).
     ...

  `for ( ForDeclaration of AssignmentExpression ) Statement` (the plain, non-`for await` production)
  always has iteratorKind sync, regardless of whether ForDeclaration is `await using`. Only the
  `for await ( ForDeclaration of AssignmentExpression ) Statement` production has iteratorKind async.
flags: [async]
includes: [asyncHelpers.js]
features: [explicit-resource-management, async-iteration]
---*/

asyncTest(async function () {
  var thenable = {
    then(resolve) {
      resolve(null);
    },
    [Symbol.asyncDispose]() {},
  };

  var plainSeen;
  await (async function () {
    for (await using a of [thenable]) {
      plainSeen = a === thenable;
    }
  })();
  assert.sameValue(
    plainSeen,
    true,
    'for (await using a of [thenable]) binds the element as-is (sync iteration)'
  );

  var forAwaitSeen;
  await (async function () {
    for await (await using a of [thenable]) {
      forAwaitSeen = a === thenable;
    }
  })();
  assert.sameValue(
    forAwaitSeen,
    false,
    'for await (await using a of [thenable]) awaits the element (AsyncFromSyncIterator unwraps the thenable)'
  );
});
