/*---
description: >
  In an optional chain, a computed member access whose base is undefined or
  null after the short-circuit point evaluates its key expression before the
  TypeError for reading the base is thrown.
esid: sec-evaluate-property-access-with-expression-key
info: |
  EvaluatePropertyAccessWithExpressionKey ( baseValue, expression, strict )

  1. Let propertyNameReference be ? Evaluation of expression.
  2. Let propertyNameValue be ? GetValue(propertyNameReference).
  3. NOTE: In most cases, ToPropertyKey will be performed on propertyNameValue
     immediately after this step. However, in the case of a[b] = c, it will
     not be performed until after evaluation of c.
  4. Return the Reference Record { [[Base]]: baseValue, [[ReferencedName]]:
     propertyNameValue, [[Strict]]: strict, [[ThisValue]]: empty }.

  The key is evaluated while building the Reference, before GetValue(ref)
  throws for a base that RequireObjectCoercible rejects.
features: [optional-chaining]
---*/

var log = [];
var o = { a: undefined, n: null };

function key(name) {
  log.push(name);
  return "k";
}

assert.throws(TypeError, function () {
  o?.a[key("undefined base")];
}, "undefined base after the optional step");
assert.throws(TypeError, function () {
  o?.n[key("null base")];
}, "null base after the optional step");
assert.throws(TypeError, function () {
  o?.a.b[key("skipped")];
}, "the earlier read throws before the key");

assert.sameValue(log.join(), "undefined base,null base");

var thrown;
try {
  o?.a[{ toString: function () { throw new RangeError("ToPropertyKey ran"); } }];
} catch (e) {
  thrown = e;
}
assert.sameValue(thrown instanceof TypeError, true, "the base read fails before ToPropertyKey");
