// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-iteratorbindinginitialization
description: >
  An error produced while binding (or assigning) an array destructuring
  pattern's default-value initializer stays reachable across the garbage
  collection that can happen while the pattern's still-open iterator's
  return() runs, for both the binding form (`var [a, b = f()] = iterable`)
  and the assignment-target form (`[a, b = f()] = iterable`).
info: |
  Runtime Semantics: IteratorBindingInitialization
  SingleNameBinding : BindingIdentifier Initializer

  Runtime Semantics: DestructuringAssignmentEvaluation
  AssignmentElement : DestructuringAssignmentTarget Initializer

  A throw while evaluating the Initializer is a throw completion that
  IteratorBindingInitialization/DestructuringAssignmentEvaluation passes to
  IteratorClose (since the source iterator is not yet done); IteratorClose's
  `return()` call can run arbitrary user code.
features: [destructuring-binding, destructuring-assignment, default-parameters, host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

// An iterable that is not yet done when the second element's default
// initializer runs (its value is `undefined`, triggering the default),
// and whose return() churns the heap.
function makeDefaultTriggeringIterable() {
  return {
    [Symbol.iterator]() {
      var i = 0;
      return {
        next() {
          i++;
          return { done: false, value: i === 1 ? 1 : undefined };
        },
        return(v) {
          collect();
          return { done: true };
        },
      };
    },
  };
}

// Binding form: `var [x, y = doThrow()] = iterable`.
function doThrowBinding() {
  throw new Error("binding-default-throw-payload");
}
var bindingThrown;
function bindThrows() {
  var [x, y = doThrowBinding()] = makeDefaultTriggeringIterable();
  return [x, y];
}
try {
  bindThrows();
} catch (e) {
  bindingThrown = e;
}
assert.sameValue(
  bindingThrown.message,
  "binding-default-throw-payload",
  "array binding pattern's default-initializer error survives IteratorClose's return() call"
);

// Assignment-target form: `[x, y = doThrow()] = iterable`.
function doThrowAssign() {
  throw new Error("assignment-default-throw-payload");
}
var assignThrown;
function assignThrows() {
  var x, y;
  [x, y = doThrowAssign()] = makeDefaultTriggeringIterable();
  return [x, y];
}
try {
  assignThrows();
} catch (e) {
  assignThrown = e;
}
assert.sameValue(
  assignThrown.message,
  "assignment-default-throw-payload",
  "array assignment pattern's default-initializer error survives IteratorClose's return() call"
);
