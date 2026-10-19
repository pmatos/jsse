// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-runtime-semantics-keyedbindinginitialization
description: >
  The source of a `let`/`const` object binding pattern, the values already
  bound and the getter results stay reachable while later getters allocate
  and collect.
info: |
  BindingInitialization : ObjectBindingPattern evaluates the initializer once
  and then reads every property from that value, so it must stay reachable
  between reads even though no binding refers to it yet. A getter can run
  arbitrary code, including a garbage collection.
features: [host-gc-required, destructuring-binding]
---*/

function churn() {
  $262.gc();
  var junk = [];
  for (var i = 0; i < 300; i++) {
    junk.push({ i: i, s: "x" + i });
  }
  return junk.length;
}

function makeSource() {
  return {
    get a() { churn(); return { tag: "a", payload: [1, 2, 3] }; },
    get b() { churn(); return { tag: "b" }; },
    get c() { churn(); return "c".repeat(3); },
    get d() { churn(); return [4, 5, 6]; },
  };
}

function freshSourceOnlyOnTheStack() {
  const { a, b, c, d } = makeSource();
  return [a.tag, a.payload.join(), b.tag, c, d.join()].join("|");
}
assert.sameValue(freshSourceOnlyOnTheStack(), "a|1,2,3|b|ccc|4,5,6");

function boundValuesSurviveLaterGetters() {
  const { first, second } = {
    get first() { return { kept: "alive" }; },
    get second() { churn(); return 1; },
  };
  churn();
  return first.kept + second;
}
assert.sameValue(boundValuesSurviveLaterGetters(), "alive1");

function primitiveSourceWrapper() {
  Object.defineProperty(String.prototype, "probe", {
    get() { "use strict"; churn(); return { wrapped: typeof this }; },
    configurable: true,
  });
  try {
    const { probe, length } = "abc";
    return probe.wrapped + length;
  } finally {
    delete String.prototype.probe;
  }
}
assert.sameValue(primitiveSourceWrapper(), "string3");

function getterThrowsAfterCollection() {
  try {
    const { a, b } = {
      get a() { churn(); return { v: 1 }; },
      get b() { churn(); throw new RangeError("late"); },
    };
    return "no throw";
  } catch (e) {
    return e.constructor.name + e.message;
  }
}
assert.sameValue(getterThrowsAfterCollection(), "RangeErrorlate");

function nestedPatternsInsideGetters() {
  const { outer } = {
    get outer() {
      const { inner, other } = {
        get inner() { churn(); return { depth: 2 }; },
        get other() { churn(); return 3; },
      };
      return { inner, other };
    },
  };
  return outer.inner.depth + outer.other;
}
assert.sameValue(nestedPatternsInsideGetters(), 5);

function repeatedInLoop() {
  var total = 0;
  for (var i = 0; i < 50; i++) {
    const { a, b } = {
      get a() { return { n: i }; },
      get b() { if (i % 10 === 0) churn(); return { n: i + 1 }; },
    };
    total += a.n + b.n;
  }
  return total;
}
assert.sameValue(repeatedInLoop(), 2500);
