/*---
description: >
  Reserved words and literals are valid IdentifierName property names after
  the optional chaining punctuator.
info: |
  OptionalChain[Yield, Await] :
    `?.` IdentifierName

  IdentifierName includes ReservedWord, BooleanLiteral, and NullLiteral tokens;
  only the Identifier production excludes reserved words.
esid: prod-OptionalChain
features: [optional-chaining]
---*/

var value = {
  continue: 1,
  if: 2,
  true: 3,
  null: 4
};

assert.sameValue(value?.continue, 1);
assert.sameValue(value?.if, 2);
assert.sameValue(value?.true, 3);
assert.sameValue(value?.null, 4);
assert.sameValue(null?.continue, undefined);

// A reserved word used as an IdentifierName property after `?.` is never an
// IdentifierReference, so `await` / `yield` here are ordinary property names,
// not operator/keyword uses.
assert.sameValue(({ await: 1 })?.await, 1);
assert.sameValue(({ await: { then: 9 } })?.await.then, 9);
assert.sameValue(({ yield: 2 })?.yield, 2);

// The property name remains an IdentifierName even inside an async arrow's
// parameter default, where a genuine `await` identifier reference would be a
// SyntaxError. This previously threw "'await' is not allowed in async arrow
// formal parameters" because the optional-chain property name was walked as an
// identifier reference by the async-arrow parameter validator.
var asyncArrow = async (x = ({ await: 1 })?.await) => x;
