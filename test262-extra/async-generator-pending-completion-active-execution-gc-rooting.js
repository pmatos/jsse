/*---
description: >
  A throw or return value parked on an async generator's try context while
  the generator is still actively executing (not yet suspended) stays
  reachable across a forced garbage collection triggered from directly
  inside the finally that owns it, before any yield or await serializes the
  value into rooted state.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  While a Finally clause runs to restore an earlier throw or return, that
  Completion Record's value must remain a GC root for as long as the async
  generator that owns it is reachable, including for the entire duration of
  the finally's own (possibly non-suspending) execution.
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration, host-gc-required]
---*/

async function* throwCase() {
  try {
    yield 0;
    throw { marker: 'OUTER-THROW' };
  } finally {
    $262.gc();
    yield 1;
  }
}

async function* returnCase() {
  try {
    yield 0;
  } finally {
    $262.gc();
    yield 1;
  }
}

async function main() {
  var it = throwCase();
  assert.sameValue((await it.next()).value, 0, 'throw case: suspended inside the try block');
  assert.sameValue(
    (await it.next()).value,
    1,
    'throw case: the finally ran $262.gc() while the throw was parked, then suspended'
  );

  var thrown;
  try {
    await it.next();
  } catch (e) {
    thrown = e;
  }
  assert.notSameValue(thrown, undefined, 'throw case: the parked object survived garbage collection');
  assert.sameValue(thrown.marker, 'OUTER-THROW', 'throw case: the parked object kept its identity');

  var it2 = returnCase();
  assert.sameValue((await it2.next()).value, 0, 'return case: suspended inside the try block');
  var pending = { marker: 'OUTER-RETURN' };
  var result = await it2.return(pending);
  assert.sameValue(
    result.value,
    1,
    'return case: the finally ran $262.gc() while the return was parked, then suspended'
  );
  pending = null;

  result = await it2.next();
  assert.sameValue(result.done, true, 'return case: the generator completes');
  assert.notSameValue(result.value, undefined, 'return case: the parked object survived garbage collection');
  assert.sameValue(result.value.marker, 'OUTER-RETURN', 'return case: the parked object kept its identity');
}

asyncTest(main);
