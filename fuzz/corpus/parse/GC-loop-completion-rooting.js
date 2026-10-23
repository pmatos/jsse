// Tests GC rooting across non-loop safepoints.
// Issue: https://github.com/pmatos/jsse/issues/118 (follow-up to PR #112).
// Without rooting, GC pressure mid-iteration frees a heap value held only by a
// Rust local (the iterable wrapper or the running completion accumulator).

// --- Case 1: exec_for_in obj_val rooting ---
// Object("ABCDE") creates a primitive String wrapper held only by exec_for_in's
// `obj_val` Rust local. Allocations inside the body trip the adaptive
// `gc_threshold_bytes`, and inner safepoints (e.g. the inner loop's backedge)
// fire GC. Without rooting `obj_val`, the wrapper is freed and the bare
// `obj_id` used by `proxy_has_property` either dangles or hits a reused slot.
{
  let count = 0;
  for (var k in Object("ABCDE")) {
    for (let i = 0; i < 2000; i++) ({});
    count++;
  }
  if (count !== 5) {
    throw new Test262Error(
      "for-in over Object(\"ABCDE\") should yield 5 keys, got " + count
    );
  }
}

// --- Case 2: exec_statements completion accumulator ---
// The eval program is a labeled block whose 3 inner statements run under
// exec_statements:
//   stmt 1: expression statement -> Completion::Normal({tag:"live"}) sets `result`.
//   stmt 2: VariableStatement -> Completion::Empty (preserves `result`); its IIFE
//           initializer runs ~30k allocations to trip gc_threshold_bytes, and
//           inner safepoints fire GC while `result` is held only by the outer
//           Rust local.
//   stmt 3: `break outer` -> Completion::Break(Some("outer"), None);
//           exec_statements captures `Some(result.value_or(Undefined))` into the
//           Break value.
// The Statement::Labeled handler unwraps that Break to
// Completion::Normal({tag:"live"}). eval returns it.
const sentinel = eval(`
  outer: {
    ({tag: "live", magic: 305419896});
    var trigger = (function () {
      for (let i = 0; i < 30000; i++) ({});
      return 0;
    })();
    break outer;
  }
`);
if (typeof sentinel !== "object" || sentinel === null) {
  throw new Test262Error(
    "eval completion lost identity (expected object, got " + typeof sentinel + ")"
  );
}
if (sentinel.tag !== "live" || sentinel.magic !== 305419896) {
  throw new Test262Error(
    "eval completion corrupted: " + JSON.stringify(sentinel)
  );
}
