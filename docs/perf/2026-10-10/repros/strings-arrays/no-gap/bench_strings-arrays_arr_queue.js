// STATUS: NO GAP BEYOND BASELINE (global ratio 1.36x = 1.03x the scalar-loop control). Kept as a control / absolute-cost case, not as a builtin-gap claim.
// shift / unshift / pop / splice on a 1000-element array used as a queue.
// Models: UniPoker `dealOneCard() { return this._cards.shift(); }` (18% of jsse's UniPoker time), worklists in pdfjs/Babylon.
// Cause: shift/unshift/splice have no dense fast path: O(n) per call with per-element to_string + has/get/set + delete (profile: has_own_property 9%, set_property_value 9%, PropertyMap::get 9%, obj_set_throw 8%). Boa is also O(n) per call (original 20k-element queue exceeded 100 s on both), so the ratio is at baseline; the absolute win from Vec::remove is ~100x and UniPoker spends 17.9% of instructions in shift.
// Measured (jsse v0.11.0 @ 2b621c5 vs boa 0.22.0, median of 5, buildbox3, 2026-10-10): global-scope jsse 3.23 s / boa 2.37 s = 1.36x; function-scope variant (fn-scope/) jsse 3.71 s / boa 2.68 s = 1.38x.
// Prints one checksum line; jsse and boa print the same line. Deterministic (LCG, no Date/Math.random).
// Global-scope variant (top-level `var`): isolates the builtin cost; the scalar-loop control (ctl_scalar_global) defines the baseline ratio here.
var q = [];
for (var i = 0; i < 1000; i++) q.push(i);
var acc = 0;
for (var r = 0; r < 5000; r++) { acc += q.shift(); q.push(r); }
for (var r = 0; r < 2000; r++) { q.unshift(r); q.pop(); }
for (var r = 0; r < 2000; r++) { q.splice(100, 1); q.splice(50, 0, r); }
console.log(acc, q.length, q[0], q[50]);
