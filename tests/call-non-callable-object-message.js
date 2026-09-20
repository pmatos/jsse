// EvaluateCall (§13.3.6.2) requires a TypeError when the callee is not
// callable; the message text is implementation-defined. jsse must not leak its
// internal object representation into that message (issue #652), and the
// TypeError must still be thrown only after the arguments were evaluated.

function messageOf(fn) {
  try {
    fn();
  } catch (e) {
    if (!(e instanceof TypeError)) {
      throw new Error("expected a TypeError, got " + e);
    }
    return e.message;
  }
  throw new Error("expected the call to throw");
}

function check(label, fn, expectedSubject) {
  var message = messageOf(fn);
  if (/class=|callable=|id=|keys=|JsPropertyKey|GC'd/.test(message)) {
    throw new Error(label + ": message leaks internals: " + message);
  }
  if (message !== expectedSubject + " is not a function") {
    throw new Error(
      label + ": expected '" + expectedSubject + " is not a function', got '" + message + "'"
    );
  }
}

class B {}

check("plain object", function () { var x = {}; x(); }, "#<Object>");
check("array", function () { var a = []; a(); }, "#<Array>");
check("class instance", function () { new B()(); }, "#<Object>");
check("call.call on object", function () { Function.prototype.call.call({}); }, "#<Object>");
check("undefined", function () { var u; u(); }, "undefined");
check("null", function () { var n = null; n(); }, "null");
check("number", function () { var n = 1; n(); }, "1");
check("string", function () { var s = "str"; s(); }, "\"str\"");
check("symbol", function () { var s = Symbol("tag"); s(); }, "Symbol(tag)");
check("symbol without description", function () { var s = Symbol(); s(); }, "Symbol()");
check("bigint", function () { var b = 12n; b(); }, "12n");
check("infinity", function () { var n = Infinity; n(); }, "Infinity");
check("negative zero", function () { var n = -0; n(); }, "0");
check("large number", function () { var n = 1e21; n(); }, "1e+21");

function checkNew(label, fn, expectedSubject) {
  var message = messageOf(fn);
  if (message !== expectedSubject + " is not a constructor") {
    throw new Error(
      label + ": expected '" + expectedSubject + " is not a constructor', got '" + message + "'"
    );
  }
}

checkNew("new number", function () { var n = 5; new n(); }, "5");
checkNew("new string", function () { var s = "abc"; new s(); }, "\"abc\"");
checkNew("new symbol", function () { var s = Symbol("tag"); new s(); }, "Symbol(tag)");
checkNew("new object", function () { var o = {}; new o(); }, "#<Object>");
checkNew("new anonymous arrow", function () { new (() => {})(); }, "#<Function>");

var evaluated = 0;
messageOf(function () { ({})(evaluated++); });
if (evaluated !== 1) {
  throw new Error("arguments must be evaluated before the TypeError, evaluated=" + evaluated);
}

var getterRan = false;
var trap = {};
Object.defineProperty(trap, "constructor", {
  get: function () { getterRan = true; return B; },
});
Object.defineProperty(trap, "toString", {
  get: function () { getterRan = true; return function () { return "x"; }; },
});
messageOf(function () { trap(); });
if (getterRan) {
  throw new Error("describing a non-callable callee must not run user code");
}
