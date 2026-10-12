/*---
description: >
  A sloppy assignment to a non-writable global property fails silently even
  after a strict script has run in the same realm: the failure is reported by
  the strictness of the assigning code, not of the realm's global environment.
info: |
  PutValue (sec-putvalue) passes the strictness of the code performing the
  assignment as the throw flag of SetMutableBinding / [[Set]]; a strict
  Script elsewhere in the realm does not change it.
esid: sec-putvalue
flags: [noStrict]
---*/

$262.evalScript('"use strict";');

Object.defineProperty(globalThis, "nwSloppy", { value: 1, writable: false, configurable: true });

function assign() { nwSloppy = 2; return nwSloppy; }
function compound() { nwSloppy += 2; return nwSloppy; }
function update() { nwSloppy++; return nwSloppy; }
function strictAssign() { "use strict"; nwSloppy = 2; }

assert.sameValue(assign(), 1, "sloppy assignment");
assert.sameValue(compound(), 1, "sloppy compound assignment");
assert.sameValue(update(), 1, "sloppy update");
assert.throws(TypeError, strictAssign, "strict assignment");
assert.sameValue(nwSloppy, 1);
