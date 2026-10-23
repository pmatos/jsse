// Tests that the with-statement target object stays live across a GC
// triggered inside the with-block.
// Spec: ECMAScript 2024 §14.11 (The with Statement) — the binding object
// must remain reachable for the duration of the with-block. Regression test
// for PR 1b.4 (issue #66 / #107) where WithObject._object was dropped and
// the obj_id is now rooted only via Interpreter::collect_env_roots.

var probe = { sentinel: 0xdeadbeef, marker: "with-target" };

with (probe) {
  // Allocate enough garbage to push the GC over the threshold, then force a
  // collection while we're still inside the with-scope.
  for (var i = 0; i < 5000; i++) {
    var tmp = { idx: i, payload: "filler" };
  }
  $262.gc();

  // Property reads on the with-target should still resolve correctly.
  if (sentinel !== 0xdeadbeef) {
    throw new Error("with-target collected: sentinel=" + sentinel);
  }
  if (marker !== "with-target") {
    throw new Error("with-target collected: marker=" + marker);
  }

  // Writes inside the with-block must mutate the original object, not leak
  // to the surrounding scope.
  sentinel = 1;
}

if (probe.sentinel !== 1) {
  throw new Error("with-block write did not reach probe: " + probe.sentinel);
}
// `sentinel` must be unresolvable outside the with-block.
var threw = false;
try {
  sentinel;
} catch (e) {
  threw = e instanceof ReferenceError;
}
if (!threw) {
  throw new Error("with-binding leaked outside with-block");
}
