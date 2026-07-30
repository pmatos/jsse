/*---
description: >
  Sloppy-mode counterpart to primitive-receiver-set-must-not-mutate-prototype:
  a property write on a primitive base whose receiver walks up to an inherited
  writable data property must be silently rejected (no throw), and — this is
  the part a receiver-shadowing bug can get wrong even when the throw-vs-not
  distinction is masked by sloppy mode — the inherited property itself must
  never be mutated as a side effect of the rejected write.
info: |
  jsse issue #417 / #416. See primitive-receiver-set-must-not-mutate-prototype.js
  for the strict-mode half of this regression pin.
esid: sec-putvalue
flags: [noStrict]
---*/

Number.prototype.probeValue = 1;
var n = 5;
n.probeValue = 99;
if (Number.prototype.probeValue !== 1) {
  throw new Test262Error(
    'sloppy-mode n.probeValue = 99 must not mutate Number.prototype.probeValue'
  );
}
delete Number.prototype.probeValue;
