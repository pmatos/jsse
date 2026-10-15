// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-getiterator
description: >
  GetIterator with the async hint reads @@asyncIterator with GetV, so an
  inherited accessor on String.prototype is called with the string
  primitive as its receiver and its result is used for for-await.
info: |
  GetIterator ( obj, kind )

  1. If kind is async, then
     a. Let method be ? GetMethod(obj, @@asyncIterator).
     b. If method is undefined, then
        ...
  ...

  GetMethod ( V, P )

  1. Let func be ? GetV(V, P).
  ...

  GetV ( V, P )

  1. Let O be ? ToObject(V).
  2. Return ? O.[[Get]](P, V).
flags: [async]
features: [async-iteration, Symbol.asyncIterator]
---*/

var receivers = [];
Object.defineProperty(String.prototype, Symbol.asyncIterator, {
  get: function () {
    "use strict";
    receivers.push(typeof this);
    return async function* () { yield "custom"; };
  },
  configurable: true
});

(async function () {
  var seen = [];
  for await (var v of "abc") seen.push(v);
  assert.sameValue(receivers.length, 1, "getter calls");
  assert.sameValue(receivers[0], "string", "receiver");
  assert.sameValue(seen.length, 1, "iterations");
  assert.sameValue(seen[0], "custom", "value from @@asyncIterator");
})().then($DONE, $DONE);
