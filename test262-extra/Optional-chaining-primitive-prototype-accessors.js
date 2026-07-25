// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-optional-chaining-evaluation
description: >
  Optional-chain property access on a primitive invokes accessor getters found
  on the corresponding wrapper prototype.
info: |
  13.3.7.1 Runtime Semantics: Evaluation
    OptionalExpression : MemberExpression OptionalChain
      4. Return ? ChainEvaluation of OptionalChain with arguments baseValue and
         baseReference.

  13.3.7.2 Runtime Semantics: ChainEvaluation
    OptionalChain : ?. [ Expression ]
      2. Return ? EvaluatePropertyAccessWithExpressionKey(baseValue,
         Expression, strict).
    OptionalChain : ?. IdentifierName
      2. Return EvaluatePropertyAccessWithIdentifierKey(baseValue,
         IdentifierName, strict).

  6.2.5.5 GetValue ( V )
    3.a. Let baseObj be ? ToObject(V.[[Base]]).
    3.d. Return ? baseObj.[[Get]](V.[[ReferencedName]], GetThisValue(V)).

  10.1.8.1 OrdinaryGet ( O, P, Receiver )
    If an accessor descriptor is found, Call(getter, Receiver).
features: [optional-chaining, BigInt, Symbol]
---*/

function install(proto, key, expectedThis, result) {
  Object.defineProperty(proto, key, {
    configurable: true,
    get: function () {
      "use strict";
      assert.sameValue(this, expectedThis, key + " getter receiver");
      return result;
    },
  });
}

var symbol = Symbol("receiver");
install(String.prototype, "01", "abc", 41);
install(String.prototype, "5", "abc", 42);
install(Number.prototype, "optionalAccessor", 5, 43);
install(Boolean.prototype, "optionalAccessor", true, 44);
install(Symbol.prototype, "optionalAccessor", symbol, 45);
install(BigInt.prototype, "optionalAccessor", 7n, 46);

assert.sameValue("abc"?.["01"], 41, "computed String look-alike key");
assert.sameValue("abc"?.["5"], 42, "computed String out-of-range key");
assert.sameValue((5)?.optionalAccessor, 43, "named Number key");
assert.sameValue((true)?.["optionalAccessor"], 44, "computed Boolean key");
assert.sameValue(symbol?.optionalAccessor, 45, "named Symbol key");
assert.sameValue((7n)?.["optionalAccessor"], 46, "computed BigInt key");

var marker = {};
Object.defineProperty(Boolean.prototype, "throwingOptionalAccessor", {
  configurable: true,
  get: function () {
    throw marker;
  },
});
var caught;
try {
  (false)?.throwingOptionalAccessor;
} catch (error) {
  caught = error;
}
assert.sameValue(caught, marker, "getter abrupt completion is propagated");
