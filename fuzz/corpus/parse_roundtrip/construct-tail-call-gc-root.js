/*---
description: >
  The object under construction must survive a garbage collection triggered
  from within a constructor's proper-tail-call chain. When a strict-mode
  constructor ends in `return <call>`, [[Construct]] captures the constructed
  `this` and then drives the tail-call chain; a callee that does not reference
  that object and forces a GC must not cause it to be swept before
  [[Construct]] returns it.
info: |
  A strict `return <call>` is a proper tail call, so the constructor's frame is
  popped while the tail callee runs. The constructed object is then reachable
  only through [[Construct]]'s internal capture, which must be kept as a GC root
  for the duration of the tail-call drive — otherwise a `$262.gc()` (or an
  allocation safepoint) in the callee can free the object and reuse its slot
  before [[Construct]] returns it.
---*/

function forceGcAndReturnUndefined() {
  "use strict";
  // The constructor's frame is already popped and this callee never receives
  // the constructed object, so nothing else keeps it alive. Churn the heap to
  // encourage slot reuse, then force a collection mid-drive.
  for (var i = 0; i < 5000; i++) {
    var filler = { idx: i, payload: "filler" };
  }
  $262.gc();
  // implicitly returns undefined -> [[Construct]] must fall back to `this`
}

function Ctor() {
  "use strict";
  this.sentinel = 0xdeadbeef;
  this.marker = "survivor";
  return forceGcAndReturnUndefined(); // strict tail call; callee cannot see `this`
}

// Base construct path (new).
var instance = new Ctor();
assert.sameValue(typeof instance, "object", "the constructed object survives the tail-call GC");
assert.sameValue(instance.sentinel, 0xdeadbeef, "sentinel survived (object not swept/reused)");
assert.sameValue(instance.marker, "survivor", "marker survived (object not swept/reused)");

// Base construct path (Reflect.construct).
var reflected = Reflect.construct(Ctor, []);
assert.sameValue(reflected.sentinel, 0xdeadbeef, "Reflect.construct: sentinel survived");
assert.sameValue(reflected.marker, "survivor", "Reflect.construct: marker survived");

// Derived construct path: the super-initialized `this` must survive too.
class Base {}
class Derived extends Base {
  constructor() {
    super();
    this.tag = "derived-survivor";
    return forceGcAndReturnUndefined(); // strict tail call after super()
  }
}
var derived = new Derived();
assert.sameValue(typeof derived, "object", "derived: the constructed object survives");
assert.sameValue(derived.tag, "derived-survivor", "derived: property survived the tail-call GC");
assert.sameValue(derived instanceof Derived, true, "derived: prototype chain intact");
