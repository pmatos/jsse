/*---
description: >
  A throw reached while lowering an object-destructuring pattern whose
  trailing `...rest` sits beside a suspending (`yield`) sibling -- either
  from a computed key's `ToPropertyKey` conversion, or from `CopyDataProperties`
  itself (e.g. a Proxy `ownKeys` trap) -- still runs `DisposeResources` for an
  active function-level `using` resource before propagating, exactly like any
  other throw inside the generator body. Regression test for the
  `ObjectRestCopy`/`ToPropertyKey` state-machine terminators added for issue
  #771: both terminators used to retire the generator immediately on throw,
  skipping disposal.
esid: sec-generatorstart
info: |
  GeneratorStart ( generator, generatorBody )

  4. Let result be Completion(Evaluation of generatorBody).
  [...]
  Evaluating a Block containing a `using` declaration disposes its resources
  (DisposeResources) when control leaves the block abruptly, including via a
  throw raised while evaluating a later statement in the same block.
features: [generators, destructuring-binding, object-rest, explicit-resource-management]
---*/

var log = [];

function* gComputedKey() {
  using r = { [Symbol.dispose]() { log.push('disposed'); } };
  var badKey = { toString() { throw new Error('bad key'); } };
  var { b = yield 1, [badKey]: a, ...rest } = { b: undefined };
}

var it1 = gComputedKey();
it1.next();
log = [];
var caught1;
try {
  it1.next(99);
} catch (e) {
  caught1 = e;
}
assert.sameValue(caught1.message, 'bad key', 'the computed key conversion throws');
assert.compareArray(
  log,
  ['disposed'],
  'the using resource is disposed before the ToPropertyKey throw propagates'
);

function* gRestCopy() {
  using r = { [Symbol.dispose]() { log.push('disposed'); } };
  var p = new Proxy({}, { ownKeys() { throw new Error('ownKeys boom'); } });
  var { a = yield 1, ...rest } = p;
}

var it2 = gRestCopy();
it2.next();
log = [];
var caught2;
try {
  it2.next(99);
} catch (e) {
  caught2 = e;
}
assert.sameValue(caught2.message, 'ownKeys boom', 'CopyDataProperties throws via the proxy trap');
assert.compareArray(
  log,
  ['disposed'],
  'the using resource is disposed before the ObjectRestCopy throw propagates'
);
