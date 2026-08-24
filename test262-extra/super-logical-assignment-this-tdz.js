/*---
description: >
  A super-property logical assignment performs GetThisBinding before evaluating
  its key expression, so an uninitialized `this` throws ReferenceError just as
  it does for simple and compound super assignment.
info: |
  MakeSuperPropertyReference ( actualThis, propertyKey, strict )

  Runtime Semantics: Evaluation
    SuperProperty : super [ Expression ]
    1. Let env be GetThisEnvironment().
    2. Let actualThis be ? env.GetThisBinding().

  Function Environment Records GetThisBinding ( )
    1. Assert: envRec.[[ThisBindingStatus]] is not lexical.
    2. If envRec.[[ThisBindingStatus]] is uninitialized, throw a ReferenceError
       exception.
esid: sec-super-keyword-runtime-semantics-evaluation
features: [logical-assignment-operators]
---*/

class Base {}

var keyEvaluated = false;
function trackedKey() {
  keyEvaluated = true;
  return "value";
}

assert.throws(
  ReferenceError,
  function () {
    new (class extends Base {
      constructor() {
        super.value ??= 1;
        super();
      }
    })();
  },
  "super.value ??= throws before `this` is initialized"
);

assert.throws(
  ReferenceError,
  function () {
    new (class extends Base {
      constructor() {
        super.value ||= 1;
        super();
      }
    })();
  },
  "super.value ||= throws before `this` is initialized"
);

assert.throws(
  ReferenceError,
  function () {
    new (class extends Base {
      constructor() {
        super[trackedKey()] &&= 1;
        super();
      }
    })();
  },
  "super[key] &&= throws before `this` is initialized"
);

assert.sameValue(
  keyEvaluated,
  false,
  "GetThisBinding runs before the super key expression is evaluated"
);

assert.throws(
  ReferenceError,
  function () {
    new (class extends Base {
      constructor() {
        super.value = 1;
        super();
      }
    })();
  },
  "simple super assignment already threw for an uninitialized `this`"
);

var afterSuper;
class Derived extends Base {
  constructor() {
    super();
    super.value ??= 5;
    afterSuper = this.value;
  }
}
new Derived();
assert.sameValue(afterSuper, 5, "the same form succeeds once `this` is initialized");
