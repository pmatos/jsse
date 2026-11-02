// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-identifiers-static-semantics-early-errors
description: >
  `yield` and `await` written with a unicode escape are plain identifiers in sloppy code outside
  generators and async functions, so they are valid shorthand properties and pattern targets
info: |
  IdentifierReference[Yield, Await] :
    Identifier
    [~Yield] yield
    [~Await] await

  It is a Syntax Error if this production has a [Yield] parameter and StringValue of Identifier is
  "yield". It is a Syntax Error if this production has an [Await] parameter and StringValue of
  Identifier is "await".
flags: [noStrict]
---*/

var yield = 1;
var await = 2;

var o = { yi\u0065ld, aw\u0061it };
assert.sameValue(o.yield, 1, "shorthand yield");
assert.sameValue(o.await, 2, "shorthand await");

({ yi\u0065ld, aw\u0061it } = { yield: 5, await: 6 });
assert.sameValue(yield, 5, "pattern target yield");
assert.sameValue(await, 6, "pattern target await");

({ yi\u0065ld = 7, aw\u0061it = 8 } = {});
assert.sameValue(yield, 7, "pattern default yield");
assert.sameValue(await, 8, "pattern default await");
