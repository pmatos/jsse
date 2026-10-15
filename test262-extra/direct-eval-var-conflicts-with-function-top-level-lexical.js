/*---
description: Sloppy direct eval var and function declarations conflict with function-top-level let, const and class bindings
info: |
  19.2.1.3 EvalDeclarationInstantiation ( body, varEnv, lexEnv, privateEnv, strict )

  3. If strict is false, then
    a. If varEnv is a Global Environment Record, ...
    b. Let thisEnv be lexEnv.
    c. Assert: The following loop will terminate.
    d. Repeat, while thisEnv is not varEnv,
      i. If thisEnv is not an Object Environment Record, then
        1. NOTE: The environment of with statements cannot contain any lexical declaration so it doesn't need to be checked for var/let hoisting conflicts.
        2. For each element name of varNames, do
          a. If ! thisEnv.HasBinding(name) is true, then
            i. Throw a SyntaxError exception.
      ii. Set thisEnv to thisEnv.[[OuterEnv]].

  10.2.11 FunctionDeclarationInstantiation step 30: a separate lexical
  Environment Record is created for the function's top-level lexical
  declarations when the function is not strict, so the eval's walk visits it.
esid: sec-evaldeclarationinstantiation
flags: [noStrict, async]
---*/

function outcome(thunk) {
  try {
    return thunk();
  } catch (e) {
    return e.constructor === SyntaxError ? "SyntaxError" : "other:" + e;
  }
}

// Function-top-level let, with the eval inside a nested block
assert.sameValue(outcome(function() {
  let y = 1;
  { eval("var y"); }
  return "no throw";
}), "SyntaxError", "block-nested eval var vs top-level let");

assert.sameValue(outcome(function() {
  let y = 1;
  try { eval("var y"); } finally {}
  return "no throw";
}), "SyntaxError", "try-nested eval var vs top-level let");

// Eval directly in the function's top level
assert.sameValue(outcome(function() {
  let y = 1;
  eval("var y");
  return "no throw";
}), "SyntaxError", "top-level eval var vs top-level let");

assert.sameValue(outcome(function() {
  const y = 1;
  eval("var y");
  return "no throw";
}), "SyntaxError", "eval var vs top-level const");

assert.sameValue(outcome(function() {
  class y {}
  eval("var y");
  return "no throw";
}), "SyntaxError", "eval var vs top-level class");

assert.sameValue(outcome(function() {
  let y = 1;
  { eval("function y() {}"); }
  return "no throw";
}), "SyntaxError", "eval function declaration vs top-level let");

assert.sameValue(outcome(function() {
  const y = 1;
  { eval("function y() {}"); }
  return "no throw";
}), "SyntaxError", "eval function declaration vs top-level const");

assert.sameValue(outcome(function() {
  class y {}
  { eval("function y() {}"); }
  return "no throw";
}), "SyntaxError", "eval function declaration vs top-level class");

assert.sameValue(outcome(function() {
  { eval("var y"); }
  let y = 1;
  return "no throw";
}), "SyntaxError", "conflict is detected before the let is initialized (TDZ)");

assert.sameValue(outcome(function() {
  let y = 1;
  { eval("var a; var y;"); }
  return "no throw";
}), "SyntaxError", "conflict in a later var name");

assert.sameValue(outcome(function() {
  let y = 1;
  { eval("if (true) { var y; }"); }
  return "no throw";
}), "SyntaxError", "conflict through a nested var statement");

assert.sameValue(outcome(function() {
  let [a, { y }] = [1, { y: 2 }];
  { eval("var y"); }
  return "no throw";
}), "SyntaxError", "destructured top-level let binding");

assert.sameValue(outcome(() => {
  let y = 1;
  { eval("var y"); }
  return "no throw";
}), "SyntaxError", "arrow function top-level let");

// The failed eval must not leak any of its declarations
assert.sameValue(outcome(function() {
  let y = 1;
  try { eval("var leaked; var y;"); } catch (e) {}
  return typeof leaked;
}), "undefined", "failed eval does not create the non-conflicting var");

assert.sameValue(typeof leaked, "undefined", "no global leak either");

