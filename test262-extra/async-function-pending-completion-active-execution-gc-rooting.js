/*---
description: >
  A throw or return value parked on a plain async function's try context
  while the function is still actively executing (not yet suspended) stays
  reachable across a forced garbage collection triggered from directly
  inside the finally that owns it, before any await serializes the value
  into rooted state.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  While a Finally clause runs to restore an earlier throw or return, that
  Completion Record's value must remain a GC root for as long as the async
  function that owns it is reachable, including for the entire duration of
  the finally's own (possibly non-suspending) execution.
flags: [async]
includes: [asyncHelpers.js]
features: [async-functions, host-gc-required]
---*/

async function throwCase() {
  try {
    await 0;
    throw { marker: 'OUTER-THROW' };
  } finally {
    $262.gc();
    await 0;
  }
}

async function returnCase() {
  try {
    await 0;
    return { marker: 'OUTER-RETURN' };
  } finally {
    $262.gc();
    await 0;
  }
}

async function main() {
  var thrown;
  try {
    await throwCase();
  } catch (e) {
    thrown = e;
  }
  assert.notSameValue(thrown, undefined, 'throw case: the parked object survived garbage collection');
  assert.sameValue(thrown.marker, 'OUTER-THROW', 'throw case: the parked object kept its identity');

  var value = await returnCase();
  assert.notSameValue(value, undefined, 'return case: the parked object survived garbage collection');
  assert.sameValue(value.marker, 'OUTER-RETURN', 'return case: the parked object kept its identity');
}

asyncTest(main);
