/*---
description: >
  A direct eval textually inside a class field initializer must throw a
  SyntaxError when the eval'd code Contains a SuperCall (`super(...)`),
  regardless of where in the syntax tree the call hides. A field initializer
  is not a derived constructor, so `super()` is never legal there.
info: |
  sec-performeval (PerformEval): when a direct eval is contained within a
  class field initializer, it is a Syntax Error if the parsed body Contains a
  SuperCall (the running context is not a derived constructor).

  jsse rejects `super()` in this position at parse time, so this file is a
  characterization anchor rather than a red test: it pins the observable
  behavior (SyntaxError in every position, cross-checked against Node/V8) so
  that unifying the Contains-SuperCall walker with the shared scope-respecting
  traversal cannot regress it. `super.prop` (a SuperProperty, not a SuperCall)
  is a negative control — it is legal in a field initializer.
esid: sec-performeval
---*/

var mustThrow = [
  "super();",
  "while (0) super();",
  "for (;;) { super(); break; }",
  "try { super(); } catch (e) {}",
  "switch (0) { case 1: super(); }",
  "do { super(); } while (0);",
  "l: { super(); }",
  "`${super()}`",
  "({ x: super() })",
  "typeof super()",
  "() => super()",
  "a ? super() : b",
];

mustThrow.forEach(function (src) {
  assert.throws(
    SyntaxError,
    function () {
      class B {}
      class C extends B {
        f = eval(src);
      }
      new C();
    },
    "direct eval in field initializer must reject SuperCall in: " + src
  );
});

// Negative control: SuperProperty (`super.x`) is NOT a SuperCall and is legal
// in a field initializer; reading it must not be rejected. `super.x` resolves
// against the home object's prototype (B.prototype), so its value (undefined
// here) is beside the point — what matters is that no SyntaxError is raised.
(function () {
  var threw = false;
  var err;
  try {
    class B {}
    class C extends B {
      f = eval("super.x");
    }
    new C();
  } catch (e) {
    threw = true;
    err = e;
  }
  assert(!threw, "super.x (SuperProperty) must not be rejected" + (threw ? " (threw " + err + ")" : ""));
})();
