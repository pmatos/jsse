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
