/*---
description: >
  Array initializer values remain reachable while later elements and iterator
  steps trigger garbage collection.
esid: sec-runtime-semantics-arrayaccumulation
features: [Symbol.iterator]
info: |
  ECMAScript sec-runtime-semantics-arrayaccumulation creates each data property
  on the result Array before evaluating the following element or requesting the
  next iterator value. Therefore an object already accumulated by an Array
  initializer must remain strongly reachable until the Array is returned, and
  temporary implementation roots must also be released on abrupt completion.
---*/

function makeEarlierSpread() {
  return [{ opts: { code: { marker: "earlier spread" } } }];
}

function collectAfterGc() {
  $262.gc();
  return [];
}

var fromEarlierSpread = [...makeEarlierSpread(), ...collectAfterGc()];
assert.sameValue(
  fromEarlierSpread[0].opts.code.marker,
  "earlier spread",
  "an object from an earlier spread survives evaluation of a later spread"
);

function evaluateAfterGc() {
  $262.gc();
  return 1;
}

var fromEarlierElement = [
  { opts: { code: { marker: "earlier element" } } },
  evaluateAfterGc(),
];
assert.sameValue(
  fromEarlierElement[0].opts.code.marker,
  "earlier element",
  "an earlier ordinary element survives evaluation of a later element"
);

function makeCollectingIterator() {
  var produced = false;
  return {
    [Symbol.iterator]: function () {
      return this;
    },
    next: function () {
      if (!produced) {
        produced = true;
        return {
          value: { opts: { code: { marker: "iterator value" } } },
          done: false,
        };
      }
      $262.gc();
      return { done: true };
    },
  };
}

var fromOneSpread = [...makeCollectingIterator()];
assert.sameValue(
  fromOneSpread[0].opts.code.marker,
  "iterator value",
  "a yielded object survives collection during a later iterator step"
);
