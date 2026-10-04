/*---
description: >
  A computed key whose expression itself contains an `await`, ahead of a
  trailing object rest, suspends the async function at the await and then
  converts the resumed raw value to a property key (ToPropertyKey) exactly
  once: the same converted key is reused for both the property's own GetV
  read and the rest's exclusion list (issue #771).
esid: sec-destructuring-binding-patterns-runtime-semantics-propertybindinginitialization
info: |
  PropertyName : ComputedPropertyName

  1. Let exprValue be ? Evaluation of AssignmentExpression.
  2. Let propName be ? ToPropertyKey(exprValue).
  3. Return propName.

  ObjectBindingPattern : { BindingPropertyList , BindingRestProperty }

  1. Let excludedNames be ? PropertyBindingInitialization of
     BindingPropertyList ...
  2. Perform ? RestBindingInitialization of BindingRestProperty with
     excludedNames.
flags: [async]
features: [async-functions, destructuring-binding, computed-property-names, object-rest]
---*/

async function run() {
  var n = 0;
  var k = {
    toString: function () {
      n += 1;
      return 'x';
    }
  };
  var { [await k]: a, ...rest } = { x: 10, c: 3 };
  return { a: a, rest: rest, n: n };
}

run().then(function (result) {
  assert.sameValue(result.a, 10, 'the awaited-key property is bound from its GetV read after resume');
  assert.sameValue(result.n, 1, "the key's toString is called exactly once, not once per read");
  assert.sameValue(result.rest.x, undefined, 'the computed key is excluded from rest');
  assert.sameValue(result.rest.c, 3, 'properties not named by the pattern remain in rest');
}).then($DONE, $DONE);
