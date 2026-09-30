/*---
description: >
  A throw or return value parked on a suspended generator's try context,
  reachable only through that try context, stays reachable across a forced
  garbage collection while the generator sits suspended inside the finally
  that owns it.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  While a Finally clause runs to restore an earlier throw or return, that
  Completion Record's value must remain a GC root for as long as the
  generator that owns it is reachable, including while suspended at a yield
  inside the finally.
features: [generators, host-gc-required]
---*/

function* throwCase() {
  try {
    yield 0;
    throw { marker: 'OUTER-THROW' };
  } finally {
    yield 1;
  }
}

var it = throwCase();
assert.sameValue(it.next().value, 0, 'throw case: suspended inside the try block');
assert.sameValue(it.next().value, 1, 'throw case: suspended inside the finally, throw already parked');

$262.gc();

var thrown;
try {
  it.next();
} catch (e) {
  thrown = e;
}
assert.notSameValue(thrown, undefined, 'throw case: the parked object survived garbage collection');
assert.sameValue(thrown.marker, 'OUTER-THROW', 'throw case: the parked object kept its identity');

function* returnCase() {
  try {
    yield 0;
  } finally {
    yield 1;
  }
}

var it2 = returnCase();
assert.sameValue(it2.next().value, 0, 'return case: suspended inside the try block');
var pending = { marker: 'OUTER-RETURN' };
var result = it2.return(pending);
assert.sameValue(result.value, 1, 'return case: suspended inside the finally, return already parked');
pending = null;

$262.gc();

result = it2.next();
assert.sameValue(result.done, true, 'return case: the generator completes');
assert.notSameValue(result.value, undefined, 'return case: the parked object survived garbage collection');
assert.sameValue(result.value.marker, 'OUTER-RETURN', 'return case: the parked object kept its identity');
