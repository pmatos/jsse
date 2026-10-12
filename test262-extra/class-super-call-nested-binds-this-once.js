// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-super-keyword-runtime-semantics-evaluation
description: >
  A super() call checks whether `this` is already bound after the parent
  constructor returns, not before it starts. When an arrow function runs a
  nested super() while the outer one is still constructing, the nested call
  binds `this` and initializes the fields, and the outer call then throws a
  ReferenceError without initializing the fields again.
info: |
  SuperCall : super Arguments
    ...
    6. Let result be ? Construct(func, argList, newTarget).
    7. Let thisER be GetThisEnvironment().
    8. Assert: thisER is a Function Environment Record.
    9. Perform ? BindThisValue(thisER, result).
    10. Let F be thisER.[[FunctionObject]].
    11. Assert: F is an ECMAScript function object.
    12. Perform ? InitializeInstanceElements(result, F).

  BindThisValue ( envRec, V )
    1. Assert: envRec.[[ThisBindingStatus]] is not lexical.
    2. If envRec.[[ThisBindingStatus]] is initialized, throw a ReferenceError exception.
includes: [compareArray.js]
features: [class, class-fields-public, arrow-function]
---*/

var log = [];

class A {
  constructor(runNested) {
    if (runNested) {
      runNested();
    }
    log.push("A");
  }
}

var instance;
var caught;

class B extends A {
  field = log.push("field");
  constructor() {
    var nested = () => super();
    try {
      super(nested);
    } catch (e) {
      caught = e;
    }
    instance = this;
  }
}

new B();

assert.sameValue(caught.constructor, ReferenceError, "the outer super() throws a ReferenceError");
assert.compareArray(log, ["A", "field", "A"], "the fields are initialized once, by the nested call");
assert.sameValue(instance.field, 2, "`this` is the object the nested super() bound");
