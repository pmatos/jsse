// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-runtime-semantics-classdefinitionevaluation
description: >
  The class constructor and its prototype stay alive while a computed member
  key or a static field initializer runs user code that triggers a collection.
features: [class, class-static-fields-public, Symbol.toPrimitive]
includes: [propertyHelper.js]
---*/

function collectingKey(result) {
  var key = {};
  key[Symbol.toPrimitive] = function() {
    $262.gc();
    var churn = [];
    for (var i = 0; i < 2000; i++) {
      churn.push({ index: i });
    }
    return result;
  };
  return key;
}

var sym = Symbol("k");

var C = class {
  static get [collectingKey(sym)]() { return 1; }
  static [collectingKey("later")]() { return 2; }
  static field = ($262.gc(), 3);
  method() { return 4; }
};

assert.sameValue(typeof C, "function", "class value");
assert.sameValue(C.prototype.method(), 4, "prototype method");
assert.sameValue(C[sym], 1, "static getter through coerced Symbol key");
assert.sameValue(C.later(), 2, "static method through coerced String key");
assert.sameValue(C.field, 3, "static field");
assert.sameValue(
  Object.getOwnPropertyDescriptor(C, sym).get.name,
  "get [k]",
  "getter name from the coerced Symbol key"
);
verifyProperty(C, "prototype", { writable: false, enumerable: false, configurable: false });
