/*---
description: >
  Fixing the [[Construct]] tail-call return-value substitution must not disable
  proper tail calls in a constructor body: a deep strict-mode tail recursion
  reached through `return <call>` in a constructor must run in bounded stack
  space and complete, still yielding the constructed object.
info: |
  A strict-mode `return <call>` is a proper tail call (PrepareForTailCall), so
  the caller frame is discarded before the callee runs. This holds for a call
  in a constructor body. The [[Construct]] machinery resolves the tail-call
  chain iteratively and then applies its "return this if the result is not an
  Object" step — it must not turn the tail call into an ordinary (stack-growing)
  call. Without proper tail calls this recursion overflows the stack.
flags: [onlyStrict]
features: [tail-call-optimization]
---*/

function countdown(n) {
  "use strict"; // proper tail calls require strict mode; pin it per-function
  if (n <= 0) return;
  return countdown(n - 1); // proper tail call
}

function Deep() {
  "use strict";
  this.built = true;
  return countdown(200000); // tail call returning undefined; O(1) stack via PTC
}

var viaNew = new Deep();
assert.sameValue(typeof viaNew, "object", "new Deep(): constructed object after deep tail recursion");
assert.sameValue(viaNew.built, true, "new Deep(): constructor body ran");

var viaReflect = Reflect.construct(Deep, []);
assert.sameValue(typeof viaReflect, "object", "Reflect.construct(Deep): constructed object");
assert.sameValue(viaReflect.built, true, "Reflect.construct(Deep): constructor body ran");
