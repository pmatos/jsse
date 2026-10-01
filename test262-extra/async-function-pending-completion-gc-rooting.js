/*---
description: >
  A throw or return value parked on a suspended plain async function's try
  context, reachable only through that try context, stays reachable across a
  forced garbage collection while the function is suspended at an await
  inside the finally that owns it.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  While a Finally clause runs to restore an earlier throw or return, that
  Completion Record's value must remain a GC root for as long as the async
  function that owns it is reachable, including while suspended at an await
  inside the finally.
flags: [async]
includes: [asyncHelpers.js]
features: [async-functions, host-gc-required]
---*/

async function throwCase() {
  try {
    await 0;
    throw { marker: 'OUTER-THROW' };
  } finally {
    await 0;
  }
}

async function returnCase() {
  try {
    await 0;
    return { marker: 'OUTER-RETURN' };
  } finally {
    await 0;
  }
}

async function main() {
  // One tick resumes the function past its first `await` (inside the try),
  // which throws/returns, gets intercepted by the finally, and suspends
  // again at the finally's own `await` — with the parked completion still
  // unsettled. A second tick would let the finally finish and the function
  // settle before `$262.gc()` runs, testing nothing about this rooting.
  var throwPromise = throwCase();
  await 0;
  $262.gc();

  var thrown;
  try {
    await throwPromise;
  } catch (e) {
    thrown = e;
  }
  assert.notSameValue(thrown, undefined, 'throw case: the parked object survived garbage collection');
  assert.sameValue(thrown.marker, 'OUTER-THROW', 'throw case: the parked object kept its identity');

  var returnPromise = returnCase();
  await 0;
  $262.gc();

  var value = await returnPromise;
  assert.notSameValue(value, undefined, 'return case: the parked object survived garbage collection');
  assert.sameValue(value.marker, 'OUTER-RETURN', 'return case: the parked object kept its identity');
}

asyncTest(main);
