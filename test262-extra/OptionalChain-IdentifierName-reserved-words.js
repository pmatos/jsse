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
