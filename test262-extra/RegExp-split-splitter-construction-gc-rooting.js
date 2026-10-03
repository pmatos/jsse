// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-regexp.prototype-@@split
description: >
  The splitter constructed by RegExp.prototype[@@split] step 7 stays
  reachable across a garbage collection triggered while step 10
  (ToUint32(limit)) is still running, before the loop ever stores a
  reference to the splitter anywhere traced.
info: |
  RegExp.prototype [ @@split ] ( string, limit )

  ...
  7. Let splitter be ? Construct(C, « rx, newFlags »).
  8. Let A be ! ArrayCreate(0).
  9. Let lengthA be 0.
  10. If limit is undefined, let lim be 2^32 - 1; else let lim be ?
      ToUint32(limit).
  ...
  15. Repeat, while q < size,
      a. Perform ? Set(splitter, "lastIndex", 𝔽(q), true).
      b. Let z be ? RegExpExec(splitter, S).
      ...

  Between step 7 and the function's final use of splitter, splitter is
  reachable only through the engine's own construction-time bookkeeping —
  not yet through any script-visible value. ToUint32 in step 10 calls
  ToNumber, which can invoke a user-defined valueOf and so run arbitrary
  script; a GC cycle inside it must still find the freshly constructed
  splitter, and the splitter must stay reachable through the rest of the
  loop (steps 15.a and 15.b can also run arbitrary script via a
  user-defined lastIndex setter or exec override).
features: [host-gc-required]
includes: [compareArray.js]
---*/

var re = /b/;
var limit = {
  valueOf() {
    $262.gc();
    // Force allocation churn so a freed arena slot is likely reused before
    // the next access to the (incorrectly) unrooted splitter.
    for (var i = 0; i < 64; i++) {
      [{}, {}, {}];
    }
    return 10;
  },
};

var result = re[Symbol.split]("abc", limit);
assert.compareArray(result, ["a", "c"]);
