// Tests eval() scoping behavior.
// Spec: ECMAScript 2024, sec-eval-x

// eval var scoping — var in eval leaks to enclosing function scope
(function() {
  eval("var evalVar = 42;");
  if (evalVar !== 42) {
    throw new Test262Error('eval var should leak to function scope, got: ' + typeof evalVar);
  }
})();

// eval let scoping — let in eval stays in eval scope
(function() {
  eval("let evalLet = 99;");
  try {
    evalLet;
    throw new Test262Error('eval let should not leak to function scope');
  } catch (e) {
    if (!(e instanceof ReferenceError)) {
      throw new Test262Error('accessing eval let should throw ReferenceError, got: ' + e);
    }
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

// Direct eval inherits scope
var directScope = 100;
(function() {
  var localVar = 200;
  var result = eval("localVar + directScope");
  if (result !== 300) {
    throw new Test262Error('direct eval should access local scope, got: ' + result);
  }
})();

// Indirect eval uses global scope
var globalForEval = 500;
(function() {
  var localForEval = 600;
  var indirectEval = eval;
  try {
    var result = indirectEval("localForEval");
    throw new Test262Error('indirect eval should not access local scope');
  } catch (e) {
    if (!(e instanceof ReferenceError)) {
      throw new Test262Error('indirect eval accessing local should throw ReferenceError');
    }
  }
  var result2 = indirectEval("globalForEval");
  if (result2 !== 500) {
    throw new Test262Error('indirect eval should access global scope, got: ' + result2);
  }
})();

// eval function declarations in sloppy mode
(function() {
  eval("function evalFunc() { return 'hello'; }");
  if (evalFunc() !== "hello") {
    throw new Test262Error('eval function declaration should be accessible');
  }
})();

// eval return value is the last expression
(function() {
  var r = eval("1; 2; 3;");
  if (r !== 3) {
    throw new Test262Error('eval should return last expression value, got: ' + r);
  }
  var r2 = eval("");
  if (r2 !== undefined) {
    throw new Test262Error('eval of empty string should return undefined, got: ' + r2);
  }
})();

// eval this binding
(function() {
  var obj = {
    method: function() {
      return eval("this");
    }
  };
  if (obj.method() !== obj) {
    throw new Test262Error('eval this should match enclosing this');
  }
})();
