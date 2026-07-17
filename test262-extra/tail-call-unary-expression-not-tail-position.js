/*---
description: A call nested in a unary expression is not a tail position call
esid: sec-static-semantics-hascallintailposition
flags: [onlyStrict]
features: [tail-call-optimization]
---*/

function isSchemaType(schema, schemaType) {
  return schemaType === "array"
    ? Array.isArray(schema)
    : schemaType === "object"
      ? schema && typeof schema === "object" && !Array.isArray(schema)
      : false;
}

assert.sameValue(isSchemaType([], "array"), true);
assert.sameValue(isSchemaType({}, "object"), true);
assert.sameValue(isSchemaType([], "object"), false);

function returnsNumber() {
  return 42;
}

function fallback() {
  return "fallback";
}

function voidCall(useUnary) {
  return useUnary ? void returnsNumber() : fallback();
}

function typeofCall(useUnary) {
  return useUnary ? typeof returnsNumber() : fallback();
}

function deleteCall(useUnary) {
  return useUnary ? delete returnsNumber() : fallback();
}

assert.sameValue(voidCall(true), undefined);
assert.sameValue(voidCall(false), "fallback");
assert.sameValue(typeofCall(true), "number");
assert.sameValue(typeofCall(false), "fallback");
assert.sameValue(deleteCall(true), true);
assert.sameValue(deleteCall(false), "fallback");
