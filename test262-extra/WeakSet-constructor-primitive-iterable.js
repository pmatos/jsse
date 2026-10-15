/*---
description: WeakSet constructor looks up @@iterator on a primitive iterable via GetV and reads next once
info: |
  24.4.1.1 WeakSet ( [ iterable ] )
  7.4.2 GetIterator ( obj, kind )
  7.4.1 GetV ( V, P )

  Step 6 calls GetIterator(iterable, sync). GetMethod(iterable, @@iterator) uses
  GetV, so a primitive iterable (such as a String) is accepted and its method is
  looked up on the wrapper prototype with the primitive as receiver.
esid: sec-weakset-iterable
---*/

assert.sameValue(
  Object.getPrototypeOf(new WeakSet("")),
  WeakSet.prototype,
  "the empty string iterates nothing"
);

assert.throws(TypeError, function() {
  new WeakSet("s");
}, "a string yields code points, which are not valid WeakSet values");

assert.throws(TypeError, function() {
  new WeakSet(1);
}, "a Number has no @@iterator");

assert.throws(TypeError, function() {
  new WeakSet(true);
}, "a Boolean has no @@iterator");

var descriptor = Object.getOwnPropertyDescriptor(String.prototype, Symbol.iterator);
var receiverType;
Object.defineProperty(String.prototype, Symbol.iterator, {
  configurable: true,
  get: function() {
    "use strict";
    receiverType = typeof this;
    return function() {
      return { next: function() { return { done: true }; } };
    };
  }
});
try {
  new WeakSet("s");
} finally {
  Object.defineProperty(String.prototype, Symbol.iterator, descriptor);
}
assert.sameValue(receiverType, "string", "the @@iterator getter receives the primitive");

var nextReads = 0;
var count = 0;
var obj = {};
var iterable = {};
iterable[Symbol.iterator] = function() {
  var iterator = {};
  Object.defineProperty(iterator, "next", {
    get: function() {
      nextReads += 1;
      return function() {
        count += 1;
        return count > 3 ? { done: true } : { done: false, value: {} };
      };
    }
  });
  return iterator;
};
new WeakSet(iterable);
assert.sameValue(nextReads, 1, "next is read once by GetIteratorFromMethod");
assert.sameValue(count, 4, "next is called until done");
