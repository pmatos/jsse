// A call in the `try` Block of a try statement is NEVER in tail position, even
// in strict mode where proper tail calls apply: the exception handler must stay
// live on the stack. jsse used to (incorrectly) optimize `return f()` inside a
// try block into a tail call and discard the handler, so a throw from f()
// escaped the catch. This is what prevented acorn's `catchStackOverflow`
// (a strict-mode `try { return f() } catch { ... }`) from ever catching.

"use strict";

function thrower() {
  throw new Error("boom");
}

// `return f()` sits inside the try block -> must NOT be a tail call.
function wrap(f) {
  try {
    return f();
  } catch (e) {
    return "caught:" + e.message;
  }
}

var r = wrap(thrower);
if (r !== "caught:boom") {
  throw new Error("expected 'caught:boom' (handler must catch), got: " + r);
}

// A genuine tail call outside any try must still be optimized (no stack growth).
// If TCO were disabled wholesale this would overflow; it must simply return.
function count(n, acc) {
  "use strict";
  if (n <= 0) return acc;
  return count(n - 1, acc + 1); // real tail position
}
if (count(200000, 0) !== 200000) {
  throw new Error("proper tail call outside try regressed");
}
