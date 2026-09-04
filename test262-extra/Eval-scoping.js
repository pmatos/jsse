// Tests eval() scoping behavior that holds identically in sloppy and
// strict mode: `let` declared inside an eval never leaks regardless of
// mode, reading an outer binding through a direct eval works in both,
// indirect eval always targets the global scope in both, and eval's
// return value / `this` binding are mode-independent. The var/function-
// declaration leakage assertions that are sloppy-mode-only live in
// Eval-scoping-sloppy-leakage.js instead — a direct eval called from
// strict-mode code gets its own fresh VariableEnvironment
// (EvalDeclarationInstantiation) and never leaks a `var` or function
// declaration outward, so those assertions cannot hold here.
// Spec: ECMAScript 2024, sec-eval-x

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
