/*---
description: >
  An optional chain short-circuits only when its base is undefined or null, so
  an IsHTMLDDA base (which is loosely equal to null) is not short-circuited
  when the chain contains an `await`.
esid: sec-optional-chains
info: |
  OptionalExpression : MemberExpression OptionalChain

  1. Let baseReference be ? Evaluation of MemberExpression.
  2. Let baseValue be ? GetValue(baseReference).
  3. Let optionalChain be OptionalChain of OptionalExpression.
  4. Return ? ChainEvaluation of optionalChain with arguments baseValue and baseReference.

  OptionalChain : ?. [ Expression ]

  1. If baseValue is either undefined or null, then
    a. Return undefined.
flags: [async]
features: [async-functions, optional-chaining, IsHTMLDDA]
---*/

var dda = $262.IsHTMLDDA;
dda.p = 1;

async function chainKey() {
  return dda?.[await 'p'];
}

async function chainKeyAfterAwaitedBase() {
  return (await dda)?.[await 'p'];
}

async function deleteChainKey() {
  return delete dda?.[await 'p'];
}

chainKey()
  .then(function (v) {
    assert.sameValue(v, 1, 'IsHTMLDDA base is not nullish for ?.[await key]');
    return chainKeyAfterAwaitedBase();
  })
  .then(function (v) {
    assert.sameValue(v, 1, 'awaited IsHTMLDDA base is not nullish');
    return deleteChainKey();
  })
  .then(function (v) {
    assert.sameValue(v, true, 'delete on an IsHTMLDDA base is performed');
    assert.sameValue(dda.p, undefined, 'the property was deleted');
  })
  .then($DONE, $DONE);
