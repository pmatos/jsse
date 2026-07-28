/*---
description: >
  A direct eval called from sloppy-mode code shares the caller's
  VariableEnvironment, so `var` and function declarations inside it leak
  to the enclosing function scope. Split out of Eval-scoping.js because a
  direct eval called from strict-mode code always gets its own fresh
  VariableEnvironment instead (EvalDeclarationInstantiation), so none of
  this leakage happens under the "strict" test variant. Cross-checked
  against Node, which agrees: `"use strict"; eval("var x = 1")` throws
  ReferenceError on the outer read.
esid: sec-eval-x
flags: [noStrict]
---*/

// eval var scoping — var in eval leaks to enclosing function scope
(function() {
  eval("var evalVar = 42;");
  if (evalVar !== 42) {
    throw new Test262Error('eval var should leak to function scope, got: ' + typeof evalVar);
  }
})();

// eval var in block scope still leaks to function scope
(function() {
  {
    eval("var blockEvalVar = 10;");
  }
  if (blockEvalVar !== 10) {
    throw new Test262Error('eval var in block should leak to function scope');
  }
})();

// eval function declarations in sloppy mode
(function() {
  eval("function evalFunc() { return 'hello'; }");
  if (evalFunc() !== "hello") {
    throw new Test262Error('eval function declaration should be accessible');
  }
})();
