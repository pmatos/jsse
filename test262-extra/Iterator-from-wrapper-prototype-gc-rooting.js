// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-iterator.from
description: >
  %WrapForValidIteratorPrototype% stays alive while no wrapper object exists,
  so Iterator.from still creates wrappers that inherit next and return after a
  garbage collection.
info: |
  Iterator.from ( O )

  4. Let hasInstance be ? OrdinaryHasInstance(%Iterator%, iteratorRecord.[[Iterator]]).
  5. If hasInstance is true, return iteratorRecord.[[Iterator]].
  6. Let wrapper be OrdinaryObjectCreate(%WrapForValidIteratorPrototype%, « [[Iterated]] »).

  %WrapForValidIteratorPrototype%.return ( )

  5. Let returnMethod be ? GetMethod(iterator, "return").
  6. If returnMethod is undefined, then
    a. Return CreateIterResultObject(undefined, true).
features: [iterator-helpers, host-gc-required]
---*/

$262.gc();
$262.gc();

var wrapper = Iterator.from({
  next: function() {
    return { done: true, value: undefined };
  }
});

assert.sameValue(typeof wrapper.next, "function", "wrapper inherits next");
assert.sameValue(typeof wrapper.return, "function", "wrapper inherits return");

var result = wrapper.return();
assert.sameValue(result.done, true, "return() reports done");
assert.sameValue(result.value, undefined, "return() reports an undefined value");
assert(result.hasOwnProperty("value"), "the result has an own value property");
