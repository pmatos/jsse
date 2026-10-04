// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-%regexpstringiteratorprototype%.next
description: >
  The RegExp String Iterator's [[IteratingRegExp]] internal slot (the
  matcher object created by RegExp.prototype[@@matchAll]) stays reachable
  across a garbage collection even when nothing but the iterator itself
  references it.
info: |
  CreateRegExpStringIterator ( R, S, global, fullUnicode )

  1. Let iterator be OrdinaryObjectCreate(%RegExpStringIteratorPrototype%,
     « [[IteratingRegExp]], [[IteratedString]], [[Global]], [[Unicode]],
     [[Done]] »).
  2. Set iterator.[[IteratingRegExp]] to R.
  ...

  %RegExpStringIteratorPrototype%.next ( )

  ...
  5. Let R be O.[[IteratingRegExp]].
  ...
  9. Let match be ? RegExpExec(R, S).

  RegExp.prototype [ @@matchAll ] ( string ) step 5 constructs a fresh
  matcher via Construct(C, « R, flags »), so it is never exposed to script
  through any path other than the iterator's own [[IteratingRegExp]] slot.
  Once the only script-visible binding is the iterator, a GC cycle between
  next() calls must still find the matcher through the iterator's engine-
  internal state, not through a script-reachable root.
features: [host-gc-required]
---*/

var it = "abc".matchAll(/b/g);

$262.gc();

var r1 = it.next();
assert.sameValue(r1.done, false, "first match is not done");
assert.sameValue(r1.value[0], "b", "first match text");
assert.sameValue(r1.value.index, 1, "first match index");

$262.gc();

// A null RegExpExec result (no more matches) still dereferences
// O.[[IteratingRegExp]], so this exercises the same root on the
// "no more matches" path too.
var r2 = it.next();
assert.sameValue(r2.done, true, "iterator is exhausted after the only match");
