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
