/*---
description: >
  A non-suspending computed key ahead of a property whose default suspends
  on `await`, itself ahead of a trailing object rest, is converted to a
  property key (ToPropertyKey) exactly once: the same converted key is
  reused for both the property's own GetV read and the rest's exclusion
  list, rather than re-running ToPropertyKey a second time (which would be
  user-observable through a custom `toString`/`Symbol.toPrimitive`)
  (issue #771).
esid: sec-destructuring-binding-patterns-runtime-semantics-propertybindinginitialization
info: |
  BindingProperty : PropertyName : BindingElement

  1. Let P be ? Evaluation of PropertyName.
  2. Perform ? KeyedBindingInitialization of BindingElement ... with P.

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
  var { [k]: a, b = await 1, ...rest } = { x: 10, c: 3 };
  return { a: a, b: b, rest: rest, n: n };
}

run().then(function (result) {
  assert.sameValue(result.a, 10, 'the computed-key property is bound from its GetV read');
  assert.sameValue(result.b, 1, "a later property's await default still runs");
  assert.sameValue(result.n, 1, "the key's toString is called exactly once, not once per read");
  assert.sameValue(result.rest.x, undefined, 'the computed key is excluded from rest');
  assert.sameValue(result.rest.b, undefined, 'the defaulted property is excluded from rest');
  assert.sameValue(result.rest.c, 3, 'properties not named by the pattern remain in rest');
}).then($DONE, $DONE);
