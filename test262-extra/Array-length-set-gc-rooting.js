/*---
description: >
  ArraySetLength's Array receiver must remain strongly reachable across the
  ToUint32/ToNumber coercion of the new length value, even when that
  coercion (via a user valueOf) triggers garbage collection and the
  receiver is otherwise reachable only through a Rust-local reference
  (e.g. an array literal used solely as a destructuring-assignment target,
  never bound to a variable).
info: |
  jsse issue #417/#416 fix, PR #442: a follow-up review comment found that
  the new ArraySetLength dispatch path could crash (Rust panic, exit 101)
  when the Array receiver was ephemeral. ArraySetLength's ToUint32(Desc.
  [[Value]]) / ToNumber(Desc.[[Value]]) steps run user code (valueOf), and
  every step after that re-derefs the receiver by id — an unrooted,
  ephemeral receiver could be collected in between, turning the
  implementation's internal object lookup into a null dereference.
esid: sec-arraysetlength
---*/

var gcRan = false;
var rhs = {
  valueOf: function () {
    $262.gc();
    gcRan = true;
    return 1;
  },
};

// The array literal is the destructuring-assignment target itself — never
// bound to a variable, so it is reachable only through the engine's own
// transient state while the assignment runs. Surviving this without
// crashing is the property under test; there is no post-assignment handle
// to assert a length on, since the array is discarded immediately after.
({ x: [].length } = { x: rhs });

assert(gcRan, "the valueOf side effect must have actually run");
