// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-optional-chaining-chain-evaluation
description: >
  The callee, receiver, and base value of an optional-chain call or computed
  member access stay reachable while the argument list or key expression
  triggers garbage collection.
info: |
  13.3.7.2 Runtime Semantics: ChainEvaluation
    OptionalChain : ?. Arguments
      1. Let thisChain be this OptionalChain.
      2. Let tailCall be IsInTailPosition(thisChain).
      3. Return ? EvaluateCall(baseValue, baseReference, Arguments, tailCall).

    OptionalChain : OptionalChain [ Expression ]
      2. Let newReference be ? ChainEvaluation of optionalChain ...
      3. Let newValue be ? GetValue(newReference).
      5. Return ? EvaluatePropertyAccessWithExpressionKey(newValue, Expression, strict).

  EvaluateCall evaluates the arguments after the callee and its this value are
  known, and EvaluatePropertyAccessWithExpressionKey evaluates the key after the
  base value is known. Both values must survive those later evaluation steps,
  even when the callee is a temporary that only exists as the result of an
  earlier step in the chain, such as the value returned by a getter or by a
  call, and user code in the arguments or key expression collects garbage.
features: [class, class-methods-private, optional-chaining]
---*/

function collectAndReturnOne() {
  $262.gc();
  return 1;
}

function collectAndReturnKey() {
  $262.gc();
  return "k";
}

class C {
  get #getter() {
    return function () {
      return "called";
    };
  }

  #data = { k: "v" };

  get #dataGetter() {
    return { k: "getter-key" };
  }

  static viaTail(o) {
    return o?.#getter(collectAndReturnOne());
  }

  static viaBase(o) {
    return o.#getter?.(collectAndReturnOne());
  }

  static viaBoth(o) {
    return o?.#getter?.(collectAndReturnOne());
  }

  static keyOnGetterResult(o) {
    return o.#dataGetter?.[collectAndReturnKey()];
  }

  static keyOnFieldTail(o) {
    return o?.#data[collectAndReturnKey()];
  }
}

assert.sameValue(C.viaTail(new C()), "called", "callee from a private getter, ?.#getter(args)");
assert.sameValue(C.viaBase(new C()), "called", "callee from a private getter, #getter?.(args)");
assert.sameValue(C.viaBoth(new C()), "called", "callee from a private getter, ?.#getter?.(args)");
assert.sameValue(C.keyOnGetterResult(new C()), "getter-key", "base from a private getter, ?.[key]");
assert.sameValue(C.keyOnFieldTail(new C()), "v", "base from a private field, .[key] in the chain tail");

function makeFunction() {
  return function () {
    return "made";
  };
}

function makeObject() {
  return { k: "made-key" };
}

assert.sameValue(makeFunction()?.(collectAndReturnOne()), "made", "temporary callee, ?.(args)");
assert.sameValue(makeObject()?.[collectAndReturnKey()], "made-key", "temporary base, ?.[key]");

var holder = {
  get getter() {
    return function () {
      return "public-getter";
    };
  },
};

assert.sameValue(holder.getter?.(collectAndReturnOne()), "public-getter", "callee from a getter, .getter?.(args)");
assert.sameValue(
  (function () { return { method: function () { return "method"; } }; })()?.method(collectAndReturnOne()),
  "method",
  "temporary receiver, ?.method(args)"
);
