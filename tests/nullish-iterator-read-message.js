// RequireObjectCoercible (§13.3.3 EvaluatePropertyAccessWithExpressionKey, ToObject in GetValue) throws a
// TypeError for a nullish base before ToPropertyKey runs; the message text is implementation-defined. Like V8,
// reading Symbol.iterator from undefined or null says so (TypeScript's evaluation unit test
// "forOfEvaluation es5 over undefined with Symbol" matches it, issue #1262).

function messageOf(fn) {
  try {
    fn();
  } catch (e) {
    if (!(e instanceof TypeError)) {
      throw new Error("expected a TypeError, got " + e);
    }
    return e.message;
  }
  throw new Error("expected the read to throw");
}

function check(label, fn, expected) {
  var message = messageOf(fn);
  if (message !== expected) {
    throw new Error(label + ": expected '" + expected + "', got '" + message + "'");
  }
}

var tail = " is not iterable (cannot read property Symbol(Symbol.iterator))";
check("undefined", function () { var u; u[Symbol.iterator]; }, "undefined" + tail);
check("null", function () { var n = null; n[Symbol.iterator]; }, "object null" + tail);
check("key in a variable", function () { var u, k = Symbol.iterator; u[k]; }, "undefined" + tail);
check("method call", function () { var u; u[Symbol.iterator](); }, "undefined" + tail);
check("update", function () { var u; u[Symbol.iterator]++; }, "undefined" + tail);
check("compound assignment", function () { var u; u[Symbol.iterator] += 1; }, "undefined" + tail);
check("other symbol", function () { var n = null; n[Symbol("x")]; }, "Cannot read properties of null (reading property)");
check("string key", function () { var u; u.x; }, "Cannot read properties of undefined (reading 'x')");

var visited = false;
var key = { toString() { visited = true; return "k"; } };
messageOf(function () { var u; u[key]; });
if (visited) {
  throw new Error("ToPropertyKey ran for a nullish base");
}
