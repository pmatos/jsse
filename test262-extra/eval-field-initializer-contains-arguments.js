/*---
description: >
  A direct eval textually inside a class field initializer must throw a
  SyntaxError when the eval'd code Contains a reference to `arguments`, no
  matter where in the syntax tree that reference hides. The check is the
  ContainsArguments static semantic, which visits every expression executing
  in the enclosing (field-initializer) scope — descending through arrow
  bodies and class computed keys, but stopping at nested function, method,
  and non-arrow bodies (which own their own `arguments`).
info: |
  sec-performeval (PerformEval): when a direct eval is contained within a
  class field initializer, it is a Syntax Error if ContainsArguments of the
  parsed body is true.

  This is a regression guard for a refactor that replaced three hand-written,
  drifting AST walkers with one exhaustive scope-respecting traversal. The
  previous eval-side walker had `_ => false` fall-throughs and therefore
  MISSED `arguments` hidden inside `typeof`/`void`/`delete`, optional chains,
  dynamic `import()`, and switch-case test expressions — evaluating such code
  instead of throwing the required SyntaxError. Expected results below are
  cross-checked against Node (V8), the reference engine.
esid: sec-performeval
---*/

// Every source Contains `arguments`; each must make the field-initializer eval
// throw a SyntaxError. Node throws SyntaxError for all of these.
var mustThrow = [
  // Previously MISSED by the eval-side walker (the observable bug this fixes):
  "typeof arguments",
  "void arguments",
  "delete arguments.x",
  "a?.[arguments]",
  "switch (0) { case arguments: break; }",
  "import(arguments)",
  // Already covered, kept as anchors so coverage cannot silently regress:
  "arguments",
  "-arguments",
  "arguments ** 2",
  "arguments ||= 1",
  "a ? b : arguments",
  "(0, arguments)",
  "new Foo(arguments)",
  "`${arguments}`",
  "tag`${arguments}`",
  "({ x: arguments })",
  "({ [arguments]: 1 })",
  "while (0) arguments;",
  "for (;;) { arguments; break; }",
  "try { arguments; } catch (e) {}",
  "do { arguments; } while (0);",
  "l: { arguments; }",
  // Scope-transparent traversal targets:
  "() => arguments",
  "class D { [arguments]() {} }",
  "class D extends arguments {}",
];

mustThrow.forEach(function (src) {
  assert.throws(
    SyntaxError,
    function () {
      class C {
        f = eval(src);
      }
      new C();
    },
    "direct eval in field initializer must reject `arguments` in: " + src
  );
});

// Negative controls: `arguments` that belongs to a NESTED scope, or is not a
// reference at all, must NOT trip the check. Node runs all of these without
// throwing.
var mustNotThrow = [
  "function g() { return arguments; } g",
  "(function () { return arguments; })",
  "function g() { return () => arguments; } g",
  "({}).arguments",
  "'arguments'",
  "class D { m() { return arguments; } }",
];

mustNotThrow.forEach(function (src) {
  var threw = false;
  var err;
  try {
    (function () {
      class C {
        f = eval(src);
      }
      new C();
    })();
  } catch (e) {
    threw = true;
    err = e;
  }
  assert(
    !threw,
    "`arguments` in a nested scope must not be rejected in: " +
      src +
      (threw ? " (threw " + err + ")" : "")
  );
});
