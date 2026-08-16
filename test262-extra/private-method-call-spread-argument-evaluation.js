/*---
description: >
  A spread argument in a private-method call is evaluated through the shared
  argument-list evaluation: a non-iterable spread throws a TypeError, an
  iterable spread is forwarded, and arguments already accumulated stay reachable
  across a garbage collection triggered while a later argument is evaluated.
esid: sec-argument-lists-runtime-semantics-argumentlistevaluation
features: [class-methods-private, Symbol.iterator]
info: |
  ArgumentListEvaluation for an `... AssignmentExpression` element performs
  GetIterator(spreadObj, sync). GetIterator throws a TypeError when the value has
  no @@iterator method, and this must hold regardless of how the callee is
  reached -- including a call to a private method, `this.#m(...)`. The already
  evaluated arguments must also remain strongly reachable while the remaining
  arguments (and their iterator steps) are evaluated, so an object accumulated
  before a garbage collection is not swept before the call is performed.
---*/

// A non-iterable spread argument throws a TypeError even for a private method.
class NonIterable {
  #m() {
    return "unreachable";
  }
  run() {
    return this.#m(...5);
  }
}
assert.throws(
  TypeError,
  function () {
    new NonIterable().run();
  },
  "a non-iterable spread argument to a private method throws a TypeError"
);

// An iterable spread argument is forwarded to the private method.
class Forward {
  #m(a, b, c) {
    return a + b + c;
  }
  run() {
    return this.#m(1, ...[2, 3]);
  }
}
assert.sameValue(
  new Forward().run(),
  6,
  "iterable spread arguments are forwarded to the private method"
);

// An earlier argument survives a garbage collection triggered while a later
// spread's iterator is stepped.
class Survive {
  #m(earlier) {
    return earlier.marker;
  }
  run() {
    var collectingIterator = {
      [Symbol.iterator]: function () {
        var produced = false;
        return {
          next: function () {
            if (!produced) {
              produced = true;
              return { value: { marker: "later" }, done: false };
            }
            $262.gc();
            return { value: undefined, done: true };
          },
        };
      },
    };
    return this.#m({ marker: "earlier" }, ...collectingIterator);
  }
}
assert.sameValue(
  new Survive().run(),
  "earlier",
  "an earlier object argument survives GC during a later spread's iterator step"
);
