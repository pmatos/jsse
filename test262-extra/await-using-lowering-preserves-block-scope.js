// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposeresources
description: >
  An `await using` block nested in a loop or `try` does not change the lexical
  scoping of the enclosing statement: per-iteration `let` bindings stay
  distinct, block-scoped shadowing is respected, and `for-in` visits every key.
info: |
  Block : { StatementList }

  [...]
  5. Let blockValue be Completion(Evaluation of StatementList).
  6. Set blockValue to Completion(DisposeResources(blockEnv.[[DisposeCapability]], blockValue)).
  [...]

  CreatePerIterationEnvironment ( perIterationBindings )

  Each iteration of a `for (let ...)` loop gets a fresh environment, so a
  closure created in one iteration keeps that iteration's binding.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management]
---*/

asyncTest(async function () {
  var fs = [];
  for (let i = 0; i < 3; i++) {
    fs.push(() => i);
    {
      await using a = null;
    }
  }
  assert.compareArray(fs.map(f => f()), [0, 1, 2], 'for (let) closures capture per-iteration bindings');

  fs = [];
  var n = 0;
  while (n < 3) {
    let j = n;
    fs.push(() => j);
    {
      await using a = null;
    }
    n++;
  }
  assert.compareArray(fs.map(f => f()), [0, 1, 2], 'while body let bindings are per-iteration');

  var keys = [];
  for (var k in { a: 1, b: 2 }) {
    keys.push(k);
    {
      await using a = null;
    }
  }
  assert.compareArray(keys, ['a', 'b'], 'for-in visits every key');

  var x = 1;
  var seen = [];
  try {
    let x = 2;
    {
      await using a = null;
    }
    seen.push(x);
  } finally {
  }
  seen.push(x);
  assert.compareArray(seen, [2, 1], 'try block let shadows the outer binding only inside the block');

  var i = 'outer';
  for (let i = 0; i < 2; i++) {
    {
      await using a = null;
    }
  }
  assert.sameValue(i, 'outer', 'for (let i) does not clobber an outer i');
});
