/*---
description: WeakMap constructor looks up @@iterator on a primitive iterable via GetV and reads next once
info: |
  24.3.1.1 WeakMap ( [ iterable ] )
  7.4.2 GetIterator ( obj, kind )
  7.4.1 GetV ( V, P )

  Step 6 calls GetIterator(iterable, sync). GetMethod(iterable, @@iterator) uses
  GetV, so a primitive iterable (such as a String) is accepted and its method is
  looked up on the wrapper prototype with the primitive as receiver.
esid: sec-weakmap-iterable
---*/

assert.sameValue(
  Object.getPrototypeOf(new WeakMap("")),
  WeakMap.prototype,
  "the empty string iterates nothing"
);

assert.throws(TypeError, function() {
  new WeakMap("s");
}, "a string yields code points, which are not entry objects");

assert.throws(TypeError, function() {
  new WeakMap(1);
}, "a Number has no @@iterator");

assert.throws(TypeError, function() {
  new WeakMap(true);
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
  new WeakMap("s");
} finally {
  Object.defineProperty(String.prototype, Symbol.iterator, descriptor);
}
assert.sameValue(receiverType, "string", "the @@iterator getter receives the primitive");

var nextReads = 0;
var count = 0;
var iterable = {};
iterable[Symbol.iterator] = function() {
  var iterator = {};
  Object.defineProperty(iterator, "next", {
    get: function() {
      nextReads += 1;
      return function() {
        count += 1;
        return count > 3 ? { done: true } : { done: false, value: [{}, count] };
      };
    }
  });
  return iterator;
};
new WeakMap(iterable);
assert.sameValue(nextReads, 1, "next is read once by GetIteratorFromMethod");
assert.sameValue(count, 4, "next is called until done");
