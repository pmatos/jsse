// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-regexp.prototype-@@matchall
description: >
  The matcher constructed by RegExp.prototype[@@matchAll] step 5 stays
  reachable across a garbage collection triggered while step 6 (Get(R,
  "lastIndex")) is still running, before the matcher is ever stored in the
  iterator's [[IteratingRegExp]] slot.
info: |
  RegExp.prototype [ @@matchAll ] ( string )

  ...
  5. Let matcher be ? Construct(C, « R, flags »).
  6. Let lastIndex be ? ToLength(? Get(R, "lastIndex")).
  7. Perform ? Set(matcher, "lastIndex", lastIndex, true).
  ...

  Between steps 5 and the point where CreateRegExpStringIterator stores
  matcher into the new iterator's [[IteratingRegExp]] slot, the matcher is
  reachable only through the engine's own construction-time bookkeeping —
  not yet through any script-visible value. ToLength in step 6 calls
  ToNumber, which can invoke a user-defined valueOf and so run arbitrary
  script; a GC cycle inside it must still find the freshly constructed
  matcher.
features: [host-gc-required, class]
---*/

class MyRegExp extends RegExp {}

var re = new MyRegExp("a", "g");
re.lastIndex = {
  valueOf() {
    $262.gc();
    return 0;
  },
};

var it = re[Symbol.matchAll]("aaa");
var r1 = it.next();
assert.sameValue(r1.done, false, "first match is not done");
assert.sameValue(r1.value[0], "a", "first match text");
