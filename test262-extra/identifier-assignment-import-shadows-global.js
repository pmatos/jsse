/*---
description: Identifier assignment GetValue reads the captured import binding before a same-named global property
esid: sec-assignment-operators-runtime-semantics-evaluation
info: |
  AssignmentExpression : LeftHandSideExpression AssignmentOperator AssignmentExpression
    1. Let lRef be ? Evaluation of LeftHandSideExpression.
    2. If the AssignmentTargetType of LeftHandSideExpression is ~web-compat~,
       throw a ReferenceError exception.
    3. Let lVal be ? GetValue(lRef).

  GetValue of an environment Reference calls the captured Environment Record's
  GetBindingValue. Module Environment Records read indirect bindings from the
  target module, even when the global object has a property with the same name.
  Logical assignment performs this same GetValue before short-circuiting.
flags: [module]
features: [logical-assignment-operators]
---*/

import { answer as imported } from './import-attributes-javascript-dep_FIXTURE.mjs';

var globalReads = 0;
var rhsCalls = 0;
Object.defineProperty(globalThis, 'imported', {
  configurable: true,
  get: function () {
    globalReads++;
    throw new Test262Error('GetValue reached the shadowed global');
  }
});

function rhs() {
  rhsCalls++;
  return 1;
}

// Read the import, run the RHS, then reject PutValue to the immutable import.
assert.throws(TypeError, function () { imported += rhs(); });
assert.sameValue(rhsCalls, 1);
assert.throws(TypeError, function () { imported &&= rhs(); });
assert.sameValue(rhsCalls, 2);
assert.throws(TypeError, function () { (imported) &&= rhs(); });
assert.sameValue(rhsCalls, 3);

// Short-circuiting returns the imported value and never reaches PutValue.
assert.sameValue(imported ||= rhs(), 42);
assert.sameValue((imported) ||= rhs(), 42);
assert.sameValue(imported ??= rhs(), 42);
assert.sameValue((imported) ??= rhs(), 42);
assert.sameValue(rhsCalls, 3);
assert.sameValue(globalReads, 0);
assert.sameValue(imported, 42);
