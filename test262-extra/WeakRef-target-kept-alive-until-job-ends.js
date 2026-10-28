/*---
description: >
  A WeakRef target is added to [[KeptAlive]] by the constructor and by deref,
  and cannot be collected until ClearKeptObjects runs.
esid: sec-addtokeptobjects
info: |
  WeakRef ( target )
    ...
    5. Perform AddToKeptObjects(target).

  ClearKeptObjects ( ) empties the list when the synchronous sequence of
  ECMAScript execution completes. A collection inside that sequence must
  leave the target intact; one after it may reclaim it.
features: [WeakRef, host-gc-required]
---*/

var ref = new WeakRef({});
$262.gc();
$262.gc();
assert.notSameValue(ref.deref(), undefined, "kept alive by the constructor");

$262.clearKeptObjects();
$262.gc();
assert.sameValue(ref.deref(), undefined, "collected once the kept list is cleared");

var other = new WeakRef({});
$262.clearKeptObjects();
$262.gc();
assert.sameValue(other.deref(), undefined, "an unreferenced target is reclaimed");

var held = {};
var strong = new WeakRef(held);
$262.clearKeptObjects();
$262.gc();
assert.sameValue(strong.deref(), held, "a reachable target is never cleared");
