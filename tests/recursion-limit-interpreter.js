// Deep JS call recursion must throw a *catchable* RangeError, not overflow the
// native stack and abort the process (SIGABRT).
//
// This is what lets acorn's `catchStackOverflow` guard work: acorn's parser
// relies on the host engine throwing a catchable stack-overflow error (message
// matching /stack.*(exceeded|overflow)/i) when recursion is too deep. jsse used
// to exhaust the native stack and SIGABRT instead, so acorn could never catch
// it (see acorn test/tests.js "Not enough stack space" case).

// Non-tail recursion with no base case: guaranteed to keep growing the call
// stack until the engine's depth limit is hit. `1 + ...` keeps it out of tail
// position so it cannot be optimized into a loop.
function recurse(n) {
  return 1 + recurse(n + 1);
}

var threw = false;
var err = null;
try {
  recurse(0);
} catch (e) {
  threw = true;
  err = e;
}

if (!threw) {
  throw new Error("expected deep recursion to throw, but it returned normally");
}
if (!(err instanceof RangeError)) {
  throw new Error(
    "expected a RangeError, got " + (err && err.name) + ": " + (err && err.message)
  );
}
if (!/stack/i.test(String(err.message))) {
  throw new Error("expected the message to mention the stack, got: " + err.message);
}

// A second call after recovery must still work — the depth counter has to be
// restored as the stack unwinds, not left saturated.
function factorialish(n) {
  return n <= 0 ? 0 : 1 + factorialish(n - 1);
}
if (factorialish(50) !== 50) {
  throw new Error("engine did not recover after a stack-overflow RangeError");
}

// ---------------------------------------------------------------------------
// Proxy apply-trap forwarding recursion (jsse#607): each JS-level recursion
// step round-trips through the Proxy's `apply` trap and `Function.prototype
// .apply`, the stack-hungriest call shape measured while calibrating
// `CALL_DEPTH_*` — it must still raise a catchable RangeError rather than
// overflowing the native stack, the same as plain call recursion above.

function callMustRangeError(label, fn) {
  var threw = false;
  var err = null;
  try {
    fn();
  } catch (e) {
    threw = true;
    err = e;
  }
  if (!threw) {
    throw new Error("expected " + label + " to throw");
  }
  if (!(err instanceof RangeError)) {
    throw new Error(
      "expected " + label + " to throw a RangeError, got " +
        (err && err.name) + ": " + (err && err.message)
    );
  }
  if (!/stack/i.test(String(err.message))) {
    throw new Error(
      "expected " + label + " message to mention the stack, got: " + err.message
    );
  }
}

function proxyApplyRecurse(n) {
  return n <= 0 ? 0 : 1 + proxyApplyForward(n - 1);
}
var proxyApplyForward = new Proxy(proxyApplyRecurse, {
  apply(target, thisArg, args) {
    return target.apply(thisArg, args);
  },
});
callMustRangeError("Proxy apply-trap forwarding recursion", function () {
  proxyApplyForward(10000);
});

// ---------------------------------------------------------------------------
// Deep eval-time expression recursion (jsse#241).
//
// Binary/logical operators parse in a left-associative *loop*, so a flat
// expression like "1+1+1+…" never trips the parser depth limit — the tree is
// only walked recursively later, by eval_expr, once per operand. That walk
// used to exhaust the native stack (SIGABRT) instead of throwing. It must now
// throw a *catchable* RangeError, just like deep call recursion above.

function evalMustRangeError(label, src) {
  var threw = false;
  var err = null;
  try {
    eval(src);
  } catch (e) {
    threw = true;
    err = e;
  }
  if (!threw) {
    throw new Error("expected " + label + " to throw at eval time");
  }
  if (!(err instanceof RangeError)) {
    throw new Error(
      "expected " + label + " to throw a RangeError, got " +
        (err && err.name) + ": " + (err && err.message)
    );
  }
  if (!/stack/i.test(String(err.message))) {
    throw new Error(
      "expected " + label + " message to mention the stack, got: " + err.message
    );
  }
}

// Additive (left-nested Binary) and logical (left-nested Logical) arms both
// descend through eval_expr and must both be bounded. A self-referential
// member chain (`a.b.b.b…`) is the stack-hungriest pure-eval_depth shape
// measured while calibrating this constant, and is bounded the same way.
//
// 80,000 operands: comfortably above both profiles' EVAL_DEPTH_LIMIT (2,000
// debug / 50,000 release), while staying clear of jsse#612 — a *parser*-side
// native stack limit (unrelated to eval_depth, found while implementing
// jsse#607) that overflows a debug build on these same flat/member-chain
// shapes past roughly 122,000 operands, before evaluation even starts.
evalMustRangeError("deep additive expression", "1" + "+1".repeat(80000));
evalMustRangeError("deep logical expression", "1" + "&&1".repeat(80000));
evalMustRangeError(
  "self-referential member chain",
  "var a = {}; a.b = a; a" + ".b".repeat(80000) + ";"
);

// The eval-depth counter must unwind on the throw path, not leak. If it did,
// repeated deep-eval failures would accumulate phantom depth until an
// expression well *below* the limit spuriously threw. Trip the limit many
// times, then require a moderately deep (but legal) expression to still
// evaluate to the right value in the same process. 60,000 stays under
// jsse#612's parser-side ceiling noted above.
for (var i = 0; i < 20; i++) {
  try {
    eval("1" + "+1".repeat(60000));
  } catch (e) {
    // expected RangeError, swallow
  }
}
// 500 stays safely under both profiles' EVAL_DEPTH_LIMIT (2,000 debug /
// 50,000 release) — this checks the counter unwinds, not "realistic code
// depth", so the exact value is otherwise arbitrary.
if (eval("1" + "+1".repeat(500)) !== 501) {
  throw new Error("eval-depth counter leaked: a below-limit expression failed after recovery");
}