// Generators, async functions and methods share the check
assert.sameValue(outcome(function() {
  function* g() {
    let y = 1;
    { eval("var y"); }
    yield 1;
  }
  g().next();
  return "no throw";
}), "SyntaxError", "generator top-level let");

var asyncFn = async function() {
  let y = 1;
  { eval("var y"); }
};
var asyncGen = async function*() {
  const y = 1;
  { eval("var y"); }
};
var obj = {
  method() {
    class y {}
    { eval("var y"); }
  }
};
assert.sameValue(outcome(function() { obj.method(); return "no throw"; }), "SyntaxError", "method top-level class");

// Annex B.3.3.3 block-function hoisting in eval skips names that are lexically bound
assert.sameValue(typeof (function() {
  let y = 1;
  eval("{ function y() {} }");
  return y;
})(), "number", "eval block function does not overwrite a top-level let");

assert.sameValue((function() {
  class y { static v = 3 }
  eval("{ function y() {} }");
  return y.v;
})(), 3, "eval block function does not overwrite a top-level class");

assert.sameValue(typeof (function() {
  let y = 1;
  { eval("if (true) function y() {}"); }
  return y;
})(), "number", "eval if-function does not overwrite a top-level let");

assert.sameValue(typeof (function() {
  let y = 1;
  eval("switch (1) { case 1: function y() {} }");
  return y;
})(), "number", "eval switch-function does not overwrite a top-level let");

assert.sameValue(typeof (function() {
  eval("{ function y() {} }");
  return y;
})(), "function", "eval block function hoists when there is no lexical binding");

// Bindings that are not function-top-level lexical declarations do not conflict
assert.sameValue(outcome(function() {
  { let y = 1; }
  eval("var y = 2");
  return y;
}), 2, "sibling block let");

assert.sameValue(outcome(function() {
  for (let y = 0; y < 1; y++) {}
  for (const y of [1]) {}
  for (let y in { a: 1 }) {}
  eval("var y = 2");
  return y;
}), 2, "for-loop head bindings");

assert.sameValue(outcome(function() {
  switch (1) { case 1: let y = 1; }
  eval("var y = 2");
  return y;
}), 2, "switch case let");

assert.sameValue(outcome(function() {
  try { throw 1; } catch (y) {}
  eval("var y = 2");
  return y;
}), 2, "catch parameter scope");

assert.sameValue(outcome(function() {
  function* g() {
    { let y = 1; }
    eval("var y = 2");
    yield y;
  }
  return g().next().value;
}), 2, "generator sibling block let");

assert.sameValue(outcome(function() {
  function f(a = eval("var y = 3")) { let y = 2; return y; }
  return f();
}), 2, "parameter-expression eval var is separate from a body let");

// Controls
assert.sameValue(outcome(function() {
  { let y = 1; { eval("var y"); } }
  return "no throw";
}), "SyntaxError", "block-level let conflicts");

assert.sameValue(outcome(function() {
  let y = 1;
  eval("var z = 3");
  return z + y;
}), 4, "a non-conflicting eval var succeeds");

assert.sameValue(outcome(function() {
  let y = 1;
  return (function() {
    eval("var y = 5");
    return y;
  })();
}), 5, "a var in an inner function does not conflict with an outer let");

assert.sameValue(outcome(function() {
  let y = 1;
  return (() => { eval("var y = 7"); return y; })();
}), 7, "an eval in a nested arrow declares into the arrow's own variable scope");

assert.sameValue(outcome(function() {
  var y = 1;
  eval("var y = 2");
  return y;
}), 2, "var redeclared by eval var is fine");

assert.sameValue(outcome(function() {
  "use strict";
  let y = 1;
  eval("var y = 2");
  return y;
}), 1, "strict eval has its own variable environment");

assert.sameValue(outcome(function() {
  let y = 1;
  eval("'use strict'; var y = 2");
  return y;
}), 1, "eval code with a Use Strict Directive has its own variable environment");

Promise.all([
  asyncFn().then(() => "no throw", e => e.constructor.name),
  asyncGen().next().then(() => "no throw", e => e.constructor.name)
]).then(function(results) {
  assert.sameValue(results[0], "SyntaxError", "async function top-level let");
  assert.sameValue(results[1], "SyntaxError", "async generator top-level const");
}).then($DONE, $DONE);
